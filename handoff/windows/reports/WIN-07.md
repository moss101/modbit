# WIN-07 — Windows half of packaging, signing and updater (UNSIGNED leg): report

## 1. Card / graph tasks / requirements

- Card: WIN-07 — the do-not-require-owner half of M10.2 on real Windows: build the installer locally, verify its contents, install/launch/upgrade/uninstall on the machine, record Defender/SmartScreen; **signing stays BLOCKED: owner input**. Serves M10.2; PX-031 groundwork.
- Branch: `wip/win-07-packaging` (report only — no product changes) off `main` @ `f08378ac`.

## 2. Machine

Per `WIN-00.md`; real Windows 11 26200, **not** a VM snapshot (the handoff's clean-snapshot suggestion noted; every step below is fully reversible and was uninstalled at the end — the machine is clean).

## 3. Audit

Pre-existing (not recreated): electron-builder config (`nsis` per-user assisted + `zip`), UNSIGNED artifact naming + `build-info.json` labelling, `tools/release/unsigned-bundle.mjs` (refuses unlabelled or signed-claiming builds), the signed-manifest updater (no `electron-updater`). Read `evidence/m10/M10.2/TASK_CARD.md` context from the handoff; branch-archaeology questions the handoff raises (`wip/m10-2-*` branches) remain for the maintainer.

## 4. Change

None to product code — this card is build + install evidence. The report and the artifacts' digests are the deliverable.

## 5. Commands run (exit codes in parentheses)

| Step | Command | Result |
|---|---|---|
| Release build | `cargo build --release --locked -p modbit-core -p modbit-execd` | (0) core 71.9 MB, execd 2.1 MB |
| Package | `CSC_IDENTITY_AUTO_DISCOVERY=false pnpm --filter @modbit/desktop run package` | (0) NSIS + zip + blockmap; NSIS toolchain downloaded on first run |
| Bundle verifier | `node tools/release/unsigned-bundle.mjs --out target/unsigned` | (0) 2 UNSIGNED artifacts + package SBOM (CycloneDX 1.5, 10 components) + SHA256SUMS |
| Install (default) | installer `/S` | (0) per-user at `%LOCALAPPDATA%\Programs\modbit`, no elevation |
| Launch | `Modbit.exe` | Modbit ×4 + `modbit-core` + `modbit-execd` running **from the install dir** |
| Upgrade in place | installer `/S` again | (0) both exes replaced (timestamps), no reboot |
| Install to spaced path | installer `/S /D=…\modbit dir with spaces` | (0) launch from it: full stack runs |
| Uninstall (default dir) | `Uninstall Modbit.exe /S` | (0) program dir removed, no Start-Menu residue, no user-data residue |
| Uninstall (spaced dir) | same | (0) **but left `resources\modbit-execd.exe`** (finding F7-2; manually cleaned) |

## 6. Real-effect proof

- **Unsigned honesty**: `Get-AuthenticodeSignature` → `NotSigned` on installer and `Modbit.exe` (electron-builder's "signing with signtool" lines are its no-op default attempts; nothing was signed, nothing claims to be).
- **Contents**: `resources\` holds exactly `modbit-core.exe`, `modbit-execd.exe`, `apply-helper.cjs`, `update-keys.json`, `update-config.json`, `build-info.json` (`label: UNSIGNED, signed: false, purpose: tests only…`), `app.asar`; **zero debug artifacts** (no `.pdb`, nothing from `target\debug`).
- **Digests**: installer `eb5893a4…7d76e`, zip `c75e687a…c2023`, SBOM `2ac4f5ec…5996` (full set in `target/unsigned/SHA256SUMS-UNSIGNED`).
- **Real install lifecycle**: the installed app runs the whole stack (Electron → Core → broker) from the install directory; closing the app ends the Core; the broker exits on its orphan grace (observed self-exit, well under a minute).

## 7. Findings (recorded; none fixed — owner calls)

1. **Defender/SmartScreen**: Defender real-time protection ON, **zero detections** across install, launch and upgrade. SmartScreen was **not exercised**: a locally built file carries no Mark-of-the-Web; a downloaded UNSIGNED installer would meet the SmartScreen interstitial — resolved only by the owner's signing (the very thing BLOCKED).
2. **Uninstaller residue under lock timing (F7-2)**: uninstalling right after the app closed left `resources\modbit-execd.exe` behind — the broker's image was still teardown-mapped, the NSIS uninstaller exited 0 without a retry or a delayed-delete. The default-dir uninstall (long after the broker exited) was fully clean. Suggested owner fix: NSIS-side retry/`MoveFileEx` pending-rename for freshly-unlocked files (same Windows lock family WIN-05 met in worktrees).
3. **Broker exit timing is observable but correct**: the broker outlives app close by its orphan grace by design; combined with (2) it is exactly the race the uninstaller should tolerate.
4. **Non-ASCII user name**: this machine's profile is ASCII (`Mohsin`), so the `C:\Users\Zoë Müller\…` install spelling was exercised at the **binary-discovery** layer in WIN-06 (spec green) but not as a real profile install — needs the owner's account or a VM user (owner decision).
5. **Signing stage: BLOCKED: owner input** — Authenticode certificate + custody route, updater signing key + pinned update keys, updater hosting endpoint. Never improvised here; a build with no pinned update key must keep failing updates closed (verified config present but unsigned; the update suite itself is WIN-08's run).

## 8. Evidence refs

`build:Modbit-0.0.0-win-x64-UNSIGNED.exe (eb5893a47c93dd982f7bc9d51d9c4de540a1a439d382e8ae9f4adfbd0107d76e)` · `artifact:target/unsigned/SHA256SUMS-UNSIGNED` · `artifact:target/unsigned/modbit-desktop-0.0.0-win-x64-UNSIGNED.package.cdx.json` · `env:windows-11-26200-real-machine` · `test:install-launch-upgrade-uninstall-default-and-spaced-path`

## 9. Remaining gaps and blockers

1. Signing/notarization/updater keys/hosting — **BLOCKED: owner input** (certificates and endpoints are owner-supplied by design; the handoff's stop-and-report rule applied).
2. Updater behavior end-to-end (valid update applies, tampered/downgrade/wrong-key rejected, rollback) — requires the owner's signing key even to fabricate a valid update; runs in WIN-08 only against what the owner enables. Hosted `desktop-packaged` already runs the update suite against the UNSIGNED self-update path.
3. VM-snapshot isolation for destructive install cases — the real machine was used reversibly instead (noted above).
4. Non-ASCII profile install — needs an owner account/VM user.

## 10. Next safe action

Merge this report (maintainer). Then WIN-08: run `e2e:packaged` and the update suite against the **installed** build (not `dist/main`), assemble the evidence bundle under `handoff/windows/reports/WIN-08/` in the `docs/93` evidence grammar, and record deltas vs the hosted `desktop-packaged` run.
