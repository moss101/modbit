//! The port the tool layer sees (docs/17 `external.list`, `external.call`,
//! `external.cancel`). The host implements it over real transports; the
//! tools only pass a call through and bound what comes back.
//!
//! The port is where the `session_id / task_id / turn_id / call_id` identity
//! of docs/16 is carried: it goes out with every call as MCP `_meta`, so an
//! external server's own record of what it was asked can be lined up
//! against the host's tool-call log afterwards.

use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::config::Trust;
use crate::discovery::{DiscoveredTool, RejectedTool};
use crate::protocol::ServerInfo;
use crate::result::CallResult;

/// Boxed future (the host's transports are async).
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The `_meta` key the correlation travels under. Namespaced as MCP
/// requires, so it can never collide with a server's own `_meta`.
pub const CORRELATION_META_KEY: &str = "modbit.dev/correlation";

/// The identity of the call, carried to the server and kept by the host.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Correlation {
    /// Session.
    pub session_id: String,
    /// Task.
    pub task_id: String,
    /// Turn, when the call belongs to one.
    pub turn_id: Option<String>,
    /// The host's tool call id.
    pub call_id: String,
}

impl Correlation {
    /// The `_meta` object sent with a call.
    #[must_use]
    pub fn meta(&self) -> Value {
        json!({
            CORRELATION_META_KEY: {
                "session_id": self.session_id,
                "task_id": self.task_id,
                "turn_id": self.turn_id,
                "call_id": self.call_id,
            }
        })
    }
}

/// One invocation of an external tool. The session, task and turn of the
/// correlation belong to the port (the host built it for this task); the
/// tool layer supplies the call's own id and what the kernel judged it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExternalCall {
    /// Server name.
    pub server: String,
    /// Tool name on that server.
    pub tool: String,
    /// Arguments; the hub validates them against the discovered schema
    /// before anything is sent.
    pub arguments: Value,
    /// The host's tool call id.
    pub call_id: String,
    /// Whether the host judged this call able to have an effect (what makes
    /// an interrupted call an unknown outcome rather than a failure).
    pub effectful: bool,
}

/// Why a port call could not be served.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortError {
    /// Stable code.
    pub code: String,
    /// What happened.
    pub message: String,
    /// True when an effect may have happened and the host cannot tell.
    pub unknown_outcome: bool,
}

impl PortError {
    /// A failure that changed nothing.
    #[must_use]
    pub fn clean(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            unknown_outcome: false,
        }
    }

    /// A failure after which the host cannot say whether the effect
    /// happened.
    #[must_use]
    pub fn unknown(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            unknown_outcome: true,
        }
    }
}

impl std::fmt::Display for PortError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

/// What the host knows about a configured server right now. Discovery is
/// lazy: a server is `NotStarted` until something asks for its tools.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Health {
    /// Configured, never started.
    NotStarted,
    /// Handshake done, tools known.
    Ready {
        /// What the handshake established.
        server_info: ServerInfo,
        /// How many sessions are served by this one transport (REQ-EV-0193:
        /// more than one means the pool shared it instead of starting a
        /// second server).
        sharers: usize,
        /// The process serving them, when the transport has one. Two
        /// sessions reporting one pid is what "reused the transport" means.
        pid: Option<u32>,
    },
    /// Not reachable; the message says why.
    Failed {
        /// Stable code.
        code: String,
        /// What went wrong.
        message: String,
    },
    /// Proposed but not trusted: never started (REQ-EV-0224).
    Untrusted,
    /// The task's capability lease does not grant what this server needs, so
    /// it is not started for this task (REQ-EV-0128 scoped auth). A task
    /// that could not do a thing itself cannot have a server do it.
    Unleased {
        /// Capabilities the lease does not grant.
        missing: Vec<String>,
    },
}

/// One server as `external.list` reports it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ServerListing {
    /// Name.
    pub name: String,
    /// Trust state.
    pub trust: Trust,
    /// Health.
    pub health: Health,
    /// Scopes the host granted, in the server's own vocabulary (labels).
    pub scopes: Vec<String>,
    /// Host capabilities this server needs before a task may reach it.
    pub requires: Vec<String>,
    /// Which configuration layer contributed it.
    pub layer: String,
    /// How the configuration decided: which layer's definition stands and
    /// whose contrary opinion was overridden or refused (REQ-EV-0039
    /// provenance, REQ-EV-0128 audit).
    pub provenance: Vec<String>,
    /// The pool key its transport is shared under (audit; the fingerprint
    /// is what makes two sessions share one server).
    pub pool_key: String,
    /// Tools kept from its declaration.
    pub tools: Vec<DiscoveredTool>,
    /// Tools the host refused, with reasons.
    pub rejected: Vec<RejectedTool>,
    /// Tools dropped because the server declared more than the bound.
    pub dropped_over_limit: usize,
}

/// What `external.list` returns.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Listing {
    /// Servers, by name.
    pub servers: Vec<ServerListing>,
    /// Servers the configuration refused outright: a layer tried to add one
    /// a higher authority denied, or to remove one a higher authority
    /// added. They are not in `servers`, and this is where they are
    /// answered for (REQ-EV-0128 audit).
    #[serde(default)]
    pub refused_servers: Vec<String>,
}

/// A server bound to the page's origin that this task cannot reach, and
/// why — the reason the host falls back down the action ladder instead of
/// preferring a structured action (REQ-EV-0281, docs/22).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteServerUnavailable {
    /// Server name.
    pub server: String,
    /// Stable code (`EXTERNAL_SERVER_UNTRUSTED`, `EXTERNAL_CAPABILITY_NOT_LEASED`, …).
    pub code: String,
    /// What is in the way.
    pub reason: String,
}

/// What the site the browser is on offers this task (REQ-EV-0281): the
/// structured tools of the servers the host bound to the page's origin and
/// this task may reach, and the ones it may not with the reason.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SiteTools {
    /// Tools a trusted, leased, reachable bound server declares.
    pub available: Vec<DiscoveredTool>,
    /// Bound servers this task cannot reach, and why.
    pub unavailable: Vec<SiteServerUnavailable>,
}

/// What `external.cancel` returns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cancelled {
    /// The call named.
    pub call_id: String,
    /// What the host did: `NOTIFIED`, `ALREADY_CANCELLING`, `NOT_IN_FLIGHT`.
    pub outcome: String,
    /// True when the call could have had an effect that may still land.
    pub unknown_outcome: bool,
}

/// The host's MCP hub.
pub trait McpPort: Send + Sync {
    /// Every server this task may see, with its tools. Discovery is lazy:
    /// this is what starts a trusted server that has not run yet.
    fn list<'a>(&'a self) -> BoxFuture<'a, Result<Listing, PortError>>;

    /// Invoke one tool.
    fn call<'a>(&'a self, call: ExternalCall) -> BoxFuture<'a, Result<CallResult, PortError>>;

    /// Cancel a call the host issued.
    fn cancel<'a>(
        &'a self,
        call_id: &'a str,
        reason: &'a str,
    ) -> BoxFuture<'a, Result<Cancelled, PortError>>;

    /// What the site at `origin` offers this task (REQ-EV-0281). Only the
    /// servers the host bound to that origin are touched, so reading a page
    /// never starts an unrelated server.
    fn for_site<'a>(&'a self, origin: &'a str) -> BoxFuture<'a, SiteTools>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_correlation_travels_under_a_namespaced_meta_key() {
        let c = Correlation {
            session_id: "s".into(),
            task_id: "t".into(),
            turn_id: Some("u".into()),
            call_id: "c".into(),
        };
        let meta = c.meta();
        let inner = &meta[CORRELATION_META_KEY];
        assert_eq!(inner["session_id"], json!("s"));
        assert_eq!(inner["turn_id"], json!("u"));
        assert!(
            CORRELATION_META_KEY.contains('/'),
            "MCP reserves unprefixed _meta keys"
        );
    }

    #[test]
    fn a_port_error_says_whether_an_effect_may_have_landed() {
        assert!(!PortError::clean("X", "no").unknown_outcome);
        assert!(PortError::unknown("Y", "maybe").unknown_outcome);
    }
}
