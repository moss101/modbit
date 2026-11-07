# Windows Build-Agent Handoff

**Audience:** a build agent on a physical or VM Windows 11 x64 machine.
**Loop:** you build and commit on GitHub (`moss101/modbit`); the maintainer (macOS) pulls, builds, runs clippy and tests locally, checks hosted CI, and seals graph/manifest.
**Written:** 2026-10-10. State claims below were read from the repo that day; re-verify with the commands given before relying on them.

> `AGENTS.md` governs you exactly as it governs every other agent. Precedence: `AGENTS.md` > `SKILLS.md` > `docs/02` > the rest. This file adds Windows scope; it overrides nothing. The `WIN-*` labels below are a work breakdown for this handoff only. They are **not** graph nodes. The real graph tasks they serve are named in each card.

---

## 0. What "Windows" means in this project today

| Fact | Source |
|---|---|
| macOS is the only Alpha release platform. Windows is `CI_COMPATIBLE`, never "supported". | `docs/76`, `tools/platforms.json` |
| Windows becomes `RELEASE_GRADE` only through PX-031 evidence (packaged desktop E2E + applicable Release Zero subset) **and a Decision Record**. | `docs/76`, `docs/62` §PX-031 |
| PX-031 prerequisites: M10.3 (RC E2E catalog) and PX-030. PX-030 is `IMPLEMENTING`, M10.2 (updater/signing/SBOM) and M10.3 are `NOT_STARTED` in the graph. | `python3 tools/graph.py show PX-031` |
| Hosted CI already builds and tests on `windows-latest` (jobs `rust`, `node`, `desktop-e2e`). | `.github/workflows/ci.yml` |
| Windows has Rust code paths already: named-pipe transport (`crates/protocol/src/client.rs`, `services/modbit-execd/src/broker.rs`), ConPTY via `portable-pty`, `netstat`/CIM listener discovery (`services/modbit-execd/src/listeners.rs`), "job-less tree kill" (`broker.rs` ~L1357), DPAPI label in `apps/desktop/src/main/main.ts`. | code |
| Many Unix-only tests are `#[cfg(unix)]`-gated and therefore **do not run on Windows at all**. | see WIN-02 |
| No Windows packaging, signing or updater exists. No Windows computer-control actuator exists and one is explicitly out of scope. | `docs/66` §8, `docs/78` |

**Hard rule: do not change `tools/platforms.json` to `RELEASE_GRADE`, and do not write any user-facing text claiming Windows support.** `tools/support_claims.py` fails the build if you do. Promotion is a separate Decision Record the owner writes after evidence exists.

---

## 1. Ground rules (read before touching code)

1. **Read order:** `AGENTS.md`, `SKILLS.md`, `docs/01_START_HERE_FOR_BUILD_AGENTS.md`, `docs/76`, `docs/21`, `docs/78`, `docs/87` (handoff protocol), `docs/88` (parallel agents), then the card you are on.
2. **Audit before code.** For each card, trace production caller → real effector, classify the existing code (PRODUCTION-WORKING / IMPLEMENTED-PARTIAL / SCAFFOLDED / DOCUMENTED-ONLY / BROKEN-DRIFTED / NOT-FOUND), and write the first missing link into your report before editing.
3. **No second implementation.** Extend the existing owner (`crates/terminal`, `services/modbit-execd`, `crates/workspace`, `crates/git`, `crates/protocol`, `apps/desktop`). `cargo run -p architecture-lint` must stay green.
4. **A Windows test must prove a real effect on a real Windows boundary** (real ConPTY, real process tree, real named pipe, real NTFS paths, real installer). Do not stub the OS.
5. **Never make a test pass by `#[cfg(not(windows))]`-skipping it, weakening an assertion, or loosening a security check.** If behavior is genuinely Unix-only (permissions bits `0o700`, Unix sockets, microVM), add a Windows equivalent that proves the same property (ACLs, named-pipe security descriptor), or record it as an explicit, justified gap in your report.
6. **All external input is hostile.** Windows adds path attack surface; see WIN-03. The Capability Kernel, path policy, secret broker and provenance controls are never bypassed for convenience.
7. **Do not edit** locked requirement rows, the sealed counts (291 `REQ-EV`, 265 `IMP-EV`, 291 `QUAL-EV`, `EPR-000..019`), or `graph/`, `MANIFEST.md`, `manifest.json`, `docs/98_BUILD_MANIFEST.md`. The maintainer reseals. If you must change a doc, say so in the report and do not run the reseal yourself.
8. **Do not run `graph.py set ... COMPLETE`.** Propose evidence in your report; the maintainer sets status. (This is a handoff-specific split so there is a single graph writer.)
9. **Secrets:** you will need none from the repo. Code-signing certs, Apple identity, SBOM Decision Record and updater keys are **owner-supplied** and never committed. If a card needs one, stop and mark it `BLOCKED: owner input` (WIN-07).

## 2. Machine setup (WIN-00, do first)

Record every version in `handoff/windows/reports/WIN-00.md`.

| Need | How / pin |
|---|---|
| Windows 11 x64, admin once for setup | physical or VM; note build number (`winver`) |
| Visual Studio 2022 Build Tools, "Desktop development with C++" + Windows 11 SDK | MSVC toolchain |
| Rust | `rustup` default host `x86_64-pc-windows-msvc`; the channel and components come from `rust-toolchain.toml`, do not hand-pick. `rustup show` must succeed inside the repo. |
| Node | exact version in `.node-version` |
| pnpm | via `corepack enable`; version comes from `packageManager` in `package.json` |
| Git for Windows | recent; run `git config --global core.longpaths true` |
| Python 3 | for `tools/*.py` and `pytest==8.4.2` (fixture runner) |
| Symlinks | enable Developer Mode (or run tests elevated). Several path-policy tests use **real** symlinks; without this they fail for the wrong reason. |
| Long paths | set `HKLM\SYSTEM\CurrentControlSet\Control\FileSystem\LongPathsEnabled=1` |
| Line endings | `git config core.autocrlf false` in the clone. Check `.gitattributes`; if text fixtures or golden files differ by CRLF, fix at the source, do not normalize in tests. |
| Repo location | a short path on NTFS (e.g. `C:\src\modbit`), **not** OneDrive/Dropbox. Add a Defender exclusion for the repo and `target\` only if builds are unworkably slow, and record that you did. |
| Clone | `git clone https://github.com/moss101/modbit.git`, then branch `wip/win-<card>-<slug>` off current `main`. |

Acceptance for WIN-00: `rustup show`, `cargo --version`, `node -v`, `pnpm -v`, `git --version`, `python --version` recorded, and `pnpm install --frozen-lockfile` succeeds.

## 3. The work, in order

Each card: **Serves** (graph tasks) · **Goal** · **Do** · **Prove** · **Failure injection** · **Output**. Stop at the first card whose prerequisite is unmet; do not skip ahead.

### WIN-01 — Baseline: run exactly what CI runs, locally (serves PX-030)

**Do**, from the repo root in PowerShell (CI sets `RUSTFLAGS=-D warnings`, `CARGO_INCREMENTAL=0`):

```powershell
$env:RUSTFLAGS = "-D warnings"; $env:CARGO_INCREMENTAL = "0"
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked
cargo build --workspace --locked
cargo test --workspace --locked
cargo run --locked -p architecture-lint -- --manifest-path Cargo.toml --rules tools/architecture-lint/rules.toml
cargo run --locked -p architecture-lint -- spawns --repo . --rules tools/architecture-lint/rules.toml
pnpm install --frozen-lockfile
pnpm -r test
cargo build --locked -p modbit-core -p modbit-execd -p modbit-cli
pnpm --filter "@modbit/desktop" build
$env:MODBIT_CORE_BIN = "$PWD\target\debug\modbit-core.exe"
pnpm --filter "@modbit/ide-adapter-core" conformance
pnpm --filter "@modbit/vscode-adapter" test
pnpm --filter "@modbit/desktop" exec playwright test -c e2e/playwright.config.ts
```

(`.github/workflows/ci.yml` is the source of truth; if it differs from this list, follow the workflow and note the drift.)

**Prove:** a table in `reports/WIN-01.md`: every command, exit code, duration, and for each failure the first error line and whether it also fails on hosted `windows-latest` (check `gh run list --branch main --workflow ci`). Separate **local-only** failures (your machine) from **real Windows** failures.

**Output:** do not fix anything in this card unless it blocks the baseline. Failures become the input to later cards.

### WIN-02 — Close the `cfg(unix)` gap (serves PX-030, conformance suites in `docs/76`)

Windows currently does **not execute** these Unix-gated tests, so "Windows CI green" says nothing about what they cover:

`crates/workspace/tests/{real_fs.rs:348,613 · snapshot.rs · property_paths.rs:144}` · `crates/workspace/src/snapshot.rs:230,566,596` · `crates/git/tests/{hardening.rs · px_067_remaining.rs · apply_back.rs:454}` · `crates/git/src/harden.rs:68,350` · `crates/tools/tests/git_hardening.rs:230,334` · `crates/retrieval/{src/persist.rs:156 · tests/persisted_indexes.rs:656}` · `crates/prompt-compiler/src/instructions.rs:473` · `services/modbit-core/tests/worktrees_px.rs` (several) · `crates/sandbox/src/backend/mod.rs:17` (microVM, **stays Unix-only**, Linux/KVM, document and leave).

**Do:** regenerate this list with `rg -n "cfg\((unix|not\(windows\))" crates services apps` (it will have moved). For each gate, classify: (a) Unix-only by nature (permission bits, Unix socket, KVM) → write the Windows analogue (ACL / named-pipe descriptor / hidden-file attribute) or record a justified gap; (b) Unix-gated only because the test used `sh`/symlink helpers → port it so it runs on Windows with a real equivalent.

**Prove:** each ported test fails when the property under test is broken (mutate the code, show red, restore, show green). Record the before/after in the report.

**Output:** one PR per crate group (`workspace`, `git`, `core/worktrees`, `retrieval`), each ≤ a few hundred lines.

### WIN-03 — NTFS path and symlink policy (serves PX-030; security-relevant)

The workspace path policy (`crates/workspace`) is a security boundary. Windows adds ways around a Unix-minded check. Add **real-filesystem** tests (no mocks) and fix the policy where a case escapes the workspace root or a protected path:

- reserved device names (`CON`, `NUL`, `COM1`, `LPT1`, also with extensions: `nul.txt`)
- trailing dots/spaces (`file.`, `file `), case-insensitivity (`.GIT` vs `.git`, `Cargo.TOML`)
- 8.3 short names (`PROGRA~1`), `\\?\` verbatim and `\\server\share` UNC paths, drive-relative `C:foo`, `..\` and mixed `/` `\` separators
- alternate data streams (`file.txt:stream`, `file::$DATA`)
- junctions and directory symlinks pointing out of the root, symlink swap between check and use (TOCTOU), hard links
- protected paths (`.git`, `.modbit`, hook directories) reached by any of the above spellings

**Prove:** a table of attempted spellings → expected `denied`/`allowed` → actual, run on real NTFS. Every escape you find is a finding: fix it in `crates/workspace` (the owner), add the regression test, and report it prominently.

### WIN-04 — Process, terminal and IPC conformance (serves PX-030; `docs/21`)

**Do:** against the real broker (`services/modbit-execd/tests/broker.rs`) and Core on Windows prove:

1. **ConPTY:** interactive command, resize, ANSI output, replay window, cancel.
2. **Process-tree kill:** `broker.rs` uses "job-less tree kill". Audit it. A command that spawns grandchildren (e.g. `cmd /c start /b ping -n 100 127.0.0.1`, and a Node child that forks) must leave **zero** survivors after cancel/timeout/Core stop. If tree-kill leaks, implement Windows **Job Objects** (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) inside the existing broker; do not add a second process manager.
3. **Durable broker survival:** kill the real Core process (`taskkill /F`), restart, and confirm the running command is reattached through `execd.ready` (docs/21 "As built (M4.5)"), not respawned.
4. **Named-pipe security:** pipe name is per-user and unguessable; a second local user/low-integrity process cannot connect (set an explicit security descriptor; test with `runas` or a restricted token). Windows analogue of the Unix `0o700` owner-only directory.
5. **Listener discovery:** `listeners.rs` parses `netstat -ano` and CIM; run against a real listening port and a localized-Windows output sample (non-English `netstat` headers must not break parsing, or must fail closed with a typed error).
6. **Shell choice and quoting:** structured argv with spaces, quotes, `%VAR%`, `^`, `&`, and Unicode through `CreateProcess`; no command-line string is built by concatenation.

**Failure injection:** kill the broker mid-command; fill the pipe; cancel during output flood; Core crash during a long command. Each must end in a typed outcome, never a hang.

### WIN-05 — Git on Windows (serves PX-030, PX-067 hardening)

Run `crates/git` hardening against real Git for Windows: hooks (`.bat`, `.cmd`, and extensionless sh hooks executed via Git's bundled bash), `core.fsmonitor`, `core.hooksPath`, `core.sshCommand`, `include.path`, attributes/filters, `credential.helper=manager` (Git Credential Manager must never be reachable by an untrusted repo and secrets must be redacted from logs), `safe.directory`, `core.autocrlf` effects on diff/patch round-trips, `core.symlinks`, long paths, case-colliding branch names and files. The `architecture-lint spawns` rule (a Git process is built only by the hardened runner) must still pass.

**Prove:** each hostile-repo fixture is neutralized on Windows (hook does not run, fsmonitor does not run, credential helper not invoked) with the same assertions as the Unix suite. Worktree create/merge/cleanup on NTFS, including a file locked by another process during cleanup (typed failure, retained evidence, no half-deleted worktree).

### WIN-06 — Desktop shell on Windows (serves PX-031 groundwork; `docs/78`, `docs/32`)

**Do:** run the Playwright catalog against the Electron app on Windows (WIN-01 shows the baseline) and fix Windows-only defects within the existing shell:

- window chrome per `docs/78` (`titleBarOverlay` / frame behavior on Windows), DPI scaling at 100/125/150/200 %, multi-monitor
- `Control` as the modifier (tests already branch on `process.platform`), Windows key handling, IME input
- High Contrast / forced-colors, reduced-motion, `prefers-reduced-transparency` fallback opaque (ESH-B03)
- `safeStorage` is DPAPI: ciphertext only at rest, restore after restart, behaviour when the user profile is roaming or the DPAPI master key is unavailable (`apps/desktop/e2e/secrets.spec.ts`)
- tray, single-instance lock, deep links (`modbit://`) and file associations, taskbar badge
- Core binary discovery for `.exe` paths with spaces and non-ASCII user names (`C:\Users\Zoë Müller\...`)
- `webContents` sandbox, CSP and preload surface remain **byte-identical** to macOS (no Windows-only relaxation of `webPreferences`)

**Prove:** Playwright traces and screenshots for each state saved as artifacts; axe accessibility run clean on Windows.

### WIN-07 — Packaging, signing, updater, SBOM (serves M10.2) — **partly blocked on the owner**

Before starting: `git branch -a` shows a local-only branch `wip/m10-2-updater-signing-sbom` (macOS-side work in progress). Ask the maintainer for its current state; **do not create a competing packaging design**. Build the Windows half inside whatever the M10.2 design chooses.

**You may do without owner input:**

- an unsigned, reproducible Windows installer build (one chosen toolchain, recorded as an ADR proposal if none is chosen yet) that packages Electron + `modbit-core.exe` + `modbit-execd.exe` + `modbit-cli.exe` and nothing from `target\debug`
- install / launch / upgrade / uninstall on a clean VM snapshot; per-user install without admin; install path with spaces; uninstall removes app files but keeps or removes user data exactly as the spec says
- a CI job (new, `windows-latest`) that builds the installer and uploads it plus a SHA-256 digest as an artifact
- SBOM generation for the Windows artifact set (format per the M10.2 decision)

**Blocked until the owner supplies (stop and report, do not improvise):** Authenticode code-signing certificate and where it lives (HSM / cloud signing / Azure Trusted Signing), the updater signing key, the updater hosting endpoint, and the SBOM Decision Record. Never commit, print or log a key or certificate; never generate a self-signed cert and call the result "signed".

**Prove (signed stage, once unblocked):** `signtool verify /pa /v` on every PE and on the installer; SmartScreen reputation is out of scope but record the observed behavior; updater tests: valid update applies, **tampered payload is rejected**, **downgrade is rejected**, **wrong-key signature is rejected**, interrupted update rolls back, update while the app is running.

### WIN-08 — Packaged Windows E2E and evidence bundle (serves PX-031, M10.3)

Depends on WIN-06 and WIN-07. Run the packaged-app E2E catalog (the same catalog macOS uses for M10.3; do not invent a weaker Windows subset) and the Release Zero subset that `docs/60` / `docs/75` mark applicable, **against the installed build**, not `electron dist/main/main.cjs`.

**Evidence bundle** (`evidence/`-compatible, committed under `handoff/windows/reports/WIN-08/`): build digest (SHA-256 of installer), source revision, OS build, per-suite results, artifact digests, hosted-CI run ids, logs. `docs/62` §PX-031 and `docs/93` define the accepted `--evidence` grammar (`run:`, `test:`, `commit:`, `build:`, `env:`, `artifact:` …); lay the bundle out so each item maps to one.

**Do not** promote Windows. Your deliverable is a bundle the owner can attach to a Decision Record.

### Out of scope here (do not start)

- A Windows **computer-control actuator** (`services/modbit-actuator` is macOS-first; `docs/66` §8 and V06 require its own research, DR and E2E).
- microVM / Firecracker sandbox on Windows (Linux/KVM only).
- Changing `platforms.json`, support-claims text, or declaring Windows supported.
- Cloud, provider-credential and live-model work.

---

## 4. Branches, commits, PRs

- One branch per card: `wip/win-NN-<slug>` from latest `main`. Rebase on `main` before opening the PR; never push to `main`; never rewrite pushed history (add a forward-fix commit instead).
- Small commits, one concern each. Message style: `test(win-04): ...`, `fix(win-03): ...`, `ci(win-07): ...`; reference the graph task (e.g. `PX-030`) in the body.
- Open a PR per card. Hosted CI on all three OSes is part of the loop: a Windows change must not turn macOS or Linux red. Do not merge your own PR; the maintainer merges green PRs.
- Before every push: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked`, `cargo test --workspace --locked` must each be run and their **exit codes captured**. Do not pipe test output through `tail`/`head` and then treat the pipe's exit code as the test result.
- Commit only source, tests, CI workflow changes and `handoff/windows/reports/**`. Never commit `target\`, `node_modules\`, installers, certificates, `.env` files or machine-specific paths.

## 5. Report format (one file per card, `handoff/windows/reports/WIN-NN.md`)

Follow `docs/87`. Required headings:

1. **Card / graph tasks / requirements** and branch + head commit SHA
2. **Machine** (OS build, tool versions; link to WIN-00)
3. **Audit** — classification of existing code and the first missing link
4. **Change** — files changed, why each
5. **Commands run** — exact command, exit code, duration
6. **Real-effect proof** — what real Windows boundary was exercised and the observed output
7. **Failure injection** — what was broken on purpose, what happened
8. **Evidence refs** — in the `<kind>:<value>` grammar (`run:<ci-run-id>`, `commit:<sha>`, `test:<name>`, `artifact:<path>`)
9. **Remaining gaps and blockers** — enumerated; never "mostly done"
10. **Next safe action**

A card without a report, or whose report omits failed commands, is not accepted.

## 6. What the maintainer does with your branch (macOS)

1. `git fetch && git checkout wip/win-NN-...`
2. Rebuild locally: `cargo fmt --all --check && cargo clippy --workspace --all-targets --locked && cargo test --workspace --locked` plus the pnpm desktop build/typecheck, with exit codes captured. macOS cannot execute Windows code, so Windows-only logic is judged from your report plus the hosted `windows-latest` run (`gh run view <id>`).
3. Reads the PR diff, the report and the CI jobs for all three platforms.
4. Reseals the manifest/graph where a card changes the dossier, and sets graph status through `graph.py` with your evidence refs.
5. Merges only when everything is green.

## 7. Suggested first session

1. WIN-00 (setup) → WIN-01 (baseline table). Commit only the two reports on `wip/win-baseline`, open a PR.
2. Stop and wait for the maintainer to triage the baseline. The WIN-01 table decides whether WIN-02..06 run in parallel (`docs/88`) or in sequence.

## 8. Open questions for the owner (the agent must not answer these)

- Installer technology for Windows (NSIS, MSI/WiX, MSIX) and updater mechanism: not decided in the repo; M10.2 owns the decision.
- Authenticode signing route and certificate custody.
- Whether Windows promotion is wanted at all before Release Zero (`docs/77` row 7 calls it optional and separate).
- Minimum supported Windows version and whether ARM64 is in scope (this handoff assumes x64 only).
