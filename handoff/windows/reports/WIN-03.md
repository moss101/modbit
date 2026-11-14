# WIN-03 — NTFS path and symlink policy: report

## 1. Card / graph tasks / requirements

- Card: WIN-03 — real-filesystem NTFS attack tests against the workspace path policy; fix escapes in the owner (`crates/workspace`); serves PX-030; security-relevant (docs/20, docs/23, docs/52).
- Branch: `wip/win-03-paths` off `main` @ `f08378ac`.

## 2. Machine

Per `WIN-00.md`; Developer Mode on (unprivileged symlinks); 8.3 short names confirmed ACTIVE on the test volume (`C:\Users\…\LONGDI~1` minted) — the short-name attacks are real here, not hypothetical.

## 3. Audit

`crates/workspace/src/paths.rs` (the policy owner): `normalize` (lexical), `resolve` (symlink walk), `check` (protected globs). Before this card it already handled: verbatim-prefix stripping, mixed separators, `..` climbs, absolute-outside, drive-prefix components, symlink hops out of the root. First missing link: **`resolve` trusted `FileType::is_symlink()` and non-link components verbatim**, and the protected Globs were case-sensitive.

## 4. Change

Production (`src/paths.rs`):
1. **Protected globs are case-insensitive on Windows** (`GlobBuilder::case_insensitive(cfg!(windows))`): `.GIT/config` walked past `.git/**`. Both `check` and `protected_match` (the snapshot diff's check) use the same set, so a process creating a fresh `.GIT` directory is caught too.
2. **`resolve` canonicalizes every existing hop** instead of following only `is_symlink` components: this catches **junctions** (std's `is_symlink` does not see mount points — a junction out of the root was followed, top-level and nested), **8.3 short names** (canonical spelling replaces `PROGRA~1`-style hops), and normalizes on-disk casing for the resolved form. A dangling link fails closed (canonicalize error).
3. **Windows component rules** (`validate_windows_component`): colon outside a drive prefix = alternate data stream (`data.txt:stream`, `data.txt::$DATA`); trailing dot/space (Win32 drops them, so the record would lie); reserved device names with-or-without extension (`CON`, `NUL`, `COM1..9`, `LPT1..9`, `CONIN$`, `CONOUT$`, `nul.txt`). All refused as `Invalid` (fail-closed), with the reason in the error.

Tests (`tests/ntfs_paths.rs`, `#![cfg(windows)]`, all on real NTFS): the attack table below, plus a hard-link test proving the compensating control and the property fuzzer's outcome model learning the `Invalid` refusal class (`property_paths.rs:75` — a refusal, not a weakening; the Ok-branch invariants are untouched).

## 5. Commands run

`cargo test -p modbit-workspace --test ntfs_paths` — first run (before fixes): **3 passed / 6 failed** (the escape table); after fixes: **10 passed / 0 failed**. Full `modbit-workspace` suite: 6 targets, exit 0. Downstream `modbit-core --test worktrees_px` on this branch: 36/36 (exit 0). `cargo fmt --check` 0; `cargo clippy -D warnings --all-targets` 0.

## 6. Real-effect proof — the attack table (real NTFS, real links)

| Spelling / attack | Expected | Before | After |
|---|---|---|---|
| `.git/config`, `.GIT/config`, `.Git/config`, `src/../.GIT/config` | Protected | ❌ `.GIT` variants ALLOWED | ✅ Protected |
| junction `junc/evil.txt` → outside (and nested `src/deep/junc/…`) | OutsideRoot | ❌ ALLOWED (link followed) | ✅ OutsideRoot |
| 8.3 short form of the root + `\.git\config` | refusal | ❌ ALLOWED | ✅ refused (OutsideRoot/Protected) |
| `NUL`, `CON`, `PRN`, `AUX`, `COM1`, `LPT1`, `nul.txt`, `con.txt` | refusal | ❌ ALLOWED | ✅ Invalid |
| `file.`, `file `, `src/lib.rs.`, `src/lib.rs ` | refusal | ❌ ALLOWED | ✅ Invalid |
| `data.txt:stream`, `data.txt::$DATA`, `src/lib.rs:hidden` | refusal | ❌ ALLOWED | ✅ Invalid |
| `\\?\<outside>\x.txt`, `<outside>\x.txt`, `\\localhost\c$\…`, `\\?\UNC\…` | OutsideRoot | ✅ already | ✅ |
| `C:src/lib.rs`, `c:../outside` (drive-relative) | refusal | ✅ already | ✅ |
| hard link `notes.txt` ⇄ `.git/config`, write via allowed name | caught post-hoc | (untested) | ✅ snapshot diff flags `.git/config` protected and restores it |
| controls: `src/lib.rs`, `SRC/LIB.RS`, `src\lib.rs`, `./`,`../` forms, absolute plain + verbatim inside root | allowed | ✅ | ✅ |

Junction control asserts the link really reaches out before asking the policy; the hard-link control asserts the write really changed the protected file's content before the diff runs.

## 7. Failure injection

The before-run IS the injection record: six attack classes failing against the old policy (logs: `ntfs-before.log` machine-local). Additionally the property fuzzer (512 cases) found the new refusal class itself (`~/C:`) — accepted as a legitimate fail-closed outcome with the reason documented in the test.

## 8. Evidence refs

- `test:ntfs_paths.rs` (10 tests) · `test:a_junction_out_of_the_root_is_never_followed` · `test:a_write_through_a_hard_link_to_a_protected_file_is_caught_by_the_snapshot`
- `env:ntfs-8dot3-active-on-test-volume` · `env:developer-mode-symlinks`
- PR (this branch); before/after run logs machine-local under `D:\modbit-setup\work`.

## 9. Remaining gaps and blockers

1. **TOCTOU (symlink/junction swap between `check` and use)**: `check` returns a path verified at check time; a swap of a PARENT directory between check and write redirects the write. Closing it needs open-by-handle semantics (reparse-point-aware open + post-open canonical verification) in `service.rs`'s write path — an owner decision beyond this card's lexical scope; the canonicalize-walk narrows the window (every existing hop is re-verified at check time) but cannot eliminate it. Recorded as an explicit gap with the suggested fix.
2. **Hard links at check time**: invisible to a lexical policy (std has no file-ID API; detecting needs `GetFileInformationByHandle` / `windows-sys`, a dependency decision for the owner). The snapshot diff is the proven compensating control (tested).
3. `reserved_device_names`/dots/ADS rules are Windows-only by design (Unix keeps exact names legal).

## 10. Next safe action

Merge (maintainer; CI green required on all three OSes — no Unix behavior changed except the shared Glob build, which stays case-sensitive there). Then WIN-04 — process/terminal/IPC conformance against the real broker, which the WIN-02 Core ports already exercised end-to-end once (`modbit-execd.exe` must be built first).
