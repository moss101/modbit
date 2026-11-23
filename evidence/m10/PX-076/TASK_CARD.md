# PX-076 task card — Screenshot and accessibility artifacts

Record: DR-PX-2026-10-03-008 (phase 5, computer-use hardening). Row, qualification and card: docs/62 REQ-PX-076, QUAL-PX-076, `#px-076`. Evidence tier: release-critical (sensitive-data handling and evidence semantics).

## Audit (2026-11-23, against main c5294eb0)

Classification at start: IMPLEMENTED-PARTIAL. Most of the row was built with PX-069/PX-071 and already proven by `qual_px_076_*` fixture tests and the real-helper test:

| Clause | At start |
|---|---|
| APP scope captures the target window only (`CGWindowListCreateImage` with the window number; SCREEN refused under an APP lease) | PRODUCTION-WORKING |
| Secure fields masked inside the actuator, and again by the Core | PRODUCTION-WORKING |
| Locked screen / secure desktop → `SECURE_DESKTOP`, no frame | PRODUCTION-WORKING |
| Flat → lossless, others lossy on a size-preserving ladder; identical frame encoded once; exactly 1280 x 800 or refused | PRODUCTION-WORKING |
| Audit holds counts and kinds only; effect receipt bound to the intent hash | PRODUCTION-WORKING |
| Frames as media artifacts with provenance, untrusted label, retention | PRODUCTION-WORKING |
| **Accessibility trees as media artifacts with provenance and retention** | **NOT-FOUND**: `computer.state` returned the tree inline, its `ComputerObserved` had an empty `artifact_ref`, and the text persisted forever in the tool-result object and the transcript entry |
| `cloud_isolated` never receives a local frame | IMPLEMENTED-PARTIAL: only the tool-profile gate; `artifact.range` served any digest to any task |

First missing link: the tree was not an artifact (`crates/computer/src/runtime/observe.rs` `state`).

## What was built

- `crates/computer`: `TreeArtifact` on `Answer`; `computer.state` hashes the rendered, redacted tree and its `ComputerObserved` names it (`artifact_ref`, `codec: text`, `content: tree`).
- `crates/tools/src/computer.rs`: the tree goes through `media::read` (provenance `computer:<bundle>#<redacted title>@<window>`, `UNTRUSTED_WORKSPACE_CONTENT`), the stored digest must equal the recorded one (`ARTIFACT_DIGEST_MISMATCH`), the envelope's `text_derivative` is cleared, and the answer carries `tree_artifact {digest, media}`.
- `services/modbit-core/src/computer.rs`: `persisted_output` replaces the tree text with `[[computer-tree:<sha256>]]` in every persisted copy; `hydrate_tree_refs` fills it back for one model request, escaped as JSON and cut at the observation ceiling, or says the tree was removed under the retention setting; `is_local_artifact` reads the retention index. Frames and trees share the index, so the existing sweep removes both and records `ComputerArtifactExpired`. A state read's evidence ref is `computer_tree:<digest>`.
- `services/modbit-core/src/tools.rs`: the result object is stored from `persisted_output`; `StoreArtifacts` refuses an indexed local capture to a `cloud_isolated` task (`LOCAL_ARTIFACT`).
- `services/modbit-core/src/runtime.rs`: the transcript entry is rendered from `persisted_output`; `hydrate_trees` runs beside `hydrate_media` on the request copy.
- docs/25 "As built (PX-076, captured screens)", docs/52 "Data privacy".

## Verification (macOS 27.0.1 arm64, rustc 1.97.1; base main c5294eb0)

- Fixture actuator, real Core (`services/modbit-core/tests/px_computer.rs`, `tests.log`): the five `qual_px_076_*` tests, including the new `qual_px_076_an_accessibility_tree_is_an_artifact_that_follows_retention_and_no_persisted_copy_keeps_its_text`:
  - the request after the read shows the tree;
  - `tree_artifact.media` is untrusted with `computer:<bundle>#` provenance and no `text_derivative`;
  - `ComputerObserved{kind: state}` names the digest;
  - after a 2 s retention the object is gone and `ComputerArtifactExpired` names it;
  - the next request says the tree was removed;
  - no object in the store holds the label.
- The whole `px_computer` suite passes. `qual_px_069_the_faults_…` was narrowed: the tree read before the lock is now an artifact, and the locked screen still adds none.
- Unit tests: `computer::px_076_tree_tests` (3) and `tools::px_076_tests` (the `cloud_isolated` refusal).
- Real macOS helper (`real-helper-ax-tests.log`, `ax-tests.sh` with `MODBIT_REQUIRE_AX_TESTS=1`): 19 of 19 pass, none skipped.
  - `qual_px_071_the_real_core_drives_a_real_application_through_the_real_helper` now also asserts on the real tree: it is stored once by digest; its event names it; the stored bytes equal the tree the model was shown; only that object holds its text.
  - That test passed three more times in a row.
  - One earlier run failed: the model was shown an empty tree right after the two preceding real-helper tests. Another run failed with `HUMAN_ACTIVE` while the machine was in use. Neither recurred on a quiet machine.
- Mutation checks: `mutation-checks-2026-11-23.txt` (persisted_output and hydrate_trees caught; the cloud refusal asserted directly).
- clippy `-D warnings` clean (`clippy.log`).

## Limitations

- Retention is the Core's one setting (`MODBIT_COMPUTER_RETENTION_SECS`, default 24 h), which every task inherits. No per-task retention policy exists in the product to follow; QUAL-PX-076 asks that artifacts "follow the retention setting".
- Two bounded excerpts are records of their own and are not swept: a computer-use child's per-step record carries the tree lines that changed (PX-176), and its stop report the first 800 characters of its last tree (PX-075). Both are redacted, with secure values withheld. They are recorded in docs/25.
- The lossy rungs are PNG channel quantisation, not JPEG (`crates/computer/src/frame.rs`), as before.
- A locked screen and APP-scope pixel exclusion are proven on the fixture actuator and by the helper's code path, not by a real-helper test.
- A model request made by compaction or the reviewer sees a tree's reference, not its text.

## CI and merge

- PR 248, head 183b103f: CI run 45226116588 all 19 checks green (`ci-run-45226116588.json`).
- Squash-merged to main as b1484954 on 2026-11-23 (the owner asked to keep going on M10 and merge green PRs).

## Status

COMPLETE (sealed 2026-11-23). Evidence: run:gha-45226116588, commit:b1484954, the artifacts above. The limitations listed above stand.
