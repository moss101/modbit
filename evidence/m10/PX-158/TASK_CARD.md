# PX-158 — image tokens charged by size in compaction estimates

Classification at start: DOCUMENTED-ONLY (`message_text` counted a media part as its caption only).
First missing link: the estimator had no per-image cost, so a transcript of screenshots never reached the compaction threshold.

Change (PR #159, 41f7ff83): `image_tokens` (about 1 token per 750 px, floor 85, cap 1600, 1200 when the size is unnamed) and `message_tokens`, used by all four estimate sites in `runtime.rs`; text-only transcripts are estimated exactly as before.

## Proof

Unit: `image_tokens_follow_the_size_the_result_named`, `many_frames_reach_a_threshold_their_captions_never_would` (`services/modbit-core/src/runtime.rs`).

Integration (QUAL-PX-158 / PX-E2E-158), `services/modbit-core/tests/px_158_image_tokens.rs`, on the real `modbit-core` binary, its store, the gateway's HTTP/SSE client, the Media Pipeline (`fs.read` of real 1280x800 and 1920x1080 PNG files: decoded, metadata stripped, sent as `image_url` data URLs), the prompt compiler, the calibrated model-aware trigger (`MODEL_WINDOW`, a catalog entry with a 24000-token window) and the compaction ladder:

- `qual_px_158_frames_reach_the_compaction_threshold_where_reported_usage_does`: at both sizes, every request after the first is estimated by the Core (its own `request_estimate` from the compile record, corrected by `modbit_compaction::Calibration` from the reports before it) within the margin of what the provider reported; the first `CompactionStarted` fires before a request whose provider-counted transcript is over the soft threshold or short of it by less than the margin, and never a request after the provider's count crossed it by more than the margin (1280x800: same request as the provider; 1920x1080: one request early, 362 tokens short of the threshold, inside the 480 margin). Counted as captions (frames at zero), the transcript the Core compacted is under the threshold by more than the margin even at the 3x calibration bound. The epoch installs, the run keeps reading frames under it, and in every request each image follows its own tool result (which names the frame and announces the attachment), that result follows the call that asked for it, and every announced attachment is present.
- `qual_px_158_a_text_only_transcript_is_estimated_as_before`: no request carries an image; the same margin checks hold; the trigger fires at the request where the provider's count crosses.

The provider's count is computed by the scripted OpenAI-compatible server from the body it actually received and reported as `usage.prompt_tokens` in the final SSE chunk, as the API does (`px_common::scripted_model_metered`): text at four characters a token plus three tokens a message (OpenAI's published rule of thumb and cookbook framing), tool schemas by the same rule, and each image by OpenAI's published high-detail tile rule (fit in 2048x2048, shortest side to 768, 85 + 170 per 512 px tile; no `detail` is sent, so `auto`, which is high detail at these sizes) read from the PNG's IHDR. It is deliberately not the Core's estimator: the Core charges w*h/750 (1365 and 1600 tokens for the two sizes against the provider's 1105) and counts text with `estimate_tokens_v2`, so the calibration has to close a real gap.

Margin: `window / 50` (480 tokens at 24000), the headroom `compaction_model::from_window` reserves for "what the estimate misses". The code and dossier name no other margin for this comparison; the choice is ours and is a limitation below.

Mutation and differential (`mutation-check.txt`): the trigger site counting a frame as its caption fails the frames test (no compaction at all while the provider counted 20741 of a 24000 window); the pre-PX-158 estimator fails it on the margin; under the pre-PX-158 estimator the text-only test passes with identical figures (trigger before request 11, hard 12834, `tokens_before` 11271), which is the proof that a text-only transcript is estimated as before.

Local: both tests pass; `RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets --locked` and `cargo fmt --all -- --check` clean.

## Limits (why REAL_TESTING, not further)

- The provider is a scripted server applying a published rule, not a live provider's tokenizer. Its text rule (four characters a token) is an approximation real tokenizers do not follow exactly; agreement here is agreement with the stated rule. A live leg against a usage-reporting endpoint has not been run.
- The Core charges one rule (about w*h/750, Anthropic's published estimate) for every provider. The row's scope says "a documented per-provider rule ... and detail class"; there is no per-provider rule and no detail class in the code. Against the OpenAI tile rule the calibration absorbs the difference: at 1280x800 the image over-count (1365/1105) happens to sit close to the text over-count, at 1920x1080 it does not and the trigger fires one request early, inside the margin. Sizes whose two rules diverge further (very wide or very small images), or a transcript that mixes sizes, were not tested and could fall outside the margin.
- The margin (`window / 50`) is our reading of the code; the dossier names none. The first request of a run is uncalibrated and is excluded from the margin check (the Core estimated 7913 where the provider reported 6304).
- Frames reach the model here through `fs.read` of PNG files (the Media Pipeline path that `media` and external `media_parts` results share). Computer-use observation frames (`computer.*`) were not exercised; whether and with what size caption they reach the model as image parts is PX-174's ground.
- The OpenAI-compatible adapter is the only transport exercised; the Anthropic adapter's media placement is not.
- Hard-pressure (synchronous) compaction is not the path these runs take; the soft trigger fires first at both sizes.
- No CI run yet.
