# PX-158 — image tokens charged by size in compaction estimates

Classification at start: DOCUMENTED-ONLY (`message_text` counted a media part as its caption only).
First missing link: the estimator had no per-image cost, so a transcript of screenshots never reached the compaction threshold.

Change: `image_tokens` (about 1 token per 750 px, floor 85, cap 1600, 1200 when the size is unnamed) and `message_tokens`, used by all four estimate sites in `runtime.rs`; text-only transcripts are estimated exactly as before.

Proof so far: two unit tests (`image_tokens_follow_the_size_the_result_named`, `many_frames_reach_a_threshold_their_captions_never_would`); mutation-check.txt kills an estimator that ignores images.

Not yet proven (why the row is WIRED, not COMPLETE): the acceptance needs the real Core against a transport that reports usage, showing the estimate reaches the threshold where provider usage does within the calibrated margin, and that a shortened transcript never keeps an image without its caption. Neither integration leg is written.
