# Automations specification (clean-room)

> **Authority:** DR-PX-2026-10-03-010 (`decisions/DR-PX-2026-10-03-010-automations.md`), status **accepted** on 2026-10-05 (owner instruction, goal: implement research/audit/01-TASK-LIST.md). Rows PX-082..PX-086 in `62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md` are NOT_STARTED and none may start before the record is accepted and DOC-PX-010 is COMPLETE.  
> **Nature:** a product specification in Modbit's own words and design. Provenance (clean-room posture), the four verification tags (LIVE, DISK, STATIC, UNVERIFIED) and the evidence key are those of `65_AGENT_FIRST_WORKSPACE_SPECIFICATION.md` sections 2, 3 and 12 and are not repeated. It records functional facts and says how well each is known; it is not an implementation and not proof that anything exists.

## 1. Purpose

Every Modbit task today starts because a person asked. Real engineering work also starts on a schedule or on an event: a nightly check, a pull request opened, a CI run that failed. The owner has made automations a real goal. Modbit has two sealed constraints on it: MOD-AUTO-001 and REQ-EV-0149 and REQ-EV-0264 defer recurring and triggered work, and doc 81 forbids a second scheduler, policy engine, approval system or effect ledger. This document specifies automations as definitions that make the existing Core create ordinary tasks: the existing Scheduler admits them, the existing loop runs them, the existing approval aggregate and effect ledger govern them, all under a principal, a policy ceiling and a budget, with unattended runs failing closed. A model can never create, schedule or enable an automation.

## 2. Constraints and ownership

No canonical subsystem is added. The renderer holds no authority and every effect goes through the Core (doc 81, REQ-EV-0076). Decision: `MOD-AUTO-002` in `02_AUTHORITY_AND_DECISIONS.md` (PROVISIONAL; the record was accepted on 2026-10-05). Common failure, cancellation, idempotency and restart semantics are those of doc 65 section 8.

| Row | Owner | Tier |
|---|---|---|
| PX-082 Automation definitions: versions, principal, validation, enable approval and repository-supplied definitions | automation | release-critical |
| PX-083 Trigger evaluation and dispatch inside the existing Scheduler: time source, idempotency, concurrency, missed runs and budgets | core-runtime | release-critical |
| PX-084 Unattended run policy: principal ceiling, parked approvals with expiry, injection handling and kill switches | effects-security | release-critical |
| PX-085 Cloud triggers: signed webhooks, replay protection, tenant mapping and the cloud time source | sandbox-cloud | release-critical |
| PX-086 Automations surface and CLI: list, editor with validation, enable approval, test run, run history and kill switches | desktop | release-critical |

## 3. Existing implementation audit

Audit at `main` 5fdb47f, classes of doc 93.

| Area | Classification | What exists | First missing link |
|---|---|---|---|
| Scheduler and task admission | **PRODUCTION-WORKING** | `crates/core-runtime` Scheduler, capacity tickets (M6.2), transactional admission, durable queue and restart reconciliation; `CreateTask` with origin metadata (`desktop`, `cli`, `ide_adapter`, `forge_issue`, `forge_webhook`). | Reused unchanged as the only admission path. |
| Forge webhook intake in the cloud | **PRODUCTION-WORKING** | PX-011: the Cloud API verifies signature and policy, rejects replays and creates the same canonical task for the tenant (`apps/cloud-api`); PX-008 and PX-009 consume review comments and CI results. | Reused for event intake; extended with automation matching. |
| Time-based triggers | **NOT-FOUND** | No cron or timer source exists in the Scheduler or the Cloud API; REQ-EV-0149 and REQ-EV-0264 are DEFERRED and the `automation` subsystem has no crate or milestone. | A time source inside the existing Scheduler (PX-083). |
| Unattended execution policy | **IMPLEMENTED-PARTIAL** | Detached agents have a permission ceiling (REQ-EV-0046, M6.7) and background children park at boundaries; the approval aggregate, intent hashes and receipts are production-working; headless questions fail closed to Needs Attention (PX-040). There is no principal model for a definition-created run and no expiry of parked approvals. | Principal, ceiling and expiry for automation runs (PX-084). |
| Automation definitions, run history and UI | **NOT-FOUND** | No definition store, validation, enable approval, test run, run record or UI exists. | Definitions (PX-082) and surface (PX-086). |
| Read-only capability posture | **PRODUCTION-WORKING** | The Capability Kernel and read-only task profiles enforce no-write behaviour (M2.5, M5.1); the review-isolated profile exists (EPR-018). | Reused as the default definition profile. |

## 4. Requirements

### A. Authority and model

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| AUT-A01 | An automation is a versioned definition that, when a trigger fires, causes the Core to create an ordinary task through the existing CreateTask path. The Scheduler admits it with a capacity ticket, the agent loop runs it, the approval aggregate approves its protected effects and the effect ledger records them. There is no second scheduler, runtime, approval system or ledger (doc 81, REQ-EV-0275). | STATIC (the reference runs triggered agents on its own cloud); UNVERIFIED (Modbit mapping) | PX-082, PX-083 |
| AUT-A02 | A model never creates, edits, enables, schedules or deletes an automation and never grants itself scheduling authority. These are user commands under the session lease or administrator configuration, and a tool call that tries is a ToolCallPolicyDecision denial. | STATIC (the repository's own requirement text); UNVERIFIED (no reference observation) | PX-082, PX-084 |
| AUT-A03 | Every definition binds an execution principal: the creating user (the run takes that person's policy ceiling and brokered credentials) or an organisation service account defined by policy. The principal is recorded on every run. Team-shared automations are not in these rows; private scope only, because team collaboration is deferred (REQ-PX-012, REQ-PX-013). | STATIC (principal kinds: service account, creator, current user) | PX-082, PX-084 |
| AUT-A04 | A definition may also be supplied by a file in a repository (configuration as code). Repository content is hostile input: such a definition is loaded disabled, shown to the user, and enabled only by an approval bound to the hash of the exact definition and the repository revision; any edit re-prompts. | STATIC (reference: definitions managed by deployment are read-only in the UI); UNVERIFIED (Modbit approval by hash) | PX-082 |

### B. Triggers

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| AUT-B01 | Trigger kinds in the first release: a schedule in UTC with a standard five-field expression and a floor of one run per five minutes; repository events from the forge adapter (pull request opened or updated, push, check completed, label added, issue comment, review comment, review submitted); a signed generic webhook; and a manual run. Chat, incident and issue-tracker triggers of the reference product are not adopted: messaging breadth is rejected (REQ-EV-0236) and GitHub is the first forge (doc 29). | STATIC (reference trigger vocabulary); UNVERIFIED (Modbit subset) | PX-082, PX-083, PX-085 |
| AUT-B02 | A trigger may carry filters (branch patterns, labels, authors, paths). A user-supplied regular expression is compiled by a linear-time engine and an invalid or unsafe expression is rejected when the definition is saved. | STATIC (reference validates expressions with linear-time semantics) | PX-082 |
| AUT-B03 | Missed schedule slots are never replayed in bulk. After a downtime the policy is skip (default, recorded as skipped for the missed slot) or catch up once for the whole missed window. A local schedule fires only while the Core is running; the reference product states the same limit for its local scheduled tasks. | STATIC (local schedules run while the app is open); UNVERIFIED (catch-up policy is Modbit's) | PX-083 |
| AUT-B04 | A webhook trigger verifies an HMAC signature over the body with a timestamp window and a nonce against replay, maps the sender to the tenant and the definition, and rejects anything else with an audited reason. The secret is generated once, shown once and held by the secret broker. The reference product uses a generated authorisation header; Modbit's HMAC and replay window are stricter. | STATIC (generated header, admin only) | PX-085 |
| AUT-B05 | Dispatch is idempotent: every firing has an event id (the delivery id, or the schedule slot id) and the Core creates at most one run per (definition, version, event id). A duplicate delivery is recorded as a duplicate and creates nothing. | UNVERIFIED (design choice) | PX-083, PX-085 |
| AUT-B06 | Each definition declares a concurrency policy: skip if a run is active, queue up to a bound, or replace the active run at a safe boundary. Global and per-definition rate limits and a daily budget apply; when one is exhausted the run is recorded as skipped with a typed reason and an attention item is raised. | UNVERIFIED (design choice) | PX-083 |

### C. Execution

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| AUT-C01 | A run is a task whose origin is automation and whose provenance names the definition, its version and the event id. It executes in an isolated worktree or sandbox (REQ-EV-0145). A local run requires a trusted repository; the reference product states that unattended agents require a trusted workspace. | STATIC (trusted workspace required) | PX-083, PX-084 |
| AUT-C02 | A definition may begin with a gate step: a cheap read-only evaluation whose typed answer decides whether the rest of the run happens. A gate that says skip records the run as skipped with the reason and spends only the gate's own budget. | STATIC (reference supports an early filter prompt) | PX-083 |
| AUT-C03 | Modbit ships one reference definition that is real and tested: report whether the default branch has moved ahead of the local one, with a read-only capability profile that the Kernel enforces so it can never change the tree. It is data a user can copy; it is not an example in prose. | STATIC (reference ships a fetch-and-report template that never mutates the tree) | PX-082, PX-084 |

### D. Policy and safety

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| AUT-D01 | An unattended run is a detached run: it has a permission ceiling and cannot expand privilege interactively (REQ-EV-0046). A protected effect that needs approval parks the run in Needs Attention with an expiry (24 h by default); on expiry it is cancelled and receipted. Nothing is ever auto-approved and no agent in the run can approve. | STATIC (repository requirement); UNVERIFIED (expiry value) | PX-084 |
| AUT-D02 | A definition's capability profile is a subset of its principal's policy and defaults to read-only. A profile with any write, network or external-effect capability needs a typed enable approval that lists the exact capabilities, paths and hosts and is bound to the definition hash. Native computer control is never available to an automation (DR-PX-2026-10-03-008). | UNVERIFIED (design choice) | PX-084 |
| AUT-D03 | Every change to a definition creates a new version. Enabling validates the triggers, filters, schedule, capability profile against policy, budget and workspace trust, and records the approval of the exact definition hash. Enabling is refused if validation fails. | STATIC (reference validates on enable) | PX-082, PX-084 |
| AUT-D04 | Trigger payloads (pull request titles and bodies, comments, webhook bodies) are untrusted external content with provenance (forge_pr, forge_comment, webhook), scanned for injection and never authority: a payload cannot change the definition, its capability profile, its budget or its approvals. | STATIC (repository requirement, PX-008) | PX-084 |
| AUT-D05 | There is a global pause and a per-definition pause. The session emergency stop cancels automation runs. A definition disables itself after five consecutive failures and raises an attention item. | UNVERIFIED (design choice) | PX-084 |
| AUT-D06 | A definition contains no secret value. Credentials are broker handles resolved at run time for the principal; the webhook secret lives in the broker. | UNVERIFIED (design choice) | PX-082, PX-084 |
| AUT-D07 | Cloud automations run in the tenant's isolated execution under the signed policy bundle; organisation allow-lists and forced policy apply; nothing crosses the tenant boundary. | UNVERIFIED (design choice) | PX-085 |

### E. Observability and surface

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| AUT-E01 | Every firing produces a run record with a status (running, succeeded, failed, skipped, cancelled), a typed reason for any skip or failure (duplicate, paused, budget, gate, concurrency, missed, approval expired, policy), cost, duration, receipts and links to the task. Failures are counted and never dropped. History is listable per definition with retry and cancel. | STATIC (run and action vocabulary) | PX-083, PX-086 |
| AUT-E02 | A test run simulates a trigger payload through the definition's filters and runs it in a dry-run posture: read-only capabilities, every protected effect denied and listed as what would have been asked. It says when the filters would have skipped the payload. It never performs a protected effect. | STATIC (reference test-run modal honours trigger filters) | PX-086 |
| AUT-E03 | An automation's tasks show an origin badge in the agent list. Notifications follow the existing model: attention, completion and failure only, coalesced per task. | STATIC (agent source badge); DISK (source filter vocabulary in the sidebar settings) | PX-086 |

## 5. Rows and evidence tiers

Every row is release-critical: each changes effect-bearing behaviour, permissions or policy, execution, recovery, protocol or schema, a security boundary or evidence semantics (doc 83); none is eligible for the iteration tier.

| Row | Title | Owner | Prerequisites |
|---|---|---|---|
| PX-082 | Automation definitions: versions, principal, validation, enable approval and repository-supplied definitions | automation | M1.2, M2.5, DOC-PX-010 |
| PX-083 | Trigger evaluation and dispatch inside the existing Scheduler: time source, idempotency, concurrency, missed runs and budgets | core-runtime | M6.2, PX-082, DOC-PX-010 |
| PX-084 | Unattended run policy: principal ceiling, parked approvals with expiry, injection handling and kill switches | effects-security | M2.5, M6.7, PX-040, PX-082, DOC-PX-010 |
| PX-085 | Cloud triggers: signed webhooks, replay protection, tenant mapping and the cloud time source | sandbox-cloud | PX-011, M8.1, PX-082, PX-083, DOC-PX-010 |
| PX-086 | Automations surface and CLI: list, editor with validation, enable approval, test run, run history and kill switches | desktop | PX-044, PX-045, PX-046, PX-058, PX-082, PX-083, PX-084 |

## 6. Supersessions and clarifications

Nothing sealed changes beyond the scope below (in force from 2026-10-05). The explicit records, also entered in docs 02 and 03:

| Earlier decision, row or text | Kind | Scope and effect | What survives |
|---|---|---|---|
| MOD-AUTO-001 (DEFERRED): broad consumer automations and scheduling | SUPERSEDED IN SCOPE | Software-engineering automations (schedule, forge events, signed webhooks, manual) are adopted through the existing Scheduler. Broad consumer automations stay out. | Consumer and messaging breadth stays out (REQ-EV-0236). |
| REQ-EV-0149 (DEFERRED, cron, event and webhook triggers) and REQ-EV-0264 (DEFERRED, recurring goals and watch) | ADOPTED BY ADDITIVE ROWS | The sealed base rows stay byte-identical and DEFERRED; REQ-PX-082 to REQ-PX-086 are the additive adoption and carry the same constraints (explicit principal, budget and policy; the model never self-grants scheduling authority). | Row text, disposition and owner of both base rows. |
| `automation` subsystem (DEFERRED, no crate, no milestone) | ACTIVATED IN SCOPE | It owns definitions only. The Scheduler, policy kernel, approval aggregate and effect ledger keep their owners. | doc 81 single-owner list. |

## 7. Traceability: requirement to row, tag and source

| Requirement | Rows | Verification tag | Source report section |
|---|---|---|---|
| AUT-A01 | PX-082, PX-083 | STATIC (the reference runs triggered agents on its own cloud); UNVERIFIED (Modbit mapping) | F05 A1, doc 81 |
| AUT-A02 | PX-082, PX-084 | STATIC (the repository's own requirement text); UNVERIFIED (no reference observation) | doc 40 REQ-EV-0264, doc 40 REQ-EV-0149 |
| AUT-A03 | PX-082, PX-084 | STATIC (principal kinds: service account, creator, current user) | F05 A1, doc 29 |
| AUT-A04 | PX-082 | STATIC (reference: definitions managed by deployment are read-only in the UI); UNVERIFIED (Modbit approval by hash) | F05 A1, F04 §7.3 |
| AUT-B01 | PX-082, PX-083, PX-085 | STATIC (reference trigger vocabulary); UNVERIFIED (Modbit subset) | F05 A1, doc 40 REQ-EV-0236 |
| AUT-B02 | PX-082 | STATIC (reference validates expressions with linear-time semantics) | F05 A1 |
| AUT-B03 | PX-083 | STATIC (local schedules run while the app is open); UNVERIFIED (catch-up policy is Modbit's) | F02 §3.8 |
| AUT-B04 | PX-085 | STATIC (generated header, admin only) | F05 A1, doc 62 PX-011 |
| AUT-B05 | PX-083, PX-085 | UNVERIFIED (design choice) | doc 30 |
| AUT-B06 | PX-083 | UNVERIFIED (design choice) | doc 14, doc 15 |
| AUT-C01 | PX-083, PX-084 | STATIC (trusted workspace required) | F02 §3.8, doc 40 REQ-EV-0145 |
| AUT-C02 | PX-083 | STATIC (reference supports an early filter prompt) | F05 A1 |
| AUT-C03 | PX-082, PX-084 | STATIC (reference ships a fetch-and-report template that never mutates the tree) | F02 §3.8 |
| AUT-D01 | PX-084 | STATIC (repository requirement); UNVERIFIED (expiry value) | doc 40 REQ-EV-0046, doc 23 |
| AUT-D02 | PX-084 | UNVERIFIED (design choice) | doc 23 |
| AUT-D03 | PX-082, PX-084 | STATIC (reference validates on enable) | F05 A1 |
| AUT-D04 | PX-084 | STATIC (repository requirement, PX-008) | doc 62 PX-008, doc 52 |
| AUT-D05 | PX-084 | UNVERIFIED (design choice) | doc 23 |
| AUT-D06 | PX-082, PX-084 | UNVERIFIED (design choice) | doc 23 |
| AUT-D07 | PX-085 | UNVERIFIED (design choice) | doc 24 |
| AUT-E01 | PX-083, PX-086 | STATIC (run and action vocabulary) | F05 A1 |
| AUT-E02 | PX-086 | STATIC (reference test-run modal honours trigger filters) | F05 A1 |
| AUT-E03 | PX-086 | STATIC (agent source badge); DISK (source filter vocabulary in the sidebar settings) | F01 §1.4, L12 §2.1 |

### Row to requirements

| Row | Requirements |
|---|---|
| PX-082 | AUT-A01, AUT-A02, AUT-A03, AUT-A04, AUT-B01, AUT-B02, AUT-C03, AUT-D03, AUT-D06 |
| PX-083 | AUT-A01, AUT-B01, AUT-B03, AUT-B05, AUT-B06, AUT-C01, AUT-C02, AUT-E01 |
| PX-084 | AUT-A02, AUT-A03, AUT-C01, AUT-C03, AUT-D01, AUT-D02, AUT-D03, AUT-D04, AUT-D05, AUT-D06 |
| PX-085 | AUT-B01, AUT-B04, AUT-B05, AUT-D07 |
| PX-086 | AUT-E01, AUT-E02, AUT-E03 |

## 8. Non-goals

| Not in scope | Why |
|---|---|
| A second scheduler, queue or runtime | Doc 81; automations use the Core Scheduler. |
| Messaging, incident and consumer-assistant triggers | REQ-EV-0236 (REJECT). |
| Team-shared automations and shared run history | Team collaboration is DEFERRED (REQ-PX-012, REQ-PX-013). |
| A model creating or scheduling automations | REQ-EV-0264. |
| Customer evaluation of automation quality with judged metrics | Judged metrics stay advisory and never close a feature; eval phase. |
| Autonomous pull-request merge loops that keep a PR merge-ready | Would need unattended push and merge effects; its own record after this one has run. |

## 9. Still unverified

| ID | Unverified fact | Evidence that would close it |
|---|---|---|
| V01 | Whether the schedule floor of five minutes and the default expiry of 24 h suit real use | Usage in the first release; the values are policy data and change by a new policy version. |
| V02 | Behaviour of the reference product's automations beyond the static bundle | A live observation of its automations surface; none was available in the sampled account (DISK: the UI gate was on, no definitions existed). |
| V03 | Other forges and issue trackers as trigger sources | A forge-family adapter and its own qualification; a new record names them. |
| V04 | Whether users need a background daemon so local schedules fire with the app closed | Owner decision; a daemon is a shell-integration question (DR-PX-2026-10-03-012) and an execution-trust question. |
