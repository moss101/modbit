# PX-145 task card — Core-run verification after edits, repair conclusion, derived Node/TypeScript and Go plan

Record: docs/decisions/DR-PX-2026-10-25-017-autonomous-coding-engine.md (section 1 finding 7; decision D5). Rows: docs/62 PX-145.

Classification at start: IMPLEMENTED-PARTIAL. verify.run and repair conclusion existed but only ran when the model called them; the derived Node plan had no build or type check. First missing link: the run loop never verified an edit by itself.

Built: services/modbit-core/src/auto_verify.rs (+ run_loop and rebuild wiring in runtime.rs), HarnessState.{unverified_write,written_paths,auto_verifications}, crates/verification/src/plan.rs (Node, Go), per-task repair bound 12.

Proof: real-Core tests qual_px_145_* (real tsc, real npm run build, real git, SIGKILL resume); live runs 3-5 in this directory (real Electron, real z.ai glm-5.3-flash model, "Fix my broken application."): 3/3 verified fixes, 0 approvals, test file untouched. Mutation: disabling auto_verify::due fails all three Core tests.

Not proven: other models, larger repositories, Windows and Linux (tests are cfg(unix)); a cost cap on the live run (an unpriced binding is stopped as unmetered).
Status ceiling claimed: E2E_PROVEN (not COMPLETE: CI legs and the owner's review of the live evidence).
