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
    /// ms since the epoch.
    pub at_ms: i64,
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

/// The broker for one sandbox.
pub struct EgressBroker {
    sandbox_id: String,
    policy: NetworkPolicy,
    /// Secrets by handle — memory only.
    secrets: HashMap<String, String>,
    audit: Arc<dyn EgressAudit>,
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
        Arc::new(Self {
            sandbox_id: sandbox_id.to_owned(),
            policy,
            secrets,
            audit,
        })
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

    fn record(&self, kind: &str, destination: &str, allowed: bool, capability: &str, detail: &str) {
        self.audit.record(
            &self.sandbox_id,
            EgressRecord {
                kind: kind.into(),
                destination: destination.into(),
                allowed,
                capability: capability.into(),
                detail: detail.into(),
                at_ms: now_ms(),
            },
        );
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
                match self.policy.admits(&host, port) {
                    Some(rule) => {
                        let cap = rule.capability.clone();
                        match tokio::net::TcpStream::connect((host.as_str(), port)).await {
                            Ok(upstream) => {
                                self.record("tunnel", rest, true, &cap, "");
                                let mut inner = r.into_inner();
                                inner.write_all(b"OK\r\n").await?;
                                relay(inner, upstream).await
                            }
                            Err(e) => {
                                self.record(
                                    "tunnel",
                                    rest,
                                    true,
                                    &cap,
                                    &format!("connect failed: {e}"),
                                );
                                let mut inner = r.into_inner();
                                inner
                                    .write_all(format!("DENIED connect failed: {e}\r\n").as_bytes())
                                    .await
                            }
                        }
                    }
                    None => {
                        self.record(
                            "tunnel",
                            rest,
                            false,
                            "",
                            "no egress rule admits this destination",
                        );
                        let mut inner = r.into_inner();
                        inner
                            .write_all(b"DENIED no egress rule admits this destination\r\n")
                            .await
                    }
                }
            }
            "HTTP" => {
                let (host, port) = split_host_port(rest, 80);
                if let Some(grant) = self.policy.credential_for(&host) {
                    let Some(secret) = self.secrets.get(&grant.handle) else {
                        self.record(
                            "credentialed",
                            &host,
                            false,
                            &grant.capability,
                            "no secret is held under the handle",
                        );
                        let mut inner = r.into_inner();
                        return inner
                            .write_all(b"DENIED no secret is held under the handle\r\n")
                            .await;
                    };
                    let mut inner = r.into_inner();
                    inner.write_all(b"OK\r\n").await?;
                    let grant = grant.clone();
                    let secret = secret.clone();
                    self.record(
                        "credentialed",
                        &host,
                        true,
                        &grant.capability,
                        &format!("forwarded to {}", grant.target_url),
                    );
                    forward_credentialed(inner, &grant, &secret).await
                } else {
                    match self.policy.admits(&host, port) {
                        Some(rule) => {
                            let cap = rule.capability.clone();
                            match tokio::net::TcpStream::connect((host.as_str(), port)).await {
                                Ok(upstream) => {
                                    self.record("http", rest, true, &cap, "");
                                    let mut inner = r.into_inner();
                                    inner.write_all(b"OK\r\n").await?;
                                    relay(inner, upstream).await
                                }
                                Err(e) => {
                                    self.record(
                                        "http",
                                        rest,
                                        true,
                                        &cap,
                                        &format!("connect failed: {e}"),
                                    );
                                    let mut inner = r.into_inner();
                                    inner
                                        .write_all(
                                            format!("DENIED connect failed: {e}\r\n").as_bytes(),
                                        )
                                        .await
                                }
                            }
                        }
                        None => {
                            self.record(
                                "http",
                                rest,
                                false,
                                "",
                                "no egress rule admits this destination",
                            );
                            let mut inner = r.into_inner();
                            inner
                                .write_all(b"DENIED no egress rule admits this destination\r\n")
                                .await
                        }
                    }
                }
            }
            other => {
                self.record("bad", other, false, "", "unknown verb");
                let mut inner = r.into_inner();
                inner.write_all(b"DENIED unknown verb\r\n").await
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
