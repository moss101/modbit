//! The credential broker (REQ-PX-130, docs/23 "Secrets"): the one interface
//! through which every credential the product uses — a provider key, an
//! external server's credential, a forge token, a credential a browser fills,
//! a cloud or webhook secret, an exporter's header — is *registered*,
//! *granted*, *used*, *rotated* and *revoked*.
//!
//! The model:
//!
//! * A **credential** is registered under a [`CredentialId`] with a
//!   [`SecretHandle`] naming where its value lives (an environment variable,
//!   memory the Core holds, or nowhere in this process for a credential a
//!   host fills from its own custody), a [`Kind`], and the audience it may be
//!   used for. The broker is the only owner of the registration.
//! * A **grant** ([`GrantHandle`]) is a short-lived, scoped permission to use
//!   one credential: for one principal (a task or run), one audience, one
//!   purpose, until an expiry, for at most a number of uses. A grant is an
//!   opaque id — it never carries a value, serialises without one, and means
//!   nothing outside this broker.
//! * A **use** ([`Use`]) presents a grant (or, for a consumer inside the Core
//!   with nothing to delegate, asks [`CredentialBroker::acquire`], which issues
//!   and redeems a single-use grant in one step, so there is one code path)
//!   and the broker checks, in order: the credential is registered and not
//!   revoked, the grant exists and is not revoked, it has not expired, the
//!   audience, principal and purpose are the ones it was issued for, the
//!   nonce has not been seen (replay), and the call cap is not spent. Only
//!   then is the value resolved — at that moment, from its source, so a
//!   rotation takes effect on the next use — and handed to the caller as a
//!   [`Secret`] that prints as `<redacted>` and is exposed only by an
//!   explicit `expose()` at the point of the effect.
//! * Every issue, use, refusal, revocation and rotation is counted and
//!   audited (bounded, in memory, without a value).
//!
//! The broker is memory-only by design: a restarted Core has no grants, which
//! is revocation by construction; a credential must be configured again.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::redact::Redactor;

/// Where a credential's value lives.
///
/// This is the one handle type (it was `modbit_providers::SecretHandle`): it
/// *locates* a value, it does not grant its use. Outside this crate it can
/// be constructed and passed to [`CredentialBroker::register`], and nothing
/// else — the value is read only inside the broker.
#[derive(Clone, PartialEq, Eq)]
pub enum SecretHandle {
    /// Read from the named environment variable of the Core process.
    Env(String),
    /// Held in memory by the Core (OS keychain integration hands it in).
    Inline(String),
    /// Another credential the broker holds: this one is the same value under
    /// another identity and audience (an extension's provider that uses a
    /// credential the person configured by handle). The value is never
    /// copied out to build it.
    Credential(CredentialId),
    /// No credential (local, unauthenticated endpoints), or a credential
    /// whose value lives in another process and is never here: the broker
    /// can authorise its use, it has nothing to hand out.
    None,
}

impl SecretHandle {
    /// The value, if the source has one now. The broker's own read.
    fn read(&self, st: &State, depth: u8) -> Option<String> {
        match self {
            Self::Env(name) => std::env::var(name).ok().filter(|v| !v.is_empty()),
            Self::Inline(v) => Some(v.clone()),
            Self::Credential(id) if depth < 4 => {
                let c = st.credentials.get(id)?;
                if c.revoked {
                    return None;
                }
                c.source.read(st, depth + 1)
            }
            Self::Credential(_) | Self::None => None,
        }
    }

    /// Whether this handle names a source that has a value.
    #[must_use]
    pub fn has_source(&self) -> bool {
        !matches!(self, Self::None)
    }
}

impl fmt::Debug for SecretHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Env(name) => write!(f, "SecretHandle::Env({name})"),
            Self::Inline(_) => write!(f, "SecretHandle::Inline(<redacted>)"),
            Self::Credential(id) => write!(f, "SecretHandle::Credential({id})"),
            Self::None => write!(f, "SecretHandle::None"),
        }
    }
}

// A handle is never serialised with its value: an inline value is written as
// a marker and cannot be read back.
impl Serialize for SecretHandle {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Env(name) => s.serialize_str(&format!("env:{name}")),
            Self::Inline(_) => s.serialize_str("inline:<redacted>"),
            Self::Credential(id) => s.serialize_str(&format!("credential:{id}")),
            Self::None => s.serialize_str("none"),
        }
    }
}

impl<'de> Deserialize<'de> for SecretHandle {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.split_once(':') {
            Some(("env", name)) if !name.is_empty() => Ok(Self::Env(name.to_owned())),
            Some(("credential", id)) if !id.is_empty() => {
                Ok(Self::Credential(CredentialId(id.to_owned())))
            }
            _ if s == "none" => Ok(Self::None),
            // An inline value does not survive serialisation.
            _ => Ok(Self::None),
        }
    }
}

/// What a credential is for. Each kind has a ceiling on how long a grant for
/// it may live.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A model provider's key.
    Provider,
    /// An external (MCP) server's credential.
    Mcp,
    /// A credential a browser host fills into a page.
    Browser,
    /// A forge token.
    Forge,
    /// A cloud / sandbox credential.
    Cloud,
    /// A webhook signing secret.
    Webhook,
    /// A telemetry exporter's header.
    Telemetry,
}

impl Kind {
    /// The wire name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Provider => "provider",
            Self::Mcp => "mcp",
            Self::Browser => "browser",
            Self::Forge => "forge",
            Self::Cloud => "cloud",
            Self::Webhook => "webhook",
            Self::Telemetry => "telemetry",
        }
    }

    /// The longest a grant for this kind lives.
    #[must_use]
    pub fn ttl_ceiling(self) -> Duration {
        match self {
            // A credential handed to a sandbox or a host expires on its own.
            Self::Cloud | Self::Browser => Duration::from_secs(15 * 60),
            Self::Forge => Duration::from_secs(15 * 60),
            // The Core's own uses are one request long; a delegated grant
            // for them is bounded all the same.
            _ => Duration::from_secs(60 * 60),
        }
    }
}

/// A credential's identity in the broker: `<kind>:<name>`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CredentialId(String);

impl CredentialId {
    /// `kind:name`, with anything outside `[A-Za-z0-9._:/@-]` in `name`
    /// replaced by `_` and the whole bounded.
    #[must_use]
    pub fn new(kind: Kind, name: &str) -> Self {
        let clean: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '/' | '@' | '-') {
                    c
                } else {
                    '_'
                }
            })
            .take(96)
            .collect();
        Self(format!("{}:{clean}", kind.name()))
    }

    /// The id as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CredentialId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A secret value, handed out for one use. It prints as `<redacted>`, has no
/// `Serialize`, and is read only with [`Secret::expose`] at the point of the
/// effect (a request header, a child's environment).
pub struct Secret(String);

impl Secret {
    /// The value. Call this where the effect happens and nowhere else.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// What a credential is registered with.
#[derive(Clone, Debug)]
pub struct Registration {
    /// Identity.
    pub id: CredentialId,
    /// What it is for.
    pub kind: Kind,
    /// Where its value lives.
    pub source: SecretHandle,
    /// The audience it may be used for: an exact name, or a prefix ending in
    /// `*` (`mcp:*`).
    pub audience: String,
}

/// A request for a grant.
#[derive(Clone, Debug)]
pub struct IssueRequest {
    /// The credential.
    pub credential: CredentialId,
    /// Who the grant is for: `task:<id>`, `run:<id>`, `core`.
    pub principal: String,
    /// The audience it is for (a provider endpoint, an origin, a host).
    pub audience: String,
    /// Why (`model.request`, `browser.fill`, `sandbox.credential`).
    pub purpose: String,
    /// How long it lives, clamped to the kind's ceiling.
    pub ttl: Duration,
    /// The most uses; `None` = unlimited within the TTL.
    pub max_uses: Option<u32>,
}

/// A grant: an opaque, scoped, short-lived permission. It carries no value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantHandle {
    /// The handle (`grant_` and 24 hex digits); meaningless outside this broker.
    pub id: String,
    /// The credential it is for.
    pub credential: CredentialId,
    /// The principal it is for.
    pub principal: String,
    /// The audience it is for.
    pub audience: String,
    /// The purpose it is for.
    pub purpose: String,
    /// When it expires (ms since the epoch).
    pub expires_at_ms: i64,
    /// The most uses, when capped.
    pub max_uses: Option<u32>,
}

/// One use of a grant.
#[derive(Clone, Debug, Default)]
pub struct Use {
    /// Who is using it.
    pub principal: String,
    /// The audience it is used for.
    pub audience: String,
    /// The purpose.
    pub purpose: String,
    /// A nonce the presenter chooses; a nonce seen before on the same grant
    /// is a replay. `None` for a caller inside this process (the broker then
    /// makes one up, so a use is still counted once).
    pub nonce: Option<String>,
}

/// Why a use or an issue was refused. The code is stable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// No such credential, or no such grant.
    Unknown,
    /// The credential, or the grant, was revoked.
    Revoked,
    /// The grant has expired.
    Expired,
    /// The audience is not the grant's (or the credential's).
    AudienceMismatch,
    /// The principal is not the grant's.
    PrincipalMismatch,
    /// The purpose is not the grant's.
    PurposeMismatch,
    /// The nonce was seen before.
    Replayed,
    /// The grant's uses are spent.
    CallCapExhausted,
    /// The credential's source has no value now (an unset variable).
    NotConfigured,
    /// The credential's value is not in this process: it can be authorised
    /// but not read.
    Delegated,
}

impl Refusal {
    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unknown => "CREDENTIAL_UNKNOWN",
            Self::Revoked => "CREDENTIAL_REVOKED",
            Self::Expired => "CREDENTIAL_EXPIRED",
            Self::AudienceMismatch => "CREDENTIAL_AUDIENCE_MISMATCH",
            Self::PrincipalMismatch => "CREDENTIAL_PRINCIPAL_MISMATCH",
            Self::PurposeMismatch => "CREDENTIAL_PURPOSE_MISMATCH",
            Self::Replayed => "CREDENTIAL_REPLAYED",
            Self::CallCapExhausted => "CREDENTIAL_CALL_CAP_EXHAUSTED",
            Self::NotConfigured => "CREDENTIAL_NOT_CONFIGURED",
            Self::Delegated => "CREDENTIAL_DELEGATED",
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for Refusal {}

/// One line of the audit: what happened, never a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditRecord {
    /// When (ms since the epoch).
    pub at_ms: i64,
    /// `REGISTER` | `ROTATE` | `ISSUE` | `USE` | `AUTHORIZE` | `REFUSED` |
    /// `REVOKE` | `FORGET`.
    pub action: &'static str,
    /// The credential.
    pub credential: String,
    /// The grant, when one was involved.
    pub grant: String,
    /// The principal.
    pub principal: String,
    /// The audience.
    pub audience: String,
    /// The purpose.
    pub purpose: String,
    /// `OK` or the refusal code.
    pub outcome: String,
}

/// What the broker reports about one credential.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialStatus {
    /// Identity.
    pub id: CredentialId,
    /// Kind.
    pub kind: Kind,
    /// Its audience.
    pub audience: String,
    /// Bumped by every rotation.
    pub generation: u64,
    /// Revoked now.
    pub revoked: bool,
    /// Whether its source has a value right now.
    pub configured: bool,
    /// Whether its value lives in another process.
    pub delegated: bool,
    /// Successful uses.
    pub uses: u64,
    /// Refused uses.
    pub refusals: u64,
    /// Last successful use (ms since the epoch; 0 = never).
    pub last_used_ms: i64,
    /// When it was registered or last rotated.
    pub registered_at_ms: i64,
}

/// What the broker reports about one grant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrantStatus {
    /// The handle.
    pub handle: GrantHandle,
    /// Uses so far.
    pub uses: u32,
    /// Revoked.
    pub revoked: bool,
    /// Expired at the broker's clock now.
    pub expired: bool,
}

/// Counters over the broker's life.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Grants issued.
    pub issued: u64,
    /// Successful uses (value released or authorisation given).
    pub used: u64,
    /// Refusals by code.
    pub refused: BTreeMap<String, u64>,
}

type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;
type Observer = Arc<dyn Fn(&AuditRecord) + Send + Sync>;

struct Stored {
    kind: Kind,
    source: SecretHandle,
    audience: String,
    generation: u64,
    revoked: bool,
    uses: u64,
    refusals: u64,
    last_used_ms: i64,
    registered_at_ms: i64,
}

struct Grant {
    handle: GrantHandle,
    uses: u32,
    revoked: bool,
    nonces: HashSet<String>,
}

#[derive(Default)]
struct State {
    credentials: HashMap<CredentialId, Stored>,
    grants: HashMap<String, Grant>,
    audit: VecDeque<AuditRecord>,
    stats: Stats,
}

/// How many audit lines are kept.
const AUDIT_CAP: usize = 1_024;
/// How many nonces a grant remembers.
const NONCE_CAP: usize = 256;
/// How many grants the broker holds; the oldest dead ones go first.
const GRANT_CAP: usize = 4_096;

/// The broker.
pub struct CredentialBroker {
    state: Mutex<State>,
    clock: Clock,
    observer: Mutex<Option<Observer>>,
}

impl Default for CredentialBroker {
    fn default() -> Self {
        Self::new()
    }
}

fn system_ms() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis()),
    )
    .unwrap_or(0)
}

fn random_hex(bytes: usize) -> String {
    hex::encode((0..bytes).map(|_| rand::random::<u8>()).collect::<Vec<_>>())
}

fn audience_matches(allowed: &str, audience: &str) -> bool {
    match allowed.strip_suffix('*') {
        Some(prefix) => audience.starts_with(prefix),
        None => allowed == audience,
    }
}

impl CredentialBroker {
    /// A broker on the system clock.
    #[must_use]
    pub fn new() -> Self {
        Self::with_clock(Arc::new(system_ms))
    }

    /// A broker on `clock` (tests move time).
    #[must_use]
    pub fn with_clock(clock: Clock) -> Self {
        Self {
            state: Mutex::new(State::default()),
            clock,
            observer: Mutex::new(None),
        }
    }

    /// Call `f` for every refusal, revocation and rotation (the Core records
    /// them as security events). It must not call back into the broker.
    pub fn observe(&self, f: Observer) {
        *self.observer.lock().unwrap_or_else(|e| e.into_inner()) = Some(f);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn now(&self) -> i64 {
        (self.clock)()
    }

    fn audit(&self, st: &mut State, rec: AuditRecord, notify: bool) {
        if st.audit.len() >= AUDIT_CAP {
            st.audit.pop_front();
        }
        st.audit.push_back(rec.clone());
        if notify
            && let Some(obs) = self
                .observer
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        {
            obs(&rec);
        }
    }

    fn refuse(&self, st: &mut State, r: Refusal, mut rec: AuditRecord) -> Refusal {
        rec.action = "REFUSED";
        rec.outcome = r.code().to_owned();
        *st.stats.refused.entry(r.code().to_owned()).or_default() += 1;
        if let Some(c) = st
            .credentials
            .get_mut(&CredentialId(rec.credential.clone()))
        {
            c.refusals += 1;
        }
        self.audit(st, rec, true);
        r
    }

    // ------------------------------------------------------- registration

    /// Register a credential, or **rotate** it when the id is already
    /// registered: the new source applies from the next use, existing grants
    /// stay valid (they were never tied to a value), the generation moves,
    /// and a revoked credential is reinstated by being configured again.
    pub fn register(&self, reg: Registration) {
        let now = self.now();
        let mut st = self.lock();
        let id = reg.id.to_string();
        let rotated = match st.credentials.get_mut(&reg.id) {
            Some(c) => {
                c.kind = reg.kind;
                c.source = reg.source;
                c.audience = reg.audience.clone();
                c.generation += 1;
                c.revoked = false;
                c.registered_at_ms = now;
                true
            }
            None => {
                st.credentials.insert(
                    reg.id.clone(),
                    Stored {
                        kind: reg.kind,
                        source: reg.source,
                        audience: reg.audience.clone(),
                        generation: 1,
                        revoked: false,
                        uses: 0,
                        refusals: 0,
                        last_used_ms: 0,
                        registered_at_ms: now,
                    },
                );
                false
            }
        };
        self.audit(
            &mut st,
            AuditRecord {
                at_ms: now,
                action: if rotated { "ROTATE" } else { "REGISTER" },
                credential: id,
                grant: String::new(),
                principal: String::new(),
                audience: reg.audience,
                purpose: String::new(),
                outcome: "OK".into(),
            },
            true,
        );
    }

    /// Forget a credential and every grant for it. Returns whether it was
    /// registered.
    pub fn forget(&self, id: &CredentialId) -> bool {
        let now = self.now();
        let mut st = self.lock();
        let existed = st.credentials.remove(id).is_some();
        st.grants.retain(|_, g| &g.handle.credential != id);
        if existed {
            self.audit(
                &mut st,
                AuditRecord {
                    at_ms: now,
                    action: "FORGET",
                    credential: id.to_string(),
                    grant: String::new(),
                    principal: String::new(),
                    audience: String::new(),
                    purpose: String::new(),
                    outcome: "OK".into(),
                },
                true,
            );
        }
        existed
    }

    /// Revoke a credential: every grant for it is dead and no new one is
    /// issued until it is configured again. Returns whether it was
    /// registered.
    pub fn revoke_credential(&self, id: &CredentialId, why: &str) -> bool {
        let now = self.now();
        let mut st = self.lock();
        let Some(c) = st.credentials.get_mut(id) else {
            return false;
        };
        c.revoked = true;
        self.audit(
            &mut st,
            AuditRecord {
                at_ms: now,
                action: "REVOKE",
                credential: id.to_string(),
                grant: String::new(),
                principal: String::new(),
                audience: String::new(),
                purpose: crate::redact::error_text(why),
                outcome: "OK".into(),
            },
            true,
        );
        true
    }

    /// Revoke one grant. Returns whether it existed.
    pub fn revoke_grant(&self, grant: &str) -> bool {
        let now = self.now();
        let mut st = self.lock();
        let Some(g) = st.grants.get_mut(grant) else {
            return false;
        };
        g.revoked = true;
        let rec = AuditRecord {
            at_ms: now,
            action: "REVOKE",
            credential: g.handle.credential.to_string(),
            grant: grant.to_owned(),
            principal: g.handle.principal.clone(),
            audience: g.handle.audience.clone(),
            purpose: g.handle.purpose.clone(),
            outcome: "OK".into(),
        };
        self.audit(&mut st, rec, true);
        true
    }

    /// Revoke every grant issued to `principal` (a run ended, an emergency
    /// stop). Returns how many were live.
    pub fn revoke_principal(&self, principal: &str) -> usize {
        let now = self.now();
        let mut st = self.lock();
        let mut n = 0;
        for g in st
            .grants
            .values_mut()
            .filter(|g| g.handle.principal == principal && !g.revoked)
        {
            g.revoked = true;
            n += 1;
        }
        if n > 0 {
            self.audit(
                &mut st,
                AuditRecord {
                    at_ms: now,
                    action: "REVOKE",
                    credential: String::new(),
                    grant: String::new(),
                    principal: principal.to_owned(),
                    audience: String::new(),
                    purpose: format!("{n} grant(s)"),
                    outcome: "OK".into(),
                },
                true,
            );
        }
        n
    }

    // ------------------------------------------------------------- grants

    /// Issue a grant.
    ///
    /// # Errors
    /// The credential is unknown or revoked, or the audience is not one it
    /// may be used for.
    pub fn issue(&self, req: &IssueRequest) -> Result<GrantHandle, Refusal> {
        let now = self.now();
        let mut st = self.lock();
        let base = AuditRecord {
            at_ms: now,
            action: "ISSUE",
            credential: req.credential.to_string(),
            grant: String::new(),
            principal: req.principal.clone(),
            audience: req.audience.clone(),
            purpose: req.purpose.clone(),
            outcome: "OK".into(),
        };
        let Some(c) = st.credentials.get(&req.credential) else {
            return Err(self.refuse(&mut st, Refusal::Unknown, base));
        };
        if c.revoked {
            return Err(self.refuse(&mut st, Refusal::Revoked, base));
        }
        if !audience_matches(&c.audience, &req.audience) {
            return Err(self.refuse(&mut st, Refusal::AudienceMismatch, base));
        }
        let ceiling = c.kind.ttl_ceiling();
        let ttl = req.ttl.min(ceiling);
        let handle = GrantHandle {
            id: format!("grant_{}", random_hex(12)),
            credential: req.credential.clone(),
            principal: req.principal.clone(),
            audience: req.audience.clone(),
            purpose: req.purpose.clone(),
            expires_at_ms: now.saturating_add(i64::try_from(ttl.as_millis()).unwrap_or(i64::MAX)),
            max_uses: req.max_uses,
        };
        if st.grants.len() >= GRANT_CAP {
            // Make room: dead grants first, then the oldest expiry.
            let dead: Vec<String> = st
                .grants
                .iter()
                .filter(|(_, g)| g.revoked || g.handle.expires_at_ms <= now)
                .map(|(k, _)| k.clone())
                .collect();
            for k in dead {
                st.grants.remove(&k);
            }
            if st.grants.len() >= GRANT_CAP
                && let Some(oldest) = st
                    .grants
                    .iter()
                    .min_by_key(|(_, g)| g.handle.expires_at_ms)
                    .map(|(k, _)| k.clone())
            {
                st.grants.remove(&oldest);
            }
        }
        st.grants.insert(
            handle.id.clone(),
            Grant {
                handle: handle.clone(),
                uses: 0,
                revoked: false,
                nonces: HashSet::new(),
            },
        );
        st.stats.issued += 1;
        let mut rec = base;
        rec.grant.clone_from(&handle.id);
        self.audit(&mut st, rec, false);
        Ok(handle)
    }

    /// Validate a presentation of `grant_id` and, on success, count the use.
    /// Returns the credential's source and generation for the caller to read.
    fn check(
        &self,
        st: &mut State,
        grant_id: &str,
        u: &Use,
        action: &'static str,
    ) -> Result<(SecretHandle, CredentialId), Refusal> {
        let now = self.now();
        let mut rec = AuditRecord {
            at_ms: now,
            action,
            credential: String::new(),
            grant: grant_id.to_owned(),
            principal: u.principal.clone(),
            audience: u.audience.clone(),
            purpose: u.purpose.clone(),
            outcome: "OK".into(),
        };
        let Some(g) = st.grants.get(grant_id) else {
            return Err(self.refuse(st, Refusal::Unknown, rec));
        };
        rec.credential = g.handle.credential.to_string();
        let credential = g.handle.credential.clone();
        let Some(c) = st.credentials.get(&credential) else {
            return Err(self.refuse(st, Refusal::Unknown, rec));
        };
        let g = &st.grants[grant_id];
        let verdict = if c.revoked || g.revoked {
            Some(Refusal::Revoked)
        } else if g.handle.expires_at_ms <= now {
            Some(Refusal::Expired)
        } else if g.handle.audience != u.audience {
            Some(Refusal::AudienceMismatch)
        } else if g.handle.principal != u.principal {
            Some(Refusal::PrincipalMismatch)
        } else if g.handle.purpose != u.purpose {
            Some(Refusal::PurposeMismatch)
        } else if u.nonce.as_ref().is_some_and(|n| g.nonces.contains(n)) {
            Some(Refusal::Replayed)
        } else if g.handle.max_uses.is_some_and(|m| g.uses >= m) {
            Some(Refusal::CallCapExhausted)
        } else {
            None
        };
        if let Some(r) = verdict {
            return Err(self.refuse(st, r, rec));
        }
        let source = c.source.clone();
        let g = st.grants.get_mut(grant_id).expect("checked above");
        g.uses += 1;
        if let Some(n) = &u.nonce {
            if g.nonces.len() >= NONCE_CAP {
                g.nonces.clear();
            }
            g.nonces.insert(n.clone());
        }
        if let Some(c) = st.credentials.get_mut(&credential) {
            c.uses += 1;
            c.last_used_ms = now;
        }
        st.stats.used += 1;
        self.audit(st, rec, false);
        Ok((source, credential))
    }

    /// Use a grant and read the credential's value.
    ///
    /// # Errors
    /// Any [`Refusal`]; [`Refusal::Delegated`] when the value is not in this
    /// process (use [`Self::authorize`]), [`Refusal::NotConfigured`] when its
    /// source has no value now.
    pub fn redeem(&self, grant: &str, u: &Use) -> Result<Secret, Refusal> {
        let mut st = self.lock();
        let (source, credential) = self.check(&mut st, grant, u, "USE")?;
        if !source.has_source() {
            // The use was counted; the value does not exist here.
            return Err(Refusal::Delegated);
        }
        match source.read(&st, 0) {
            Some(v) => Ok(Secret(v)),
            None => {
                let rec = AuditRecord {
                    at_ms: self.now(),
                    action: "USE",
                    credential: credential.to_string(),
                    grant: grant.to_owned(),
                    principal: u.principal.clone(),
                    audience: u.audience.clone(),
                    purpose: u.purpose.clone(),
                    outcome: String::new(),
                };
                Err(self.refuse(&mut st, Refusal::NotConfigured, rec))
            }
        }
    }

    /// Use a grant for something the credential's host does itself (a
    /// browser fill): the use is checked, counted and audited, and no value
    /// is read.
    ///
    /// # Errors
    /// Any [`Refusal`] except the two about reading a value.
    pub fn authorize(&self, grant: &str, u: &Use) -> Result<(), Refusal> {
        let mut st = self.lock();
        self.check(&mut st, grant, u, "AUTHORIZE").map(|_| ())
    }

    /// A single use of a credential by a consumer inside this process:
    /// issues a one-use grant (60 s) and redeems it, so there is one code
    /// path and one set of checks, and the use is counted and audited like
    /// any other.
    ///
    /// # Errors
    /// Any [`Refusal`].
    pub fn acquire(&self, credential: &CredentialId, u: &Use) -> Result<Secret, Refusal> {
        let g = self.issue(&IssueRequest {
            credential: credential.clone(),
            principal: u.principal.clone(),
            audience: u.audience.clone(),
            purpose: u.purpose.clone(),
            ttl: Duration::from_secs(60),
            max_uses: Some(1),
        })?;
        self.redeem(&g.id, u)
    }

    /// [`Self::acquire`] for a use whose value stays in another process.
    ///
    /// # Errors
    /// Any [`Refusal`] except the two about reading a value.
    pub fn acquire_authorization(&self, credential: &CredentialId, u: &Use) -> Result<(), Refusal> {
        let g = self.issue(&IssueRequest {
            credential: credential.clone(),
            principal: u.principal.clone(),
            audience: u.audience.clone(),
            purpose: u.purpose.clone(),
            ttl: Duration::from_secs(60),
            max_uses: Some(1),
        })?;
        self.authorize(&g.id, u)
    }

    // -------------------------------------------------------- inspection

    /// Whether `id` is registered, not revoked, and (when it has a source)
    /// its source has a value now. No use is counted.
    #[must_use]
    pub fn usable(&self, id: &CredentialId) -> bool {
        let st = self.lock();
        st.credentials.get(id).is_some_and(|c| {
            !c.revoked && (!c.source.has_source() || c.source.read(&st, 0).is_some())
        })
    }

    /// Whether `id` is registered with a source that has a value now.
    #[must_use]
    pub fn configured(&self, id: &CredentialId) -> bool {
        let st = self.lock();
        st.credentials
            .get(id)
            .is_some_and(|c| c.source.read(&st, 0).is_some())
    }

    /// Every credential, sorted by id.
    #[must_use]
    pub fn credentials(&self) -> Vec<CredentialStatus> {
        let st = self.lock();
        let mut out: Vec<CredentialStatus> = st
            .credentials
            .iter()
            .map(|(id, c)| CredentialStatus {
                id: id.clone(),
                kind: c.kind,
                audience: c.audience.clone(),
                generation: c.generation,
                revoked: c.revoked,
                configured: c.source.read(&st, 0).is_some(),
                delegated: !c.source.has_source(),
                uses: c.uses,
                refusals: c.refusals,
                last_used_ms: c.last_used_ms,
                registered_at_ms: c.registered_at_ms,
            })
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    /// Every grant the broker still holds.
    #[must_use]
    pub fn grants(&self) -> Vec<GrantStatus> {
        let now = self.now();
        let st = self.lock();
        let mut out: Vec<GrantStatus> = st
            .grants
            .values()
            .map(|g| GrantStatus {
                handle: g.handle.clone(),
                uses: g.uses,
                revoked: g.revoked,
                expired: g.handle.expires_at_ms <= now,
            })
            .collect();
        out.sort_by(|a, b| a.handle.id.cmp(&b.handle.id));
        out
    }

    /// The counters.
    #[must_use]
    pub fn stats(&self) -> Stats {
        self.lock().stats.clone()
    }

    /// The most recent audit lines (newest last), at most `limit`.
    #[must_use]
    pub fn audit_tail(&self, limit: usize) -> Vec<AuditRecord> {
        let st = self.lock();
        let skip = st.audit.len().saturating_sub(limit);
        st.audit.iter().skip(skip).cloned().collect()
    }

    /// The one redactor over everything this broker holds (REQ-EV-0017): a
    /// value in custody is replaced wherever it appears.
    #[must_use]
    pub fn redactor(&self) -> Redactor {
        Redactor::new(self.custody_for_screening())
    }

    /// The values in custody, for the two things that must compare text with
    /// them and nothing else: the redactor and the screen that refuses a tool
    /// call whose arguments carry one. They are compared, never recorded,
    /// logged or sent.
    #[must_use]
    pub fn custody_for_screening(&self) -> Vec<String> {
        let st = self.lock();
        let mut out: Vec<String> = st
            .credentials
            .values()
            .filter_map(|c| c.source.read(&st, 0))
            .filter(|v| v.len() >= crate::redact::MIN_HELD_LEN)
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};

    fn broker() -> (CredentialBroker, Arc<AtomicI64>) {
        let t = Arc::new(AtomicI64::new(1_000_000));
        let t2 = Arc::clone(&t);
        (
            CredentialBroker::with_clock(Arc::new(move || t2.load(Ordering::SeqCst))),
            t,
        )
    }

    fn cred(name: &str) -> CredentialId {
        CredentialId::new(Kind::Provider, name)
    }

    fn register(b: &CredentialBroker, name: &str, value: &str) -> CredentialId {
        let id = cred(name);
        b.register(Registration {
            id: id.clone(),
            kind: Kind::Provider,
            source: SecretHandle::Inline(value.into()),
            audience: format!("provider:{name}"),
        });
        id
    }

    fn use_(principal: &str, audience: &str) -> Use {
        Use {
            principal: principal.into(),
            audience: audience.into(),
            purpose: "model.request".into(),
            nonce: None,
        }
    }

    fn issue(
        b: &CredentialBroker,
        id: &CredentialId,
        ttl_ms: u64,
        cap: Option<u32>,
    ) -> GrantHandle {
        b.issue(&IssueRequest {
            credential: id.clone(),
            principal: "task:t1".into(),
            audience: "provider:openai".into(),
            purpose: "model.request".into(),
            ttl: Duration::from_millis(ttl_ms),
            max_uses: cap,
        })
        .unwrap()
    }

    #[test]
    fn a_grant_releases_the_value_only_to_its_principal_audience_and_purpose() {
        let (b, _) = broker();
        let id = register(&b, "openai", "sk-secret-value-0123456789");
        let g = issue(&b, &id, 60_000, None);
        let s = b
            .redeem(&g.id, &use_("task:t1", "provider:openai"))
            .unwrap();
        assert_eq!(s.expose(), "sk-secret-value-0123456789");
        assert_eq!(
            b.redeem(&g.id, &use_("task:other", "provider:openai"))
                .unwrap_err(),
            Refusal::PrincipalMismatch
        );
        assert_eq!(
            b.redeem(&g.id, &use_("task:t1", "provider:evil"))
                .unwrap_err(),
            Refusal::AudienceMismatch
        );
        let mut wrong_purpose = use_("task:t1", "provider:openai");
        wrong_purpose.purpose = "exfiltrate".into();
        assert_eq!(
            b.redeem(&g.id, &wrong_purpose).unwrap_err(),
            Refusal::PurposeMismatch
        );
        assert_eq!(b.stats().used, 1);
        assert_eq!(b.stats().refused["CREDENTIAL_AUDIENCE_MISMATCH"], 1);
        // A grant cannot be issued for an audience the credential is not for.
        let bad = b.issue(&IssueRequest {
            credential: id,
            principal: "task:t1".into(),
            audience: "provider:evil".into(),
            purpose: "model.request".into(),
            ttl: Duration::from_secs(1),
            max_uses: None,
        });
        assert_eq!(bad.unwrap_err(), Refusal::AudienceMismatch);
    }

    #[test]
    fn a_grant_expires_and_the_ttl_is_clamped_to_the_kinds_ceiling() {
        let (b, t) = broker();
        let id = register(&b, "openai", "sk-secret-value-0123456789");
        let g = issue(&b, &id, 5_000, None);
        t.fetch_add(4_999, Ordering::SeqCst);
        assert!(b.redeem(&g.id, &use_("task:t1", "provider:openai")).is_ok());
        t.fetch_add(1, Ordering::SeqCst);
        assert_eq!(
            b.redeem(&g.id, &use_("task:t1", "provider:openai"))
                .unwrap_err(),
            Refusal::Expired
        );
        // A forge token cannot be granted for a day.
        let forge = CredentialId::new(Kind::Forge, "github");
        b.register(Registration {
            id: forge.clone(),
            kind: Kind::Forge,
            source: SecretHandle::Inline("ghp_0123456789abcdefghij".into()),
            audience: "forge:*".into(),
        });
        let long = b
            .issue(&IssueRequest {
                credential: forge,
                principal: "core".into(),
                audience: "forge:github".into(),
                purpose: "p".into(),
                ttl: Duration::from_secs(86_400),
                max_uses: None,
            })
            .unwrap();
        let life = long.expires_at_ms - b.now();
        assert_eq!(
            life,
            i64::try_from(Kind::Forge.ttl_ceiling().as_millis()).unwrap()
        );
    }

    #[test]
    fn revocation_of_a_grant_a_principal_or_a_credential_is_immediate() {
        let (b, _) = broker();
        let id = register(&b, "openai", "sk-secret-value-0123456789");
        let u = use_("task:t1", "provider:openai");
        let g1 = issue(&b, &id, 60_000, None);
        assert!(b.revoke_grant(&g1.id));
        assert_eq!(b.redeem(&g1.id, &u).unwrap_err(), Refusal::Revoked);
        let g2 = issue(&b, &id, 60_000, None);
        assert_eq!(b.revoke_principal("task:t1"), 1);
        assert_eq!(b.redeem(&g2.id, &u).unwrap_err(), Refusal::Revoked);
        let g3 = issue(&b, &id, 60_000, None);
        assert!(b.revoke_credential(&id, "operator"));
        assert_eq!(b.redeem(&g3.id, &u).unwrap_err(), Refusal::Revoked);
        assert_eq!(b.acquire(&id, &u).unwrap_err(), Refusal::Revoked);
        assert!(!b.usable(&id));
        // Configuring it again reinstates it (a rotation).
        let id = register(&b, "openai", "sk-another-value-9876543210");
        assert!(b.usable(&id));
        assert_eq!(
            b.acquire(&id, &u).unwrap().expose(),
            "sk-another-value-9876543210"
        );
    }

    #[test]
    fn a_nonce_is_good_once_and_the_call_cap_is_a_cap() {
        let (b, _) = broker();
        let id = register(&b, "openai", "sk-secret-value-0123456789");
        let g = issue(&b, &id, 60_000, Some(2));
        let mut u = use_("task:t1", "provider:openai");
        u.nonce = Some("n-1".into());
        assert!(b.redeem(&g.id, &u).is_ok());
        assert_eq!(b.redeem(&g.id, &u).unwrap_err(), Refusal::Replayed);
        u.nonce = Some("n-2".into());
        assert!(b.redeem(&g.id, &u).is_ok());
        u.nonce = Some("n-3".into());
        assert_eq!(b.redeem(&g.id, &u).unwrap_err(), Refusal::CallCapExhausted);
        assert_eq!(b.grants()[0].uses, 2, "refusals are not uses");
    }

    #[test]
    fn rotation_takes_effect_on_the_next_use_without_a_new_grant() {
        let (b, _) = broker();
        let id = register(&b, "openai", "sk-first-value-0123456789");
        let g = issue(&b, &id, 60_000, None);
        let u = use_("task:t1", "provider:openai");
        assert_eq!(
            b.redeem(&g.id, &u).unwrap().expose(),
            "sk-first-value-0123456789"
        );
        let id = register(&b, "openai", "sk-second-value-0123456789");
        assert_eq!(
            b.redeem(&g.id, &u).unwrap().expose(),
            "sk-second-value-0123456789"
        );
        assert_eq!(b.credentials()[0].generation, 2);
        assert_eq!(b.credentials()[0].uses, 2);
        assert!(
            !b.custody_for_screening()
                .iter()
                .any(|v| v.contains("first"))
        );
        drop(id);
    }

    #[test]
    fn a_counter_at_the_interface_equals_the_number_of_uses() {
        let (b, _) = broker();
        let id = register(&b, "openai", "sk-secret-value-0123456789");
        let u = use_("task:t1", "provider:openai");
        for _ in 0..5 {
            b.acquire(&id, &u).unwrap();
        }
        let _ = b.acquire(&id, &use_("task:t1", "provider:evil"));
        let c = &b.credentials()[0];
        assert_eq!(c.uses, 5);
        assert_eq!(c.refusals, 1);
        assert_eq!(b.stats().used, 5);
        assert_eq!(b.stats().issued, 5, "the refused acquire issued nothing");
    }

    #[test]
    fn a_delegated_credential_is_authorised_and_counted_and_its_value_never_read() {
        let (b, _) = broker();
        let id = CredentialId::new(Kind::Browser, "cred_1");
        b.register(Registration {
            id: id.clone(),
            kind: Kind::Browser,
            source: SecretHandle::None,
            audience: "https://bank.example".into(),
        });
        let u = Use {
            principal: "task:t1".into(),
            audience: "https://bank.example".into(),
            purpose: "browser.fill".into(),
            nonce: None,
        };
        assert!(b.acquire_authorization(&id, &u).is_ok());
        assert_eq!(b.acquire(&id, &u).unwrap_err(), Refusal::Delegated);
        let wrong = Use {
            audience: "https://evil.example".into(),
            ..u.clone()
        };
        assert_eq!(
            b.acquire_authorization(&id, &wrong).unwrap_err(),
            Refusal::AudienceMismatch
        );
        assert!(b.credentials()[0].delegated);
    }

    #[test]
    fn nothing_prints_or_serialises_a_value() {
        let (b, _) = broker();
        let id = register(&b, "openai", "sk-secret-value-0123456789");
        let g = issue(&b, &id, 60_000, None);
        let s = b
            .redeem(&g.id, &use_("task:t1", "provider:openai"))
            .unwrap();
        let shown = format!(
            "{s:?} {s} {:?} {:?} {} {:?} {:?}",
            g,
            SecretHandle::Inline("sk-secret-value-0123456789".into()),
            serde_json::to_string(&g).unwrap(),
            serde_json::to_string(&SecretHandle::Inline("sk-secret-value-0123456789".into()))
                .unwrap(),
            b.audit_tail(100),
        );
        assert!(!shown.contains("sk-secret-value"), "{shown}");
        assert!(!format!("{:?}", b.credentials()).contains("sk-secret-value"));
        // The one redactor knows the value and removes it from text.
        let red = b.redactor();
        assert!(
            !red.error_text("failed with sk-secret-value-0123456789 here")
                .contains("sk-secret")
        );
    }

    #[test]
    fn an_unset_environment_source_is_not_configured_and_a_set_one_is_read_at_use() {
        let (b, _) = broker();
        let id = CredentialId::new(Kind::Provider, "envy");
        b.register(Registration {
            id: id.clone(),
            kind: Kind::Provider,
            source: SecretHandle::Env("MODBIT_BROKER_TEST_UNSET_VARIABLE".into()),
            audience: "provider:envy".into(),
        });
        assert!(!b.usable(&id) && !b.configured(&id));
        assert_eq!(
            b.acquire(&id, &use_("core", "provider:envy")).unwrap_err(),
            Refusal::NotConfigured
        );
    }

    #[test]
    fn the_observer_hears_refusals_rotations_and_revocations_but_never_a_value() {
        let (b, _) = broker();
        let heard: Arc<Mutex<Vec<AuditRecord>>> = Arc::default();
        let h2 = Arc::clone(&heard);
        b.observe(Arc::new(move |r| h2.lock().unwrap().push(r.clone())));
        let id = register(&b, "openai", "sk-secret-value-0123456789");
        let _ = b.acquire(&id, &use_("task:t1", "provider:evil"));
        b.revoke_credential(&id, "operator said sk-secret-value-0123456789");
        let heard = heard.lock().unwrap();
        let actions: Vec<&str> = heard.iter().map(|r| r.action).collect();
        assert_eq!(actions, vec!["REGISTER", "REFUSED", "REVOKE"]);
        assert!(!format!("{heard:?}").contains("sk-secret-value-0123456789"));
    }

    #[test]
    fn credential_ids_are_bounded_and_clean() {
        let id = CredentialId::new(Kind::Mcp, "my server/with space\n");
        assert_eq!(id.as_str(), "mcp:my_server/with_space_");
        assert!(
            CredentialId::new(Kind::Mcp, &"x".repeat(500))
                .as_str()
                .len()
                <= 100
        );
    }
}
