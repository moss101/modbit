# Task Card — M2.10 MediaEnvelope + Media Pipeline

## Identity

- Task ID: M2.10
- Milestone: M2
- Canonical owner: media (`crates/tools` `media` module; `MediaEnvelope` in `crates/domain`; object store as the artifact store)
- Requirement IDs: REQ-EV-0184 (multimodal `fs.read` returns typed media parts), REQ-EV-0223 (media budgets: byte/pixel/page limits), REQ-EV-0185 (deterministic PDF text first; the vision fallback is a later multimodal task); docs/25 "Canonical media model", "File/media read behavior", "Security", "Completion proof"; docs/58 fixtures, MEDIA-E2E-003/005/010/012 shapes; docs/43 V3.1 row M2.10.
- Risk class: high (untrusted content, decompression abuse)
- Evidence tier: release-critical
- Release membership: BETA, RELEASE_ZERO

## Goal

Media is first-class typed content: `fs.read` on non-text bytes returns a `MediaEnvelope` (kind, MIME by magic bytes, original digest as `content_ref`, dimensions/pages, provenance with source, workspace revision and task, transform lineage, the budget applied, a trust label and a bounded text derivative) instead of raw or hex bytes. Images stay images with a metadata-stripped egress copy stored by digest; PDFs get deterministic text extraction over a bounded page range; byte, pixel and page budgets stop processing before decoding; malformed input is a typed failure; derived text is labelled untrusted data. Events carry references only; artifacts are retrievable by digest after a restart.

## Non-goals

- Vision escalation for scanned PDFs and image questions to a provider (MEDIA-E2E-001/004; the multimodal provider tasks), provider-specific media serialization (REQ-EV-0188), audio/video transcripts and frames (metadata-only envelopes now), notebook cell operations, MCP media, browser visual escalation.
- SVG rasterization and GIF/WebP dimension parsing (detected and stored by digest; no dimensions).

## Required reading

`AGENTS.md`, `docs/25`, `docs/58`, `tests/fixtures/media/README.md`.

## Existing-code audit

- classification: NOT-FOUND (`fs.read` returned hex for binary files; no media model)
- production entry point: `modbit_tools::media::read` from `fs.read`; `MediaEnvelope` in `modbit_domain::media`; object store refs served by `ReadObjectRange`
- real effector/storage boundary: real PNG/JPEG/PDF bytes from the workspace; `png` decoder with limits; `lopdf` text extraction with a decompressed-size limit; objects in the Core's object store

## Invariants

- Detection is by magic bytes; a wrong extension never changes the type.
- Budgets are checked from headers before decoding (pixel bombs never allocate); page budgets require an explicit range; byte budgets refuse before storing.
- Egress copies of images contain no EXIF/text/XMP/comment segments; the original bytes are retained by digest with the lineage recorded.
- PDF extraction is deterministic (identical envelopes for identical bytes) and never calls a model; hostile embedded text is returned as data under an untrusted label.
- Tool results and events carry digests, never bytes; artifacts survive a Core restart.

## Verification

- `crates/tools/tests/media.rs` on the hashed fixtures: PNG/JPEG dimensions and stripped egress copies, pixel bomb and byte budget refused before decoding, malformed PDF and truncated PNG typed, 80-page PDF page budget with a valid page range, deterministic text extraction with hostile text kept as untrusted data, declared truncation with the full text retained, `fs.read` through the registry
- Core `m2_10_media_reads_carry_digests_not_bytes_and_survive_restart`: `InvokeTool fs.read` on image and PDF, typed failure on the bomb, no bytes on the log, original and egress copies retrievable by digest after a Core restart
- E2E: hosted CI on macOS/Linux/Windows

## Completion evidence

See `evidence.json`.
