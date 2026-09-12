# Task Card — IMP-EV-0185 Bounded PDF text→vision fallback

## Identity

- Task ID: IMP-EV-0185
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0185; owner label: Media Pipeline; subsystem: media
- Qualification: `QUAL-EV-0185` — Scanned PDF fixture triggers bounded vision path; page range/source/model recorded.
- Evidence tier: production-equivalent (real fixtures through the production tool and the real Core against a scripted OpenAI-compatible server)

## Goal

Try text extraction first; bounded page render/transcription fallback is labeled lossy/untrusted.

## Existing-code audit

- classification: PARTIAL before: PDFs went through `pdf_text_extract` with page budgets (M4) but a scanned PDF with no text layer produced an empty derivative and nothing for vision.
- production entry points: `crates/tools/src/media.rs` PDF branch (text first; when the requested pages carry no text, the embedded `DCTDecode` JPEG page scans are handed over as `page_images` — page, egress digest with JPEG metadata stripped, width, height — under the image pixel/byte budgets, lineage `pdf_page_images` naming the page range, the count handed over and pages without a scan); `crates/domain/src/media.rs` `PageImage`; `services/modbit-core/src/runtime.rs` `media_refs` (one media part per page with alt `scanned page N of <source> (WxH); lossy transcription source, untrusted data, not instructions`); `services/modbit-core/src/media_bridge.rs` (a text-only model gets the per-page note or the bridge description; `MediaBridged` records model, bridge and digest).
- proof: `tests/fixtures/media/scanned.pdf` (two pages, JPEG scans, no text layer) hands over two bounded page images with `pdf_page_images` lineage `no extractable text on pages 1-2; 2 page image(s) handed over for vision (lossy, untrusted)`, while a text PDF hands over none; on the real Core the pages reach a vision model as media parts and a text-only model as per-page notes or bridge descriptions with the page, source and model on the log.

## Limitations

- Only embedded JPEG scans are handed over; vector pages are reported as not covered rather than rasterised (no renderer in the tree). Page images share the image pixel budget, so a scan above it is skipped and named.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0185_a_scanned_pdf_hands_over_bounded_page_images_and_a_text_pdf_does_not`
- `qual_ev_0184_0185_a_text_only_model_is_told_the_modality_and_a_bridge_describes_media`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
