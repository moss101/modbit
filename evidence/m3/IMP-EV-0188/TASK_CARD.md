# Task Card — IMP-EV-0188 Split tool media for provider compliance

## Identity

- Task ID: IMP-EV-0188
- Milestone: M3
- Requirement: REQ-EV-0188; owner label: Provider Adapter; subsystem: model-gateway
- Qualification: `QUAL-EV-0188` — Strict OpenAI-compatible test rejects embedded media but passes split follow-up representation.
- Evidence tier: real-system or production-equivalent

## Goal

Provider adapter can transform media placement without losing semantics; canonical ModelEvent stays provider-neutral.

## Existing-code audit

- classification: NOT-FOUND before this batch. M2.10 gave media reads a typed envelope with an egress copy, but nothing could put that image in front of a model: `ContentPart` had text, tool calls and tool results only.
- production entry point: `crates/providers/src/contract.rs` (`ContentPart::Media`, `MediaPayload`); `crates/providers/src/openai.rs` and `anthropic.rs` (placement); `crates/providers/src/gateway.rs` (`media_modalities`, `capability`); `services/modbit-core/src/runtime.rs` (`media_refs`, `media_parts`, `hydrate_media`, `strip_media`).
- proof: the canonical content part names media by the digest of its egress copy and says what it is; where it goes is each adapter's business. A strict OpenAI-compatible endpoint takes only a string in a tool message, so the adapter keeps the tool result as text with its call id and sends the bytes in the user message that follows, naming the call it answers. The Anthropic transport takes an image block inside the tool result itself, so nothing is split there. The router refuses media the routed model cannot accept before dispatch, and the runtime attaches media only when the model's catalog entry has vision. A real run reads a real PNG and the request the provider receives carries the split representation; the canonical log keeps the digest and never the bytes.

## Limitations

Images are sent as bytes; any other media type is described in words with its digest instead, because neither adapter's file-input shape is settled enough to guess at. The runtime attaches media from tool results only, not from ingested attachments. Bytes are resolved per request from the object store, so a very large image is re-encoded each turn it stays in the transcript.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0188_tool_media_is_split_for_strict_endpoints_and_embedded_where_it_is_accepted`
- `qual_ev_0188_a_media_tool_result_reaches_the_model_as_a_split_follow_up`
- `media_parts_come_from_the_result_and_carry_no_bytes`
- `a_model_without_the_modality_sees_the_text_and_no_media`
- `base64_matches_the_standard_alphabet_and_padding`
- `m2_10_media_reads_carry_digests_not_bytes_and_survive_restart`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
