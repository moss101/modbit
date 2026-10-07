---
id: DR-PX-2026-10-03-008
title: Native local computer control as a semantic-first Computer Runtime, with per-call exact-intent approvals and hardened browser control (PX-069..076, doc 66)
status: accepted
date: 2026-10-03
supersedes: none
approved_by: owner instruction 2026-10-05 (goal: implement research/audit/01-TASK-LIST.md); the basis of 007 is the owner's decisions of 2026-10-03
---

# DR-PX-2026-10-03-008 — Native computer control

## Problem and goal

Modbit controls one application well: its own live Chromium session, structurally first, with a takeover lease and a host-owned emergency stop (docs 22 and 17, M7). The agent cannot operate any other application on the user's computer. It cannot test a native app it has just built, drive a desktop tool, or finish a task that lives outside a browser. The owner has made local operating-system control a real goal. The goal is a Computer Runtime that does this safely: the application's accessibility tree first, coordinates only against a fresh observation, raw input last; every action approved per call with an exact intent that can never be allowlisted; failures typed and an input of unknown outcome latched; a person always able to stop it, independent of the model.

## Trigger and evidence

- **Owner decisions of 2026-10-03.** Local OS control gets its own record (not browser-only); the browser-control carry-overs (a DevTools-protocol deny list, a per-call origin gate, typed refusals with an unknown-outcome latch, emergency stop and the takeover lease retained) are hard requirements.
- **Sealed requirements already ask for it.** REQ-EV-0075, 0081, 0083 to 0090 (ADOPT, Computer Runtime) require exact application identity, a single controller lock, a watchdog, human preemption, a typed failure taxonomy and safe typing; doc 17 lists `computer.observe` and `computer.action`. They are implemented for the browser only.
- **Research (tags per doc 65 section 3).** STATIC: the reference product's computer-use extension separates a model-facing tool layer from a native helper process (`F03` F-A1); offers a background application scope driven through the accessibility tree and an escalation to full-screen takeover with an exclusive lease and a Stop overlay (F-B2); couples observation and action by scope, uses handles that expire (F-B2, F-B3), a fixed coordinate canvas and a single scaler (F-B6), a content-aware screenshot codec (F-B7), a typed refusal contract of 16 codes and 4 escalations (F-B8), a serialised call queue and an unknown-outcome latch after delivery (F-B9), a lease that releases at turn end (F-B10), a permission first-run flow for the helper (F-C1), a supply chain for the helper with pinned manifests and signature checks (F-D1), and launcher-bound local RPC with signer pinning (F-D2); its browser tools have a main-process DevTools deny list, a per-call origin allow-list that fails closed, a certificate trust flow, minimal permissions, scoped device emulation, per-agent tab ownership and a lock overlay with take-control (F-F5 to F-F8). DISK: the sampled profile has the browser tool set (16 tools) and no computer-use helper, and its worker logs the capability as disabled by flag. UNVERIFIED: the native helper's behaviour, because it is not in the shipped bundle and the capability was off in the signed-in profile; no card, overlay or approval wording was ever observed.
- **Audit of `main` 5fdb47f** (doc 66 section 3): the browser runtime is production-working; native control is DOCUMENTED-ONLY; the origin gate and DevTools deny list are IMPLEMENTED-PARTIAL; the actuator, permission flow and presence UI are NOT-FOUND.

## Current behavior

No `computer.*` tools, no actuator, no latch, no native permission flow. Browser actions are typed and risk-classified, navigation is limited to http and https, new windows and permission requests are denied, and the host never gives the model raw DevTools-protocol access; there is no per-call origin policy, no certificate trust flow and no deny list enforced at the bridge for any future raw rung.

## Proposed replacement

Specified in doc 66 (37 requirements, each tagged) and rows PX-069 to PX-076, all release-critical:

1. **PX-069** the Computer Runtime contract in the Core (typed tools, scopes, canvas, handles, typed refusals, latch).
2. **PX-070** per-call exact-intent approvals bound to application identity, window, snapshot or coordinate token, action and arguments, scope and expiry; never allowlistable, never covered by any run mode, never given by a background agent; an observation grant for one application; a versioned list of applications that cannot be driven.
3. **PX-071** the macOS actuator helper: a separate signed process with the operating-system permissions, launcher-bound authenticated local RPC, accessibility-first background control, a takeover lease, human preemption, and Stop within 500 ms from four sources.
4. **PX-072** the helper's supply chain: bundled in the signed, notarised app, hash-pinned, no run-time download, rollback.
5. **PX-073** browser control hardening: DevTools deny list at the host, per-call origin gate, certificate trust, per-task view ownership, turn-scoped emulation.
6. **PX-074** the desktop surface: first-run permission window, presence and Stop, exact approval card with no Always control, action cards.
7. **PX-075** a `computer_use` subagent profile: isolated target, constrained tools, stall rules.
8. **PX-076** screenshot and tree artifacts: window capture, pixel-level masking of secure fields, content-aware codec, retention, audit with counts only.

## Independent Modbit design

Native control is a Computer Runtime under the existing Browser and Computer Runtime owner (the `browser` subsystem), with a new crate registered under that owner by architecture-lint module metadata (the pattern of DR-M9-001). The model sees typed tools; the Core owns leases, handles, refusals, the latch and the audit; a separate helper process, which holds the operating-system permissions and no model, policy, credential or filesystem authority, does the capture and injection. Two scopes: APP (default, background, no cursor takeover) and SCREEN (exclusive lease, real cursor, visible Stop). The action ladder of doc 22 holds. Local trusted tasks on macOS only; every other platform answers ACTUATOR_UNAVAILABLE until its own packaged E2E passes (doc 76).

## Security model

- **Approvals.** Every action: per-call, exact-intent, bound to the target application's signed identity (not its window title), the window, the snapshot id or coordinate token, the action and arguments (typed text shown in full), the scope and the expiry; any change invalidates it. Never allowlisted, never covered by Run everything or any rule, never given by a background or detached agent.
- **Observation.** An observation grant is an exact-intent approval for one application in APP scope, valid for the turn and at most 15 minutes, with a call cap, not inherited. Owner question: per-grant or per-call observation approval (below).
- **Typed refusals and the latch.** 25 codes extending REQ-EV-0089, four escalations, no parallel calls, no retry after delivery, an UNKNOWN receipt and a latch after an input of unknown outcome that only a new session, a fresh observation and a new approval clears.
- **Stop.** Host-owned: the helper halts injection within 500 ms of a Stop from the on-screen control, the desktop, the session emergency stop or a Core disconnect; a user stop is a sticky per-turn state; leases release at turn end.
- **Helper.** Separate signed, notarised process; Core verifies its signature and designated requirement before connecting; per-launch token over an inherited descriptor, peer-credential and process-id checks both ways, exclusive endpoint creation, protocol major version; process identity verified before signalling; bundled with the app, no run-time download.
- **Privacy.** Screenshots and trees are sensitive artifacts: target-window capture, secure fields masked inside the helper, a secure desktop yields SECURE_DESKTOP and no frame, retention per policy, never leaving the machine for a cloud task; audit holds counts and kinds, never contents.
- **Non-drivable applications.** A versioned policy list (credential stores, system security panes, shell hosts, authentication dialogs, Modbit itself) no approval can lift.
- **Browser.** DevTools deny list enforced at the host before attach; per-call origin gate failing closed (`file:` always refused); certificate holds for 60 s; minimal page permissions; sign-out wipes partitions; per-task views, hidden-view cap, no focus stealing; takeover and lock retained.

## Canonical owner mapping (doc 81)

`browser` owns the Computer Runtime and browser hardening (PX-069, 071, 072, 073); `effects-security` owns the approval class (PX-070); `core-runtime` the subagent profile (PX-075); `media` the artifacts (PX-076); `desktop` the surface (PX-074). No second policy engine, approval system, effect ledger, scheduler or process broker is introduced; the actuator is a leaf process, not an owner.

## Alternatives rejected

- **A pixel-first, screenshot-and-coordinates agent.** Rejected: unreliable, expensive in image tokens, and it is the product doc 10 lists as a non-goal; the accessibility tree is the first rung.
- **Letting the model drive input through the shell** (osascript, event synthesis). Rejected: bypasses the typed refusals, the latch and the approval class; shell remains for non-GUI work only.
- **Allowlisting computer actions or covering them with Run everything.** Rejected: the owner's hard requirement, and OS-level input is not undoable.
- **Per-call approval of every screenshot.** Rejected as unusable; replaced by the narrow observation grant (still exact, never allowlisted), subject to the owner's question.
- **Downloading the helper at run time against a pinned manifest** (the reference's approach). Rejected: adds an update path outside the app's own signing; Modbit bundles it.
- **A server-side next-action decider for GUI steps.** Rejected: a second planning loop outside the Core.
- **Windows and Linux actuators now.** Rejected: no evidence base (the reference has none for Linux, and its Windows facts are static); promotion rules of doc 76 apply.

## Supersessions

Nothing sealed is superseded. Clarified and recorded in docs 02 and 03: doc 10's non-goal of a pixel-only computer-use product stands (native control is semantic-first); REQ-EV-0083 to 0090 are fulfilled, not changed; doc 22's locked direction that Modbit does not operate primarily from screenshots stands. The pinned surface is untouched.

## Migration

Additive. New PX rows, a change record and dossier task DOC-PX-008, a new crate and a new service registered under the existing `browser` owner when implemented (doc 12 is a locked path; the implementing task cites this record for the additive layout note).

## Compatibility

Rows are M10, RELEASE_ZERO. They depend on M7.6, M9.5, M10.2 (signing, notarisation, SBOM), PX-057, PX-058 and PX-051 of DR-PX-2026-10-03-007; ratifying this record without that one leaves the dependent rows blocked until those rows complete. Graph schema 1.2 unchanged.

## Security impact

Adds the most privileged capability in the product. Each control above has a negative proof in its qualification, including mutation checks that fail when the identity binding, the latch or the unknown-outcome rule is removed.

## Test impact

Eight qualifications and scenarios in doc 62 on real macOS machines with real fixture applications, the packaged helper and Core, kill-and-restart of Core and helper, a tampered helper, a rival socket, a locked screen, a spoofed window title. No scripted success closes any row.

## Rollback

Revert the commits adding the rows and spec and rerun the reseal; a new record is needed because doc 62 and this directory are locked paths.

## Consequences and owner questions

- **Question 1.** Is observation approved per grant (this record) or per call? A per-call rule is stricter and costs usability; the requirement CUC-C02 changes accordingly.
- **Question 2.** Is macOS-only acceptable for the first release of native control?
- **Consequence.** RELEASE_ZERO grows by eight work items; M10 grows.

## Explicit user approval

Accepted on 2026-10-05 by the owner's instruction recorded in `approved_by` (the goal to implement research/audit/01-TASK-LIST.md, whose BLD-22 is native computer control). The record is ratified as drafted, which includes its own answers to the two owner questions above (observation approved per grant, never allowlisted; macOS only for the first release); the owner may change either by a later record that names this one in `supersedes`. DOC-PX-008 is therefore COMPLETE; the rows that depend on DR-PX-2026-10-03-007 become startable only as those rows complete.
