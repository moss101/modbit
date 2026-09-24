# Security, Policy, Capabilities, Secrets, and Effect Ledger

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Status vocabulary:** **LOCKED**, **PROVISIONAL**, **EXPERIMENT**, **DEFERRED**, **REJECTED**  
> **Source-of-truth rule:** latest explicit Modbit decision > locked decisions > current dossier > older project documents. Older Code-OSS/Modbit Lite material is historical only when it conflicts with this dossier.


## Threat model

Assume hostile repository files, package scripts, model output, browser content, MCP servers, sandbox guests, downloads, symlinks, terminal output and network responses. The renderer and model are not trusted decision-makers for privileged effects.

## Capability kernel

A `CapabilityLease` binds tenant/user/session/task/agent, resource selector, operation set, effect ceiling, execution profile, expiry and generation. Tools must present a valid lease; tool name alone is not authority.

Examples:
- `fs.read:/repo/**`
- `fs.write:/repo/src/**`
- `git.commit:worktree-123`
- `network.egress:api.github.com:443`
- `browser.control:session-9`
- `secret.use:github-token -> origin api.github.com`

As built (IMP-EV-0041, REQ-EV-0041; `modbit_policy::config::{generation, permission_changes, denied}`, `services/modbit-core/src/config.rs`): the policy is refreshed between model rounds. At every round boundary the loop re-resolves the admin/project/user layers into the task's snapshot; a snapshot is identified by its generation (a digest of everything it decides), and a new generation is recorded as `PolicyGenerationChanged { from, to, tightened, loosened, withheld_tools }`. The round is decided under the snapshot it starts with: the Capability Kernel now receives it (step 5, the per-capability `DENY` refused `CAPABILITY_DENIED_BY_CONFIG` and `ASK` escalated to an approval — before this the Core passed no configuration and the step never ran), and the projection withholds every tool whose required capability the snapshot denies (`POLICY_DENIED`), so a model call naming one is refused `TOOL_NOT_PROJECTED` before any effect. A call already in flight is never re-decided: it finishes under the snapshot it was decided with. A call from a client outside a round (`InvokeTool`, the forge commands) is decided under the task's snapshot as of its latest round — a task keeps the configuration it has until its next round (M9.4) (test `qual_ev_0041_a_tightened_policy_applies_from_the_next_round_and_the_call_in_flight_finishes`).

As built (IMP-EV-0040, REQ-EV-0040; `modbit_policy::config::{Authority::Device, DeviceConstraints}`, `services/modbit-core/src/config.rs::device_policy_path`): the machine is an authority above the organization. Its managed policy — at `MODBIT_DEVICE_POLICY` when the device manager sets it, else the platform's system-wide file (`/Library/Application Support/Modbit/device-policy.json`, `/etc/modbit/device-policy.json`, `C:\ProgramData\Modbit\device-policy.json`), outside anything a user or a repository writes — is a configuration layer resolved first, and it alone sets the device constraints: `sandbox_required`, `telemetry`, `proxy`, `trust_roots`, `update_channel`, `minimum_version`. The same key in the admin, project or user layer sets nothing and is recorded as a rejected attempt; the device layer's permissions are tighten-only below it like every higher layer's. The Capability Kernel enforces `sandbox_required`: execution (`shell.exec`) runs only in an isolated profile (`review_isolated`, `cloud_isolated`), refused `DEVICE_REQUIRES_SANDBOX` otherwise. `GetEffectivePolicy { task_id }` shows the task's generation, the device constraints and where they were read, every capability's decision with the layer that made it, and every refused attempt (test `qual_ev_0040_a_project_file_cannot_disable_what_the_device_requires`). The proxy, trust roots, update floor and telemetry level are represented and shown; their enforcement belongs to the egress broker, the updater (M10.2) and telemetry (M10.1).

## Approval policy

Default policy should permit routine read/search/test activity and reversible writes inside isolated worktrees while escalating:
- protected path writes;
- destructive Git/filesystem actions;
- external sends/posts/purchases/deploys;
- new secret use;
- permission expansion;
- browser actions with irreversible effects;
- ambiguous effect outcome.

Approval binds normalized intent hash + scope + expiry. Changing parameters invalidates approval.

## Protected-effect receipt chain

For protected/external effects append:

```text
EffectReceipt {
  effect_id
  previous_receipt_hash
  session/task/turn/step/tool_call
  capability_lease
  normalized_intent_hash
  policy_decision
  approval_id?
  execution_target
  precondition/checkpoint refs
  result/evidence refs
  status
  occurred_at
  receipt_hash
}
```

The chain is append-only and independently verifiable. A rejected/failed effect is also recorded when security-relevant.

As built (M9.2, IMP-EV-0270; `crates/policy/src/ledger.rs`, `services/modbit-core/src/tools.rs`, event-store `effect_receipts`): every effect at or above `ProtectedWrite` seals a receipt as it completes — `receipt_hash` is the sha256 over the canonical JSON of every other field, and `previous_receipt_hash` links to the tenant chain's last receipt (`last_receipt_hash`) — bound to the tool call, the capability lease, the approval and the result object. The chain is a projection of the immutable `EffectReceiptAppended` events; `GetEffectReceipts` recomputes `verify_chain` over the stored rows on every read (the Core caches no verdict) and returns `chain_valid` with a `detail`, so a receipt tampered, deleted or reordered in the store fails verification until it is restored. Proven by `qual_ev_0270_…` (`services/modbit-core/tests/surface_protocol.rs`): two approved protected effects produce a linked two-receipt chain (`chain_valid`); tampering a stored field (`does not recompute`), swapping the two receipts' order, and deleting the first (a missing predecessor) each make `chain_valid` false, and restoring makes it true again.

As built (IMP-EV-0066, REQ-EV-0066; `modbit_domain::toolcall::Reversibility`, `services/modbit-core/src/compensation.rs`): every receipt carries `reversibility` — how far its effect can be taken back, from the tool's own declaration and never optimistically: a read or a workspace write `REVERSIBLE` (the typed undo restores the exact content), a protected surface `PARTIALLY_REVERSIBLE` (content restorable, observers not recalled), an external effect `COMPENSATABLE` when its tool declares a counteracting tool (`ToolSpec.compensation`; `forge.pr.create` → `forge.pr.update` closing the pull request) and `IRREVERSIBLE` when it does not, a secret access or a destruction `IRREVERSIBLE`. `UndoToolCall` refuses a `COMPENSATABLE` or `IRREVERSIBLE` effect `NOT_UNDOABLE`, naming the class and what counteracts it, instead of reporting that nothing changed. `CompensateEffect { task_id, tool_call_id }` runs the declared compensation as a new external effect through the kernel under its own approval (the first request is `APPROVAL_PENDING`; one compensating call per compensated effect, so the retry after the person decides runs that call); its receipt is its own — a new effect with `compensates` naming the original, and a class of its own — while the original receipt is never edited; a second request answers from the compensation receipt, and an effect with no declared compensation, one that did not succeed, or one without a receipt is refused `NOT_COMPENSATABLE`. Both fields are covered by `receipt_hash`; a receipt written before them omits them and keeps its hash (test `qual_ev_0066_an_external_effect_is_never_undoable_and_its_compensation_is_a_receipt_of_its_own`, and `reversibility_and_compensation_are_hashed_and_legacy_receipts_keep_their_hash`).

## Secrets

- Renderer never sees raw cloud/provider secrets.
- Sandbox images contain no tenant secrets.
- Guest receives short-lived scoped credential material only through broker at execution time, preferably via pipe/fd/memory rather than static env. *As built (M8.6, M9.3/REQ-EV-0288):* the guest addresses a virtual host over plain HTTP through its own proxy and the host's broker injects the secret it holds under the handle — the guest never holds it, and the compiled policy it is built from never had it. The handle is short-lived: the gateway stamps and caps an expiry, the broker drops the secret the moment it passes, and a longer task is renewed from the Core's custody rather than given a standing secret.
- Secret handles are origin/tool/effect scoped and auditable.
- Terminal output redactor detects known secret fingerprints before persistence/display while preserving original only in protected diagnostic vault if policy allows.
  *As built (IMP-EV-0017):* the redactor is `modbit_secrets::Redactor`, the one implementation (the gateway's, the forge client's, the external-tool client's and the Cloud API's former copies now call it). Values in the Core's custody are replaced everywhere they appear; credential shapes (provider, forge, worker, bearer, cloud and chat tokens, credential parameters, PEM keys) are replaced in error text; the event store applies it to every payload before hashing and persistence. No original is kept: there is no diagnostic vault yet, so a redacted value is gone.

### External tool servers as built (M9.4)

An MCP server is a program the host started and does not control, so it is treated as one throughout. Its credential is a *handle* in the server's configuration; the value is taken into the Core's memory at boot (`MODBIT_MCP_CREDENTIAL_<HANDLE>`), placed into the child's environment at spawn and written nowhere else — not into the configuration, a log, a listing, an argument or a result — and it joins the Core's custody set, so a tool call carrying the value is refused before policy like any other secret (M9.3). A configuration that tries to put a secret in a plain environment entry is refused `SECRET_IN_CONFIG` rather than quietly accepted. Everything a server *says* is data: names are validated and namespaced, descriptions and schemas are bounded, and fields the host does not know (a capability claim, a system instruction, an effect class) are dropped before the host reads the tool, so a server can neither inject an instruction nor grant itself a capability. What a call costs is the host's judgement, not the server's, and a transport is pooled per tenant and workspace so no tenant ever reaches another's server process.

Two further rules make the credential safe in both directions. A server declares the capabilities it needs and they are checked against the **task's** capability lease before the server is started — a named credential needs `secret.use`, so a task that could not use a secret itself cannot have a server use one for it (`UNLEASED` in the listing, `EXTERNAL_CAPABILITY_NOT_LEASED` on a call, nothing spawned). And an answer that repeats a secret in the Core's custody has it replaced before the result leaves the host, with the replacement recorded as `SecurityEventRecorded` / `SECRET_IN_EXTERNAL_RESULT`: a credential is handed to a server to use, never to put back into the model's context. Which servers exist at all is the task's resolved admin/project/user configuration (REQ-EV-0039), pinned for the task's life, with a higher layer's deny always winning and every overridden or refused definition answered for in `external.list`. See `16_TOOL_CAPABILITY_AND_PROCEDURAL_RUNTIME.md` "MCP / external tools".

## Protected paths

System config, SSH keys, credential stores, `.git` internals, CI secrets and user-defined paths default to deny or approval. Path checks are performed after symlink resolution and before each write/open, not only at task creation.

## Supply chain

Dependencies are pinned by lockfiles; release builds generate SBOM, verify licenses and run vulnerability scans. Guest/sandbox images are digest-pinned and signed. Skills and external tool manifests are signed/versioned; unsigned local development skills are visibly marked.

## Emergency stop

Global stop revokes active capability leases, blocks new effects, cancels safe tool calls, freezes dangerous external operations at broker/gateway where possible and marks ambiguous outcomes for reconciliation.

As built (M9.5, on M2.5 and IMP-EV-0085): `EmergencyStop` on a session journals `EmergencyStopActivated`, revokes every active capability lease (`CapabilityLeaseRevoked` with the reason), and the Capability Kernel refuses every non-read-only effect of that session with `EMERGENCY_STOP` from then on; the stop is a projection of the log, so a restarted Core holds it, and `StartTask` refuses any run in a stopped session (`EMERGENCY_STOP`) — a stop lasts for the session. Every live loop of the session is cancelled at once: a process a tool is running (`shell.exec`, `test.run`, or a verification stage of the engine) is cancelled at the terminal broker, which ends it with its process group, and the call is recorded `ToolCallCancelled` — a cancelled check is not evidence, the `TestReport` it would have produced is discarded; an effect already sent when the cancellation lands is recorded `ToolCallUnknownOutcome` for reconciliation (M4.1/M4.6); the run ends `RunCancelled` without another model call and the task `TaskCancelled`. Before M9.5 the stop revoked capability leases only, and a running loop — which fences on the session lease generation, not on capability leases — kept turning against refusals while a check it had started ran to its end. The browser's host-owned stop (IMP-EV-0085, M7.6) halts input independently of the model loop. Proof: `m9_5_emergency_stop_cancels_the_check_in_flight_ends_the_run_and_outlives_a_restart`, `m2_5_capability_kernel_gates_destructive_tools_behind_intent_bound_approvals` and `qual_ev_0085_an_emergency_stop_halts_browser_input_before_the_host_with_the_reason_on_record`.

## Conditional policy, independent assurance and reviewer isolation

PolicyEnvelope defines legal plan/model/provider/effect set, protected surfaces, minimum assurance, mandatory review/human and secret/network/deploy rules. Intrinsic RequestProfile and conditional plans cannot grant authority. Factual revision-bound RealizedRisk strengthens assurance; separate Acceptance Gate checks evidence. Passing tests, model confidence or a learned scalar cannot remove review/human requirements.

Isolated Non-Committing Reviewer capabilities keep canonical tracked state read-only, allow ephemeral writes and bounded sandboxed process execution only in disposable review worktrees, deny network/secrets by default, and prohibit canonical mutation, Git commit/push, deploy, persistent/external actions. The kernel enforces actual path/mount/process/egress boundaries; a prompt or all-tools-denied mock is insufficient. Default review context excludes solver hidden reasoning while retaining relevant source/static evidence. Trusted Core may export bounded evidence through its existing artifact owner; reviewer cannot persist its scratch patch. Cancel/timeout/crash kills processes, revokes handles and disposes scratch.

Every escalation/review/revision/human/provider continuation is compiled, validated and worst-case reserved before initial dispatch. Runtime may deny revoked slots but never generate new topology; missing required assurance branch stops safely. Any separate transaction inherits remaining request budget after unknown-effect/usage reconciliation. Registry and Outcome Statistics are independently versioned; neither can grant permissions. Qualifications and mutation proofs: EPR-014..019 plus amended EPR-004/007/008/010/011/012 in docs 49/61.

As built (EPR-008): the assurance section of the PolicyEnvelope lives in `crates/policy` (`assurance::AssurancePolicy`): the minimum assurance (`STANDARD` by default), the protected surfaces (auth, authorization, secrets and deployment `CRITICAL` with a human required; CI/CD, infrastructure, migrations and the product's own `.modbit/` `HIGH` with independent review required; dependency manifests and lockfiles `MEDIUM`), blast-radius thresholds, the levels at which review (`HIGH`) and a human (`CRITICAL`) become mandatory, required checks and forbidden effects. Layers only strengthen: an organization layer (`MODBIT_POLICY_FILE`; a malformed file refuses startup) and a repository layer (`.modbit/policy.json`) may add surfaces, raise minima and lower thresholds, and anything that would weaken the policy is ignored and named on the assurance surface. `derive_realized_risk` is deterministic in the facts of the candidate — changed paths and line counts, the plan's write set, the effects requested — and takes no score: a learned risk, a reviewer's confidence and a passing suite are recorded as `advisory_ignored` and change nothing (the fault corpus under `crates/policy/tests/fixtures/assurance` pins every rule). The Core derives the risk at every COMPLETION run from the same changed files the diff invariants see, joins it with the earlier derivation so later facts only strengthen, persists it as `RealizedRiskDerived` on the run beside the verification report, tells the model, serves it as `GetTaskAssurance`, and compiles routing plans under its version. The surfaces that need a typed question before a write (DI-9) come from the same policy. A candidate that needs a human decision is never proposed for acceptance from a profile that cannot wait for one: the completion is refused (`ASSURANCE_HUMAN_REQUIRED`) and the run stops safely; nothing is synthesized in the human's place. Regression fixed in `risk-rules-2` (found by EPR-019's held-out risk corpus): the secret surface matched `.env` only as a suffix, so a change to `config/.env.production` was `LOW` with no review and no human; the surface now also carries the basename prefix `.env.*` — the same credential files the protected paths above deny (`**/.env`, `**/.env.*`) — and a suffix or basename prefix (`.pem`, `.key`, `.env.*`) is compared without ASCII case, so `certs/Server.KEY` is a key; directories, segments and basenames stay exact, and a name that only looks like one (`environment.rs`, `dotenv.rs`, `hotkey.rs`) is on no surface.

As built (EPR-017): the Acceptance Gate lives in `crates/verification` (`gate::evaluate`) and answers a different question from the risk — whether the evidence at the exact candidate revision satisfies the required assurance. It takes the PolicyEnvelope minimum plus the realized risk as the obligation (`RequiredAssurance`), gathers the latest COMPLETION run with its checks and regression attribution, the diff invariants, the review decisions on record and the policy-required checks, and decides: explicit failure (a check the baseline did not already fail, a DENY invariant, a forbidden effect, a RETURN) rejects; missing, stale (another revision), cancelled or timed-out mandatory evidence is INCONCLUSIVE with `missing_evidence` naming each kind; ACCEPT needs every required evidence present, current and passing. A review or human obligation is discharged only by a decision at the same revision — a user's review discharges both, an independent reviewer's only the review — and passing tests never discharge either; a missing risk record is itself missing evidence. The Core evaluates the gate at every COMPLETION run (`AcceptanceGateEvaluated`, trigger `COMPLETION_RUN`, the result an object the run names with gate and risk versions and refs) and again when the user accepts: an accept whose gate is not ACCEPT — the worktree moved after the completion run, a check failed, an obligation unmet — is refused `ACCEPTANCE_NOT_MET` and records nothing; an accept the gate confirms lands the decision, the gate's ACCEPT (`REVIEW_DECISION`) and `TaskCompleted` together. `GetTaskAssurance` serves the risk and the gate side by side.
