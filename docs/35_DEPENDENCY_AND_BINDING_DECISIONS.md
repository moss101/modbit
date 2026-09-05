# Dependency and Binding Decisions

Most architecture is dependency-neutral. Exact names appear here only where implementation requires a concrete binding.

## Required/selected bindings

- Desktop shell: **PROVISIONAL** Electron-based shell. Must remain behind SurfaceProtocol and browser/session boundaries so it can be replaced.
- Local durable store: SQLite-class embedded SQL store with migrations and WAL behavior suitable for crash recovery.
- Structural parsing: tree-sitter-class parsers plus headless language-service adapters where needed.
- Full-text index: Tantivy-class local index.
- ANN/vector index: **PROVISIONAL** USearch-class embedded ANN implementation; benchmark before lock.
- Procedural isolation: **PROVISIONAL** QuickJS-class isolate with no ambient authority.
- Browser: Chromium/CDP-compatible runtime exposed through Modbit-owned browser abstractions.
- Cloud isolation: the currently selected implementation dependency is **CubeSandbox**, but its name and API are confined to the SandboxBackend adapter. No domain/tool/event schema may depend on it.
- Model gateway: at least two real provider adapters are required for Release Zero; exact providers remain configuration, not domain architecture. OpenAI-compatible and Anthropic-compatible transports are acceptable initial bindings.

## Binding rule

A selected dependency never owns Modbit state, policy, events, memory or product semantics. Replaceability must be proven with contract tests where practical.
