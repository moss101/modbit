# Task Card — PX-157 Secret redaction on memory proposals and use tracking from injected memory

## Identity

- Task ID: PX-157 (REQ-PX-157; owner memory); qualification QUAL-PX-157; prerequisite DOC-PX-017 (row registered on PR #148's branch)
- Evidence tier: release-critical (canonical persistence: a new memory event and its projection rule; the secret boundary)

## Existing-code audit (main 5b17f473)

- Classification: NOT-FOUND for both halves. `memory::propose` stored topic and content as given (a key in a proposal reached the log, the row and later prompts). `injected_memory` was written into the context-pack object only; nothing counted uses. The private-key shape of the one redactor matched the header line only, so a redacted block kept its body.
- Owners kept: the one redactor (`crates/secrets`), the memory aggregate and its projection (`crates/event-store`), `services/modbit-core/src/memory.rs`. No second memory store or counter table.

## Built

- `crates/secrets/src/redact.rs`: a whole `BEGIN … PRIVATE KEY … END` block is one shape (header-only stays as a fallback for a cut block); unit test.
- `services/modbit-core/src/memory.rs`: `MemoryCtx.redactor` (the Core's redactor: held values and every credential shape); `screen` runs before a proposal is stored — unchanged text keeps its id, redacted text is a new content-addressed item, a proposal that is only a secret is refused `MEMORY_SECRET` without echoing it; `propose` returns typed `(code, message)` failures. `used_events` / `usage`: one `MemoryUsed` event per injected item per turn on the item's own aggregate, counted by distinct turn and read from the log; `list_scoped` adds `uses` and `last_used_at_ms`.
- `services/modbit-core/src/runtime.rs`: the prompt's `injected_memory` is recorded with the context-pack compile.
- `crates/event-store/src/projections.rs`: `MemoryUsed` carries no row and changes no projection.
- `surface.proto` `MemoryItemView.uses = 17`, `last_used_at_ms = 18` (additive), TS bindings regenerated.

## Proof (local, macOS arm64)

- `services/modbit-core/tests/memory_graph_index.rs`:
  - `qual_px_157_a_secret_in_a_proposal_is_redacted_or_refused_and_ordinary_text_is_kept`: a real-format key is stored as `[redacted]`, a bare private-key block is refused `MEMORY_SECRET` with no echo, a block inside text loses header and body, ordinary text (unicode, tabs, newlines) is byte for byte; neither secret is in any byte of the data directory.
  - `qual_px_157_each_prompt_that_injects_an_item_counts_once_and_a_restart_rebuilds_it`: uses equal the number of requests that carried the item, last use is after the start, and after a kill plus a full projection rebuild the counts and time are identical.
  - All PX-110/111/113 tests of the file still pass (the `memory_events` helper now lists mutations only).
- Mutation check: `mutation-check.txt`.

## Limitations

- The use count is per turn that injected the item (a turn re-run after a crash compiles a new prompt and counts again, which is a new use). The CLI and desktop do not show the new fields yet.
- Graph status cannot move until PR #148 (which registers PX-157) lands on main.
