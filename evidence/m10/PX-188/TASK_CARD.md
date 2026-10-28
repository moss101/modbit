# PX-188 task card - memory privacy

Classification at start: IMPLEMENTED-PARTIAL (scopes, delete, promote existed; no scope-widening share, no redaction before sharing, no person-facing view).
First missing link: nothing widened a private item into a shared scope; the desktop had no memory surface.

Built: crates/memory redact_personal + MemoryItem::shared_as; services/modbit-core memory::share and the ShareMemory command (author-only, widening-only, redacted, shared_from link on the log); TS client + desktop IPC/preload/Customize "Memory" section (list with scope/source/status, remember, keep, share, delete).
Real proof: services/modbit-core/tests/memory_graph_index.rs qual_px_188 (real Core process, restart rebuild) and apps/desktop/e2e/px-188-memory.spec.ts (packaged Electron, real Core, recorded model requests, relaunch).
Existing proof reused: px_113_a_promoted_memory... covers delete -> next request/inspector/rebuild.
Not covered: accessibility (axe) pass on the new section; another human principal (the Core has one local user, so "another user's private item" is covered at the store level in memory.rs tests, not end to end).
