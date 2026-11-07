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

## Change
- `crates/policy/src/config.rs`: new layer key `computer_frame_window` (u32). Merge law: the smallest value any layer states wins; a lower layer may lower it, an attempt to raise it is a recorded rejected widening. The resolved field is not serialised while unset, so a configuration that does not name it keeps its generation digest (REQ-EV-0041).
- `crates/computer/src/policy.rs`: `DEFAULT_FRAME_WINDOW = 3` (the reference loop the DR cites keeps three), `FRAME_WINDOW_CEILING = 8`, `frame_window(Option<u32>)` clamps to `1..=8` (never zero: a coordinate action needs the latest frame). Why eight: a 1280 x 800 frame is charged about 1,365 tokens (PX-158, at most 1,600 per image), so eight frames are about 13,000 tokens, the size of the default compaction budget.
- `services/modbit-core/src/computer.rs`: `frames_kept(&ResolvedConfig)`, the computer host's reading of that policy.
- `services/modbit-core/src/runtime.rs`: `frame_window(transcript, keep)` projects the transcript the model is sent. The media of `computer.*` results beyond the newest `keep` are removed from the request copy, and each such step's own tool result gains `frame_not_shown: {window, call, action, target (the call's arguments, bounded to 300 characters), outcome, object (the frame's digest), mime, kept}`. The step's original result text, including the PX-176 observation, is kept. Media of other tools is untouched. The projection is applied in three places: the request copy before `hydrate_media`, the request-overhead estimate, and the compaction trigger's `measured_tokens`, so the trigger counts what the model is sent. The stored transcript, the persisted `TranscriptEntry` step outputs, the object store and the log are not changed. A restart rebuilds the full transcript and the same projection applies to it.
- The window applies to every run whose transcript holds `computer.*` frames (a computer-use child and a top-level task that drives the computer).

## Proof (local, real Core)
All runs use the real `modbit-core` process and its SQLite store and object store, the real `modbit-actuator-fixture` process, real approvals through `ResolveApproval`, and a scripted OpenAI-compatible model. The scripted model is not a live provider.
- `services/modbit-core/tests/px_computer.rs::qual_px_174_a_long_computer_use_child_is_sent_its_last_three_frames_and_older_steps_as_text`: a parent spawns the `computer-use` child, which runs 43 tool steps: plan, start, state, then twenty turns of `computer.set_value` (Name = frame-01..20) plus `computer.screenshot`, giving 20 different frames. Asserted:
  - every child request carries at most 3 images, and they are exactly the newest three screenshots;
  - every older screenshot keeps its own result text and has exactly one `frame_not_shown` record (window 3, its call, `computer.screenshot`, `SUCCESS`, a 64-hex digest); the final request has 17 such records;
  - the Core's own estimate of every request it compiled (PX-059 accounting in the context-pack objects) stays at or below the stated ceiling of 40,000 tokens. The local maximum was 35,911; with the mutation it was 58,032;
  - the child's last step names its earliest frame from the text alone (call, action, outcome, object digest). The test checks that this digest is the SHA-256 of the image bytes the child was sent for that call while it was in the window, and that the frame is no longer an image in the last request;
  - afterwards the 20 `ComputerObserved` screenshot events read back by offset range, each stored original matches its digest and decodes as a 1280 x 800 PNG (20 different digests), each egress copy the model was sent is in the object store under its digest, and each is named in a persisted `tool_result` transcript entry (what a restart rebuilds the transcript from).
- `qual_px_174_policy_sets_one_frame_and_a_window_over_the_ceiling_is_clamped`: an admin layer with `computer_frame_window: 1` gives at most one image per request (always the newest) and records with window 1. With `99`, the window is clamped to 8: 10 screenshots give 8 images and 2 records with window 8.
- Unit tests: `runtime::tests::the_frame_window_keeps_the_last_frames_and_records_the_older_steps_as_text` (pure projection; stored transcript unchanged; other tools' media untouched; N=1); `modbit_computer::policy::tests::the_frame_window_defaults_to_three_and_is_clamped_to_one_and_the_ceiling`; `modbit_policy::config::frame_window_tests::*` (a lower layer can lower the window but not raise it; an unset key is not serialised).
- Full `px_computer` suite: 34 passed, 3 ignored (the macOS real-helper tests). `cargo clippy --workspace --all-targets --locked` with `-D warnings` and `cargo fmt --all -- --check` are clean.
- Mutation: `evidence/m10/PX-174/mutation-check.txt`. Keeping every frame as an image fails both tests (more than N images; ceiling exceeded at 58,032).

## Graph status
PX-174 stays **NOT_STARTED**. Its prerequisite PX-158 is WIRED (its live legs are in open PR #175), so `tools/graph.py` refuses any move above NOT_STARTED. When PX-158 is COMPLETE, the row can be advanced one step at a time, citing this evidence and a CI run of this branch.

## Known limits
- Fixture actuator only. Per DR-PX-2026-10-26-018 (the live native-model proof row), no real model drove a real application here.
- The "model names earlier steps from the text" leg uses a scripted model that reads the text records and names the earliest frame. It proves the information is in the request and correct. It does not measure a live model's recall.
- The token ceiling (40,000) is stated for this run's shape (43 steps, 20 frames, the gpt-5 catalog entry). It bounds the Core's estimate (tokens-v2 plus image size), not a provider's count. The text history still grows with the steps until compaction drops it under the existing rules; the window bounds only the images.
- The 43 steps ran without a compaction epoch, so the interaction between the window and an epoch is not exercised end to end here. The trigger's measurement uses the windowed view (unit-level reasoning, not a dedicated test).
- No Core kill or restart mid-run in these tests. Restart safety rests on the window being a projection over the transcript a restart rebuilds from the persisted step outputs; the test shows every frame's entry is persisted.
- `frame_not_shown.target` is the call's arguments. For `computer.screenshot` these are usually empty (`{}`), so the step's subject is identified by its call id, its outcome and its frame digest, and the inputs before it stay as their own text results.
- `computer_frame_window` is not added to the signed policy bundle's `ADMIN_KEYS` (`computer_apps_allow` is not there either), so it is set through the configuration layers (admin, project, user), not the bundle.
- Telemetry `tokens_before`/`tokens_after` of compaction events still count the stored transcript, including frames outside the window.
