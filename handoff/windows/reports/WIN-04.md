# WIN-04 — Process, terminal and IPC conformance: report

## 1. Card / graph tasks / requirements

- Card: WIN-04 — broker conformance against `docs/21`: ConPTY, process-tree kill, durable broker survival, named-pipe security, listener discovery, shell quoting, with failure injection. Serves PX-030; REQ-EV-0221 (cancel ends what a command started), REQ-EV-0100/0025, PX-099.
- Branch: `wip/win-04-broker` off `main` @ `f08378ac`.

## 2. Machine

Per `WIN-00.md`. Windows 11 26200; English-locale `netstat` (the localized failure is tested with real German/French samples, see §7).

## 3. Audit

A read-only audit of all six card items against the tree (subagent report, machine-local) found: ConPTY interactive + acked writes + windowed replay already cross-platform; **resize skipped on Windows**; **ANSI/DSR untested** (the broker's Windows-only cursor-reply code had zero tests); **PTY cancel never tested**; **no Job Objects anywhere** — Windows PTY and timeout kills only killed the direct child; **no grandchild/survivor tests on any platform**; durable broker recovery (`qual_m4_5...`) already runs on Windows; **no pipe DACL** (default token-derived descriptor); **localized netstat silently yields zero listeners** (English `LISTENING` literal); quoting fully structured (proven by `hostile_arguments...`). First missing links: tree-kill coverage and the pipe descriptor.

## 4. Change

All inside the owner (`services/modbit-execd`):

1. **`src/job.rs` (new)**: Windows Job Object confinement — every spawned session child is assigned to a job with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`: the tree ends at cancel/timeout/exit by `TerminateJobObject`, and when the broker dies at all, by the kernel closing the job's last handle (the Windows answer to the Unix process group; no second process manager). Unsafe calls carry SAFETY notes under the workspace's documented exception (as `kill_group`).
2. **`src/broker.rs`**: `Session.job` slot; confine at both spawn paths; terminate in `kill()` and in `finish()` (natural exit ends survivors too; idempotent via `take`). **Empirical finding, coded around**: `AssignProcessToJobObject` on a ConPTY child is refused with `ACCESS_DENIED` on this host (reproducible; loud stderr diagnostics retained), so the **PTY cancel arm and the timeout branch now end the tree with `taskkill /T /F`** exactly like the piped arm — the survivor hole for PTY sessions is closed job-lessly.
3. **Pipe security**: `Listener::bind` creates the pipe with an **explicit owner-only DACL** (SDDL `D:P(A;;GRGW;;;OW)(A;;GRGW;;;SY)(A;;GRGW;;;BA)`) — the Windows analogue of the Unix socket's `0o600` — instead of the default token-derived descriptor.
4. **`src/listeners.rs`**: `parse_netstat_fail_closed` — a localized state column (German `ABHÖREN`, French `ÉCOUTE`) is now a **typed error** instead of silently zero listeners; wired into the Windows discovery path.
5. **`examples/execd_child.rs`**: roles `spawns-grandchild-and-exits` / `spawns-grandchild-and-runs` (a DETACHED_PROCESS grandchild with null stdio — only the session's containment reaches it), `sleep-until-killed` (bounded), `dsr-probe` (raw-mode DSR query with verdict), and a Windows arm for `stty-size` (exact `GetConsoleScreenBufferInfo` window rectangle).

## 5. Commands run

`cargo test -p modbit-execd` — full suite, 6 targets, exit 0 (includes 19 broker tests + terminal_flow + listeners). `cargo fmt --check` 0 · `cargo clippy -D warnings --all-targets` 0. Cargo.lock delta: +1 line (windows-sys edge for modbit-execd; version already in the tree transitively).

## 6. Real-effect proof (all on real Windows boundaries)

| Property | Proof |
|---|---|
| Clean exit leaves no survivor | `win04_a_detached_grandchild_dies_with_the_session_even_after_a_clean_exit`: a detached grandchild of an exited command is gone within grace (job terminate at finish + KILL_ON_JOB_CLOSE) |
| PTY cancel leaves no survivor | `win04_cancelling_a_pty_session_leaves_no_survivor_grandchild` (cross-platform): ConPTY child + detached grandchild, cancel → both gone |
| ANSI/DSR | `win04_the_broker_answers_a_cursor_position_query_for_a_conpty_child`: the child's `ESC[6n` is answered by the broker's Windows-only `ESC[1;1R`, read in raw console mode through the real ConPTY |
| Resize through a real child | `px_099_resize_changes_the_size_the_child_reads` now runs on Windows (30×100 → 55×200 via `GetConsoleScreenBufferInfo`), no longer skipped |
| Pipe descriptor | `win04_the_brokers_pipe_descriptor_is_owner_only`: the live pipe's DACL queried via `GetNamedSecurityInfoW` — owner/SYSTEM/Admins grants only; no `WD`/`AN`/`AU` |
| Localized netstat | `win04_a_localized_netstat_fails_closed...`: English parses, header-only is empty, German/French are typed errors |
| Durable survival | pre-existing `qual_m4_5_a_killed_broker_comes_back...` (ungated) green on Windows |
| Quoting | pre-existing `hostile_arguments...` + `qual_ev_0100` green on Windows (structured argv, no concatenation anywhere in the broker) |

## 7. Failure injection

1. **PTY tree kill disabled** (mutation) → the PTY-cancel survivor test fails with `grandchild N survived the PTY cancel` (caught); restored → green.
2. **Job confinement disabled** (mutation) → the clean-exit test *passed anyway* through a slow path (~100 s): the observable is muddied because the exit observation itself waits on pipe EOF the survivor's sibling handles. Recorded honestly: the job's clean-exit kill is defense-in-depth (the explicit red/green for the containment code path is the PTY mutation above); root of the slow path not fully identified.
3. ConPTY job assignment fails with `ACCESS_DENIED` (observed live, twice, with the diagnostics kept) — that is why the PTY/timeout arms use `taskkill /T`.
4. DSR first run: the reply arrived but was swallowed by cooked-mode line input (echoed as `^[[1;1R`) — the role now reads raw; a silent terminal is named, never a hang.
5. First DACL draft (GW only) made even the owner's client fail with `ACCESS_DENIED` — the tests caught it; grants are now `GRGW`.

## 8. Evidence refs

`test:win04_a_detached_grandchild_dies_with_the_session_even_after_a_clean_exit` · `test:win04_cancelling_a_pty_session_leaves_no_survivor_grandchild` · `test:win04_the_broker_answers_a_cursor_position_query_for_a_conpty_child` · `test:win04_the_brokers_pipe_descriptor_is_owner_only` · `test:win04_a_localized_netstat_fails_closed_instead_of_reporting_no_listeners` · `test:px_099_resize_changes_the_size_the_child_reads` · `env:windows-11-26200-conpty-job-assign-access-denied`

## 9. Remaining gaps and blockers

1. **ConPTY children cannot be job-assigned on this host** (ACCESS_DENIED, empirical): PTY trees are ended by `taskkill /T` at cancel/timeout; if the broker is hard-killed with live PTY sessions, PTY-tree cleanup relies on ConPTY handle closure ending attached children — not proven here (broker-level recovery is; the process-level orphans were not observed surviving in `qual_m4_5`, which runs PTY-less). Suggested owner follow-up: CREATE_SUSPENDED + assign + resume via a patched portable-pty, or a watchdog.
2. **Cross-account behavioral test** (a real second local user or a restricted-token process connecting): needs owner decision/account (handoff anticipates `runas`); the descriptor itself is asserted instead.
3. **Core-level kill/reattach** (`px_099_killing_the_core_mid_stream...`) remains `#[cfg(unix)]`; the broker-level durable-recovery test covers the Windows transport today. Port follows the WIN-02 pattern when scheduled.
4. `execd.ready` has no owner-only ACL on Windows (Unix: 0600) — same SDDL treatment applies; left for a follow-up with the file-store code.
5. px_148/px_150_152 suites remain unix/macos-gated (review-sandbox: seatbelt/Linux namespaces) — out of Windows scope by design.

## 10. Next safe action

Merge (maintainer; CI green on all three OSes required — Unix behavior unchanged: confine is a no-op there, the PTY/timeout taskkill arms are `cfg(windows)`). Then WIN-05 — Git on Windows hardening beyond what WIN-02 already landed (autocrlf round-trips, `safe.directory`, case-colliding names, locked worktrees).
