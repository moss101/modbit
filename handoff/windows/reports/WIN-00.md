# WIN-00 — Machine setup report

## 1. Card / graph tasks / requirements

- Card: WIN-00 (machine setup) — prerequisite for WIN-01..WIN-08; serves PX-030 groundwork.
- Graph tasks: none directly (setup card). Serves the Windows legs of PX-030.
- Branch: `wip/win-baseline` off `main` @ `a1c0836c` (handoff refresh commit).

## 2. Machine

| Item | Value |
|---|---|
| OS | Windows 11 Pro, x64, build 10.0.26200.8875 (registry ProductName still reads "Windows 10 Pro"; chassis physical-vs-VM not yet recorded — owner may clarify) |
| Repo | `D:\modbit` (short path, NTFS fixed disk, 635 GB free). Deviation from the handoff's example `C:\src\modbit`: chosen for disk headroom (C: has 120 GB free vs D: 635 GB); still a short NTFS path outside any sync folder. |
| Long paths | `HKLM\...\FileSystem\LongPathsEnabled=1` (pre-existing) |
| Developer Mode | enabled this session (`AllowDevelopmentWithoutDevLicense=1`) so non-elevated processes can create real symlinks |
| Defender exclusion | NOT set (builds not unworkably slow: cold `architecture-lint` build 35.8 s). Will record if ever added. |
| WSL2 | Ubuntu 26.04 LTS (default user `mohsin`), WSL default version 2 |

### Tool versions (native Windows)

| Tool | Version | Source of pin | How installed |
|---|---|---|---|
| Git | 2.54.0.windows.1 | handoff: recent | preinstalled |
| rustup / cargo / rustc | rustup home `C:\Users\Mohsin\.rustup`; cargo 1.97.1 (c980f4866 2026-06-30); rustc 1.97.1 (8bab26f4f 2026-07-14) | `rust-toolchain.toml` (1.97.1, minimal, rustfmt+clippy+rust-analyzer) | `rustup-init.exe -y --default-toolchain 1.97.1 --profile minimal -c rustfmt -c clippy`; `rustup show` in repo reports "active because: overridden by D:\modbit\rust-toolchain.toml" |
| Node | v26.5.1 | `.node-version` | official zip → `D:\tools\node` (user scope, exact version) |
| pnpm | 11.8.0 | `package.json` `packageManager` | `npm install -g pnpm@11.8.0`. Node 26 ships **no corepack** (removed from the default distribution), so `corepack enable` was impossible; npm -g into the user-owned node prefix is the deterministic equivalent (CI's `pnpm/action-setup` also installs standalone). |
| Python | 3.12.7 (venv `D:\modbit-setup\venv`) + pytest 8.4.2 | CI `rust` job installs `pytest==8.4.2`; dossier job uses Python 3.12 | venv from the `py -3.12` launcher |
| MSVC | VS Build Tools 2022 17.14.37906.3, VCTools workload, x64 toolset | handoff §2 | winget (elevated, one UAC consent) |
| Windows SDK | 10.0.26100.0 (`C:\Program Files (x86)\Windows Kits\10\Include`) | `--includeRecommended` | same |
| winget | 1.29.380 | — | preinstalled |

### Tool versions (WSL2 Ubuntu 26.04)

- apt (root): `build-essential pkg-config curl git ca-certificates xvfb python3-pip` (gcc 15.2.0). Only `xvfb` is an explicit CI apt package; the rest mirror what hosted `ubuntu-latest` images preinstall.
- rustup 1.97.1 (same channel/components as native), Node v26.5.1 (tar.xz → `~/tools`), pnpm 11.8.0 (see §7 incident W2), pytest 8.4.2 (`--user --break-system-packages`).
- Clone `~/modbit` @ `a1c0836c` on WSL's own filesystem (not `/mnt/c`); `git config --global core.autocrlf false`.
- PATH block appended to `~/.profile` (login profile, so `wsl -e bash -lc` sees it).

## 3. Audit

Setup card — no production code audited. Machine-state audit before changes: no rustup/cargo, no node/pnpm/corepack, no VS/vswhere, no pytest; `py` launcher present with 3.14.3 and 3.12; the PATH `python` was a third-party venv shim (hermes-agent), so a dedicated venv was created instead of relying on it. First missing link in the toolchain: MSVC linker (installed this card).

## 4. Change

Repo changes: **none** (this file is the first). Machine changes:

- Installed: VS Build Tools 2022 (VCTools + Win 11 SDK 10.0.26100), rustup + 1.97.1 toolchain, Node 26.5.1 at `D:\tools\node`, pnpm 11.8.0, Python venv `D:\modbit-setup\venv` + pytest 8.4.2.
- Registry: Developer Mode enabled (admin, one UAC). LongPathsEnabled was already 1.
- Git: `--global core.longpaths true`; clone-local `core.autocrlf false`; working tree renormalized to LF (see §7 incident N1).
- Bash profiles (`~/.bashrc`/`~/.bash_profile` native, `~/.profile` WSL) carry the toolchain PATH block.
- WSL: apt packages above; rustup/Node/pnpm/pytest; `~/modbit` clone.

## 5. Commands run (all native unless marked WSL)

| # | Command | Exit | Duration |
|---|---|---|---|
| 1 | `git clone https://github.com/moss101/modbit.git D:\modbit` | 0 | ~1 min |
| 2 | `rustup-init.exe -y --default-toolchain 1.97.1 --profile minimal -c rustfmt -c clippy` | 0 | ~2 min |
| 3 | Node zip extract + `node -v` | 0 | ~30 s |
| 4 | `npm install -g pnpm@11.8.0` → `pnpm -v` = 11.8.0 | 0 | ~20 s |
| 5 | `py -3.12 -m venv` + `pip install pytest==8.4.2` | 0 | ~30 s |
| 6 | `rustup show` (in repo; synced rust-analyzer from toml) | 0 | ~30 s |
| 7 | `pnpm install --frozen-lockfile` | 0 | 55.2 s, 396 packages |
| 8 | `python tools/check_dossier.py --manifest` (native) | 1 | ~20 s — see §7 N1/N2 |
| 9 | `git rm --cached -r . && git reset --hard` (renormalize, local `autocrlf=false`) | 0 | ~30 s |
| 10 | `python tools/check_dossier.py --manifest` (native, re-run) | 1 | ~20 s — only [M1] separator artifact remains (§7 N2); **0 hash mismatches** |
| 11 | `cargo build --locked -p architecture-lint` (MSVC linker smoke) | 0 | 35.8 s |
| 12 | WSL apt install (root) | 0 | ~3 min |
| 13 | WSL rustup/node/pnpm/pytest + `~/modbit` clone | 0 | ~4 min |
| 14 | WSL `pnpm install --frozen-lockfile` | 0 | 7.8 s (warm store) |
| 15 | WSL `python3 tools/check_dossier.py --manifest` | **0** | ~15 s — "OK: 97 docs; … manifest hashes verified" |

WIN-00 acceptance: `rustup show`, `cargo --version`, `node -v`, `pnpm -v`, `git --version`, `python --version` all recorded above; `pnpm install --frozen-lockfile` exit 0. **Met.**

## 6. Real-effect proof

- Real MSVC link: `cargo build --locked -p architecture-lint` compiled and linked a real PE with the VS 2022 x64 toolchain (exit 0).
- Real NTFS boundary: the CRLF renormalization (§7 N1) was proven byte-exact by the dossier manifest — after re-checkout, **every** manifest sha256/size matched (0 "hash mismatch" findings), and the same tree passes `check_dossier --manifest` with exit 0 inside WSL.
- Real Linux boundary: WSL2 Ubuntu 26.04 runs the real Linux toolchain (gcc 15.2.0, Linux-target Rust 1.97.1) and verified the manifest hashes on a POSIX path model.

## 7. Failure injection

None injected (setup card). Two real failures occurred and were root-caused:

- **N1 — CRLF checkout (caught by the manifest check, exactly the §2 `.gitattributes` gap).** The clone ran under Git for Windows' *system*-level `core.autocrlf=true` before the clone-local `false` existed, so the first working tree was CRLF; `check_dossier` failed with real hash mismatches ([D6]/[G6]/[M1]). Fix: set local `autocrlf=false`, `git rm --cached -r . && git reset --hard` → LF tree, 0 hash mismatches. **Reported to the maintainer as required; no files edited.** Anyone cloning on Windows hits this; the durable fix is a `.gitattributes` `eol=lf` rule plus a macOS-side reseal (maintainer's).
- **N2 — `check_dossier --manifest` cannot exit 0 on native Windows even with a byte-perfect tree.** `tools/build_manifest.py:117` builds expected paths with `str(p.relative_to(root))`, which yields `\` separators on Windows; the sealed `manifest.json` (sealed on macOS) uses `/`. The set comparison in `check_dossier.py` (M1) then reports every package path missing/extra (4013 findings; 0 hash mismatches). **Fixed this session on branch `wip/win-00-manifest-windows-paths` @ `e562e0a1`** (owner directed the fix): `package_files()` returns `.as_posix()` paths, and both manifest writes pin `newline="\n"` — a second latent Windows bug found while verifying, where text-mode writes made a Windows regeneration differ from the macOS seal in every line (CRLF). Proof, all in a throwaway clone so the WIN-01 baseline tree stayed pristine: with the sealed manifest still in place the only remaining finding is the builder's own hash/size (self-reference until reseal); after regenerating in the clone, `check_dossier --manifest` **exits 0 on Windows** ("manifest hashes verified") and the reseal delta vs the macOS seal collapses to exactly one entry — the builder's own bytes+sha256 (14871→14905). `tools/*.py` are not in the locked-path set (`tools/architecture-lint/rules.toml`), so no Decision-Record trailer is required; the reseal itself (manifest.json/MANIFEST.md) is left to the maintainer per AGENTS.md.
- **W1 (WSL) — first cold start timed out** (`Wsl/Service/CreateInstance/CreateVm/HCS_E_CONNECTION_TIMEOUT`); a retry minutes later succeeded and the distro has been stable since. Transient; no config change made.
- **W2 (WSL) — Ubuntu 26.04 ships uutils coreutils `env` 0.8.0**, which rejects pnpm's `#!/usr/bin/env -S node` shebang ("env: use -[v]S to pass options in shebang lines"); `npm install -g pnpm` also silently produced no `lib/node_modules/pnpm`. Fix: fetch `pnpm-11.8.0.tgz` directly from the npm registry, **verify the sha512 against the registry's `dist.integrity`** (matched: `sha512-wfXnxMsk…VfJg==`), unpack under `~/tools/pnpm-11.8.0`, and install a `/bin/sh` wrapper invoking node by absolute path. `pnpm -v` = 11.8.0. Same pin, verifiable provenance.

## 8. Evidence refs

- `commit:a1c0836ce8ad28fe8b8271270c2e2b317672d968` (both clones at this SHA)
- `commit:e562e0a1` (N2 fix, branch `wip/win-00-manifest-windows-paths`)
- `env:windows-11-pro-26200.8875-x64` · `env:vs-buildtools-2022-17.14.37906.3-sdk-10.0.26100` · `env:rust-1.97.1-msvc` · `env:node-26.5.1` · `env:pnpm-11.8.0` · `env:python-3.12.7-pytest-8.4.2` · `env:wsl2-ubuntu-26.04`
- `build:architecture-lint-dev-msvc-35.8s` (exit 0)
- `test:pnpm-install-frozen-lockfile-native-exit0` · `test:check_dossier-manifest-wsl-exit0`
- Setup transcript: `D:\modbit-setup\logs\admin-step1.log` (machine-local, not committed)

## 9. Remaining gaps and blockers

1. `gh auth login` has not been run on this machine — needed before the first push/PR (owner action).
2. N2 is fixed on `wip/win-00-manifest-windows-paths` @ `e562e0a1`, but until the maintainer merges it **and reseals** (regenerate manifest.json/MANIFEST.md on macOS), `check_dossier --manifest` on native Windows shows the builder's own hash/size mismatch and exits 1. Strict RUN-3 native acceptance lands after that merge+reseal. A Linux cross-check of the patch (apply → expect only the self-hash finding → restore) is deferred: the WSL VM could not cold-start under the WIN-01 baseline's full-core load (HCS_E_CONNECTION_TIMEOUT; pristine-Linux exit 0 was already verified before the baseline started).
3. The `.gitattributes`/`eol=lf` gap (N1) is reported for a macOS-side decision; every fresh Windows clone needs the renormalize workaround until then.
4. Chassis (physical vs VM) unrecorded; OS build recorded.
5. No Defender exclusion added (not needed at current build speeds).

## 10. Next safe action

WIN-01 — run the CI command list unchanged, capture the per-command exit-code/duration table into `reports/WIN-01.md`, fixing nothing unless a failure blocks the baseline. Then commit both reports on `wip/win-baseline` and open the PR (after the owner runs `gh auth login`).
