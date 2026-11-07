//! The egress broker (M8.6; docs/21 "Sandbox substrate boundary": explicit
//! egress domains/ports by capability, dynamic credential handles via
//! broker injection; docs/33 "Sandbox Gateway": brokers dynamic secret
//! handles and network policy). A guest has no network interface. What its
//! policy grants is served here, on the host side of the sandbox's private
//! channel: the guest's local proxy opens one channel per client
//! connection and names the destination; the broker admits it by the
//! policy or refuses it, records either, and then either tunnels bytes to
//! an admitted `host:port` (TLS stays end to end between the process and
//! the destination) or, for a credentialed virtual host, performs the
//! plain-HTTP request the guest sent against the real target over TLS with
//! the secret it holds under the handle — the guest never sees the secret,
//! the target or TLS.
//!
//! Channel protocol (guest → broker, one line): `TUNNEL <host>:<port>` or
//! `HTTP <host>[:<port>]`; the broker answers `OK` or `DENIED <reason>`;
//! then bytes flow.

// PX-168 lint policy: this file baselined expect/unwrap sites; new ones need review.
#![allow(clippy::expect_used)]
use std::collections::HashMap;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use crate::backend::Channel;
use crate::policy::NetworkPolicy;

/// One egress decision, for the audit.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EgressRecord {
    /// `tunnel` | `http` | `credentialed`.
    pub kind: String,
    /// What the guest asked for (`host:port` or the virtual host).
    pub destination: String,
    /// Admitted or refused.
    pub allowed: bool,
    /// The capability of the rule or grant that admitted it (empty when refused).
    pub capability: String,
    /// Why, when refused; what was forwarded to, when credentialed.
    pub detail: String,
    /// The typed reason of a refusal or a failure (`NO_EGRESS_RULE`,
    /// `ORG_ALLOW_LIST`, ...; see [`reason`]); empty when admitted.
    #[serde(default)]
    pub reason: String,
    /// ms since the epoch.
    pub at_ms: i64,
}

/// The typed reasons an egress decision carries.
pub mod reason {
    /// No rule of the sandbox's policy names the destination.
    pub const NO_EGRESS_RULE: &str = "NO_EGRESS_RULE";
    /// A rule names it and the organisation's allow-list does not: the list
    /// caps every rule below it (PX-085, AUT-D07).
    pub const ORG_ALLOW_LIST: &str = "ORG_ALLOW_LIST";
    /// The destination was admitted and could not be reached.
    pub const CONNECT_FAILED: &str = "CONNECT_FAILED";
    /// A credential grant is past its lifetime.
    pub const CREDENTIAL_EXPIRED: &str = "CREDENTIAL_EXPIRED";
    /// A credential grant holds no secret.
    pub const CREDENTIAL_MISSING: &str = "CREDENTIAL_MISSING";
    /// The guest asked in a form the broker does not speak.
    pub const BAD_REQUEST: &str = "BAD_REQUEST";
    /// A rule admits the name and the name resolves to an address that is not
    /// public (loopback, private, link-local, unique-local, multicast, cloud
    /// metadata); only a rule that names the IP literal itself reaches one
    /// (PX-147).
    pub const TARGET_NOT_PUBLIC: &str = "TARGET_NOT_PUBLIC";
}

/// Whether `ip` is an address a name-based rule may reach: not loopback,
/// unspecified, private, link-local, unique-local, shared (CGNAT), documentation,
/// benchmarking, multicast or reserved, and not an IPv4 address carried inside an
/// IPv6 one (mapped, NAT64, 6to4) that is itself refused (PX-147). The cloud
/// metadata endpoints are inside these ranges (`169.254.169.254`,
/// `fd00:ec2::254`, `100.100.100.200`).
#[must_use]
pub fn is_public_address(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(a) => public_v4(a),
        IpAddr::V6(a) => {
            if let Some(v4) = a.to_ipv4_mapped() {
                return public_v4(v4);
            }
            let s = a.segments();
            // NAT64 well-known prefix 64:ff9b::/96 and 6to4 2002::/16 embed
            // an IPv4 address; the embedded one decides.
            if s[0] == 0x0064 && s[1] == 0xff9b && s[2..6].iter().all(|x| *x == 0) {
                let [a0, a1] = s[6].to_be_bytes();
                let [a2, a3] = s[7].to_be_bytes();
                return public_v4(std::net::Ipv4Addr::new(a0, a1, a2, a3));
            }
            if s[0] == 0x2002 {
                let [a0, a1] = s[1].to_be_bytes();
                let [a2, a3] = s[2].to_be_bytes();
                return public_v4(std::net::Ipv4Addr::new(a0, a1, a2, a3));
            }
            !(a.is_unspecified()
                || a.is_loopback()
                || a.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00 // unique-local fc00::/7
                || (s[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
                || (s[0] & 0xffc0) == 0xfec0 // site-local, deprecated
                || (s[0] == 0x2001 && s[1] == 0x0db8) // documentation
                || (s[0] == 0x0100 && s[1..4].iter().all(|x| *x == 0))) // discard 100::/64
        }
    }
}

fn public_v4(a: std::net::Ipv4Addr) -> bool {
    let o = a.octets();
    !(a.is_unspecified()
        || a.is_loopback()
        || a.is_private()
        || a.is_link_local()
        || a.is_broadcast()
        || a.is_multicast()
        || a.is_documentation()
        || o[0] == 0 // "this network"
        || (o[0] == 100 && (o[1] & 0xc0) == 64) // shared address space 100.64/10
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // IETF protocol assignments
        || (o[0] == 198 && (o[1] & 0xfe) == 18) // benchmarking 198.18/15
        || o[0] >= 240) // reserved
}

/// How the broker turns an admitted name into a connection. The one
/// production implementation is [`SystemDialer`]; the trait is the seam that
/// lets a suite answer a name differently on a second lookup (a DNS rebind)
/// and observe which address is connected.
pub trait EgressDialer: Send + Sync {
    /// Resolve `host` to socket addresses. The broker calls this once per
    /// destination.
    fn resolve<'a>(
        &'a self,
        host: &'a str,
        port: u16,
    ) -> futures_util::future::BoxFuture<'a, std::io::Result<Vec<std::net::SocketAddr>>>;

    /// Connect to an address the broker has already checked.
    fn connect(
        &self,
        addr: std::net::SocketAddr,
    ) -> futures_util::future::BoxFuture<'_, std::io::Result<tokio::net::TcpStream>>;
}

/// The system resolver and a plain TCP connect.
pub struct SystemDialer;

impl EgressDialer for SystemDialer {
    fn resolve<'a>(
        &'a self,
        host: &'a str,
        port: u16,
    ) -> futures_util::future::BoxFuture<'a, std::io::Result<Vec<std::net::SocketAddr>>> {
        Box::pin(async move { Ok(tokio::net::lookup_host((host, port)).await?.collect()) })
    }

    fn connect(
        &self,
        addr: std::net::SocketAddr,
    ) -> futures_util::future::BoxFuture<'_, std::io::Result<tokio::net::TcpStream>> {
        Box::pin(tokio::net::TcpStream::connect(addr))
    }
}

/// Why an admitted destination could not be dialled.
enum DialError {
    /// Every answer was checked and one is not public: nothing is connected.
    NotPublic(std::net::SocketAddr),
    /// Resolution or connection failed.
    Io(std::io::Error),
}

/// Where decisions go.
pub trait EgressAudit: Send + Sync {
    /// Record one decision.
    fn record(&self, sandbox_id: &str, record: EgressRecord);
}

/// An audit that keeps records in memory (tests, and the gateway's live view).
#[derive(Default)]
pub struct MemoryAudit {
    records: std::sync::Mutex<Vec<(String, EgressRecord)>>,
}

impl MemoryAudit {
    /// Records for a sandbox.
    #[must_use]
    pub fn records(&self, sandbox_id: &str) -> Vec<EgressRecord> {
        self.records
            .lock()
            .expect("records")
            .iter()
            .filter(|(s, _)| s == sandbox_id)
            .map(|(_, r)| r.clone())
            .collect()
    }
}

impl EgressAudit for MemoryAudit {
    fn record(&self, sandbox_id: &str, record: EgressRecord) {
        self.records
            .lock()
            .expect("records")
            .push((sandbox_id.to_owned(), record));
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// What the broker will do for a credentialed virtual host.
enum CredentialLookup {
    /// The host is not credentialed.
    NotCredentialed,
    /// Inject this grant's secret.
    Ready(Box<crate::policy::CredentialGrant>, String),
    /// Refuse, with the reason the audit records.
    Refused(
        Box<crate::policy::CredentialGrant>,
        &'static str,
        &'static str,
    ),
}

/// What the policy decides for one destination.
enum Decision {
    /// A rule admits it (its capability, for the audit). `literal` is true
    /// when the rule names an IP address itself, the one case that may reach a
    /// non-public address (PX-147).
    Admit { capability: String, literal: bool },
    /// No rule names it.
    NoRule,
    /// A rule names it and the organisation's list does not (the list's note).
    Org(String),
}

/// The detail an organisation refusal carries.
fn org_detail(note: &str) -> String {
    format!("the organisation's egress allow-list does not name this destination ({note})")
}

/// The broker for one sandbox.
pub struct EgressBroker {
    sandbox_id: String,
    policy: std::sync::Mutex<NetworkPolicy>,
    /// Secrets by handle — memory only, and only while the grant that
    /// named them is live (REQ-EV-0288): the moment a grant's lifetime
    /// passes, the secret is dropped here, not merely refused.
    secrets: std::sync::Mutex<HashMap<String, String>>,
    audit: Arc<dyn EgressAudit>,
    dialer: Arc<dyn EgressDialer>,
}

impl EgressBroker {
    /// A broker over `policy` with the secrets for its credential grants.
    #[must_use]
    pub fn new(
        sandbox_id: &str,
        policy: NetworkPolicy,
        secrets: HashMap<String, String>,
        audit: Arc<dyn EgressAudit>,
    ) -> Arc<Self> {
        Self::with_dialer(sandbox_id, policy, secrets, audit, Arc::new(SystemDialer))
    }

    /// A broker that resolves and connects through `dialer` (a suite's
    /// rebinding resolver; production uses [`SystemDialer`] through [`Self::new`]).
    #[must_use]
    pub fn with_dialer(
        sandbox_id: &str,
        policy: NetworkPolicy,
        secrets: HashMap<String, String>,
        audit: Arc<dyn EgressAudit>,
        dialer: Arc<dyn EgressDialer>,
    ) -> Arc<Self> {
        Arc::new(Self {
            sandbox_id: sandbox_id.to_owned(),
            policy: std::sync::Mutex::new(policy),
            secrets: std::sync::Mutex::new(secrets),
            audit,
            dialer,
        })
    }

    /// Resolve `host` once, refuse unless every answer is public (or the rule
    /// named an IP literal), and connect to an address that was checked. The
    /// name is never resolved a second time, so an answer that changes after
    /// the check cannot steer the connection.
    async fn dial(
        &self,
        host: &str,
        port: u16,
        literal: bool,
    ) -> Result<tokio::net::TcpStream, DialError> {
        let addrs = self
            .dialer
            .resolve(host, port)
            .await
            .map_err(DialError::Io)?;
        if addrs.is_empty() {
            return Err(DialError::Io(std::io::Error::other(
                "the name did not resolve",
            )));
        }
        if !literal && let Some(bad) = addrs.iter().find(|a| !is_public_address(a.ip())) {
            return Err(DialError::NotPublic(*bad));
        }
        let mut last = std::io::Error::other("no address connected");
        for addr in addrs {
            match self.dialer.connect(addr).await {
                Ok(s) => return Ok(s),
                Err(e) => last = e,
            }
        }
        Err(DialError::Io(last))
    }

    /// Hand the broker a fresh secret and lifetime for a handle it already
    /// grants (REQ-EV-0288 "dynamic credential handles"): a task that
    /// outlives one short lifetime is renewed rather than given a standing
    /// secret. Returns false when the sandbox has no grant under `handle`.
    pub fn renew(&self, handle: &str, secret: String, expires_at_ms: i64) -> bool {
        let mut policy = self.policy.lock().unwrap_or_else(|e| e.into_inner());
        let Some(grant) = policy.credentials.iter_mut().find(|c| c.handle == handle) else {
            return false;
        };
        grant.expires_at_ms = expires_at_ms;
        self.secrets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(handle.to_owned(), secret);
        true
    }

    /// What the broker will do for a virtual host: inject, refuse with a
    /// reason, or nothing (the host is not credentialed at all). An expired
    /// grant drops its secret here and now — it stops existing, not merely
    /// stops being used (REQ-EV-0288).
    fn live_credential(&self, host: &str) -> CredentialLookup {
        let grant = {
            let policy = self.policy.lock().unwrap_or_else(|e| e.into_inner());
            match policy.credential_for(host) {
                Some(g) => Box::new(g.clone()),
                None => return CredentialLookup::NotCredentialed,
            }
        };
        if !grant.live_at(now_ms()) {
            self.secrets
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&grant.handle);
            return CredentialLookup::Refused(
                grant,
                reason::CREDENTIAL_EXPIRED,
                "the credential handle has expired; the secret is dropped and a fresh one must be granted",
            );
        }
        let secret = self
            .secrets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&grant.handle)
            .cloned();
        match secret {
            Some(secret) => CredentialLookup::Ready(grant, secret),
            None => CredentialLookup::Refused(
                grant,
                reason::CREDENTIAL_MISSING,
                "no secret is held under the handle",
            ),
        }
    }

    /// Serve every channel the guest opens, until the receiver ends.
    pub async fn serve(self: Arc<Self>, mut incoming: tokio::sync::mpsc::Receiver<Channel>) {
        while let Some(ch) = incoming.recv().await {
            let me = Arc::clone(&self);
            tokio::spawn(async move {
                if let Err(e) = me.handle(ch).await {
                    eprintln!("egress broker [{}]: {e}", me.sandbox_id);
                }
            });
        }
    }

    fn record(
        &self,
        kind: &str,
        destination: &str,
        allowed: bool,
        capability: &str,
        reason: &str,
        detail: &str,
    ) {
        self.audit.record(
            &self.sandbox_id,
            EgressRecord {
                kind: kind.into(),
                destination: destination.into(),
                allowed,
                capability: capability.into(),
                detail: detail.into(),
                reason: reason.into(),
                at_ms: now_ms(),
            },
        );
    }

    /// What the policy says about `host:port`: the rule that admits it, or
    /// why nothing does. The organisation's list is applied here, after the
    /// rules, so a rule can never admit what the list does not name.
    fn decide(&self, host: &str, port: u16) -> Decision {
        let policy = self.policy.lock().unwrap_or_else(|e| e.into_inner());
        let Some(rule) = policy.admits(host, port) else {
            return Decision::NoRule;
        };
        match policy.org_refusal(host, port) {
            Some(org) => Decision::Org(org.note.clone()),
            None => Decision::Admit {
                capability: rule.capability.clone(),
                literal: rule.host.trim().parse::<std::net::IpAddr>().is_ok(),
            },
        }
    }

    /// Whether the organisation's list refuses the host a credential grant
    /// forwards to.
    fn org_refuses_target(&self, target_url: &str) -> Option<String> {
        let (scheme, rest) = target_url
            .split_once("://")
            .unwrap_or(("https", target_url));
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        let authority = authority.rsplit('@').next().unwrap_or_default();
        let (host, port) = split_host_port(authority, if scheme == "http" { 80 } else { 443 });
        self.policy
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .org_refusal(&host, port)
            .map(|o| o.note.clone())
    }

    /// One channel.
    pub async fn handle(&self, ch: Channel) -> std::io::Result<()> {
        let mut r = BufReader::new(ch);
        let mut line = String::new();
        tokio::time::timeout(std::time::Duration::from_secs(10), r.read_line(&mut line))
            .await
            .map_err(|_| std::io::Error::other("no request line"))??;
        let line = line.trim().to_owned();
        let (verb, rest) = line.split_once(' ').unwrap_or((line.as_str(), ""));
        match verb {
            "TUNNEL" => {
                let (host, port) = split_host_port(rest, 443);
                self.plain("tunnel", rest, &host, port, r).await
            }
            "HTTP" => {
                let (host, port) = split_host_port(rest, 80);
                match self.live_credential(&host) {
                    CredentialLookup::Ready(grant, secret) => {
                        let mut inner = r.into_inner();
                        // The grant forwards to its target; the organisation's
                        // list caps that host like any other.
                        if let Some(note) = self.org_refuses_target(&grant.target_url) {
                            let detail = org_detail(&note);
                            self.record(
                                "credentialed",
                                &host,
                                false,
                                &grant.capability,
                                reason::ORG_ALLOW_LIST,
                                &detail,
                            );
                            return inner
                                .write_all(
                                    format!("DENIED {} {detail}\r\n", reason::ORG_ALLOW_LIST)
                                        .as_bytes(),
                                )
                                .await;
                        }
                        inner.write_all(b"OK\r\n").await?;
                        self.record(
                            "credentialed",
                            &host,
                            true,
                            &grant.capability,
                            "",
                            &format!("forwarded to {}", grant.target_url),
                        );
                        return forward_credentialed(inner, &grant, &secret).await;
                    }
                    CredentialLookup::Refused(grant, why, detail) => {
                        self.record("credentialed", &host, false, &grant.capability, why, detail);
                        let mut inner = r.into_inner();
                        return inner
                            .write_all(format!("DENIED {why} {detail}\r\n").as_bytes())
                            .await;
                    }
                    CredentialLookup::NotCredentialed => {}
                }
                self.plain("http", rest, &host, port, r).await
            }
            other => {
                self.record("bad", other, false, "", reason::BAD_REQUEST, "unknown verb");
                let mut inner = r.into_inner();
                inner
                    .write_all(b"DENIED BAD_REQUEST unknown verb\r\n")
                    .await
            }
        }
    }

    /// A tunnel or a plain HTTP destination: the policy decides, the
    /// decision is recorded, and an admitted destination is connected and
    /// relayed.
    async fn plain(
        &self,
        kind: &str,
        rest: &str,
        host: &str,
        port: u16,
        r: BufReader<Channel>,
    ) -> std::io::Result<()> {
        let mut inner = r.into_inner();
        match self.decide(host, port) {
            Decision::Admit {
                capability: cap,
                literal,
            } => match self.dial(host, port, literal).await {
                Ok(upstream) => {
                    self.record(kind, rest, true, &cap, "", "");
                    inner.write_all(b"OK\r\n").await?;
                    relay(inner, upstream).await
                }
                Err(DialError::NotPublic(addr)) => {
                    let detail = format!(
                        "{host} resolves to {}, which is not a public address; only a rule that names the IP address itself reaches it",
                        addr.ip()
                    );
                    self.record(kind, rest, false, "", reason::TARGET_NOT_PUBLIC, &detail);
                    inner
                        .write_all(
                            format!("DENIED {} {detail}\r\n", reason::TARGET_NOT_PUBLIC).as_bytes(),
                        )
                        .await
                }
                Err(DialError::Io(e)) => {
                    self.record(
                        kind,
                        rest,
                        true,
                        &cap,
                        reason::CONNECT_FAILED,
                        &format!("connect failed: {e}"),
                    );
                    inner
                        .write_all(
                            format!("DENIED {} connect failed: {e}\r\n", reason::CONNECT_FAILED)
                                .as_bytes(),
                        )
                        .await
                }
            },
            Decision::Org(note) => {
                let detail = org_detail(&note);
                self.record(kind, rest, false, "", reason::ORG_ALLOW_LIST, &detail);
                inner
                    .write_all(format!("DENIED {} {detail}\r\n", reason::ORG_ALLOW_LIST).as_bytes())
                    .await
            }
            Decision::NoRule => {
                let detail = "no egress rule admits this destination";
                self.record(kind, rest, false, "", reason::NO_EGRESS_RULE, detail);
                inner
                    .write_all(format!("DENIED {} {detail}\r\n", reason::NO_EGRESS_RULE).as_bytes())
                    .await
            }
        }
    }
}

fn split_host_port(s: &str, default_port: u16) -> (String, u16) {
    match s.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => (
            h.trim_matches(|c| c == '[' || c == ']').to_owned(),
            p.parse().unwrap_or(default_port),
        ),
        _ => (s.to_owned(), default_port),
    }
}

async fn relay(guest: Channel, upstream: tokio::net::TcpStream) -> std::io::Result<()> {
    let (mut gr, mut gw) = tokio::io::split(guest);
    let (mut ur, mut uw) = upstream.into_split();
    let a = async {
        let _ = tokio::io::copy(&mut gr, &mut uw).await;
        let _ = uw.shutdown().await;
    };
    let b = async {
        let _ = tokio::io::copy(&mut ur, &mut gw).await;
        let _ = gw.shutdown().await;
    };
    tokio::join!(a, b);
    Ok(())
}

/// Read one HTTP/1.1 request from the guest, perform it against the grant's
/// target with the secret injected, write the response back.
async fn forward_credentialed(
    mut guest: Channel,
    grant: &crate::policy::CredentialGrant,
    secret: &str,
) -> std::io::Result<()> {
    // Head.
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    let head_end = loop {
        if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        let n = guest.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        raw.extend_from_slice(&buf[..n]);
        if raw.len() > 64 * 1024 {
            guest
                .write_all(
                    b"HTTP/1.1 431 Request Header Fields Too Large\r\nContent-Length: 0\r\n\r\n",
                )
                .await?;
            return Ok(());
        }
    };
    let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_owned();
    let target = parts.next().unwrap_or("/").to_owned();
    // The guest's proxy sends an absolute URI; keep the path and query.
    let path = if let Some(rest) = target.strip_prefix("http://") {
        rest.find('/')
            .map(|i| rest[i..].to_owned())
            .unwrap_or_else(|| "/".into())
    } else {
        target.clone()
    };
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut content_length = 0usize;
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            let k = k.trim();
            let v = v.trim();
            if k.eq_ignore_ascii_case("content-length") {
                content_length = v.parse().unwrap_or(0);
            }
            if k.eq_ignore_ascii_case("host")
                || k.eq_ignore_ascii_case("proxy-connection")
                || k.eq_ignore_ascii_case("connection")
                || k.eq_ignore_ascii_case(&grant.header)
            {
                continue;
            }
            headers.push((k.to_owned(), v.to_owned()));
        }
    }
    let mut body = raw[head_end..].to_vec();
    while body.len() < content_length {
        let n = guest.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&buf[..n]);
    }
    let url = format!("{}{}", grant.target_url.trim_end_matches('/'), path);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(std::io::Error::other)?;
    let mut req = client.request(
        reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET),
        &url,
    );
    for (k, v) in &headers {
        req = req.header(k.as_str(), v.as_str());
    }
    req = req.header(
        grant.header.as_str(),
        format!("{}{secret}", grant.value_prefix),
    );
    if !body.is_empty() {
        req = req.body(body);
    }
    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            let mut out = format!(
                "HTTP/1.1 {} {}\r\n",
                status.as_u16(),
                status.canonical_reason().unwrap_or("")
            );
            for (k, v) in resp.headers() {
                let name = k.as_str();
                if name.eq_ignore_ascii_case("transfer-encoding")
                    || name.eq_ignore_ascii_case("content-length")
                    || name.eq_ignore_ascii_case("connection")
                {
                    continue;
                }
                if let Ok(v) = v.to_str() {
                    out.push_str(&format!("{name}: {v}\r\n"));
                }
            }
            let bytes = resp.bytes().await.unwrap_or_default();
            out.push_str(&format!(
                "Content-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            ));
            guest.write_all(out.as_bytes()).await?;
            guest.write_all(&bytes).await?;
            guest.shutdown().await
        }
        Err(e) => {
            let msg = format!("the broker could not reach the target: {e}");
            guest
                .write_all(format!("HTTP/1.1 502 Bad Gateway\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{msg}", msg.len()).as_bytes())
                .await?;
            guest.shutdown().await
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, SocketAddr};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use futures_util::future::BoxFuture;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::policy::EgressRule;

    fn ip(s: &str) -> IpAddr {
        s.parse().expect("address")
    }

    #[test]
    fn qual_px_147_the_address_classifier_refuses_every_non_public_range() {
        for refused in [
            "127.0.0.1",
            "127.255.255.254",
            "0.0.0.0",
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254", // cloud metadata
            "169.254.0.1",
            "100.64.0.1",
            "100.100.100.200", // a cloud metadata address in shared space
            "100.127.255.255",
            "192.0.0.8",
            "192.0.2.1",
            "198.18.0.1",
            "198.19.255.255",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "239.255.255.255",
            "240.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "fe80::1",
            "fc00::1",
            "fd00:ec2::254", // cloud metadata over IPv6
            "ff02::1",
            "2001:db8::1",
            "::ffff:127.0.0.1", // IPv4-mapped
            "::ffff:10.0.0.1",
            "::ffff:169.254.169.254",
            "64:ff9b::7f00:1", // NAT64 around 127.0.0.1
            "64:ff9b::a9fe:a9fe",
            "2002:7f00:1::1", // 6to4 around 127.0.0.1
            "100::1",
        ] {
            assert!(!is_public_address(ip(refused)), "{refused} must be refused");
        }
        for admitted in [
            "93.184.216.34",
            "8.8.8.8",
            "1.1.1.1",
            "172.15.255.255", // just outside 172.16/12
            "172.32.0.1",
            "100.63.255.255", // just outside 100.64/10
            "100.128.0.1",
            "198.17.255.255",
            "198.20.0.1",
            "2606:4700:4700::1111",
            "2a00:1450:4001:81b::200e",
            "::ffff:8.8.8.8",
            "64:ff9b::808:808",
        ] {
            assert!(is_public_address(ip(admitted)), "{admitted} must pass");
        }
    }

    /// A dialer whose answers and connections the test controls: it answers
    /// the n-th lookup with `answers[n]` (the last answer repeats), counts
    /// lookups and connections, records the addresses it was asked to
    /// connect to, and redirects every connection to `backend` (a real
    /// listener) so bytes can flow.
    struct Scripted {
        answers: Vec<Vec<SocketAddr>>,
        lookups: AtomicUsize,
        connects: Mutex<Vec<SocketAddr>>,
        backend: SocketAddr,
    }

    impl Scripted {
        fn new(answers: Vec<Vec<&str>>, port: u16, backend: SocketAddr) -> Arc<Self> {
            Arc::new(Self {
                answers: answers
                    .into_iter()
                    .map(|set| {
                        set.into_iter()
                            .map(|a| SocketAddr::new(ip(a), port))
                            .collect()
                    })
                    .collect(),
                lookups: AtomicUsize::new(0),
                connects: Mutex::new(Vec::new()),
                backend,
            })
        }
    }

    impl EgressDialer for Scripted {
        fn resolve<'a>(
            &'a self,
            _host: &'a str,
            _port: u16,
        ) -> BoxFuture<'a, std::io::Result<Vec<SocketAddr>>> {
            let n = self.lookups.fetch_add(1, Ordering::SeqCst);
            let answer = self.answers[n.min(self.answers.len() - 1)].clone();
            Box::pin(async move { Ok(answer) })
        }

        fn connect(
            &self,
            addr: SocketAddr,
        ) -> BoxFuture<'_, std::io::Result<tokio::net::TcpStream>> {
            self.connects.lock().expect("connects").push(addr);
            Box::pin(tokio::net::TcpStream::connect(self.backend))
        }
    }

    /// A real TCP listener that upper-cases what it reads (so a relay in
    /// both directions is observable) and counts accepted connections.
    async fn backend() -> (SocketAddr, Arc<AtomicUsize>) {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = l.local_addr().expect("addr");
        let accepted = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&accepted);
        tokio::spawn(async move {
            while let Ok((mut s, _)) = l.accept().await {
                seen.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let mut buf = [0u8; 256];
                    while let Ok(n) = s.read(&mut buf).await {
                        if n == 0 {
                            break;
                        }
                        let up: Vec<u8> = buf[..n].iter().map(u8::to_ascii_uppercase).collect();
                        if s.write_all(&up).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });
        (addr, accepted)
    }

    struct Rig {
        audit: Arc<MemoryAudit>,
        broker: Arc<EgressBroker>,
    }

    fn rig(rule_host: &str, port: u16, dialer: Arc<dyn EgressDialer>) -> Rig {
        let audit = Arc::new(MemoryAudit::default());
        let broker = EgressBroker::with_dialer(
            "sbx-px147",
            NetworkPolicy {
                egress: vec![EgressRule {
                    host: rule_host.into(),
                    port,
                    capability: "network.egress".into(),
                }],
                ..NetworkPolicy::default()
            },
            HashMap::new(),
            Arc::clone(&audit) as Arc<dyn EgressAudit>,
            dialer,
        );
        Rig { audit, broker }
    }

    /// One guest channel against the broker: send `request`, return the
    /// broker's answer line and the still-open guest end.
    async fn ask(broker: &Arc<EgressBroker>, request: &str) -> (String, tokio::io::DuplexStream) {
        let (mut guest, host_end) = tokio::io::duplex(8192);
        let b = Arc::clone(broker);
        tokio::spawn(async move {
            let _ = b.handle(Box::new(host_end)).await;
        });
        guest
            .write_all(format!("{request}\r\n").as_bytes())
            .await
            .expect("write");
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        while guest.read_exact(&mut byte).await.is_ok() {
            if byte[0] == b'\n' {
                break;
            }
            line.push(byte[0]);
        }
        (String::from_utf8_lossy(&line).trim().to_owned(), guest)
    }

    #[tokio::test]
    async fn qual_px_147_a_name_that_resolves_to_a_non_public_address_is_refused_and_audited() {
        let (backend_addr, accepted) = backend().await;
        for (label, answers) in [
            ("loopback", vec!["127.0.0.1"]),
            ("private 10/8", vec!["10.0.0.5"]),
            ("private 192.168/16", vec!["192.168.7.7"]),
            ("cloud metadata", vec!["169.254.169.254"]),
            ("ipv6 loopback", vec!["::1"]),
            ("ipv4-mapped loopback", vec!["::ffff:127.0.0.1"]),
            ("unique-local metadata", vec!["fd00:ec2::254"]),
            (
                "a public answer next to a private one",
                vec!["93.184.216.34", "10.0.0.5"],
            ),
        ] {
            for (verb, rule_host) in [("TUNNEL", "svc.example.test"), ("HTTP", "*.example.test")] {
                let dialer = Scripted::new(vec![answers.clone()], 443, backend_addr);
                let r = rig(rule_host, 443, Arc::clone(&dialer) as Arc<dyn EgressDialer>);
                let (reply, _guest) = ask(&r.broker, &format!("{verb} svc.example.test:443")).await;
                assert!(
                    reply.starts_with(&format!("DENIED {}", reason::TARGET_NOT_PUBLIC)),
                    "{label} via {verb}: {reply}"
                );
                assert!(
                    dialer.connects.lock().expect("connects").is_empty(),
                    "{label} via {verb}: nothing may be connected"
                );
                let records = r.audit.records("sbx-px147");
                assert_eq!(records.len(), 1, "{label} via {verb}");
                assert!(!records[0].allowed);
                assert_eq!(records[0].reason, reason::TARGET_NOT_PUBLIC);
                assert!(
                    records[0].detail.contains("not a public address"),
                    "{records:?}"
                );
            }
        }
        assert_eq!(
            accepted.load(Ordering::SeqCst),
            0,
            "the backend saw no connection"
        );
    }

    #[tokio::test]
    async fn qual_px_147_the_checked_address_is_the_one_connected_and_the_name_is_resolved_once() {
        // The DNS answer changes between the check and a would-be connect:
        // the first lookup is public, every later one is loopback. The broker
        // must look up once and connect to the address it checked.
        let (backend_addr, accepted) = backend().await;
        let dialer = Scripted::new(
            vec![vec!["93.184.216.34"], vec!["127.0.0.1"]],
            443,
            backend_addr,
        );
        let r = rig(
            "svc.example.test",
            443,
            Arc::clone(&dialer) as Arc<dyn EgressDialer>,
        );
        let (reply, mut guest) = ask(&r.broker, "TUNNEL svc.example.test:443").await;
        assert_eq!(reply, "OK");
        guest.write_all(b"hello broker").await.expect("write");
        let mut back = [0u8; 12];
        guest.read_exact(&mut back).await.expect("relayed back");
        assert_eq!(&back, b"HELLO BROKER", "bytes flow both ways");
        assert_eq!(
            dialer.lookups.load(Ordering::SeqCst),
            1,
            "resolved exactly once"
        );
        assert_eq!(
            *dialer.connects.lock().expect("connects"),
            vec![SocketAddr::new(ip("93.184.216.34"), 443)],
            "the connection went to the address that was checked"
        );
        assert_eq!(accepted.load(Ordering::SeqCst), 1);
        let records = r.audit.records("sbx-px147");
        assert_eq!(records.len(), 1);
        assert!(records[0].allowed, "{records:?}");
        assert_eq!(records[0].capability, "network.egress");
        assert_eq!(
            records[0].reason, "",
            "the receipt of an admitted destination is unchanged"
        );
        assert_eq!(records[0].kind, "tunnel");
    }

    #[tokio::test]
    async fn qual_px_147_a_rule_that_names_the_ip_literal_still_reaches_it() {
        // The existing suites reach fixture servers on 127.0.0.1 by a rule
        // that names that address; that is the one way past the check.
        let (backend_addr, _accepted) = backend().await;
        let port = backend_addr.port();
        let r = rig("127.0.0.1", port, Arc::new(SystemDialer));
        let (reply, mut guest) = ask(&r.broker, &format!("TUNNEL 127.0.0.1:{port}")).await;
        assert_eq!(reply, "OK");
        guest.write_all(b"ping").await.expect("write");
        let mut back = [0u8; 4];
        guest.read_exact(&mut back).await.expect("relayed back");
        assert_eq!(&back, b"PING");
        let records = r.audit.records("sbx-px147");
        assert!(
            records[0].allowed && records[0].reason.is_empty(),
            "{records:?}"
        );

        // A rule that names an IPv6 literal works the same way.
        let r6 = rig("::1", port, Arc::new(SystemDialer));
        let (reply6, _g) = ask(&r6.broker, &format!("TUNNEL [::1]:{port}")).await;
        // The backend listens on IPv4 only: the guard admits the literal and
        // the failure is a plain connect failure, never TARGET_NOT_PUBLIC.
        assert!(
            reply6.starts_with(&format!("DENIED {}", reason::CONNECT_FAILED)),
            "{reply6}"
        );
    }

    #[tokio::test]
    async fn qual_px_147_a_name_rule_does_not_reach_loopback_through_the_system_resolver() {
        // No fixture dialer: the real resolver answers `localhost` with a
        // loopback address and the broker refuses it.
        let (backend_addr, accepted) = backend().await;
        let port = backend_addr.port();
        let r = rig("localhost", port, Arc::new(SystemDialer));
        let (reply, _g) = ask(&r.broker, &format!("TUNNEL localhost:{port}")).await;
        assert!(
            reply.starts_with(&format!("DENIED {}", reason::TARGET_NOT_PUBLIC)),
            "{reply}"
        );
        assert_eq!(accepted.load(Ordering::SeqCst), 0);
    }
}
