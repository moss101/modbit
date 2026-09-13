---
id: DR-M6-001
title: The VS Code adapter seals on the real-extension-host and real-Core halves; the live-provider run of PX-E2E-002 waits for credentials
status: accepted
date: 2026-09-13
supersedes: none
approved_by: repository owner (moss101), standing instruction "complete all tasks"; evidence-scope decision applying DR-M2-001, DR-M3-002 and DR-M3-003 (live provider proof pending credentials) to PX-002, under docs/15 "Live provider proof" and docs/82 (no simulated substrate is claimed as the live proof)
---

# DR-M6-001 — PX-002 seals on the extension-host and Core halves; its live-provider run waits

## Trigger / evidence

PX-E2E-002 ("VS Code adapter drives a real task") names a "live provider
test model" among its setup: a real VS Code extension host, a real local
Core, a fixture repository and a live model. The repository holds no provider
secrets and the build host holds no `OPENAI_API_KEY` / `ANTHROPIC_API_KEY`
(DR-M2-001, confirmed by DR-M3-002 and DR-M3-003 on 2026-09-11 and again on
2026-09-13).

Everything else PX-E2E-002 and QUAL-PX-002 name is provable here, and is:

- the real VS Code extension host (`@vscode/test-electron`, VS Code 1.104.0
  pinned) loads the adapter and drives a real local Core: a task is created
  from the editor, the built-in TypeScript language service's diagnostic is
  forwarded with revision provenance (an unsaved edit's batch is dropped by
  revision, the reverted document's is recorded), the protected effect is
  approved by the intent shown, the review is opened and accepted with a
  commit, and no workspace write originated in the adapter;
- the editor restart mid-task and the resume by cursor
  (`packages/vscode-adapter/src/adapter.test.ts` against the real Core: the
  adapter stops, its tethered Core dies, a new adapter on the same persisted
  state joins the same session from its cursor with the task still waiting,
  and resumes the suspended run);
- the negative proof (a forged decision naming another intent is refused,
  a revision-mismatched diagnostics batch is discarded with an event) and
  the static proof that the adapter has no workspace-write, tool, Git or
  secret code path (PX-001's scan covers `packages/vscode-adapter`).

What only a live model can show is that the same flow completes with a
model that chooses its own tools; the scripted wire-faithful provider plays
the model here, exactly as it does for the desktop and CLI qualifications.

## Current behavior

`packages/vscode-adapter` was an empty placeholder. No adapter existed.

## Proposed replacement

1. PX-002 ships the adapter on `@modbit/ide-adapter-core` with the
   extension-host suite and the restart/resume test above, run by
   `.github/workflows/ci.yml` (desktop-e2e job) on macOS, Linux and Windows.
2. The live-provider run of PX-E2E-002 — the same extension-host suite with
   `MODBIT_OPENAI_BASE_URL` unset and a real key — runs in
   `.github/workflows/live-providers.yml` once the owner adds the repository
   secrets, exactly as DR-M2-001, DR-M3-002 and DR-M3-003 defer theirs. Its
   first green run is appended to PX-002's `evidence.json`.
3. PX-002 seals on item 1 with its task card stating the open item.

## Migration

None: a new package, an additive CI step, `ResolveApproval.intent_hash`
already landed with PX-001.

## Compatibility

No interface changes meaning.

## Security impact

None beyond DR-M2-001: the live workflow receives secrets through
repository secrets read only by the Core process; the adapter never sees
them (the static scan forbids the credential names in client code).

## Test impact

`packages/vscode-adapter/src/adapter.test.ts`, `packages/vscode-adapter/test/suite.ts`
(extension host), the PX-001 static scan over the adapter. The live half is
the one deferred to item 2.

## Rollback

Revert the commits that introduce the package and the CI steps. Nothing on
the log changes shape.
