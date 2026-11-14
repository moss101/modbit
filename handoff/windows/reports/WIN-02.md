# WIN-02 — Close the `cfg(unix)` gap: report

## 1. Card / graph tasks / requirements

- Card: WIN-02 — port/complete the Unix-gated conformance suites on Windows; serves PX-030 (conformance suites per `docs/76`), touches PX-067 hardening coverage and PX-111 persistence-kill coverage.
- Branches (one per crate group, off `main`): `wip/win-02-retrieval` @ `e4d1b59f` (PR #208), `wip/win-02-workspace` @ `2d346f24` (PR #209), `wip/win-02-git` @ `e7f15516` (PR #211), `wip/win-02-core` (this branch; code + this report).

## 2. Machine

Per `WIN-00.md` (same machine; Developer Mode on, so unprivileged symlink creation works — the ported tests rely on it and fail loudly without it).

## 3. Audit

Regenerated inventory: **78 files** carry `cfg(unix)`/`cfg(not(windows))` gates (`rg -n "cfg\((unix|not\(windows\))" crates services apps`). Read `git log --grep=windows` for prior ports (PX-082/115/052, hooks-on-Windows) to avoid redoing them. Per-group audits classified every handoff-named site as PORTABLE / ANALOGUE / UNIX-ONLY; classifications drove the ports below. Key unlock: **Git for Windows runs shebang hooks and shell-evaluated config commands through its bundled sh without the executable bit** — the chmod-based gating was the only thing keeping the entire FIX-01 hardening matrix off Windows.

## 4. Change

**Production (2 fixes):**
- `crates/retrieval/src/persist.rs` — `sync_dir` Windows arm: open the directory with `FILE_FLAG_BACKUP_SEMANTICS` and `sync_all()`, so the atomic blob/manifest rename is explicitly durable like Unix's fsync (was a silent no-op).
- `crates/workspace/src/snapshot.rs` — `restore_one` Symlink arm on Windows: recreate the link (`symlink_dir`/`symlink_file` by probing the target's kind, dangling → file link). Previously an error stub: a protected symlink destroyed by a process could never be restored on Windows.

**Test ports:** retrieval (`persisted_indexes` fault-injection kill test with the verified `STATUS_STACK_BUFFER_OVERRUN (0xC0000409)` fastfail analogue), prompt-compiler (symlink conformance ungated with house-style `link` helper), workspace (atomic-rename failure via read-only target; protected-paths restore; symlink-swap; NEW destroyed-symlink-restore test), git (arm/script helpers portable; native `.cmd` gpg trap — CreateProcess cannot exec shebang scripts; ssh/credential/include-filter/neutralized-report tests), tools (`git_hardening` symlink + hooks tests), core/worktrees (px_065 timeout via real killed `ping -n 61`; px_065 removal link-follow; px_067 hostile repository through the real Core; `find_bytes` ungated; `memory_graph_index` sigkill via `taskkill /F /PID`).

**Test-environment fixes:** `px_067_remaining`'s `plain()` now sets `GIT_CONFIG_NOSYSTEM=1` + empty `GIT_CONFIG_GLOBAL` — the machine's system `credential.helper=manager` (Git Credential Manager) answers a 401 headless, blocks minutes and starves the repo's own helper of the control sentinel. Hostile-repo verification command's `core.hooksPath=/dev/null` became an empty temp dir (`/dev/null` is not a Windows path).

## 5. Commands run (per group; all real exit codes, no pipe-eaten codes)

| Group | Gates | Suite |
|---|---|---|
| retrieval | fmt 0 · clippy `-D warnings` 0 | `modbit-retrieval` + `modbit-prompt-compiler` all targets green (kill test 53s; persisted_indexes 115s) |
| workspace | fmt 0 · clippy 0 (one `permissions_set_readonly_false` allow with justification) | `modbit-workspace` 5 targets green |
| git | fmt 0 · clippy 0 | `modbit-git` 46 tests / 7 targets + `tools::git_hardening` 5 green (credential test 1.22s after GCM isolation; was a 5-minute stall) |
| core | fmt 0 · clippy 0 | `worktrees_px` **39/39** (154.6s) + `memory_graph_index` 20+1 ignored (23.6s) |

## 6. Real-effect proof

Every ported test drives a real boundary: real dying child processes (fastfail/taskkill), real symlinks, real read-only attributes, real Git for Windows running (control) and not running (negative) repository-owned programs, real local HTTP-401 credential challenge, real Core + real execd broker end-to-end (px_067), real killed setup command (px_065), real mid-write process kills (px_111).

## 7. Failure injection (red/green, all mutations restored and re-verified green)

1. prompt-compiler containment `if false && !starts_with(repo)` → ported symlink test FAILS (escape caught).
2. snapshot restore stubbed back → destroyed-symlink test FAILS (unrestored).
3. `harden.rs` `core.fsmonitor=false` commented out → crate-level armed test FAILS with exactly `RAN_fsmonitor`; same mutation caught **through the real Core** by `px_067_a_hostile...` (14.2s).
4. Empirical exit-code probes: `abort()` = `0xC0000409` via both a standalone rustc probe and the real test child (the audited `0xC0000609` was wrong).

## 8. Evidence refs

- PRs: `#208`, `#209`, `#211`, this branch's PR; commits `e4d1b59f`, `2d346f24`, `e7f15516` + this branch head.
- `test:px_067_a_hostile_repository_runs_nothing_and_leaks_no_token_through_core_operations` · `test:px_111_sigkill_during_repeated_refreshes_never_leaves_an_unusable_index` · `test:a_process_killed_inside_a_checkpoint_leaves_a_whole_snapshot_and_the_answers_stay_right`
- `env:git-for-windows-bundled-sh-without-exec-bit` · `env:developer-mode-symlinks`

## 9. Remaining gaps and blockers

1. **UNIX-ONLY, justified, left in place:** `crates/sandbox/src/backend/mod.rs:17` microVM (Firecracker/KVM, Linux cloud substrate — handoff records it); double-quote filename fragments (`hardening.rs:80,97`; `"` illegal in Win32 names); exec-bit/mode snapshot fidelity (`mode_of`/mode restore — no Windows concept; read-only-attribute fidelity is a semantic extension for the owner).
2. **Deferred to their owning cards (audited, not WIN-02 scope):** `services/modbit-core/src/server.rs` production analogues (`write_owner_only` DACL, `owner_is_gone` pid probe, `process_command` richer ps, `orphaned_tethered_core` needs job objects) — WIN-04; `surface_protocol.rs:15318` node_modules link under CI privilege caveats — WIN-06.
3. **Testability nit for the maintainer:** when `modbit-execd.exe` is not built, Core tests that spawn the broker fail with a misleading `exited None` fallback instead of surfacing the missing binary (observed: `terminal broker unavailable … os error 2` on stderr only). A typed broker-unavailable error would have saved a diagnosis cycle.
4. Hosted CI on the four PRs is the pass/fail authority for the other two OSes; no PR touches non-Windows behavior (all changes are `cfg(windows)` additions or test-env isolation local to Windows runs).

## 10. Next safe action

Merge the four WIN-02 PRs (maintainer; CI green required), then WIN-03 — the NTFS path/symlink policy attack table (reserved names, trailing dots/spaces, 8.3 names, ADS, `\\?\`, drive-relative, junctions, TOCTOU), which builds directly on the workspace path-policy conformance now running on Windows.
