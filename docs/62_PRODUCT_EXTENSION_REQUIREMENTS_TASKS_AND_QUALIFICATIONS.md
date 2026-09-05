# Product extension requirements, tasks and qualifications

Additive ledger authorized by DR-PX-2026-09-05 (`07_PRODUCT_EXTENSION_DECISION_RECORD.md`). It never modifies the 291 base rows or the EPR ledger. `tools/dossier_px.py` parses it into the same graph node types as the base and EPR ledgers; it pins no count, so effective totals are whatever the rows sum to, and `tools/check_dossier.py` D9 enforces structure: contiguous IDs from 000, one task card and one `PX-E2E` scenario per ADOPT row, one qualification per row with the same owner, and DEFERRED rows with no task, milestone or prerequisites. Rows are appended by stage; earlier rows are never edited except by a new Decision Record.

Columns: requirement, title, task, qualification, canonical owner, milestone, release (ALPHA, BETA, RELEASE_ZERO or POST_ZERO for DEFERRED rows), disposition, prerequisites that must be COMPLETE first. A task joins the release named in its row and every later release; readiness is derived in `75_PHASED_RELEASE_PLAN_AND_READINESS.md`.

## Ledger

| Requirement | Title | Task | Qualification | Owner | Milestone | Release | Disposition | After |
|---|---|---|---|---|---|---|---|---|
| REQ-PX-000 | Headless CLI thin client for the task lifecycle | PX-000 | QUAL-PX-000 | desktop | M2 | ALPHA | ADOPT | M1.3,M2.7 |

## Qualifications

| Qualification | Requirement | Owner | Real qualification | Failure and negative proof |
|---|---|---|---|---|
| QUAL-PX-000 | REQ-PX-000 | desktop | Real Core plus the CLI process on a fixture repository: create a task, stream events by cursor as JSON lines, answer a question, approve one protected effect, kill and restart Core, resume by cursor; exit codes match the documented contract and exactly one effect receipt exists | Kill the CLI mid-stream and reconnect: no duplicate command, no duplicate effect. A forged command without the boot secret is rejected. The CLI binary contains no provider, filesystem, Git or policy code path: static dependency check and runtime tracing both prove every effect went through Core |

## Task cards

<a id="px-000"></a>

## PX-000 — Headless CLI thin client for the task lifecycle

- **Requirement:** REQ-PX-000; **related preserved requirements:** REQ-EV-0010.
- **Owner / milestone / release:** desktop / M2 / ALPHA; **prerequisites:** M1.3, M2.7.
- **Scope and acceptance:** `modbit` command-line client speaking the authenticated local SurfaceProtocol: create task, steer, pause, resume, cancel, respond to questions, approve or deny effects, subscribe to events from a cursor, read OutputRefs and artifacts, print JSON lines and human text, and return documented exit codes. It is a thin client: no orchestration, context, memory, Git state, recovery, policy or tool execution lives in it, and it holds no provider credentials. Headless use in scripts and CI is the design target; the desktop app and the CLI are interchangeable clients of one Core.
- **Production wiring:** reuse the desktop's protocol client library (`packages/surface-protocol`) or a Rust twin of it; authenticate exactly as Electron main does; no new Core command types beyond those in `30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md`.
- **Real qualification:** QUAL-PX-000 / PX-E2E-000.
- **Failure and negative proof:** as in QUAL-PX-000; additionally cancellation from the CLI reconciles in-flight tools through the ordinary cancellation domains.
- **Evidence:** build digest, Core and CLI revisions, event cursor ranges, effect receipt id, exit codes, and the dependency check output under the existing evidence rules.
- **Completion:** production-equivalent real proof and no unresolved acceptance criteria; graph.py is the only status writer. Product status is NOT_STARTED at adoption.

## Real-system scenarios

### PX-E2E-000 — Headless CLI task lifecycle

**Setup:** packaged Core, CLI binary, real `ts-webapp` fixture, live provider test model.  
**Action:** from a shell, create a coding task, follow events, answer the agent's question, approve the protected write, then `SIGKILL` Core and resume following from the last cursor.  
**Pass:** the task reaches ReadyForReview with real tests passing; exactly one effect receipt; the CLI exit code is 0 on success and documented non-zero on cancel, denial or failure; no duplicate command after reconnect; the CLI process never opened the repository, a provider endpoint or the policy store directly.
