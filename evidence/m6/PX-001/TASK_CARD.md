# Task Card — PX-001 Thin-client conformance contract for external development-environment adapters

## Identity

- Task ID: PX-001
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-PX-001; owner label: desktop; subsystem: `packages/ide-adapter-core`, desktop main, CLI, core (`ResolveApproval`)
- Qualification: QUAL-PX-001 — the conformance suite runs against the CLI and the desktop protocol client on a real Core: command idempotency, cursor replay after disconnect, intent-hash-bound approval, attention reasons and acceptance verdict rendering, Core rejection paths; a static dependency scan proves no provider, filesystem, Git or policy code in the client; a client that retries with a new id, replays without a cursor, or links a Git or provider library fails the suite.
- Evidence tier: real-system (the real Core spawned by the shared supervisor; the real CLI binary; a scripted OpenAI-compatible server; `cargo metadata` of the real workspace)

## Goal

Define and implement the conformance suite every SurfaceProtocol client must pass, publish the contract as `packages/ide-adapter-core`, and pass it with the CLI and the desktop client (docs/29).

## Existing-code audit

- classification: PARTIAL before: the CLI (PX-000) and Electron main each spoke the protocol with their own client; `packages/ide-adapter-core` was an empty placeholder; no suite existed; `ResolveApproval` carried no intent hash, so a client could decide an approval without naming what it showed.
- production entry points: `packages/ide-adapter-core/src/client.ts` (`CoreClient`, moved from the desktop; `joinSessionLease`, `resolveApproval` with the intent hash, `listApprovals`, `taskAssurance`, `taskStatus`, task origin from the client kind), `src/supervisor.ts` (`CoreSupervisor`, moved; client kind, `readyLine`), `src/conformance.ts` (`runConformance`, `ConformanceSubject`, `clientSubject`, `scanClientSources`, `THIN_CLIENT_RULE`, `scanCargoClosure`), `apps/desktop/src/main/main.ts` (imports the shared library; joins the lease on reconnect), `apps/cli` (`task create --command-id`, `approval resolve --intent`), `services/modbit-core/src/server.rs` (`ResolveApproval.intent_hash` → `INTENT_MISMATCH`), `.github/workflows/ci.yml` (the suite on the real Core in the desktop-e2e job).
- proof: on a real Core with a scripted model the shared protocol client and the CLI pass all six cases — the same command id twice yields one task marked replayed and another id another task; after reading 15 events and dropping the connection the client resumes from its cursor with 93 more, none repeated, equal to what the Core holds; the `APPROVAL` attention item is rendered with the Core's kind, reason and action; a decision naming another intent is refused `INTENT_MISMATCH` with the approval still `REQUESTED` and the bound intent is `APPROVED`; the verdict of the Core's assurance view (`INCONCLUSIVE` at revision 2) is rendered verbatim; `UNKNOWN_TASK` and `UNKNOWN_APPROVAL` are surfaced with the Core's code and detail. Four deliberately broken clients fail at their case (fresh id on retry → `idempotency`; replay from zero → `cursor-replay`; decision without the intent → `intent-bound-approval`; swallowed refusal → `rejection-paths`). The static scan finds nothing in `packages/ide-adapter-core`, `apps/desktop` and `packages/vscode-adapter` under the named exceptions, flags a fixture client that links `simple-git` and `openai`, reads `OPENAI_API_KEY` and spawns `git`, and `modbit-cli`'s `cargo metadata` closure reaches none of the forbidden crates. The Core side of the binding is proven in `m2_5_capability_kernel_gates_destructive_tools_behind_intent_bound_approvals`.

## Limitations

- The suite's behavioral half needs a built Core (`MODBIT_CORE_BIN`); the node job runs the static half, the desktop-e2e job runs both.
- A subject's session creation is not tested for idempotency (the CLI's `session create` takes no command id); task creation is, on every client.
- The desktop's renderer rendering of attention and verdicts is covered by its own E2E (`attention.spec.ts`, `review.spec.ts`); the suite covers the shared client's views and the CLI's text.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `packages/ide-adapter-core/src/conformance.test.ts` ("PX-E2E-001: the shared protocol client and the CLI pass every conformance case on a real Core; broken clients fail theirs"; the three static-proof tests)
- `m2_5_capability_kernel_gates_destructive_tools_behind_intent_bound_approvals`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
