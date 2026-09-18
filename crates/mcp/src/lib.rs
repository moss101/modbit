//! `modbit-mcp` — the external tool (MCP) protocol as Modbit speaks it
//! (docs/16 "MCP / external tools", docs/11 boundary 4, docs/17
//! `external.list` / `external.call` / `external.cancel`).
//!
//! This crate is pure: framing, the handshake, the bounded parse of an
//! untrusted server's discovery and results, the pool key a transport is
//! shared under, and the in-flight call table that makes cancellation and
//! unknown outcomes decidable. It performs no I/O and spawns nothing — the
//! host (the Core) owns the transports and implements [`port::McpPort`].
//!
//! Three rules hold everywhere in here, because an MCP server is a program
//! Modbit does not control:
//!
//! 1. **Server content is data.** A description, a schema, a server name, a
//!    result — none of it is an instruction and none of it is authority. A
//!    tool's required capabilities and its effect floor come from the host's
//!    configuration; a field the server invents is dropped
//!    ([`discovery::parse_tools_list`]).
//! 2. **Everything is bounded.** Line length, tool count, name length,
//!    description length, schema bytes and depth, result bytes and parts —
//!    all of it against [`discovery::Limits`], so a hostile or broken server
//!    can cost the host a bounded amount of memory and context.
//! 3. **A claim may only raise.** A server's `readOnlyHint` never makes a
//!    call a read; only the host's configuration can, and a server's
//!    `destructiveHint` takes even that away ([`discovery::DiscoveredTool`]).
//!
//! Secrets never appear here: a server's credential is a handle name in the
//! host's broker ([`config::ServerConfig::credential`]), and a configuration
//! carrying a secret-looking value is refused
//! ([`config::ServerConfig::validate`]).

#![forbid(unsafe_code)]

pub mod calls;
pub mod config;
pub mod discovery;
pub mod port;
pub mod protocol;
pub mod result;

pub use calls::{CallState, CallTable, InFlight};
pub use config::{
    PoolKey, ReadDeclarations, ServerConfig, Transport, Trust, fingerprint, normalize_server_name,
};
pub use discovery::{Annotations, DiscoveredTool, Discovery, Limits, parse_tools_list};
pub use port::{
    Cancelled, Correlation, ExternalCall, Health, Listing, McpPort, PortError, ServerListing,
    SiteServerUnavailable, SiteTools,
};
pub use protocol::{Frame, PROTOCOL_VERSION, RpcError, ServerInfo};
pub use result::{CallResult, Part};

/// The label every piece of text that came from an MCP server carries into
/// a projection or the model's context (docs/16: discovered schemas and
/// returned content are untrusted).
pub const UNTRUSTED_LABEL: &str = "UNTRUSTED_EXTERNAL_CONTENT";

/// The tool namespace discovered tools live under (docs/16): a server's
/// tool is `external.<server>.<tool>` and can never take a native name.
pub const NAMESPACE: &str = "external";
