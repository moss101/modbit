# Task Card — IMP-EV-0187 Rich MCP media results (M9.4)

## Identity

- Task ID: IMP-EV-0187 (moved to M9 by DR-M5-001: rich MCP media needs the External Tool Hub of M9.4)
- Milestone: M9 Engineering memory / effects / security hardening (P0/P1)
- Requirements: REQ-EV-0187 (normalize text/image/audio/file/resource outputs into typed ToolResult media parts); QUAL-EV-0187 (MCP test server returns image+text; both reach a vision-capable model and the evidence store); docs/58 MEDIA-E2E-006; docs/25.
- Qualification: a real MCP server process returning a real PNG, through the real Core, to a scripted vision-capable endpoint.
- Evidence tier: real-system.

## Goal

An image a server returns is media like any other media the host reads — and is treated exactly as such, so nothing about the path is special-cased for a program the host does not control.

## Existing-code audit

- classification: PARTIAL before this task. The Media Pipeline (M2.10, IMP-EV-0188) already scanned, budgeted, stripped metadata and produced the egress copy a vision-capable model receives, and the Core's runtime already turned a tool result's media into content parts and the split follow-up representation — but only from the single-media shape `fs.read` emits. The MCP gateway (IMP-EV-0104) decoded and typed an `image`/`audio` block and stopped there: no envelope, no egress copy, nothing reaching a model. DR-M5-001 deferred this task to M9 for exactly that reason.
- production entry points: `crates/tools/src/external.rs` (each image/audio part through `crate::media::read` with the source `external.<server>.<tool>`, emitting `media_parts` and `refused_media`); `services/modbit-core/src/runtime.rs` (`media_refs` reads `media_parts`, an array, through the extracted `envelope_ref`, so one result may carry several images); `tools/mcp-testserver` (`chart` returns a caption and a real PNG carrying an instruction-shaped `tEXt` comment).

## What the path is

`tools/call` result → the gateway decodes the base64 and bounds the block → `crate::media::read` scans, budgets, strips metadata and stores the original and egress copies by digest → `media_parts` on the tool's structured output → `media_refs` → the split follow-up representation of REQ-EV-0188: the caption in the tool message, the egress copy in the user message that names the call. The canonical log keeps digests, never bytes.

## Limitations

- Audio blocks are normalized and stored the same way, but no routed model takes audio input today, so they reach the vision bridge's "unsupported modality" path (REQ-EV-0184) rather than a model.
- `resource` blocks with embedded binary are counted and bounded but not put through the pipeline: their URI and text are the untrusted data the host keeps. A resource that is an image would be normalized only if a server returns it as an `image` block.
- The pipeline's budget is the host's default; a per-server media budget is not configurable.

## Verification

- `services/modbit-core/tests/surface_protocol.rs::qual_ev_0187_an_mcp_image_result_reaches_a_vision_capable_model_through_the_media_pipeline`
- `crates/mcp` `result::tests::text_and_image_blocks_are_kept_as_typed_parts`, `::a_payload_that_is_not_base64_never_becomes_media`, `::parts_and_bytes_are_bounded`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
