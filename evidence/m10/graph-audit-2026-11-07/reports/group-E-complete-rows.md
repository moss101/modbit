<!-- Subagent report, 2026-11-07, code read at d4c5458a, nothing run. Spot-checked, not proof; see ../AUDIT.md. -->

Three of the 32 rows are contradicted: PX-197, IMP-EV-0152 and PX-196. This comes from reading the code only; nothing was run, so every "false today" below is a code-path claim. All paths are relative to `/private/tmp/claude-501/-Users-mohsin-projects-modbit/5259dee3-ce69-40d7-96a7-c7cfe8d0e8af/scratchpad/graph-fix` at d4c5458a.

## 1. Contradicted rows

### PX-197

Sureness: high that the code does this, because the path is read end to end. Item (b) needs narrow timing.

**(a) Failure-proof clause:** "an Escape that denies an effect the person only meant to step back from ... fails the spec". This is false when Task details is open.
- `apps/desktop/src/renderer/approvals/model.ts:154` returns `skip` for Escape from any non-editable focus.
- `packages/ui/src/hooks.ts:111-113` dispatches in the capture phase and stops propagation.
- The panel's own Escape listener is a bubble listener (`conversation/conversation.tsx:103-110`), and the panel is not a layer. So the effect is denied and the panel stays open.
- Tests pass because `keyboard.spec.ts:257-259` presses Escape on details with no card pending, and `approvals.spec.ts:326-328` asserts the bare Escape skip on purpose.

**(b) Acceptance clause:** "Escape on a confirmation does not skip the card behind it". This is false if the card docks after the confirmation opened.
- Scopes run innermost-last-pushed, and the deck pushes at `approval-deck.tsx:90` on top of `shell/shell.tsx:360`. Enter can also mean Run in that order.
- `keyboard.spec.ts:202` only covers the card-first order.

**(c) Acceptance clause:** "A step is in the past tense only once it happened". This is false for fold summaries.
- `conversation/rows.tsx:184` counts FAILED and UNKNOWN_OUTCOME calls as done, so lines 196-197 say `Edited x` or `Ran x`.
- `toolVerb` (`conversation/model.ts:302-305`) says the opposite for single rows.
- `model.test.ts:176` pins `unfinishedCalls(["SUCCEEDED","FAILED"])` as empty, and no spec asserts a fold over a failed call.

Note: `2026-11-07 code read at d4c5458a, not run: with an approval docked, Escape while Task details is open skips the effect and leaves the panel open (cardKey approvals/model.ts:154, capture dispatch ui/hooks.ts:111, panel listener conversation.tsx:105). A card docking after a confirmation also takes its Escape. groupSummary rows.tsx:184 says Ran/Edited for FAILED and UNKNOWN_OUTCOME calls.`

### IMP-EV-0152

**Clause:** "UI reload derives all task state from Core APIs."
- `apps/desktop/src/renderer/model.ts:143,151` fill `phase: "drafting"`, empty agent counts and `attachments: 0` locally. `state/use-app.ts:139` then subscribes from the snapshot's last offset, so nothing is replayed.
- The only read after loading is `taskStatus` for diagnostics, and only for Waiting or Failed tasks (`use-app.ts:127`).
- A task completed before the restart shows phase "drafting" in Task details.

Sureness: high on the code, medium-high that this counts as the clause. Test: `fleet.spec.ts:101-105` passes because it checks only row count and `data-state` after restart.

Note: `2026-11-07 code read at d4c5458a, not run: after a reload fromSnapshot (renderer/model.ts:143,151) fills phase drafting, agents 0/0 and attachments 0 locally, and load subscribes from the snapshot offset (use-app.ts:139), so Task details shows defaults, not Core state. fleet.spec asserts only row state after restart.`

### PX-196

**Clause:** "the details show each fact for the open task".
- The cause is the same as IMP-EV-0152. `conversation/task-details.tsx:67,71,76` render the defaults, and risk and evidence vanish (lines 78, 90).
- DR-020 section 5.9.2 hides the unknown phase on the row but not in the details.

Sureness: medium, because it is true in a live session and false after any app or renderer reload. Tests: `fleet.spec.ts:73,79` and `fleet-agents.spec.ts:116,146` read details only in-session. `task-phase` is asserted nowhere.

Note: `2026-11-07 code read at d4c5458a, not run: the details show each fact only in a live session. After an app or renderer reload task-details.tsx:67,71,76 show phase drafting, agents 0/0 and attachments 0 from the defaults at renderer/model.ts:143; risk and evidence disappear. No spec opens the details after a restart.`

## 2. Standing rows, by depth

**Deep (code path plus spec read):**
- **PX-023:** stands as scoped by DR-M6-003. Stale bundle, offline, daily budget, organization override and the Browser rows seal outside it (see section 3).
- **PX-024:** `y`/`x` are keyboard-only in-session (see section 3). The Enter path on the card still works.
- **PX-195:** two borderline clauses, not moved.
  - "no provider leaves it Queued and says so": after send, the conversation says only "Not started" (`services/modbit-core/src/transcript.rs:955`). The cause appears only after clicking Start.
  - "every fact of section 5.6 is reachable": false after a reload, same cause as above. Fixing IMP-EV-0152 clears it.
- **PX-022:** `conv-start` is refused by the Core with `NO_PROVIDER`. The live-model half is deferred by DR-M3-003.
- **IMP-EV-0037, IMP-EV-0143:** the board clause is superseded by DR-020's front matter, and buckets come from Core headers and attention.
- **IMP-EV-0118:** the Core and CLI hold the clause, and the task card records that the desktop has no plan browser.

**Medium (clauses checked, supersession checked):**
- **PX-193:** the nav rows and entries are superseded by DR-020.
- **PX-194:** the status row (AWL-08) and banners (AWL-09) are superseded by DR-020.
- **PX-192:** the tokens match the DR-019 tables I compared (surfaces, base, accent).
- **PX-141, PX-142:** capture parity and the fidelity gate are retired by DR-019 sections 4, 6 and 10.
- **PX-143:** the geometry is superseded by DR-019 section 5.3, and Fleet by DR-020.
- **PX-044:** the "Open" note is stale. `tools/check_dossier.py` check G11 and its test now exist.
- **PX-045.**
- **PX-049:** the `main.ts` diff and hash clause is point-in-time for that change.
- **PX-106.**
- **IMP-EV-0151, IMP-EV-0275.**
- **IMP-EV-0035, IMP-EV-0175:** behaviour holds, but the proof is one layer short. The test compares the protocol view with the request, and no e2e asserts `context-entry` in the UI.

**Light (tests exist, wiring skimmed):**
- **IMP-EV-0036, IMP-EV-0103, IMP-EV-0265, PX-001, PX-002, PX-010.**
- **PX-000:** its direct skill-file writes are authorized by REQ-EV-0181, per the comment at `apps/cli/src/main.rs:388-392`.

**M8.8: the desktop not rendering the stream lies outside it.**
- Its docs/43 entry is bare.
- docs/22 "Cloud browser" ends "Not built: the desktop's rendering of the API stream".
- The task card lists the same limitation and proves the transport with a protocol client.

**Packaged clauses:** PX-022, PX-024, PX-044, PX-045, PX-049, PX-106, PX-141 and PX-142. All ran on unpackaged `dist/main/main.cjs`. No record decides this counts as production-equivalent.
- PX-022 and PX-024 cards describe it as packaged anyway. Their tests launch the unpackaged build.
- PX-045 and PX-106 state the limitation plainly.
- PX-141 defers shipping notices to M10.2.
- PX-044 and PX-049 are silent, and PX-142 names no packaged-versus-unpackaged limitation.

## 3. Wrong-looking, not in any row's clause

| Issue | Where | Should own it |
|---|---|---|
| The rows DR-M6-003 handed to M7, M8 and M9 were never forced, and no M7-M9 task card names it. `REGISTRY_STALE` has no producer. `organizationOverrides` is always empty. The visual-fallback state has no field. | `screens.ts:87`, `use-app.ts:493` | M7.5, M8 and M9 per the DR; no open row |
| No key to take or return browser control, and no pause in the desktop, though the Core now has `PauseTask`. | `keyboard.ts`; docs/39 keyboard model | Unowned |
| `y`/`x` silently do nothing after a reload, because `card.approval` comes only from events. | `use-app.ts:412` | PX-058 or PX-024 |
| The Core dot differs by colour only between ok and warn. Other visible words cover the bad states. | `styles.ts:170-172` | PX-196 or AFW-A14 |
| The gate's `REMOVED` list names `language-labels` and `platform-state`, which correctly moved to Settings, and the check never runs on Settings. | `awl.spec.ts:20`, `settings-page.tsx:312,317` | PX-195 test fix |
| The decision controls in the transcript are unreachable dead code. | `rows.tsx:341-366` | PX-197 cleanup |
| "No ZCode name in the bundle" has no test anywhere, and source maps carry the name. | `build.mjs` (`sourcemap: true`) | PX-192 or M10.2 |
| "Leave terminal" has no click handler. | `terminal-app.tsx:99` | PX-048 |
| Remote browser viewer on the desktop (docs/32) is unowned, because QUAL-PX-128 omits it. | `docs/32_DESKTOP_FRONTEND_IMPLEMENTATION.md` | New row or PX-128 |
| E2E-017's "desktop disconnect/reconnect" cannot hold, because the desktop has no cloud client. | `docs/51_E2E_ACCEPTANCE_TEST_CATALOG.md:95` | M8.7 or PX-128 |
| Evidence-expiry candidates: specs were rewritten by #169 while the rows cite older runs. Those are fleet, keyboard, states, onboarding, attention and issue-intake, affecting IMP-EV-0037, 0143, 0152, 0151, 0275 and PX-022, 023, 024, 010. Run 41983983308 is green on the new specs, so appending it should clear them. I did not diff each spec, and the green claim comes from the retained PX-195..197 closure record, not a log I read. | graph evidence refs | Owner's call under docs/93 |
