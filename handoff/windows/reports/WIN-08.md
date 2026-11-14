# WIN-08 — Packaged Windows E2E and evidence bundle: report

## 1. Card / graph tasks / requirements

- Card: WIN-08 — run `e2e:packaged` and the update suite against the **installed** Windows build on a real machine (not `electron dist/main/main.cjs`), plus the Release-Zero subset `docs/60`/`docs/75` mark applicable where it can run today; assemble an `evidence/`-compatible bundle under `handoff/windows/reports/WIN-08/`. Serves PX-031 / M10.3 groundwork; **no promotion of Windows**.
- Branch: `wip/win-08-bundle` (evidence only) off `main` @ `f08378ac`; bundle also carries the artifacts of the WIN-07 build.

## 2. Machine

Per `WIN-00.md` (real machine; WIN-07's lifecycle left it clean; this card reinstalled, tested, uninstalled, and left it clean again).

## 3. Audit

The packaged subset exists (`packaged.spec.ts`, `packaged-update.spec.ts`, config `e2e/playwright.packaged.config.ts`, subset = conversation/approvals/supervision/perf-budgets). The packaged config resolves its executable at `release\win-unpacked` (`e2e/packaged-app.ts`), so running against the **installed** build needed no code change: the built `win-unpacked` was moved aside and a **junction** placed at that path pointing at the real installed directory `C:\Users\…\Programs\modbit`. The suite therefore exercised the installed files — Modbit.exe, its `app.asar`, the packaged Core and broker — not dev `dist/main`. Build outputs were restored afterwards.

## 4. Change

Evidence only: `handoff/windows/reports/WIN-08/` (per-spec results, Playwright JSON, runner log, SHA256SUMS, build-info, evidence manifest). No product or test code changed.

## 5. Commands run

| Command | Exit | Result |
|---|---|---|
| install (per WIN-07 artifact) `/S` | 0 | installed per-user |
| junction `release\win-unpacked` → installed dir | — | suite resolves the installed executable |
| `pnpm --filter @modbit/desktop run e2e:packaged` | 0 | **23 passed / 0 failed / 6 skipped, 10.0 min** |
| restore `win-unpacked`, uninstall `/S` | 0 / 0 | build outputs restored; machine clean |

## 6. Real-effect proof

The packaged suite ran against the real installed UNSIGNED package: the app booted from `C:\Users\…\Programs\modbit`, connected **its own** packaged `modbit-core.exe`/`modbit-execd.exe` (no `MODBIT_CORE_BIN`), and passed the packaged-only specs (package integrity, unsigned self-update path) plus the representative checkout subset — including `supervision` (Core kill and recovery) and `perf-budgets` — inside the installed environment. Per-spec table: `WIN-08/per-spec-results.md`.

## 7. Failure injection

The suite's own injections ran green in the installed environment: Core kill/recovery (`supervision`), update-path checks (`packaged-update`), reload/reattach. No additional injection was added: the card's limit is the existing packaged subset until M10.3 defines the RC catalog.

## 8. Evidence refs

See `WIN-08/evidence-manifest.md` — `build:eb5893a4…7d76e`, `revision:wip/win-08-bundle`, `env:windows-11-26200-real-machine`, `test:packaged-suite-installed-23-passed-0-failed-6-skipped`, `run:43302557685`, artifacts in the bundle.

## 9. Remaining gaps and blockers

1. **The full M10.3 RC E2E catalog does not exist** (M10.3 `NOT_STARTED`) — this bundle is the M10.2-era subset the handoff limits the card to; a Windows-specific substitute was correctly not invented.
2. **Release-Zero subset**: the applicable runs against the installed build today are the packaged/perf-budgets/supervision legs above; the remaining Release-Zero scenarios need the owner's signing story (WIN-07 BLOCKED item) before they can run as release-candidates.
3. **Signing-gated updater proofs** (tampered/downgrade/wrong-key rejection) require the owner's signing key to fabricate valid updates; the UNSIGNED-path suite passed everything runnable.
4. Windows is **not promoted**: this bundle is attachable to a Decision Record once M10.3 + PX-031 exist and the owner decides.

## 10. Next safe action

Merge the WIN-07 and WIN-08 evidence (maintainer); the eight WIN cards of the handoff are then complete on this machine, with the owner decisions enumerated in each report (F2-family DPI gates, platform-integration features, uninstaller lock retry, signing custody, non-ASCII profile install, cross-account pipe test).
