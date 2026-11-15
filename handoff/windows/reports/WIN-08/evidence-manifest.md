# WIN-08 evidence bundle — packaged Windows E2E against the installed build

Every item maps to one `docs/93` evidence reference. Build digest, source
revision, OS build, per-suite results, artifact digests, hosted-CI run id,
logs — the grammar the owner can attach to a Decision Record once M10.3 and
PX-031 exist. This bundle does **not** promote Windows (PX-031 needs the
M10.3 RC E2E catalog, a Decision Record, and the owner's signature story).

| kind | value | file in this directory |
|---|---|---|
| `build:` | `Modbit-0.0.0-win-x64-UNSIGNED.exe` sha256 `eb5893a47c93dd982f7bc9d51d9c4de540a1a439d382e8ae9f4adfbd0107d76e` | `SHA256SUMS-UNSIGNED` |
| `build:` | label `UNSIGNED`, `signed: false`, `purpose: tests only` | `build-info.json` |
| `revision:` | `wip/win-08-bundle` @ this branch head; base `main` @ `f08378ac` | — |
| `env:` | Windows 11 Pro x64 build 10.0.26200 (real machine, per WIN-00) | — |
| `env:` | suite executed against the **installed** per-user build at `C:\Users\…\Programs\modbit` via a junction at `release\win-unpacked` (the packaged config's resolution point); Core + broker are the package's own binaries; no `MODBIT_CORE_BIN` | `packaged-suite.log` |
| `test:` | packaged suite: **23 passed / 0 failed / 6 skipped, 10.0 min** — packaged + packaged-update + conversation + approvals + supervision + perf-budgets | `per-spec-results.md`, `desktop-packaged-report.json` |
| `run:` | hosted anchor: CI run `43302557685` (green `main` run of the same workflow family) | — |
| `artifact:` | `desktop-packaged-report.json` (Playwright JSON) | here |
| `artifact:` | `packaged-suite.log` (runner output) | here |
| `effect:` | install → launch → full packaged suite → uninstall, all on the real machine; machine left clean (WIN-07 report has the lifecycle detail) | — |

## Deltas vs the hosted `desktop-packaged` leg

- Hosted runs against `win-unpacked`; this run against the **installed** copy
  of the same UNSIGNED artifacts (digests above). **No case differed**: the
  update suite (UNSIGNED self-update path) and every subset spec passed in
  both environments.
- The 6 skips are the suite's own platform/condition skips, identical in
  kind to the hosted run.
