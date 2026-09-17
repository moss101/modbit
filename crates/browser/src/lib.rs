//! `modbit-browser` — semantic browser protocol and control leases (docs/22,
//! M7.1). The Core never runs Chromium: a *browser host* — Electron main,
//! holding a sandboxed `WebContentsView` in a dedicated partition with Node
//! disabled and strict context isolation — attaches to a browser session
//! over the authenticated surface socket (REQ-EV-0110) and answers the
//! Core's typed [`HostRequest`]s through the Chrome DevTools Protocol. The
//! Core owns the session's identity, its control lease and its recorded
//! state; the host owns the live `webContents` the user sees, the same one
//! the agent acts on. Page content that comes back is untrusted evidence
//! (docs/22 "Prompt-injection isolation"): it is tagged as such by the
//! consumer, never read as instruction.
//!
//! Canonical owner: browser (`docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`,
//! `docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`).
//! Dependency direction is enforced by `tools/architecture-lint`.

pub mod compiler;
pub mod injection;

use std::fmt;
use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A browser session: one live Chromium session a host holds for a task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BrowserSessionId(pub uuid::Uuid);

impl BrowserSessionId {
    /// A fresh id.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }

    /// From the 16 wire bytes.
    #[must_use]
    pub fn from_bytes(b: [u8; 16]) -> Self {
        Self(uuid::Uuid::from_bytes(b))
    }

    /// The 16 wire bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
}

impl Default for BrowserSessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for BrowserSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.simple())
    }
}

/// Who may send input to the session (docs/22 "Live user takeover"): the
/// agent by default; the user the moment they take control. Observation is
/// never blocked. Every hand-over increments the generation so an input
/// carrying a stale generation is fenced, not applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Controller {
    /// The agent's actions are applied.
    Agent,
    /// The person at the keyboard; agent input is refused.
    User,
}

/// The control lease of a session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlLease {
    /// Holder.
    pub controller: Controller,
    /// Fencing generation, incremented on every hand-over.
    pub generation: u64,
}

impl ControlLease {
    /// The agent holds a fresh session.
    #[must_use]
    pub fn initial() -> Self {
        Self {
            controller: Controller::Agent,
            generation: 1,
        }
    }

    /// Hand control to `to`; a no-op when already held.
    #[must_use]
    pub fn handed_to(self, to: Controller) -> Self {
        if self.controller == to {
            self
        } else {
            Self {
                controller: to,
                generation: self.generation + 1,
            }
        }
    }

    /// Whether an agent input stamped `generation` may be applied now.
    #[must_use]
    pub fn admits_agent_input(&self, generation: u64) -> bool {
        self.controller == Controller::Agent && generation == self.generation
    }
}

/// What the Core asks a host to do (docs/22 "Local browser": the host is
/// the CDP bridge). Every request names the session; the host refuses a
/// session it does not hold.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HostRequest {
    /// Load `url` in the session's top frame and wait for the load to settle.
    Navigate {
        /// Absolute http(s) URL.
        url: String,
    },
    /// The page's current state: URL, title, readiness, fingerprint.
    State,
    /// The accessibility tree of the top frame, bounded.
    Snapshot {
        /// At most this many nodes (the host truncates and says so).
        max_nodes: u32,
    },
    /// A PNG of a region (targeted; a full page only as diagnostic evidence).
    Capture {
        /// Region in CSS pixels; `None` = the viewport.
        clip: Option<Clip>,
    },
    /// Act on one element the compiler resolved at this version (M7.4):
    /// the host targets the DOM node, performs the action as a person
    /// would (a real click at the element's box, real key events, text
    /// insertion), waits for the page to settle and reports the state.
    Act {
        /// The DOM node behind the entity at the version it was resolved at.
        backend_dom_node_id: i64,
        /// `click` | `fill` | `select` | `check` | `uncheck` | `press`.
        action: String,
        /// The text to fill or the option to select.
        #[serde(default)]
        value: String,
        /// The key to press (`Enter`, `Tab`, `Escape`, `ArrowDown`, …).
        #[serde(default)]
        key: String,
        /// For a click on a visual region (M7.5): where inside the element's
        /// box, in CSS pixels from its top-left; `None` = its centre.
        #[serde(default)]
        at: Option<(u32, u32)>,
        /// For `fill_credential` (M7.8, docs/22 "Credentials"): the handle
        /// of the credential the host fills from its own custody. The
        /// value never crosses this protocol in either direction.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        credential_handle: Option<String>,
    },
    /// Whether the session's document can reach Node or Electron
    /// privileges (must be false; a host answers from the page itself).
    Isolation,
    /// Release the session's view.
    Close,
}

/// A rectangle in CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clip {
    /// Left.
    pub x: u32,
    /// Top.
    pub y: u32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// The page's state as the host reports it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageState {
    /// Current URL.
    pub url: String,
    /// Document title (untrusted).
    pub title: String,
    /// Whether the document finished loading.
    pub ready: bool,
    /// Monotonic state version the host maintains (bumped on navigation and
    /// on DOM mutation it observes); references are scoped to it (M7.2).
    pub state_version: u64,
}

impl PageState {
    /// A fingerprint of the observable state: URL, title and version — what
    /// a postcondition compares (docs/22 "Verification").
    #[must_use]
    pub fn fingerprint(&self) -> String {
        let mut h = Sha256::new();
        h.update(self.url.as_bytes());
        h.update([0]);
        h.update(self.title.as_bytes());
        h.update([0]);
        h.update(self.state_version.to_le_bytes());
        hex::encode(h.finalize())
    }
}

/// The host's answer to a [`HostRequest`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HostResponse {
    /// State (for `Navigate`, `State` and `Close`).
    State {
        /// The page.
        state: PageState,
    },
    /// Snapshot.
    Snapshot {
        /// The page the tree belongs to.
        state: PageState,
        /// Nodes in document order (raw; the compiler derives entities).
        nodes: Vec<compiler::RawAxNode>,
        /// Whether `max_nodes` cut the tree.
        truncated: bool,
    },
    /// Capture.
    Capture {
        /// The page.
        state: PageState,
        /// PNG bytes, base64.
        png_base64: String,
        /// The region captured.
        clip: Option<Clip>,
    },
    /// The outcome of an `Act`.
    Acted {
        /// The page after the action settled.
        state: PageState,
        /// Whether the action started a navigation the host saw settle.
        navigated: bool,
        /// What the host actually did (`click at 120,44`, `inserted 9 chars`, …).
        detail: String,
    },
    /// Isolation report from inside the document.
    Isolation {
        /// `typeof process`, `typeof require`, `typeof window.electron` … all `undefined`.
        node_reachable: bool,
        /// The session partition the host put the view in.
        partition: String,
        /// Whether the host's view runs with the renderer sandbox.
        sandboxed: bool,
        /// Whether context isolation is on.
        context_isolated: bool,
    },
    /// The host could not do it.
    Error {
        /// `NO_SUCH_SESSION`, `NAVIGATION_BLOCKED`, `CDP`, `TIMEOUT`, `USER_HAS_CONTROL`, …
        code: String,
        /// Detail (untrusted when it quotes the page).
        message: String,
    },
}

/// Why a request could not be delivered to a host (before any answer).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PortError {
    /// No host is attached to the session.
    NoHost,
    /// The host did not answer within the deadline.
    Timeout,
    /// The host connection closed while the request was pending.
    HostGone,
    /// The Core refused the request itself (a fenced generation, a bad URL).
    Refused {
        /// Code.
        code: String,
        /// Message.
        message: String,
    },
}

impl fmt::Display for PortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoHost => write!(f, "NO_HOST: no browser host is attached to the session"),
            Self::Timeout => write!(f, "TIMEOUT: the browser host did not answer in time"),
            Self::HostGone => write!(f, "HOST_GONE: the browser host disconnected"),
            Self::Refused { code, message } => write!(f, "{code}: {message}"),
        }
    }
}

/// A boxed future (the port is async and object-safe).
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The Core-side port to a session's host: request in, answer out.
pub trait BrowserPort: Send + Sync {
    /// Send `request` to the host of `session` and wait for its answer.
    fn request<'a>(
        &'a self,
        session: BrowserSessionId,
        request: HostRequest,
    ) -> BoxFuture<'a, Result<HostResponse, PortError>>;

    /// The control lease of `session` as the Core records it.
    fn lease<'a>(&'a self, session: BrowserSessionId) -> BoxFuture<'a, Option<ControlLease>>;

    /// The open browser session of `task`, when it has one.
    fn session_for<'a>(
        &'a self,
        task: modbit_domain::ids::TaskId,
    ) -> BoxFuture<'a, Option<BrowserSessionId>>;

    /// Keep the entities a page compiled to for `session` (a reference the
    /// agent holds is looked up here when it no longer resolves, to name
    /// the look-alikes that remain).
    fn remember_page<'a>(
        &'a self,
        session: BrowserSessionId,
        page: compiler::PageEntities,
    ) -> BoxFuture<'a, ()>;

    /// The entity a reference last compiled to in `session`, if any.
    fn known_entity<'a>(
        &'a self,
        session: BrowserSessionId,
        reference: &'a str,
    ) -> BoxFuture<'a, Option<compiler::Entity>>;

    /// The page last compiled for `session` (what a delta starts from).
    fn last_page<'a>(
        &'a self,
        session: BrowserSessionId,
    ) -> BoxFuture<'a, Option<compiler::PageEntities>>;

    /// The credential registered under `handle` (M7.8), if any.
    fn credential<'a>(&'a self, handle: &'a str) -> BoxFuture<'a, Option<CredentialHandle>> {
        let _ = handle;
        Box::pin(async { None })
    }

    /// The credentials bound to `origin` (M7.8).
    fn credentials_for<'a>(&'a self, origin: &'a str) -> BoxFuture<'a, Vec<CredentialHandle>> {
        let _ = origin;
        Box::pin(async { Vec::new() })
    }
}

/// A credential the host holds for one origin (M7.8, docs/22
/// "Credentials"): what the Core and the model know of it — a handle, a
/// label, the origin it is bound to and the account name. The secret stays
/// with the broker (the desktop's keychain custody); only the host fills it,
/// only into a field of a page at that origin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialHandle {
    /// Opaque handle (`cred_…`).
    pub handle: String,
    /// A label the person chose.
    pub label: String,
    /// `scheme://host[:port]` the credential is bound to.
    pub origin: String,
    /// The account name (not secret; it may be filled and read back).
    pub username: String,
}

/// The origin (`scheme://host[:port]`, lower-case) of an http(s) URL.
#[must_use]
pub fn origin_of(url: &str) -> Option<String> {
    let u = url.trim();
    let (scheme, rest) = u.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?;
    if host.is_empty() {
        return None;
    }
    Some(format!(
        "{}://{}",
        scheme.to_ascii_lowercase(),
        host.to_ascii_lowercase()
    ))
}

/// URL schemes a session may be navigated to by the agent (docs/22: page
/// content is untrusted; privileged and local schemes are never reachable).
pub const NAVIGABLE_SCHEMES: &[&str] = &["http://", "https://"];

/// Whether `url` may be navigated to by the agent.
#[must_use]
pub fn navigable(url: &str) -> bool {
    let u = url.trim();
    NAVIGABLE_SCHEMES
        .iter()
        .any(|s| u.len() > s.len() && u[..s.len()].eq_ignore_ascii_case(s))
        && !u.contains(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_is_scheme_host_and_port_only() {
        assert_eq!(
            origin_of("https://App.Test:8443/login?next=/x#f").as_deref(),
            Some("https://app.test:8443")
        );
        assert_eq!(
            origin_of("http://127.0.0.1:3000/").as_deref(),
            Some("http://127.0.0.1:3000")
        );
        assert_eq!(origin_of("file:///etc/hosts"), None);
        assert_eq!(origin_of("https://"), None);
    }

    #[test]
    fn control_lease_fences_stale_agent_input() {
        let l = ControlLease::initial();
        assert!(l.admits_agent_input(1));
        let user = l.handed_to(Controller::User);
        assert_eq!(user.generation, 2);
        assert!(!user.admits_agent_input(1));
        assert!(!user.admits_agent_input(2));
        let back = user.handed_to(Controller::Agent);
        assert_eq!(back.generation, 3);
        assert!(
            !back.admits_agent_input(1),
            "an input from before the takeover is fenced"
        );
        assert!(back.admits_agent_input(3));
        assert_eq!(
            back.handed_to(Controller::Agent),
            back,
            "a no-op hand-over keeps the generation"
        );
    }

    #[test]
    fn only_http_and_https_are_navigable() {
        assert!(navigable("https://example.test/a"));
        assert!(navigable("HTTP://127.0.0.1:8080/"));
        assert!(!navigable("file:///etc/passwd"));
        assert!(!navigable("chrome://settings"));
        assert!(!navigable("javascript:alert(1)"));
        assert!(!navigable("https://"));
        assert!(!navigable("https://a b"));
    }

    #[test]
    fn fingerprint_changes_with_url_title_or_version() {
        let a = PageState {
            url: "https://x".into(),
            title: "t".into(),
            ready: true,
            state_version: 1,
        };
        let mut b = a.clone();
        b.state_version = 2;
        assert_ne!(a.fingerprint(), b.fingerprint());
        let mut c = a.clone();
        c.title = "u".into();
        assert_ne!(a.fingerprint(), c.fingerprint());
        assert_eq!(a.fingerprint(), a.clone().fingerprint());
    }

    #[test]
    fn requests_and_responses_round_trip_as_tagged_json() {
        let r = HostRequest::Snapshot { max_nodes: 200 };
        let j = serde_json::to_value(&r).unwrap();
        assert_eq!(j["kind"], "snapshot");
        assert_eq!(serde_json::from_value::<HostRequest>(j).unwrap(), r);
        let e = HostResponse::Error {
            code: "CDP".into(),
            message: "x".into(),
        };
        let j = serde_json::to_value(&e).unwrap();
        assert_eq!(j["kind"], "error");
        assert_eq!(serde_json::from_value::<HostResponse>(j).unwrap(), e);
    }
}
