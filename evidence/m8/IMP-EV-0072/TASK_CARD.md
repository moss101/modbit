# Task Card — IMP-EV-0072 Bidirectional capability negotiation

## Identity

- Task ID: IMP-EV-0072
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: REQ-EV-0072 (the Core and the worker negotiate supported protocols, tools, media and runtime features before task dispatch); QUAL-EV-0072 (an older worker missing a capability receives a compatible task projection or an explicit rejection); docs/24 "Capability negotiation".
- Qualification: two workers on the real control plane — one announcing the cloud profile's tools without a browser, one the full set — a task requiring the browser, a plain task, on a real Sandbox Gateway declaring its guests' features.
- Evidence tier: real-system.

## Goal

Nothing is dispatched to a worker that cannot run it, and a worker runs only what it serves: capabilities announced by the worker, required by the task, checked at creation and at claim, and reflected in the task's tool surface.

## Existing-code audit

- classification: PARTIAL before this task: the surface hello negotiated client capabilities (REQ-EV-0043), the guest hello negotiated methods (M8.3), the handoff checked parity against a static set (M8.7); no worker announced what it served, any worker claimed any session, a task could not say what it required, and a task on a browserless sandbox still carried the browser tools in its surface.
- production entry points: `crates/event-store/src/cloud/{schema.rs,mod.rs}` (schema v5: `workers`, `session_leases.requirements`; `register_worker`, `worker_seen`, `live_workers`, `require_for_session`, `session_requirements`, `claim_ready_session_serving` with `requirements <@ capabilities`), `apps/cloud-worker/src/{lib.rs,session.rs}` (`negotiate`: the gateway's features → the served set, `MODBIT_CLOUD_WORKER_CAPABILITIES` for a narrower worker; registration at start, seen at every poll; the claim by capabilities; `ConfigureSandboxGateway.features` narrowed to what the worker serves), `apps/cloud-api/src/routes.rs` (`served_capabilities` — the union of live workers, the build's set when none registered; `create_task {capabilities}` → `NO_CAPABLE_WORKER` on the ledger or the requirement recorded; the handoff's parity and requirements), `apps/sandbox-gateway/src/routes.rs` and `crates/sandbox/src/backend/{mod.rs,reference.rs,microvm.rs}` (`features`: `egress`, `pty`, `browser` when the reference backend has a Chromium or the image declares one — `MODBIT_GUEST_FEATURES`), `services/modbit-core/src/{tools.rs,sandboxes.rs,runtime.rs}` (`SandboxGatewayCustody.features`; a browser asked of the sandbox only when the guests have one; `visible_specs_for`: the browser tools out of a browserless task's surface).
- proof: `apps/cloud-worker/tests/cloud_worker.rs::qual_ev_0072_capabilities_are_negotiated_before_dispatch_an_older_worker_gets_a_compatible_projection_or_the_task_is_refused` — the gateway's health declares `browser`; the older worker registers without `browser.control`; a task with `capabilities: ["browser.control"]` is `409 NO_CAPABLE_WORKER` while only it is live; a plain task it hosts reaches review on a sandbox with `browser: false`, no `BrowserSessionOpened`, and its `browser.snapshot` answers `TOOL_NOT_VISIBLE`; a full worker registers with `browser.control`; the same browser task is `201`, the session's requirements carry `browser.control`, the full worker hosts it (the older one never claims it) and its `browser.snapshot` reads the sandbox's browser (`about:blank`).

## Limitations

- Requirements are capability names (the lease's vocabulary); media and runtime features (`pty`, `egress`) are negotiated as the gateway's features into the Core, not as task requirements.
- A worker seen within 60 s counts as live for the rejection; a task created while a capable worker is momentarily unseen is refused rather than queued.
- The guest ↔ gateway and client ↔ Core negotiations are the earlier ones (M8.3, REQ-EV-0043); this task adds the worker ↔ control plane and worker ↔ Core legs.

## Verification

- `qual_ev_0072_capabilities_are_negotiated_before_dispatch_an_older_worker_gets_a_compatible_projection_or_the_task_is_refused`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
