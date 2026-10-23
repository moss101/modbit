---
id: DR-PX-2026-10-03-012
title: Desktop shell platform integration, the Electron main-process change: window chrome, native integration, proven hardening, update application and version policy (PX-094..098, doc 78)
status: proposed
date: 2026-10-03
supersedes: none
approved_by: pending owner ratification; the scope assumption below needs the owner's confirmation
---

# DR-PX-2026-10-03-012 — Desktop shell platform integration

## Problem and goal

The owner listed "an Electron change" among five goals without saying which change. The synthesis names it as something needing its own record (`SYN` §5) and the research ties it to one concrete subject: the main-process window options, which it marks as touching security-sensitive code (`F06` §14.1). The repository audit shows what the shell lacks: a default 1200 x 800 window with no minimum size, no hidden title bar or drag regions, no native menus, badge, tray, protocol handler or single-instance lock, no pinned security fuses or ASAR integrity, no update flow and no written version policy. The agent-first workspace of DR-PX-2026-10-03-007 needs more of the platform than a bare window.

**Assumption stated plainly.** This record specifies the main-process platform integration of the existing Electron shell: window chrome, native integration, hardening, update application and a version policy. It does **not** replace Electron. If the owner meant a framework replacement or something else, the research and the repository give no basis for specifying it and this record would be amended (question below).

## Trigger and evidence

- **Owner's statement of 2026-10-03** that an Electron change is a real goal; synthesis section 5 lists it as needing its own record.
- **Research (tags per doc 65 section 3).** LIVE: the signed-in agents window has window controls inside the agent-list strip at about 20 pt from the top, no separate top bar across that strip in Settings, a corner radius of about 10 pt, no visible translucency in the light theme; a macOS not-responding dialog appeared once during a slow command probe (`L14` §3.1, §P3.7). STATIC: a hidden title bar with an in-app bar as the drag region, a window-control offset formula derived from the bar height, per-platform window options, optional vibrancy with an opaque fallback and a reduce-transparency preference, a zoom model with a 13 px baseline, template icons for a menu-bar item, and a minimum window of 500 x 520 (`F06` §3.3, §7.1, §14.1, §14.3); a permission allow-list for web views, a certificate trust flow and session wipes (`F03` §6 F-F8); a deep-link install that opens a confirmation with name validation (`F04` §7.6, §10); a sidecar update path with signature and lock handling (`F03` §4). DISK: the reference's process graph (main, shared, search, extension hosts, pty host, utility workers), a renderer ping interval of 5 s with a 15 s block threshold, menu accelerators cached for its agents window (`L13` §3.13, `L12` §3). LIVE (signed-out): the reference runs Electron 42.10.0 with Chromium 148 (`L11`). UNVERIFIED: placement constants on newer operating systems; fuse values (no research source covers fuses, ASAR integrity or updater application of the shell itself).
- **Audit of `main` 5fdb47f** (doc 78 section 3): renderer hardening is partial and proven by existing E2E; window chrome, native integration, fuses and the update flow are NOT-FOUND; M10.2 (updater, signing, SBOM) is NOT_STARTED and stays the owner of those.

## Current behavior

`createWindow` opens a 1200 x 800 window with `sandbox`, `contextIsolation`, no `nodeIntegration` and `webSecurity`; a CSP with `connect-src 'none'` is set on every response; navigation and new windows are denied; every IPC handler validates its sender; a crashed renderer is replaced on the same session. Electron 44.3.0 is pinned exactly. `before-quit` stops the browser host and the Core immediately.

## Proposed replacement

Specified in doc 78 (19 requirements) and rows PX-094 to PX-098, all release-critical:

1. **PX-094** window chrome: inset hidden title bar on macOS, drag regions, measured control placement, opt-in translucency with an opaque fallback and Reduce transparency, persisted display-aware window state, a zoom model; webPreferences, CSP, preload and channel set byte-identical.
2. **PX-095** native menus whose accelerators come from the shortcut registry, a badge counting attention only, a tray or menu-bar icon, a `modbit://` handler with a single-instance lock where links never act on their own, native dialogs whose paths the Core re-validates.
3. **PX-096** main-process hardening: packaged-binary fuses verified in CI, ASAR integrity, app-session permission denial, no web-view attach, origin-limited navigation, a pinned CSP and a pinned IPC channel inventory, main-thread responsiveness within 50 ms at p95, a renderer watchdog.
4. **PX-097** update application at idle with staged, verified apply, rollback and truthful states, on top of M10.2.
5. **PX-098** Electron and Chromium version policy and an upgrade qualification gate.

## Independent Modbit design

Everything stays inside the existing process model: the main process holds windows, IPC, credential custody, the browser host and the updater; heavy work stays in the Core (single owner, doc 81), so the reference's utility-process search is not adopted. Window changes touch constructor options and window state only. The deep-link handler is a narrow allow-list whose routes can only focus or open confirmations. Fuses are set to the hardened values and verified from the packaged binary. Updates never apply during a run or a pending approval, are staged and verified before apply, and keep the previous version. Versions are pinned exactly and move only through a gate that runs the browser-host, CDP deny list, origin gate, security, fuse, accessibility and smoke suites.

## Security model

The renderer's isolation model, CSP, preload allow-list and sender checks are unchanged and pinned by hash and inventory tests. New inbound surfaces are bounded: deep links (allow-listed routes, validated names, confirmation for every state change, no implied session), native dialogs (paths for display only; the Core re-validates), menu and tray actions (typed commands through the same preload functions). Fuses and ASAR integrity close tampering and node-injection paths. The app session denies every permission; browser partitions keep their minimal list. Translucency is off by default and never lowers a contrast ratio.

## Canonical owner mapping (doc 81)

`desktop` owns the main-process integration (PX-094 to PX-097); `governance` owns the version policy and its CI gate (PX-098); M10.2 owns signing, the update channel and the SBOM. No new subsystem, store or process type is introduced.

## Alternatives rejected

- **Replace Electron with another shell framework.** Rejected here: no evidence in the research or the repository, MOD-DESK-001 stands, and the browser host depends on Chromium embedding.
- **Frameless window with a custom drawn title bar on every platform now.** Rejected: only macOS is the release platform; others stay behind capability checks (doc 76).
- **Translucency on by default.** Rejected: the sampled reference window showed none and it costs contrast; opt-in only.
- **Hosting search or indexing in an Electron utility process.** Rejected: a second owner of that work (doc 81).
- **Adopting the reference's 500 x 520 minimum window now.** Rejected: unobserved in a running window; PX-049 sets 900 x 600 until single-pane mode exists.
- **Remote crash reporting.** Not specified; local diagnostics export exists.

## Supersessions

Recorded in docs 02 and 03 and in doc 78 section 6, effective only on acceptance: MOD-DESK-001 (PROVISIONAL, Electron) is retained with its scope clarified; the invariant of PX-049 that webPreferences, CSP and preload stay byte-identical is retained and PX-094 keeps it; PX-095 and PX-096 add main-process code under their own gates and pinned inventories. No sealed row or pinned count changes.

## Migration

Additive. Packaging and fuse configuration land through M10.2. Existing profiles are unaffected; window state migrates by defaulting.

## Compatibility

Rows are M10, RELEASE_ZERO. They depend on PX-049, PX-045 and M10.2 here and in DR-PX-2026-10-03-007, and PX-095 depends on PX-088 and PX-090 of DR-PX-2026-10-03-011 and PX-098 on PX-073 of DR-PX-2026-10-03-008; ratifying this record alone leaves those rows blocked until their prerequisites complete.

## Security impact

Positive overall (fuses, integrity, denials, inventories); new surfaces (deep links, dialogs, update apply) each have negative proofs, including mutation checks that fail when signature verification, fuse reading or the route allow-list is removed.

## Test impact

Five qualifications and scenarios in doc 62 on packaged builds on real macOS machines, tampered archives, flipped-fuse copies, hostile link fixtures, slow-Core and hung-renderer fixtures; the full desktop E2E suite must pass with the fuses set.

## Rollback

Revert the commits adding the rows and spec and rerun the reseal; a new record is needed for the locked paths.

## Consequences and owner questions

- **Question 1.** What is the Electron change meant to be? This record assumes platform integration and hardening; a framework replacement, a version jump or something else needs the owner's words.
- **Question 2.** Should local automations fire with the app closed (a login-item daemon)? That is shell integration; this record does not add one.
- **Question 3.** Is translucency wanted at all? It is opt-in either way.
- RELEASE_ZERO grows by five work items.

## Explicit user approval

Pending. Acceptance flips `status` to `accepted` in the commit that lands the rows, and should say which reading of "the Electron change" the owner ratifies.
