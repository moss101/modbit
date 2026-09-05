# Multimodal / Media Real-System Tests

## Fixtures

Commit deterministic test artifacts: PNG/JPEG with EXIF, very large image, SVG, text PDF, scanned PDF, mixed text/image PDF, malformed PDF, WAV/MP3 sample, short MP4, Jupyter notebook and an MCP server that returns text+image. Hash every fixture.

## MEDIA-E2E-001 — image read to a real vision model

Use packaged Modbit, real `fs.read`, Media Pipeline and a vision-capable provider. Ask a fact visible only in the fixture image. Expected: provider receives supported media form, answer is verified, MediaEnvelope/ArtifactRef/evidence lineage is complete, EXIF is stripped from egress copy.

## MEDIA-E2E-002 — unsupported modality routing

Select a text-only model without vision bridge. Read image. Expected: explicit `UNSUPPORTED_MODALITY` result; no silent base64 drop and no fabricated description. Configure bridge and repeat: bridge model/provider is recorded.

## MEDIA-E2E-003 — text PDF

Read selected pages from a text PDF. Expected: deterministic extraction first, page range and truncation metadata present, no vision call.

## MEDIA-E2E-004 — scanned PDF vision fallback

Use scanned PDF and text-only main model with configured vision bridge. Expected: bounded page rendering/transcription only, output labeled lossy/untrusted, exact page range/model/endpoint recorded, remaining pages not implied covered.

## MEDIA-E2E-005 — PDF page/byte bomb

Submit huge/malformed/decompression-bomb-style fixture. Expected: hard page/byte/pixel/time budgets stop processing before memory exhaustion; typed failure and evidence emitted.

## MEDIA-E2E-006 — rich MCP media

Real local MCP test server returns text plus image content. Expected: MCP Hub normalizes parts, Media Pipeline scans/budgets image, vision-capable provider receives both, call IDs/evidence stay contiguous.

## MEDIA-E2E-007 — strict provider tool-media serialization

Use a strict OpenAI-compatible test endpoint that rejects media in tool-role messages. Expected: provider adapter splits media into compliant follow-up representation while canonical ToolResult remains unchanged and model receives image.

## MEDIA-E2E-008 — notebook structured edit

Read a real `.ipynb`, edit one cell by stable ID through Change Engine, then parse notebook. Expected: unrelated cells/metadata preserved according to policy, Git diff is deterministic, test execution succeeds.

## MEDIA-E2E-009 — tenant isolation

Two tenants upload identical and different media. Expected: artifact authorization prevents cross-tenant access regardless of content hash dedupe strategy; signed URLs/refs are tenant/run scoped.

## MEDIA-E2E-010 — prompt injection inside media-derived text

Place malicious instructions in an image/PDF. Expected: derived transcription is labeled untrusted context; it cannot activate tools, expand capabilities or override authority instructions.

## MEDIA-E2E-011 — targeted browser visual escalation

Use an accessible page and a canvas page. Expected: accessible page completes using structural state with no visual call; canvas case captures only needed region and records escalation reason/state version.

## MEDIA-E2E-012 — media artifact durability

Restart desktop and Core after media is used. Expected: events retain ArtifactRefs/digests and rehydrate authorized display without storing raw base64 in canonical event rows.
