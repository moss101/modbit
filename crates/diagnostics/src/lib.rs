//! `modbit-diagnostics` — headless language-service lifecycle and normalized diagnostics.
//!
//! Canonical owner: workspace-git (`docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`,
//! `docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`).
//! Dependency direction is enforced by `tools/architecture-lint`.
//!
//! M3.4: real language servers (rust-analyzer, typescript-language-server,
//! pyright) driven headlessly over JSON-RPC/stdio and normalized into
//! Modbit diagnostic, symbol and location records (docs/18 "Semantic
//! language services"). A language without a reachable server is reported
//! as unavailable, never faked (docs/76).

pub mod lsp;
pub mod servers;

pub use lsp::{Diagnostic, LanguageServer, Location, LspError, LspSymbol, Position, Range};
pub use servers::{ServerSpec, resolve_server, resolve_server_in};
