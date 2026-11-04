# Task Card — PX-048 Typed apps panel (Terminal scroll-back and per-task state)

- Status set: REAL_TESTING (not COMPLETE). Merged in #132 (374abcb8).
- Tier: release-critical (touches the preload bridge, a new main-process IPC call).

## Built
- Older terminal output loads by OutputRef range through `terminal:history` (hash read from the Core's own listing, 256 KiB cap), 64 KiB ranges in cursor order.

## Proven (real Electron + real Core, PR run 40086967864)
- terminal-history.spec.ts: live stream of a paced build; scroll-back reads OutputRef ranges; pane + view equals the Core's live stream from the oldest retained cursor and the sealed object by cursor (all three OSes).
- apps-panel-state.spec.ts: per-task tab state across selection and restart. apps-browser.spec.ts: lease badge generations and takeover. Existing: apps-panel, terminal-renderer-restart, changes-app.

## Not proven (why this stays REAL_TESTING)
1. A user terminal accepting typing only with policy: the Core has only agent- and host-owned terminals; the panel cannot list a user terminal. Needs a spec/owner decision or a user-terminal feature.
2. QUAL-PX-043 wording says agent terminals are read-only to clients; the shipped lease model (PX-099) lets the person take the input lease. Tests cover the lease behaviour.
3. Windows: exact line order is not asserted; the Windows pseudo-console dropped line 18752 from the Core's own stream once (run 40072387738). Open question whether this is ConPTY or the execd Windows path.
4. History for a still-running terminal is available only after the process ends (no sealed object before exit).
5. The preload pin was re-pinned for the read-only terminalHistory call; the pin change itself has had no human review.
6. Prerequisite PX-043 is COMPLETE, but its px_043 flake (extra newline in the PTY log) was fixed by #133 and remains unproven until CI stays green.

## Fix: reading the change set no longer changes the index (2026-11-05)

Status unchanged: REAL_TESTING, for the six reasons above, none of which this touches. Tier: release-critical (the read sits behind every Changes and review view, and the fix moves what a review accept stages).

- **Bug.** `review::candidate` ran `git add -N` on untracked files in the person's checkout. `GetChangesState`, `GetReviewBundle`, `GetCodeView`, `RevertChanges` and `DecideHunk` reach it, so opening the Changes panel marked new files intent-to-add; a later `git clean -fd` reported success and left them, and a reverted new file stayed behind as ` D path` in `git status`. Found by an e2e test of PR moss101/modbit#169 (DR-PX-2026-11-04-020 section 12).
- **Classification on audit:** BROKEN-DRIFTED. First missing link: the read owned no index of its own.
- **Fix.** `Repo::diff_worktree_with_untracked` computes the same diff in a throwaway copy of the index. `Repo::status` runs with `GIT_OPTIONAL_LOCKS=0`, so a read does not refresh the real index's stat data either.
- **What depended on the old entry, now explicit.** `changes::state` reports an untracked file as `unstaged: "A"`, as before. `review::decide` no longer asks git to stage a new file whose every hunk was rejected: with no index entry `git add -A -- <path>` exits 128.
- **Behaviour that changed on purpose.** Accepting with the only new file rejected now completes without a commit; it used to delete the file and then fail on `git commit`. A failed `add -N` is an error (GIT), not a diff that silently lacks the new files; a file gone since the status is left out. `git.diff` and the retrieval changed-lines read no longer see a new file merely because a panel had opened: they now always answer plain `git diff HEAD`, as they did for a task whose panel never opened.
- **Proven, real git and real Core** (`read-leaves-index/tests.log`): the read leaves the index byte-identical, `git status --porcelain` and `ls-files --stage` unchanged, with a stat-dirty file present; `git clean -fd` then removes the new file and directory; digests, revisions and staged columns equal those under a real `add -N`; the diff is byte-identical to a real `add -N`; linked worktree; 4000 untracked paths; the three specs in the real app, and PR #169's versions of them on this Core.
- **Faults injected:** a corrupt index fails the read and leaves the file as it was with no temporary left behind; a file vanishing between status and diff. `read-leaves-index/mutations.log` restores each old behaviour in turn and the tests fail on each.

### Not proven or not done
1. Linux and Windows: CI of this change's PR, not run locally.
2. Index entries the old read already left in a checkout stay until the person clears them: they cannot be told apart from a person's own `git add -N`, so the Core does not touch them. `git clean` skips those files until then.
3. A Core killed between copying the index and deleting the copy leaves one `modbit-diff-index-*` file in the git directory. Nothing reads it; the same holds for `modbit-snapshot-index-*`. Not cleaned up.
