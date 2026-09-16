# Task Card — M7.2 AX/DOM/layout semantic entities and stable IDs

## Identity

- Task ID: M7.2
- Milestone: M7 Live browser (P0)
- Requirements: docs/22 "Semantic Browser Compiler" (accessibility tree, DOM/layout metadata and form/control state fused into persistent semantic entity IDs; full AX snapshots are not repeatedly dumped into context), REQ-EV-0277 (fuse AX/DOM/layout/network state into a compact model-facing page representation), REQ-EV-0278 (references scoped to the browser-state version, invalidating safely on relevant change), docs/22 "Prompt-injection isolation".
- Qualification: QUAL-EV-0277 (an accessible workflow completes without screenshot/OCR dependency — the entities alone name what to act on), QUAL-EV-0278 (a DOM mutation makes a stale ref return `TARGET_STALE`, never a click on the wrong element).
- Evidence tier: real-system (the real Core with a fake host that re-renders; the real Electron app on a real page re-served with a mutation)

## Goal

Give the agent a compact page it can address by stable references — derived from what makes an element the same element to a person — and make a reference that no longer matches resolve to nothing, never to whatever took its place.

## Existing-code audit

- classification: MISSING before this task: M7.1's `browser.snapshot` returned the raw bounded accessibility tree with host node ids valid for one version; nothing derived entities, references, text or an identity hash.
- production entry points: `crates/browser/src/compiler.rs` (`RawAxNode` with `parent`, `backend_dom_node_id`, `bounds`, `disabled`; `EntityKind`; `Entity` with `ref`, `path`, `ordinal`, `bounds`, `backend_dom_node_id`; `PageEntities` with `text` and `entity_hash`; `reference_of`, `compile` (bounded), `resolve` → `Stale::TargetStale { candidates }`); `crates/browser/src/lib.rs` (`HostResponse::Snapshot` carries raw nodes; `BrowserPort::remember_page` / `known_entity`); `services/modbit-core/src/browser.rs` (`SessionRecord.known`, bounded); `crates/tools/src/browser.rs` (`browser.snapshot` compiles and returns entities without DOM node ids; `browser.inspect {ref}` resolves by identity, `TARGET_STALE` with candidates; `MAX_ENTITIES`); `apps/desktop/src/main/browser.ts` (the host's snapshot with `parent`, `backendDOMNodeId`, the `disabled` / `readonly` properties, and `DOM.getBoxModel` boxes for the first 80 actionable nodes); `crates/tools/tool-matrix.json` (the `browser.inspect` row).
- proof: `qual_m7_2_entities_carry_stable_references_and_a_changed_element_resolves_stale` (real Core, a fake host whose every snapshot after the first is a re-render with fresh node ids and the form's "Sign in" button renamed "Continue"): the snapshot the model sees names the Email field by the reference the test computed from role, name, landmark path and ordinal, with `FIELD` / `ACTION` / `LANDMARK` kinds, the `["main:","form:Login"]` path, the page's instruction-shaped paragraph as text with `UNTRUSTED_WEB_CONTENT`, and no `backend_dom_node_id`; `browser.inspect` of the field after the re-render succeeds at state version 2 with its box; `browser.inspect` of the renamed button is `TARGET_STALE` at version 3 with exactly the outer "Sign in" button as the candidate; the host answered one navigation and three snapshots. `apps/desktop/e2e/browser.spec.ts` (the real app, a real page): the agent's snapshot on the fixture yields the compiled entities (the field, the path, the text, no node id), the inspection of the field's reference on the live page resolves with a non-empty box, and after navigating to the page re-served with the form's button renamed, the button's reference is `TARGET_STALE` with a candidate, while the view the person opens is the mutated page at the same URL and title.
- unit: `references_are_stable_across_versions_and_dom_node_ids`, `a_removed_or_changed_element_resolves_stale_never_to_another_node`, `ordinals_tell_look_alikes_apart_and_the_bound_holds`.

## Limitations

- Network state and form-control state beyond value/disabled/readonly are not fused yet (M7.3 deltas will carry what changed; M7.4 actions will use the references); the page transition graph and page classification are M7.3/M7.4; boxes are fetched for the first 80 actionable nodes.
- A reference is by identity (role, name, landmark path, ordinal): two elements with the same identity in the same landmark are told apart only by ordinal, which a reorder changes — the reorder is a change the model must re-read.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_m7_2_entities_carry_stable_references_and_a_changed_element_resolves_stale` (services/modbit-core, real Core)
- `apps/desktop/e2e/browser.spec.ts` (the M7.2 steps of the browser session test)
- `crates/browser` compiler unit tests
- Regression: `qual_m7_1_…` (its snapshot now compiles), `qual_ev_0217_…` / `qual_ev_0230_…`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside: `ci-run-35143904650.json` (main at cc5e95e; the change commit 8b825dc and the keyboard E2E forward-fix cc5e95e)
