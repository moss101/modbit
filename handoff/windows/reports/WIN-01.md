# WIN-01 — Baseline report: run exactly what CI runs, locally

## 1. Card / graph tasks / requirements

- Card: WIN-01 — local Windows baseline of the CI Windows legs; serves PX-030.
- Branch: `wip/win-baseline` off `main` @ `a1c0836c`; test run executed against that exact tree (no repo file modified during the run).
- Requirements touched: none (observation card — nothing fixed, per the card's Output clause).

## 2. Machine

See `WIN-00.md`. Relevant to this run: Windows 11 build 26200.8875, MSVC 17.14 + SDK 26100, Rust 1.97.1, Node 26.5.1, pnpm 11.8.0, Python 3.12.7 venv on PATH. Display: single 1280×800 panel at **150 % scaling (AppliedDPI 144)**. Local `rg` on PATH: **ripgrep 15.1.0 (winget)**.

## 3. Audit

Baseline card — no production code audited or changed. The command list was taken from `.github/workflows/ci.yml` (jobs `rust`, `node`, `desktop-e2e`, Windows legs). Drift vs the handoff's list, resolved in favour of the workflow as the handoff instructs: CI additionally runs the `modules` architecture lint, `pnpm typecheck`, the PX-041 perf re-run, and an Electron warm-up — all included below.

## 4. Change

None to tracked files. Environment for the run: `RUSTFLAGS=-D warnings`, `CARGO_INCREMENTAL=0`, toolchain PATH per WIN-00, `MODBIT_CORE_BIN=D:\modbit\target\debug\modbit-core.exe` for the conformance/e2e legs.

## 5. Commands run

Per-step exit codes and durations (script: `D:\modbit-setup\win-01.sh`, logs: `D:\modbit-setup\win-01-logs\<step>.log`; no output was piped through `head`/`tail` for exit codes):

| Step | Command | Exit | Duration |
|---|---|---|---|
| pnpm-install | `pnpm install --frozen-lockfile` | 0 | 2s |
| fmt | `cargo fmt --all --check` | 0 | 26s |
| clippy | `cargo clippy --workspace --all-targets --locked` | 0 | 5m04s |
| build | `cargo build --workspace --locked` | 0 | 6m29s |
| test | `cargo test --workspace --locked` | **101** | 32m05s |
| perf-px041 | `cargo test --locked -p modbit-core --test stream_transcript perf_px_041 -- --nocapture` | 0 | 3m02s |
| lint-manifest | `architecture-lint --manifest-path` | 0 | 14s |
| lint-spawns | `architecture-lint spawns` | 0 | <1s |
| lint-credentials | `architecture-lint credentials` | 0 | 1s |
| lint-modules | `architecture-lint modules` (+ `--ts-manifest` per package) | 0 | <1s |
| node-typecheck | `pnpm typecheck` | 0 | 5s |
| node-test | `pnpm -r test` | 0 | 15s |
| core-build | `cargo build --locked -p modbit-core -p modbit-execd -p modbit-cli -p modbit-actuator-fixture` | 0 | 1m47s |
| desktop-build | `pnpm --filter @modbit/desktop build` | 0 | 2s |
| electron-warm | `electron --version` | 0 | 25s |
| conformance | `pnpm --filter @modbit/ide-adapter-core conformance` (real Core) | 0 | 8m09s |
| vscode-test | `pnpm --filter @modbit/vscode-adapter test` | 0 | 7s |
| desktop-e2e | `playwright test -c e2e/playwright.config.ts` (real Electron + real Core) | **1** | 36m28s |

Total wall time: **1h 34m 40s**. 15 of 17 steps green.

Rust suite shape: 46 test binaries executed, **200 passed, 1 failed, 1 ignored**. (The macOS/Ubuntu legs finish in 25–45 min per CI comments; the smaller executed count on Windows is itself input to WIN-02 — large parts of the suite are `cfg(unix)`-gated.)

## 6. Real-effect proof

- Real MSVC compile of the whole workspace under `-D warnings` (clippy + build green).
- Real process boundary: the failing/passing retrieval benchmark drove a **real ripgrep process**; the conformance suite ran against the real `modbit-core.exe` over the real transport; desktop-e2e drove the real Electron app (201/208 specs green, 36.4m).
- Real Windows display boundary: the one e2e failure is a measured, computed-style value on this machine's real 150 % display (see below).

## 7. Failure injection

None (baseline card). The two observed failures, root-caused:

**F1 — `cargo test`: 1 test of 201.** `benchmarks/retrieval/tests/external_baseline.rs:331 ripgrep_ranks_by_distinct_words_and_runs_as_a_real_process`:
`assertion left == right failed: whole words, most words first — left ["./b.txt","./a.txt"], right ["b.txt","a.txt"]`.
The ranking order is correct (b before a); only the path spelling differs (`./` prefix). The test shells out to the **real `rg` resolved from PATH**; this machine has winget ripgrep **15.1.0**, whose relative-path output gained the `./` prefix, while hosted runners (where main @ `a1c0836c` is green, run 1055) ship a different version. **Classification: local-only** (tool-version drift on PATH), not a Windows defect. Hardening suggestion for the owner: the test should pin/vendor the `rg` it drives, or normalize the `./` prefix — otherwise every machine with rg ≥15 fails it.

**F2 — `desktop-e2e`: 1 spec of 208** (201 passed, 6 skipped, 36.4m). `e2e/awl.spec.ts:241 — PX-192..194, PX-198` failed at `expect(await shellDrift(page, density), "light shell in comfortable")` with one drift: `top bar rule {"width":"0.666667px","color":"rgba(23,23,23,0.08)"}`. This machine's display runs **150 % scaling**: a 1px CSS hairline computes as 0.666667px (1 ÷ 1.5). Hosted `windows-latest` (run 1055, green) runs at 100 %, and the token gate's expected values come from the reference environment. **Classification: local-only w.r.t. hosted CI**, but a genuine data point for WIN-06, whose scope is exactly DPI scaling at 100/125/150/200 % — the shellDrift gate as written cannot pass on a real 150 % Windows display. Trace retained at `apps\desktop\test-results\awl-PX-192-194-PX-198-the--5c59e-ls-on-each-superseded-value\trace.zip` (machine-local, not committed).

Teardown noise (not failures): `shutdown did not finish cleanly … session-end … archive store is full` and assorted `DISCONNECTED` handler errors appear in the log around kill/restart specs (e.g. PX-055) where disconnecting the Core is the point of the test; all those specs passed.

## 8. Evidence refs

- `commit:a1c0836ce8ad28fe8b8271270c2e2b317672d968` (tested tree)
- `run:1055` — hosted CI on `main` @ a1c0836c, green (all three OSes; the comparison anchor for both failures)
- `test:ripgrep_ranks_by_distinct_words_and_runs_as_a_real_process` (F1) · `test:awl.spec.ts:241-PX-192-194-198-shellDrift-light-comfortable` (F2)
- `env:windows-11-26200.8875-display-1280x800-at-150pct-applieddpi-144` · `env:ripgrep-15.1.0-winget-on-PATH`
- `artifact:D:\modbit-setup\win-01-logs\` (17 step logs + summary.tsv, machine-local) · `artifact:apps\desktop\test-results\...\trace.zip` (F2 Playwright trace, machine-local)

## 9. Remaining gaps and blockers

1. `gh auth login` still not run on this machine — blocks push/PR of `wip/win-baseline` and `wip/win-00-manifest-windows-paths` (owner action).
2. Hosted comparison for F1/F2 was made against the public run list (anonymous); per-job log inspection needs `gh` once authed.
3. Only 200 Rust tests executed on Windows — the `cfg(unix)` gap is WIN-02's input; this baseline quantifies it.
4. F2 implies the shellDrift gate needs a DPI-aware expectation before WIN-06 runs its 100/125/150/200 % matrix — flagged for the maintainer's WIN-06 planning.

## 10. Next safe action

Commit `WIN-00.md` + `WIN-01.md` on `wip/win-baseline`; after `gh auth login`, push `wip/win-baseline` and `wip/win-00-manifest-windows-paths` and open their PRs. No code fixes from this card: F1's rg pinning and F2's DPI-aware gate are proposals for the maintainer to schedule (F2 belongs to WIN-06's scope).
