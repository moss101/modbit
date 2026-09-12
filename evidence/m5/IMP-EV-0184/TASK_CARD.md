# Task Card — IMP-EV-0184 Multimodal read_file

## Identity

- Task ID: IMP-EV-0184
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0184; owner label: Media + File Tool; subsystem: media
- Qualification: `QUAL-EV-0184` — Read actual PNG/PDF/audio/video with capable and incapable models; unsupported modality is explicit.
- Evidence tier: production-equivalent (real fixtures through the production tool and the real Core against a scripted OpenAI-compatible server)

## Goal

Text and supported image/PDF/audio/video return typed media parts according to model capability.

## Existing-code audit

- classification: PARTIAL before: `fs.read` returned typed `MediaEnvelope`s for PNG/JPEG/PDF/audio/video (M4, REQ-EV-0188/0189) and a vision model received image bytes as split follow-ups, but a text-only model had its media parts dropped in silence (`strip_media`) and audio/video carried no modality on the envelope.
- production entry points: `crates/domain/src/media.rs` `MediaEnvelope.input_modality`; `crates/tools/src/media.rs` (typed audio/video with `metadata_only` lineage, no transcript invented); `services/modbit-core/src/media_bridge.rs` (`BridgeSession::substitute`: per-part `UNSUPPORTED_MODALITY` note naming the routed model, the modality and the retained digest inside the tool result text, or the bridge's description when `MODBIT_VISION_BRIDGE=<endpoint>/<model>` names a vision-capable model, recorded as `MediaBridged`); `services/modbit-core/src/runtime.rs` `hydrate_media` (capability decides bytes or note); `crates/providers/src/gateway.rs` (`o3-mini` text-only catalog entry).
- proof: on the real Core a text-only routed model (`o3-mini`) reading a PNG, a scanned PDF and a WAV gets explicit `UNSUPPORTED_MODALITY` notes (routed model, modality, digest, nothing invented) with no image block in any request; with `MODBIT_VISION_BRIDGE=openai/gpt-5-mini` a scripted vision model describes each distinct digest once, the description enters the tool result labelled lossy and untrusted, and `MediaBridged` records digest, mime, routed model, bridge, description object and tokens; audio is typed `input_modality: audio` with no transcript.

## Limitations

- Audio/video transcription and frame extraction wait for a provider that takes those modalities; the bridge covers images (including scanned pages) only. A substituted note lives in the tool result text, so the model's view of a media read for a text-only model is prose, not a media part.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0184_0185_a_text_only_model_is_told_the_modality_and_a_bridge_describes_media`
- `qual_ev_0184_audio_and_video_are_typed_with_their_modality_and_nothing_is_invented`
- `qual_ev_0188_a_media_tool_result_reaches_the_model_as_a_split_follow_up`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
