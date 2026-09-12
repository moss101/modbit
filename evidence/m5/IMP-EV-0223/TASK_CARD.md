# Task Card — IMP-EV-0223 ReadMediaFile / media budgets

## Identity

- Task ID: IMP-EV-0223
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0223; owner label: Media Pipeline; subsystem: media
- Qualification: `QUAL-EV-0223` — Oversized image/video is bounded; explicit crop improves targeted recognition.
- Evidence tier: production-equivalent (real fixtures through the production tool and the real Core against a scripted OpenAI-compatible server)

## Goal

Media reads enforce edge/byte/page/duration budgets and targeted crop/full-resolution escalation.

## Existing-code audit

- classification: PARTIAL before: pixel, byte, page and text budgets were enforced (M4, `MEDIA_BUDGET_EXCEEDED` on the pixel bomb) but there was no way to ask for a region at full resolution.
- production entry points: `crates/tools/src/media.rs` (`ReadRequest.region`; `crop_png` producing the egress copy as the crop with `crop` lineage and `region` on the envelope; JPEG and non-8-bit PNG crops refused `MEDIA_CROP_UNSUPPORTED`; the pixel budget is checked on the original before any decode so a bomb is refused with or without a region); `crates/tools/src/direct.rs` `fs.read` `region {x, y, width, height}`; `services/modbit-core/src/runtime.rs` `media_refs` (the region on the media part's alt text).
- proof: on the real Core a vision model reading `label.png` whole and then with `region {4,4,16,8}` receives two PNG payloads, the second smaller, the tool result carrying `region: [4,4,16,8]`, the crop's dimensions and `crop` lineage; `bomb.png` with a region is refused `MEDIA_BUDGET_EXCEEDED` and sends nothing; tool-level tests cover the crop bytes, the JPEG refusal and the budgets.

## Limitations

- Regions apply to 8-bit PNG only (no JPEG decoder in the tree); video duration budgets wait for a provider that takes video.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0223_a_region_read_sends_the_crop_and_the_bomb_stays_bounded`
- `qual_ev_0223_a_region_crops_the_egress_copy_and_oversized_media_stays_bounded`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
