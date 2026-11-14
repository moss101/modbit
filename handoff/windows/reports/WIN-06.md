# WIN-06 — Desktop shell on Windows: report

## 1. Card / graph tasks / requirements

- Card: WIN-06 — run the current Playwright catalog against the real Electron app on Windows and fix Windows-only defects **within the existing shell** (no redesign); groundwork for PX-031. Serves PX-030 / PX-195..201 conformance.
- Branch: `wip/win-06-desktop` off `main` @ `f08378ac`.

## 2. Machine

Per `WIN-00.md`; **single 1280×800 panel at 150 % scaling (AppliedDPI 144)** — the display is itself a finding generator (see §7).

## 3. Audit

A read-only coverage audit of every checklist item (machine-local report) found: DPAPI custody (`secrets.spec.ts`), Control-modifier (`keyboard.spec.ts`), reduced-motion (PX-044), sandbox/CSP/preload pins (`hardening-guard.test.ts`, CRLF-normalized hashes) already proven; **tray, single-instance lock, deep links (`modbit://`), file associations and taskbar badge are not implemented on any platform** (no API usage anywhere — a cross-platform feature gap, not a Windows defect); Windows-key handling not found (not deliverable to apps by the OS); IME covered only by the composition-safety unit test; OS-level forced-colors and `prefers-reduced-transparency` never exercised; no test used a Core path with spaces or non-ASCII characters. First missing links: the last two.

## 4. Change (tests only; the shell is not redesigned)

1. **`e2e/win06-platform.spec.ts` (new, 3 tests)**:
   - the Core (and broker beside it) **starts from a path with spaces and non-ASCII characters** — `…/Zoë Müller/bin with spaces/modbit-core.exe` — and the app reaches "Core connected" (real spawn from a real profile-shaped spelling; discovery and spawn are argv-safe, now proven, not assumed);
   - **OS forced colors** (`emulateMedia({ forcedColors: "active", reducedMotion: "reduce" })`): the first-run surface's primary controls stay visible and every checked surface sits on an **opaque backdrop** (ESH-B03's trivially-true fallback is now pinned; the app has no translucent surface at all);
   - on Windows the keychain backend **is DPAPI specifically** (`providerStatus().keychainBackend === "dpapi"`).
2. **`e2e/worktrees.spec.ts`**: a mouse-away (`page.mouse.move(2, 2)`) before each `worktree-apply` click. Reason: the agents rail's hover preview lives **inside the rail's subtree**, so a locator click aimed at a button underneath it never leaves the subtree and the preview never closes — a deterministic interception at this display's geometry (two consecutive full runs + isolated rerun). A user simply moves the pointer away; a locator cannot. This is interaction hygiene, not a weakened assertion: the entire PX-068 modal flow is still asserted.

## 5. Commands run

- Full catalog: `playwright test -c e2e/playwright.config.ts` with `MODBIT_CORE_BIN=target\debug\modbit-core.exe` — **199 passed / 4 failed / 6 skipped, 49.5 min** (builds: pnpm install frozen ✓, core/execd/cli/actuator-fixture ✓, desktop dist ✓).
- Reruns: `PX-068` with the fix → **4 passed**; `win06-platform.spec.ts` → **3 passed**; `pnpm --filter @modbit/desktop typecheck` → exit 0.

## 6. Real-effect proof — the catalog result and its triage

| Failure | Triage |
|---|---|
| `awl.spec.ts:242` (PX-192..198 shellDrift) | **F2** (recorded in WIN-01): `top bar rule 0.666667px` = a 1px hairline at 150 % scaling. Hosted CI (100 %) is green. **Owner decision: DPI-aware gate expectations** — the gate as written cannot pass on a real 150 % Windows display. |
| `awl.spec.ts:550` (PX-194/198, second gate) | Same root: `rail 0.666667px solid…` at 150 %. Same owner decision. |
| `px-201-details-rail.spec.ts:250` | **Second DPI-family instance**: `window.innerWidth` expected 1089, got 1090 — a one-pixel rounding of the window resize at 150 %. Same owner decision (pixel-exact window assertions need DPI-aware tolerances). |
| `worktrees.spec.ts:211` (PX-068 apply-back modal) | **Fixed here**: deterministic hover-preview interception (above). Green with the mouse-away fix. |

## 7. Failure injection

The two awl failures and the px-201 failure are the *environment doing the injecting*: a real 150 % display against pixel-exact gates designed on the macOS reference — three concrete specimens for the owner's DPI decision. The hover-preview interception was reproduced three times (two full runs + isolated rerun) before the fix and zero after.

## 8. Evidence refs

`test:WIN-06 Core starts from a path with spaces and non-ASCII characters` · `test:WIN-06 under OS forced colors the shell stays legible and its surfaces stay opaque` · `test:WIN-06 on Windows the keychain backend is DPAPI` · `test:PX-068 apply-back modal (green with interaction fix)` · `env:windows-11-26200-display-1280x800-at-150pct` · catalog log machine-local (`/tmp/win06-catalog.log`)

## 9. Remaining gaps and blockers

1. **Owner decision required (F2 family)**: DPI-aware expectations for the awl shellDrift/rail gates and the px-201 exact-window assertion — three failures will remain red on any real ≥125 % Windows display until decided. Not fixed here: changing pixel gates without the reference-profile owner's sign-off would weaken them.
2. **Cross-platform feature gaps recorded, not implemented** (the card forbids redesign; these do not exist on macOS either): tray, single-instance lock, deep links (`modbit://`), file associations, taskbar badge — plus `electron-builder.yml` has no `protocols`/`fileAssociations` blocks. Scheduling them is an owner decision with its own DRs.
3. Windows-key handling: not deliverable to applications by the OS; IME: composition-safety is unit-proven, a real IME e2e is not automatable meaningfully.
4. Minor follow-up: the browser view's `webPreferences` block (`browser.ts:295-310`) is not covered by `hardening-guard` pins (the main window's is); an owner may want the pin extended.
5. Roaming-profile / DPAPI-master-key-unavailable branches of keychain custody remain exercised only generically (needs a domain-joined profile — owner environment).

## 10. Next safe action

Merge (maintainer; hosted CI runs the catalog at 100 % scaling where the three DPI-family failures do not occur). Decide the F2-family DPI tolerances (three concrete specimens attached). Then WIN-07 — build the UNSIGNED installer on this machine, install/upgrade/uninstall on a clean snapshot, record Defender/SmartScreen behavior; signing stays BLOCKED on owner-supplied certificates.
