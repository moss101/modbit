# Parallel Agent Coordination Rules

## Ownership

Each active task has one primary writer for a canonical subsystem. Other agents may research/test but do not concurrently redesign the same owner boundary.

## Isolation

Use independent worktrees/branches. Shared database migrations, event schemas, policy rules and protocol definitions require serialized ownership or explicit coordination because textual merge success does not imply semantic compatibility.

## Admission

Before spawning parallel work, verify dependencies are stable enough to avoid agents implementing incompatible temporary contracts. Broad parallelism is forbidden on an unstable Core/event/policy foundation.

## Merge

Merge is a transaction: rebase/refresh, run semantic conflict checks, rerun affected integration/E2E tests, verify migration order and event compatibility, then update manifest. Never merge only because Git reports no textual conflict.

## Agent-to-agent claims

A child/peer agent's statement that something is implemented is untrusted until backed by code path + test/evidence references.
