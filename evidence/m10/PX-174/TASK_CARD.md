# PX-174 task card — screenshot window for computer-use runs

Decision Record: DR-PX-2026-10-26-018 (D1, D2; Appendix A UPG-G04). Evidence tier: release-critical (it changes what an effect-bearing tool loop shows the model, adds a policy key, and changes the compaction measurement).

## Audit (before, at 71fedf81)
Traced from the production caller to the request the provider receives:

1. `computer.screenshot` (`crates/tools/src/computer.rs`) puts the frame through the Media Pipeline and returns `media` (egress copy) and `artifact` (stored original) in its structured output; `ComputerHost::flush` logs `ComputerObserved { artifact_ref }` and indexes the frame for retention.
2. `runtime.rs` `media_refs(&r.structured_output)` turns that into a `MediaRef` on the `TranscriptEntry::ToolResult`, which is persisted as the step's output object; `apply_entry` turns it into a `ContentPart::Media` (reference only, no bytes) in the in-memory transcript.
3. Each model round, `hydrate_media` fills **every** `ContentPart::Media` of the whole transcript with its bytes before `modbit_prompt_compiler::compile`; the OpenAI adapter sends each as an `image_url` block.

Classification:
- Keep-last-N frame window: **NOT-FOUND** (DOCUMENTED-ONLY in the dossier). Nothing between the persisted transcript and `hydrate_media` limits frames; a computer-use child's request carries every frame it took since the last compaction epoch. The only bounds are compaction (which charges each image by size since PX-158) and storage retention (`MODBIT_COMPUTER_RETENTION_SECS`, which deletes pixels after a day; a frame whose bytes are gone is not sent).
- No policy key for N exists in `modbit_policy::config`.
- The full record (frame objects, `ComputerObserved` events, the persisted transcript entries with their media references, the PX-176 observation text): **PRODUCTION-WORKING** and untouched by this task.
- The compaction trigger and the request-overhead estimate count every media part of the stored transcript, so a window applied to the request alone would make them count frames the model is no longer sent.

First missing link: a projection step from the stored transcript to the model-visible request that keeps the last N computer frames as images and puts a text record in place of each older one, with N from policy and clamped.
