# Task Card — EPR-018 Isolated Non-Committing Reviewer environment

## Identity

- Task ID: EPR-018
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EPR-018; owner label: effects-security; subsystem: terminal broker (`modbit-execd`), core (`review_env.rs`), protocol
- Qualification: QUAL-EPR-018 — the disposable review environment is real: sandboxed processes, ephemeral scratch writes, canonical tree read-only, deny-default network/secrets, cleanup that kills, revokes and disposes; unsupported hosts admit none.
- Evidence tier: real-system (a real terminal broker confining real processes with the host OS sandbox; real git worktrees; the real Core)

## Goal

Extend existing local worktree/execution/capability owners with disposable review environment: canonical tree read-only, bounded sandbox processes and ephemeral scratch writes permitted, deny-default network/secrets, no canonical mutation/Git commit/push/deploy/persistent/external actions. Default context excludes solver hidden reasoning; cleanup revokes/kills/disposes resources.

## Existing-code audit

- classification: PARTIAL before: the `review_isolated` profile existed (a lease without `git.worktree`) but no environment was provisioned, no process was confined and nothing was disposed.
- production entry points: `services/modbit-execd/src/broker.rs::review_sandbox` (probe; wrap with `sandbox-exec` / the broker's own seccomp launcher `modbit-execd --review-sandbox-exec --` (`seccomp_net.rs`); env cleared and secret-like keys stripped; `SANDBOX_UNAVAILABLE`), `ProbeSandbox` / `SandboxProbed` and `SessionInfo.cwd` in `exec.proto`, `crates/terminal` (`probe_sandbox`, `Event::SandboxProbed`), `services/modbit-core/src/review_env.rs` (`admit`, `find`, `dispose`), `server.rs` (`AdmitReviewEnvironment`, `DisposeReviewEnvironment`), `crates/domain` (`TaskOrigin::Review`, `ReviewEnvironmentAdmitted` / `Disposed`).
- proof: on macOS (seatbelt) and Linux (seccomp-net) a candidate task gets a review environment: a scratch worktree at HEAD on `modbit/review-<env>`, a review task under `review_isolated` carrying no fork/capsule/context pack, a lease reading the canonical tree and writing only the scratch tree; a write lands in the scratch `a.txt` while the canonical one is untouched, a write named by canonical path is refused, a process asked to inherit the environment sees neither `OPENAI_API_KEY` nor `MODBIT_TEST_SECRET`, a loopback connection to the model server is `DENIED`, and disposal kills the 120 s sleeper (`killed ≥ 1`, seen exited from the candidate's listing), removes the worktree and branch, cancels the review task, revokes the lease (a later read is refused) and records it all on the candidate's log; on Windows admission refuses `SANDBOX_UNAVAILABLE` with nothing taken.

## Limitations

- Linux confines network only (seccomp filter: `socket(2)` refused for every family but `AF_UNIX`, `io_uring_setup(2)` refused, `no_new_privs`); write confinement there is the lease's (macOS confines both); scratch-tree `git commit` is possible inside the disposable worktree and disposed with it (no push: no network, no `git.write` capability).
- The first build wrapped Linux processes in `unshare -rn`; hosted Ubuntu 24.04 refuses unprivileged user namespaces (`apparmor_restrict_unprivileged_userns=1`, no `unshare` profile: `write failed /proc/self/uid_map: Operation not permitted`), so the probe answered `SANDBOX_UNAVAILABLE` on the Linux runner and the test failed there (run 34734605112). The seccomp launcher replaced it: no namespace, no helper binary.


## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_epr_018_the_review_environment_is_sandboxed_confined_and_disposed`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
