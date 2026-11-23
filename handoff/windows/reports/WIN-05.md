# WIN-05 — Git on Windows: report

## 1. Card / graph tasks / requirements

- Card: WIN-05 — git hardening against real Git for Windows: `core.autocrlf` effects on diff/patch round-trips, `safe.directory`, `core.symlinks`/long paths, case-colliding branch names and files, worktree create/merge/cleanup with a locked file. Serves PX-030 / PX-067.
- Branch: `wip/win-05-git` off `main` @ `f08378ac`.
- Scope note: the hook (`​.bat`/`.cmd`/extensionless-sh), `core.fsmonitor`, `core.hooksPath`, `core.sshCommand`, `include.path`, attributes/filters, and `credential.helper=manager` conformance landed in WIN-02's git group (PR #211) — this card covers the remainder.

## 2. Machine

Per `WIN-00.md`. Git for Windows (bundled sh; 8.3 names active; NTFS case-insensitive).

## 3. Audit

- `core.autocrlf=false` was pinned only at **init** time (`lib.rs:352`); the per-command hardening (`fixed_config`) did not pin `core.autocrlf`/`core.eol`, so a repository's own config could keep line-ending rewriting live under every typed command.
- `safe.directory` is never widened with a typed `UnsafeRepository` mapping on git's "detected dubious ownership" (`harden.rs:377`, `lib.rs:127`) — mapping existed, untested.
- Case collisions: no tests anywhere; the apply plan's conflict machinery had never met a case-colliding add.
- `worktree_remove` was a bare `git worktree remove --force`: on a locked file it fails **after deleting some files and after pruning the registration** — the exact half-deleted worktree the card forbids.

## 4. Change

1. **`harden.rs`**: `fixed_config` now pins `core.autocrlf=false` and `core.eol=lf` per command — the repository's line-ending config can never rewrite what the runner commits (content fidelity: digests, diffs and patch round-trips see the bytes on disk).
2. **`lib.rs`**: `worktree_remove` does a **rename-first pre-flight**: moving the whole tree one name aside fails while any file inside is held without share-delete (verified on NTFS), so the refusal — a new typed `Error::WorktreeHeld` — happens **before anything is deleted** and the worktree stays registered; the retry after the lock is gone succeeds. (An attempted `worktree add`-based re-registration of git's residue proved impossible — git refuses existing directories even forced — which is why the pre-flight exists.)
3. **Tests** (`crates/git/tests/win05_windows.rs`) + a unit test for the dubious-ownership mapping inside `harden.rs`.

## 5. Commands run

`cargo test -p modbit-git`: 8 targets, exit 0 (includes the 4 new WIN-05 tests + hardening + px_067 + apply_back). Downstream: `modbit-tools git_hardening` 4/4; `modbit-core worktrees_px` 36/36 (171s) with the new removal semantics. `cargo fmt --check` 0 · `clippy -D warnings --all-targets` 0.

## 6. Real-effect proof

| Property | Proof |
|---|---|
| Repo-controlled `core.autocrlf` never rewrites stored bytes | a hostile repo sets `core.autocrlf=true`+`core.eol=crlf`; a CRLF file committed through the typed runner is stored **byte-exact** (`cat-file` compared) |
| Case-colliding branch names refused typed | `Feature` then `feature`: typed `Git`/`InvalidRef` error, existing branch untouched |
| Case-colliding file add refused, nothing half-applied | candidate tree adds `FILE.TXT` over the checkout's `file.txt` (staged via `hash-object`+`update-index`, so the collision is purely in the plan): the plan marks it `Unsupported`/`Conflict`, never a silent overwrite |
| Locked worktree cleanup | a real exclusive handle (`share_mode(0)`) on a worktree file: typed `WorktreeHeld`, **the locked file and its directory are retained**, registration intact, retry after unlocking removes everything |
| `safe.directory` | never widened; the typed `UnsafeRepository` mapping is proven against git's real refusal words (unit test; a live cross-owner repo needs a second account — owner decision) |

## 7. Failure injection (red/green)

1. Autocrlf pin removed (mutation) → the fidelity test fails with exactly the rewritten blob (`line one\n` where CRLF was written); restored → green.
2. `worktree_remove` reverted to plain git remove (mutation) → the locked test fails with git's generic halfway error (and, per observation, a pruned registration and partial deletion); the rename-first fix → green.
3. Pre-flight design probe: an exclusively-held file blocks renaming its parent directory on NTFS (verified out-of-band) — the property the pre-flight relies on.

## 8. Evidence refs

`test:a_repository_autocrlf_never_rewrites_what_the_runner_commits` · `test:case_colliding_branch_names_are_refused_not_created` · `test:a_case_colliding_add_is_refused_and_nothing_is_half_applied` · `test:a_locked_worktree_fails_cleanup_typed_and_retains_the_worktree` · `test:dubious_ownership_is_detected_in_gits_real_words` · `env:git-for-windows-bundled-sh`

## 9. Remaining gaps and blockers

1. `safe.directory` live cross-owner behavior (a real second local account or a restricted-token repo) — owner decision per the handoff's `runas` note; the mapping and the never-widen rule are tested.
2. `core.symlinks` interplay (git writing link files vs smudge text when false) — the apply layer refuses link changes outright (WIN-02), which dominates; nothing further observed failing.
3. Long-path worktree operations are covered by the repo-level `core.longpaths` config from WIN-00 plus the existing suites; no dedicated 260+-char test added (a gap, low risk given the config).

## 10. Next safe action

Merge (maintainer; CI on all three OSes — the pin and the pre-flight are cross-platform; on Unix the pre-flight rename is equally safe). Then WIN-06 — the desktop shell catalog on Windows, which inherits the F2 DPI finding and decides the DPI-aware shellDrift gate with the owner.
