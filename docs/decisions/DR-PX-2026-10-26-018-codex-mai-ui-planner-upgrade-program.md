---
id: DR-PX-2026-10-26-018
title: The upgrade program drawn from Codex, MAI-UI / Qwen-UI-Agent and Qwen-Planner-Agent is registered as PX-147..191 — 27 adopted rows and 18 deferred rows, with seven proposed tasks folded into rows the ledger already owns
status: accepted
date: 2026-10-26
supersedes: none
approved_by: owner, chat instruction of 2026-10-26 ("Yes, promote the upgrade program into the dossier: write DR-PX-2026-10-26-018, DOC-PX-017, the doc 62 rows for PX-147..198, update the pinned counts, and reseal per AGENTS.md")
---

# DR-PX-2026-10-26-018 — The upgrade program

> **Accepted 2026-10-26** on the owner's chat instruction, for the **registration** of the program: decisions D1 to D6 of section 5 and nothing outside them. The approval does **not** cover any of the twelve open decisions of section 11 (a grounding model role, sequential action batches, frame sensitivity routing, Android, Forbid rules, Landlock, local sandboxing of autonomous runs, new memory kinds, a planner role, proactive event sources, flag stages, and the capability-growth items), which are what the 18 DEFERRED rows wait for; nor any change to a LOCKED requirement row or ADR (ADR-R-039 and 041 stay LOCKED); nor spending on a live model provider, a model download, or the owner's signing material.

## 1. Trigger and evidence

The owner asked on 2026-10-26 what Modbit could adopt from OpenAI Codex, and then from MAI-UI and Qwen-Planner-Agent, and asked for one governing task list. The study was static: the three sources were cloned shallow into a scratch directory and read, and five analysts compared areas of Codex with the Modbit crates while a sixth audited Modbit's computer, browser, planning, memory and automation code against the two Alibaba projects. Nothing was run. The retained reports are `research/codex-parity/SYNTHESIS.md` and `research/upgrade-program/UPGRADE_TASKS.md` (git-excluded, as `research/` is); this record carries what the dossier needs.

| Source | Revision | License | What it is |
|---|---|---|---|
| `openai/codex` | `970111f6` (2026-10-23) | Apache-2.0 | A full coding agent (about two million lines of Rust). Mechanisms only. |
| `Tongyi-MAI/MAI-UI` | `3deabf1d` (2026-08-19) | Apache-2.0 code; weights license not verified | About 1,300 lines of Python client code, a grounding evaluation harness, and 2B and 8B weights on Hugging Face. |
| `Tongyi-MAI/Qwen-UI-Agent` (inside the MAI-UI repository) | `3deabf1d` | Apache-2.0 | A technical report and demos; no code or weights. |
| `Tongyi-MAI/Qwen-Planner-Agent` | `070038bc` (2026-09-26) | not stated in the repository files read | A website and a technical report; the README states that it is not the release of weights, training code or the agent. |

What the Modbit audit found (code read, nothing run): native computer control is IMPLEMENTED-PARTIAL with real Core and fixture-actuator proofs, real-actuator tests that are ignored for want of OS permissions, and no live model run against a native app; the browser semantic path is PRODUCTION-WORKING; mobile control is NOT-FOUND; the planner is single-model; skills select by trigger phrase and the evolution lab is library-only with no producer; memory ranks by keyword, scope, confidence and validation; automations have no plan-then-confirm step; no GUI benchmark exists; the egress broker connects to an admitted name without checking the resolved address (verified at `crates/sandbox/src/egress.rs`). Several graph rows lag the code (PX-069 WIRED blocks PX-070..076).

## 2. Goal and exit criteria

**Goal UPG.** Close the verified holes and the documented-but-absent gates first, prove what exists before adding to it, and prepare the GUI-agent and planner-harness work so that no gain is claimed before Modbit measures it. Done for this record when: the 45 rows, their qualifications, the 27 task cards and the 27 scenarios are in doc 62; the dossier integrity gate and the tool tests pass with the new counts pinned; this record, the register entry in doc 02, the ledger row and the log entry exist; and the graph carries DOC-PX-017 and the change record.

## 3. Options

| Option | Verdict |
|---|---|
| A. Add the three projects' agents to the product | **Rejected.** Two of the three are papers and a website; MAI-UI's useful artifact is a model served elsewhere. There is no agent to add. |
| B. Register all 52 proposed tasks as ADOPT, Release Zero | **Rejected.** It would make capability growth and undecided policy changes block the release, and seven tasks duplicate rows the ledger already owns (doc 81 forbids a second owner). |
| C. Register nothing and keep the study in `research/` | **Rejected** by the owner's instruction. |
| D. Chosen: 27 ADOPT rows for defects and absent gates, 18 DEFERRED rows with activation text and the decision each waits for, seven deltas bound to existing rows. | |

## 4. Current behaviour

Section 1. Rows PX-000..146 are untouched; the counts of the base ledgers (291 REQ-EV, 265 IMP-EV, 291 QUAL-EV, EPR-000..019) are unchanged.

## 5. Decisions

**D1.** Register the program as REQ-PX-147..191: 27 ADOPT rows (M10, RELEASE_ZERO, existing owners) and 18 DEFERRED rows (POST_ZERO, activation text in Appendix B). Seven proposed tasks that existing rows already own are not rows; Appendix C binds their deltas. No requirement row, owner or disposition of any earlier row is changed.

**D2.** Provenance. OpenAI Codex, MAI-UI and the Qwen-UI-Agent and Qwen-Planner-Agent reports are used as reference for mechanisms only (clean-room, as docs 02, 03 and 36 require). No source, prompt text, dataset, screenshot or model weight of any of them is copied into the repository, bundled into the product or downloaded by it. A model reached for qualification is reached through a provider endpoint. Copying anything needs a later record and a notice under `third-party/`.

**D3.** Source facts. Qwen-Planner-Agent (revision `070038bc`) is a website and a paper; its own README says it is not the release of weights, training code or the agent. Qwen-UI-Agent is a paper and demos inside the MAI-UI repository (`3deabf1d`), with no code or weights. MAI-UI ships about 1,300 lines of Python client code (Apache-2.0) and the 2B and 8B weights on Hugging Face behind an OpenAI-compatible vLLM endpoint; the 32B and 235B models are not released. The model-card license of the 2B and 8B weights and the licenses of the grounding datasets were not verified; that is a blocker for activating PX-172 and PX-173. Vendor benchmark scores are not Modbit evidence.

**D4.** Classification rule. A row is ADOPT only when it closes a defect or a documented-but-absent gate and needs no policy decision the owner has not made. A row that needs such a decision, or is capability growth that Release Zero does not need, is DEFERRED with its activation text and the decision it waits for in Appendix B.

**D5.** Binding notes. The deltas in Appendix C are acceptance additions that the implementing tasks of PX-069..076, PX-087, PX-088, PX-090, PX-092, PX-097, PX-098, PX-117 and PX-140 must also satisfy. They change no row text, owner or disposition.

**D6.** Program rules for every row here: a task starts only when `graph.py ready` shows its prerequisites COMPLETE; each task follows the execution loop of `AGENTS.md`; each names one canonical owner and adds no second scheduler, policy engine, approval system, memory system, event store, tool runtime or context engine (doc 81); a grounding model is a provider role and a planner is a routing role, not an orchestrator; every new effect path fails closed on ambiguity, timeout or missing data; screens, pages, notifications and model output are data, never instruction; retrieved memory is evidence and never grants a tool permission; a stop-the-line finding (a shipped-build exploit in a hardening row, a need for a second owner, a proof that needs a weakened gate, unclear licence terms) stops the work and is reported.

## 6. Migration

None. The change adds rows, cards, scenarios, a record and a dossier task; no code, protocol, schema or stored data changes. The PX ledger pins no count in `tools/dossier_px.py`; `tools/test_dossier.py` pins 192 requirement rows, 170 ADOPT tasks and the 22 DEFERRED rows by identifier, in the same reseal.

## 7. Compatibility

No earlier row changes meaning. The ADOPT rows are RELEASE_ZERO only, so Alpha and Beta readiness (doc 75) are unchanged. Rows PX-158 and PX-178 are prerequisites of PX-174 and PX-179; PX-150 of PX-152; PX-157 of PX-188; PX-184 of PX-186; none depends on a row of a proposed record.

## 8. Security impact

The program is net-tightening: PX-147 closes an address-check gap in the egress broker, PX-148 hardens the two processes that hold keys, PX-149 binds allow rules to the program they were made for, PX-150 narrows the review sandbox. No row relaxes a kernel rule, an always-ask class, a lease, the emergency stop or a capability ceiling. A row that would (Forbid rules, batches, local sandboxing, grounding, Android) is DEFERRED and waits for a decision that states the effect on the kernel.

## 9. Test impact

Added: a dossier test (`test_dr_018_registers_the_upgrade_program_without_a_second_owner_and_defers_what_needs_a_decision`) and the new pins in the parity-count test. No product test changes in this record; the product tests are specified by QUAL-PX-147..191 and run under their rows.

## 10. Rollback

Revert the commit. No data is migrated. A single row can be withdrawn by a later record that moves it to DEFERRED, as REQ-PX-140 was entered.

## 11. Risks and open items (not decided here)

- **Twelve decisions are open** and are what the DEFERRED rows wait for:
  - Grounding model as a provider role, with no mandatory local model (PX-172, PX-173).
  - Sequential action batches against CUC-D02, with a per-action approval binding (PX-175).
  - Frame sensitivity routing, default deny of cloud for a sensitive frame (PX-180).
  - Lifting the doc 66 non-goal for Android (PX-181).
  - Forbid and Prompt added to the allow-only rule type (PX-160).
  - Landlock kernel floor and refuse-or-degrade behaviour (PX-151).
  - Local autonomous runs routed through the egress broker (PX-154).
  - Episodic and prospective memory kinds in the canonical store (PX-185, PX-187).
  - A planner routing role (only if the experiment supports it; ADR-R-039 and 041 are LOCKED) (PX-190).
  - Allowed event sources for the proactive pipeline, and a privacy record for operating-system notification and email sources (PX-191).
  - Flag stages and who may set them (PX-169).
  - Publishing an SDK; goal runs; local provider presets; the Responses wire format; a patch-grammar input (PX-170, PX-165, PX-166, PX-164, PX-163).
- **Licences are not verified:** the Hugging Face model card of MAI-UI-2B and 8B, the grounding datasets (ScreenSpot-v2 and Pro, OSWorld-G, MMBench-GUI, UI-Vision), and the license of Qwen-Planner-Agent.
- **Release Zero grows by 27 rows.** An owner who does not want one of them in Release Zero moves it to DEFERRED by a later record.
- **Inferred findings.** PX-149 (binary pinning) and PX-156 (parked time) rest on reading, not on a reproduction; their tasks begin by reproducing the finding and say so on the card. The check for hard-link escape in the workspace write path is not a row and was not verified.
- **Graph lag.** PX-069 is WIRED and blocks PX-070..076; the reconcile in Appendix C needs evidence refs per row, not a bulk edit. `docs/66` section 3 still describes the audit at `5fdb47f` and is not corrected here.
- **Estimated effort** per row is not in the ledger and was not benchmarked against past throughput.

## Appendix A — Registration map

Working identifiers from the study (`UPG-*`) and the rows they became.

| Working id | Row | Disposition | Owner | Title |
|---|---|---|---|---|
| UPG-H01 | PX-147 | ADOPT | sandbox-cloud | SSRF guard in the sandbox egress broker: the resolved address is checked before connect |
| UPG-H02 | PX-148 | ADOPT | terminal | Process hardening for the Core and the execution broker: no core dumps, not attachable, no injected libraries |
| UPG-H03 | PX-149 | ADOPT | effects-security | Allow rules pinned to the resolved program path |
| UPG-H04 | PX-150 | ADOPT | terminal | Tighter review sandbox: parameterised macOS profile, credential read denials, protected metadata |
| UPG-H05 | PX-151 | DEFERRED | terminal | Linux write confinement through Landlock for the review sandbox, without a user namespace |
| UPG-H06 | PX-152 | ADOPT | terminal | A typed sandbox-denied result so the agent stops retrying a confined command blindly |
| UPG-H07 | PX-153 | ADOPT | core-runtime | Model-visible denial and timeout guidance and a denial circuit breaker that parks the run |
| UPG-H08 | PX-154 | DEFERRED | terminal | A local workspace-write sandbox profile with optional proxy-routed network for local autonomous runs |
| UPG-R01 | PX-155 | ADOPT | durability | Context-overflow recovery: a typed provider error forces a synchronous compaction and one retry |
| UPG-R02 | PX-156 | ADOPT | core-runtime | Time parked on an approval is excluded from tool timeouts and wall-clock accounting |
| UPG-R03 | PX-157 | ADOPT | memory | Secret redaction on memory proposals and use tracking from injected memory |
| UPG-R04 | PX-158 | ADOPT | durability | Image tokens are charged by size in compaction estimates |
| UPG-R05 | PX-159 | ADOPT | observability | Compaction telemetry promised by doc 34: tokens before and after, duration, mode, source, rejection and cost |
| UPG-R06 | PX-160 | DEFERRED | effects-security | Argv rule decisions Allow, Prompt and Forbid with a justification and strictest-wins evaluation |
| UPG-R07 | PX-161 | ADOPT | core-runtime | Read-only tool calls of one turn run concurrently and are journaled in call order |
| UPG-R08 | PX-162 | ADOPT | context-engine | Rule, skill and memory changes arrive as appended transcript notes, not as rewritten early prompt segments |
| UPG-C01 | PX-163 | DEFERRED | tool-runtime | A patch-grammar input to the change tool, with rename and a live argument-diff preview |
| UPG-C03 | PX-164 | DEFERRED | model-gateway | An OpenAI Responses API adapter as a third provider kind |
| UPG-C04 | PX-165 | DEFERRED | core-runtime | Budgeted persistent goals with idle auto-continuation and a wrap-up turn at the budget |
| UPG-C06 | PX-166 | DEFERRED | model-gateway | Local provider presets for Ollama and LM Studio with a health probe, model list and pull |
| UPG-E01 | PX-167 | ADOPT | governance | Dependency advisory, license and unused-crate gates in CI, and a SECURITY.md |
| UPG-E02 | PX-168 | ADOPT | governance | Lint policy: no unwrap or expect outside tests, no lock held across await, no raw process spawn outside the owners |
| UPG-E03 | PX-169 | DEFERRED | effects-security | A feature-flag registry with lifecycle stages, strict configuration that rejects unknown keys, a published schema and a provenance view |
| UPG-E04 | PX-170 | DEFERRED | desktop | A one-shot `modbit exec` with structured output, then a thin published SDK over the adapter core |
| UPG-E05 | PX-171 | ADOPT | eval-bench | Fault-injecting scripted provider server for the provider and Core suites |
| UPG-G02 | PX-172 | DEFERRED | model-gateway | A grounding role in the provider layer: a frame token and an instruction in, a validated coordinate out |
| UPG-G03 | PX-173 | DEFERRED | eval-bench | A grounding qualification suite that scores candidate models on Modbit's own canvas before any adoption |
| UPG-G04 | PX-174 | ADOPT | browser | Screenshot window for computer-use runs: recent frames as images, older steps as text, the full record kept |
| UPG-G05 | PX-175 | DEFERRED | browser | Ordered action batches in one decision, each action individually approved and bound to its own frame |
| UPG-G06 | PX-176 | ADOPT | browser | One structured observation after a GUI action or a command: frame, stdout, stderr, exit code and tool result together |
| UPG-G07 | PX-177 | ADOPT | core-runtime | Human takeover for steps only the person can do: a typed handover that pauses, shows the live state and resumes from the new state |
| UPG-G08 | PX-178 | ADOPT | verification | GUI task verifier and a failure judge that separates model, environment and policy outcomes |
| UPG-G09 | PX-179 | ADOPT | eval-bench | GUI regression suite: Modbit-owned tasks on the fixture desktop and a small real-app set, with a retained baseline |
| UPG-G10 | PX-180 | DEFERRED | model-gateway | Sensitivity-aware routing for frames: a sensitive window's frame goes only to a provider the policy allows |
| UPG-G11 | PX-181 | DEFERRED | browser | Android control through the computer-runtime contract, with virtual-display sessions |
| UPG-P01 | PX-182 | ADOPT | skills | Scenario adapter: per-tool skill guidance compiled for the tools actually exposed, as a versioned configuration |
| UPG-P02 | PX-183 | ADOPT | skills | Ranked skill selection beyond exact trigger phrases, inside the existing budget and trust rules |
| UPG-P09 | PX-184 | ADOPT | eval-bench | Four-axis agent benchmark: tool use, memory, skills and sub-agent tasks with cost per thousand tasks |
| UPG-P03 | PX-185 | DEFERRED | memory | Episodic and prospective memory kinds with evidence spans linking each item to its source events |
| UPG-P04 | PX-186 | ADOPT | memory | Memory retrieval ranking with recency, importance and diversity, kept only if it beats the current ranking on the benchmark |
| UPG-P05 | PX-187 | DEFERRED | memory | A consolidation proposer: an idle-time job that merges duplicates, flags contradictions and proposes supersession |
| UPG-P06 | PX-188 | ADOPT | memory | Memory privacy: private and shared scope, redaction before sharing, deletion that clears derived context, and a person-facing inspect and delete view |
| UPG-P07 | PX-189 | ADOPT | skills | A trace schema for execution evidence and a Core producer that feeds the Skill Evolution Lab |
| UPG-P10 | PX-190 | DEFERRED | model-gateway | A planner routing role, adopted only if a controlled run shows a gain over the single-model plan at equal or lower cost |
| UPG-P11 | PX-191 | DEFERRED | automation | A proactive pipeline: event, affair and proposed task, with a value gate, read-only preparation and confirmation before any external effect |

Folded, not registered as rows: UPG-C02, UPG-C05, UPG-C07, UPG-R09, UPG-R10, UPG-G01, UPG-P08 (Appendix C).

## Appendix B — Activation text for the DEFERRED rows

Each entry is the text that would become the task card and scenario if a later record changes the disposition to ADOPT. None of it is in doc 62.

### PX-151 — Linux write confinement through Landlock for the review sandbox, without a user namespace

- **Owner:** terminal. **Phase:** hardening.
- **Basis:** AUDIT (code read): `seccomp_net.rs` confines network only; writes are left to the lease.
- **Waits for:** the owner records the minimum Linux kernel and whether an older kernel refuses the process or runs with network confinement only.
- **Scope and scenario:** Add the Landlock ruleset beside the seccomp filter in the broker; same suite as UPG-H04 on Linux; the scenario runs a confined writer and reader on a supported and an unsupported kernel image.
- **Deferred qualification:** Deferred proof, entry condition and not a current proof: a real confined process on a supported Linux kernel cannot write outside the lease and cannot read the credential stores of the H04 row; a kernel without the mechanism refuses the review process or degrades exactly as the activating record states; the existing seccomp filter keeps working.

### PX-154 — A local workspace-write sandbox profile with optional proxy-routed network for local autonomous runs

- **Owner:** terminal. **Phase:** hardening.
- **Basis:** AUDIT (code read): `local_autonomous` and `local_trusted` run unconfined behind the argv classifier, which calls itself a tripwire.
- **Waits for:** the owner decides whether local runs route their network through the egress broker, and accepts the SSRF row and the review-sandbox rows as done first.
- **Scope and scenario:** Add a workspace-write profile for local runs on macOS and Linux, route network through the broker with deny-wins and an ask-then-persist-rule loop, keep the classifier as the backstop; the scenario runs an autonomous task that tries an outside write and an unlisted host.
- **Deferred qualification:** Deferred proof, entry condition and not a current proof: a real autonomous run cannot write outside its worktree or reach a host the policy does not allow; an approved host persists as a rule; a missing sandbox refuses the run; the egress broker and `NetworkPolicy` are reused with no second policy engine.

### PX-160 — Argv rule decisions Allow, Prompt and Forbid with a justification and strictest-wins evaluation

- **Owner:** effects-security. **Phase:** robustness.
- **Basis:** AUDIT (code read): `AllowRule` is allow-only literal prefixes; the organisation layer's rules are an opaque list of strings.
- **Waits for:** the owner accepts adding Forbid and Prompt to the allow-only rule type, which touches the rule semantics of REQ-PX-057 and the layer merge laws.
- **Scope and scenario:** Extend `AllowRule` and `RunPolicy::decide` in `crates/policy` and `crates/domain` with a decision and a justification, tighten-only across layers, examples checked at registration; the scenario merges four layers with conflicting rules.
- **Deferred qualification:** Deferred proof, entry condition and not a current proof: a Forbid rule from an administrator layer refuses a command that a project or user layer cannot lift; of several matching rules the strictest wins; a rule carries a justification shown on the card; a rule that cannot match its own declared examples is rejected at registration.

### PX-163 — A patch-grammar input to the change tool, with rename and a live argument-diff preview

- **Owner:** tool-runtime. **Phase:** capability.
- **Basis:** AUDIT (code read): `change.apply` and `change.batch` use match-once edits with rollback; no patch-grammar input; `workspace.move` exists but is not exposed in batch.
- **Waits for:** the owner chooses to spend effort on a model-trained input format; no policy decision is needed.
- **Scope and scenario:** Add `change.patch` parsing the `*** Begin Patch` grammar into the existing ChangeOps, expose rename in batch, add the preview event from provider argument deltas; the scenario applies a mixed patch, a failing patch and a stale one.
- **Deferred qualification:** Deferred proof, entry condition and not a current proof: a real patch with add, update, delete and move applies atomically through `apply_transaction`; a failure rolls back every file; a stale revision is refused; a preview of the arguments while the model is still writing changes no file and is marked non-authoritative.

### PX-164 — An OpenAI Responses API adapter as a third provider kind

- **Owner:** model-gateway. **Phase:** capability.
- **Basis:** AUDIT (code read): `ProviderKind` has two variants, Chat Completions and Messages; reasoning items and `previous_response_id` are unavailable.
- **Waits for:** the owner chooses to support the Responses wire format; the fault-injecting provider server row is complete.
- **Scope and scenario:** Add the adapter behind `contract.rs` with an event-stream parser and registry capability flags; the scenario runs a tool turn, a cut stream and a rate-limited request.
- **Deferred qualification:** Deferred proof, entry condition and not a current proof: a tool turn through a real Responses transport completes with reasoning items carried and `previous_response_id` used; a stream cut before the first token retries and after it does not; the fault-injecting server row covers the failures.

### PX-165 — Budgeted persistent goals with idle auto-continuation and a wrap-up turn at the budget

- **Owner:** core-runtime. **Phase:** capability.
- **Basis:** AUDIT (code read): budget windows exist (PX-146); a token-budgeted objective that persists across user turns does not, and exhaustion goes to needs-attention.
- **Waits for:** the owner chooses goal runs as a product feature; no policy decision is needed beyond the existing budget windows.
- **Scope and scenario:** Add a goal record folded from the log, continuation through the existing queue, and a wrap-up turn at exhaustion; the scenario runs a three-turn goal, a budget stop and a SIGKILL.
- **Deferred qualification:** Deferred proof, entry condition and not a current proof: a goal spanning several user turns continues on idle through the existing input queue and harness, stops at its budget with a wrap-up report and not in needs-attention, and records its state on the log so a restart resumes it.

### PX-166 — Local provider presets for Ollama and LM Studio with a health probe, model list and pull

- **Owner:** model-gateway. **Phase:** capability.
- **Basis:** AUDIT: no local-provider support exists; a user must set a base URL by hand.
- **Waits for:** the owner chooses local models as an onboarding path; consistent with the existing position that no local model is mandatory.
- **Scope and scenario:** Add presets and a probe in `crates/providers` and the onboarding flow; the scenario runs against a real local server.
- **Deferred qualification:** Deferred proof, entry condition and not a current proof: a real local server is detected, its models are listed, a model is pulled with progress and a tool turn runs through the existing OpenAI-compatible path; an absent server is reported plainly and nothing is downloaded without the person's confirmation.

### PX-169 — A feature-flag registry with lifecycle stages, strict configuration that rejects unknown keys, a published schema and a provenance view

- **Owner:** effects-security. **Phase:** capability.
- **Basis:** AUDIT: no feature registry; configuration layers are untyped JSON and unknown keys are likely ignored; `ResolvedConfig` records provenance but no command shows it.
- **Waits for:** the owner chooses the flag stages and who may set them; the merge laws of the policy crate are not to change.
- **Scope and scenario:** Add the registry to the policy crate, the schema generation and the CLI verb; the scenario loads four layers with an unknown key and a pinned flag.
- **Deferred qualification:** Deferred proof, entry condition and not a current proof: an unknown key fails with its file and path; an administrator can pin a flag off and cannot pin it on; the schema validates the shipped defaults; `config show --origins` prints the layer of each key; a flag moves through its stages with a recorded date.

### PX-170 — A one-shot `modbit exec` with structured output, then a thin published SDK over the adapter core

- **Owner:** desktop. **Phase:** capability.
- **Basis:** AUDIT (code read): task create, task run and events tail compose to a one-shot, with exit codes documented; no single verb and no published SDK.
- **Waits for:** the owner chooses to publish an SDK, which is a distribution decision.
- **Scope and scenario:** Add the verb as client-side composition and a package re-exporting the adapter core; the scenario runs one-shot success, denial and cancel.
- **Deferred qualification:** Deferred proof, entry condition and not a current proof: a real one-shot run emits the documented event vocabulary, honours an output schema for the final message and exits with the documented code; the CLI opens no repository, provider or policy store itself (REQ-PX-000); the SDK passes the adapter conformance suite.

### PX-172 — A grounding role in the provider layer: a frame token and an instruction in, a validated coordinate out

- **Owner:** model-gateway. **Phase:** gui agent.
- **Basis:** AUDIT (code read): only a `vision` capability flag exists; the planning model emits pixel coordinates itself; MAI-UI-2B and 8B are served through an OpenAI-compatible endpoint (vLLM) and fit the existing provider path.
- **Waits for:** the owner accepts a provider-neutral grounding role with no mandatory local model, names the models that may be qualified and where a frame may be sent; and these facts are recorded first: the Hugging Face model-card license of MAI-UI-2B and 8B and the licenses of the grounding datasets (not verified by this record).
- **Scope and scenario:** Add a `grounding` capability and slot in `crates/providers` and a resolver in `crates/computer` that binds the answer to the frame token; the scenario drives a real click through a real endpoint with the fixture actuator.
- **Deferred qualification:** Deferred proof, entry condition and not a current proof: a real grounding endpoint returns a coordinate that drives `computer.click` through the existing exact-intent approval, and the approval card shows the intended target; an out-of-frame, stale-token or malformed answer is refused; with no grounding configured the behaviour is today's; the model is reached only through a provider endpoint.

### PX-173 — A grounding qualification suite that scores candidate models on Modbit's own canvas before any adoption

- **Owner:** eval-bench. **Phase:** gui agent.
- **Basis:** AUDIT: no GUI benchmark exists; MAI-UI's repository ships a grounding evaluation harness and benchmark-format data whose licenses are not verified here.
- **Waits for:** the grounding role row is activated and the dataset and model licenses are verified.
- **Scope and scenario:** Add an offline harness under `benchmarks/` reading ScreenSpot and OSWorld-G style records where the license permits and frames captured by the fixture and the real actuator; the scenario runs the comparison and retains the bundle.
- **Deferred qualification:** Deferred proof, entry condition and not a current proof: a retained result bundle compares the planning model's own grounding, a frontier vision model and MAI-UI-2B or 8B on the same Modbit frames; a threshold is written before the run; the bundle names model, endpoint, frames and cost.

### PX-175 — Ordered action batches in one decision, each action individually approved and bound to its own frame

- **Owner:** browser. **Phase:** gui agent.
- **Basis:** AUDIT (code read): CUC-D02 forbids parallel computer calls; sequential batching is not specified; Qwen-UI-Agent reports about 40% of outputs batched.
- **Waits for:** the owner records that sequential batches are consistent with CUC-D02 and how approvals bind per action inside a batch.
- **Scope and scenario:** Add a `computer.batch` with sequential execution and per-action approvals; the scenario runs a four-action batch, a failing batch and a batch cut by Stop.
- **Deferred qualification:** Deferred proof, entry condition and not a current proof: a real batch of four actions runs in order; a failed second action skips the rest and reports which ran; an unknown outcome latches input; each action carries its own exact-intent approval bound to its own frame; one observation follows the batch.

### PX-180 — Sensitivity-aware routing for frames: a sensitive window's frame goes only to a provider the policy allows

- **Owner:** model-gateway. **Phase:** gui agent.
- **Basis:** AUDIT (code read): secure fields are masked; a routing constraint by frame sensitivity does not exist; MAI-UI describes routing by data sensitivity between device and cloud.
- **Waits for:** the owner records the default (deny cloud for sensitive frames) and what counts as sensitive.
- **Scope and scenario:** Add a frame-sensitivity input to the routing compiler and the policy constraint; the scenario runs a sensitive and an ordinary window with a local and a cloud endpoint.
- **Deferred qualification:** Deferred proof, entry condition and not a current proof: a real run in a window the policy marks sensitive refuses cloud routing, uses the permitted endpoint and records the routing decision; with no permitted endpoint the step stops with a typed reason.

### PX-181 — Android control through the computer-runtime contract, with virtual-display sessions

- **Owner:** browser. **Phase:** gui agent.
- **Basis:** AUDIT: no mobile control exists (MOD-MOBILE-001 DEFERRED; doc 66 non-goal); the Qwen-UI-Agent report describes adb-based mobile and virtual displays but ships no code.
- **Waits for:** the owner lifts the doc 66 non-goal for Android only, after the grounding, verifier and live native proof work is complete.
- **Scope and scenario:** Add an Android actuator service beside the macOS one under the existing contract; the scenario runs a small task on an emulator and a real device.
- **Deferred qualification:** Deferred proof, entry condition and not a current proof: a real device or emulator completes a task through an actuator that implements the same contract, per-call approvals and Stop as the macOS actuator; two sessions on separate virtual displays do not cross; a lost device is an environment failure.

### PX-185 — Episodic and prospective memory kinds with evidence spans linking each item to its source events

- **Owner:** memory. **Phase:** planner harness.
- **Basis:** AUDIT (code read): seven record kinds exist; no episodic or prospective (activation condition, scope, expiry, completion) kind; no evidence-span link.
- **Waits for:** the owner accepts adding kinds to the canonical memory store.
- **Scope and scenario:** Add the kinds and span links to `crates/memory` and the Core projection; the scenario runs a reminder-style item through condition, expiry and completion.
- **Deferred qualification:** Deferred proof, entry condition and not a current proof: a prospective item fires only under its activation condition, expires and is marked complete; an episodic item opens its source span; the new kinds pass the existing promotion, scope, sensitivity and supersession rules.

### PX-187 — A consolidation proposer: an idle-time job that merges duplicates, flags contradictions and proposes supersession

- **Owner:** memory. **Phase:** planner harness.
- **Basis:** AUDIT (code read): no automatic consolidation or revision exists; the Planner-Agent paper describes consolidation as gated, with human review of ambiguous updates.
- **Waits for:** the episodic and prospective memory row is complete.
- **Scope and scenario:** Add the job beside the automation owner, reading the log and writing proposals through `memory.propose`; the scenario runs it over a seeded log.
- **Deferred qualification:** Deferred proof, entry condition and not a current proof: a real run over a log projection yields proposals only; nothing is promoted without the existing gate; a contradiction is flagged and a supersession is proposed with its evidence.

### PX-190 — A planner routing role, adopted only if a controlled run shows a gain over the single-model plan at equal or lower cost

- **Owner:** model-gateway. **Phase:** planner harness.
- **Basis:** AUDIT (code read): slot roles are solver, reviewer and reviser (ADR-R-039 and 041 LOCKED); the Planner-Agent paper reports a trained planner model with a harness, but no model or code is released.
- **Waits for:** the four-axis benchmark row is complete; the owner accepts a scoped change of the locked routing roles only if the experiment supports it.
- **Scope and scenario:** Run the experiment in the policy lab with a planner leg declared as a conditional slot; no production change is part of this row.
- **Deferred qualification:** Deferred proof, entry condition and not a current proof: on the four-axis benchmark a configuration with a planner slot is compared with the single-model plan at equal cost; the bundle states the result; a later record adopts or rejects the role on that evidence.

### PX-191 — A proactive pipeline: event, affair and proposed task, with a value gate, read-only preparation and confirmation before any external effect

- **Owner:** automation. **Phase:** planner harness.
- **Basis:** AUDIT (code read): automations have cron, interval, forge and signed-webhook triggers; no event normalisation, affair record, value gate or plan-then-confirm step; PX-084..086 are NOT_STARTED; Qwen-UI-Agent describes the pipeline for mobile notifications.
- **Waits for:** the owner names the allowed event sources; operating-system notification and email sources need their own privacy record; PX-084, PX-085 and PX-086 are complete.
- **Scope and scenario:** Add the event, affair and task records and the value gate over the existing automation owner and policy; the scenario runs a forge event, a webhook and a flood.
- **Deferred qualification:** Deferred proof, entry condition and not a current proof: a real event produces a decision-ready card with options and executes nothing until confirmed; a rejected card is remembered; a flood of events produces a bounded number of cards; the first sources are forge events and signed webhooks.

## Appendix C — Deltas bound to rows the ledger already owns

| Proposed task | Existing row | Delta the implementing task must also satisfy |
|---|---|---|
| MCP streamable HTTP, OAuth and elicitation | PX-090 (REQ-PX-090, `external-tools`, OAuth and connection state) | PX-090's implementation covers, in addition to its recorded acceptance: the streamable-HTTP transport beside stdio, an OAuth token held only by the credential broker (REQ-PX-130) and revoked fail-closed, and MCP elicitation reaching `user.ask`. Codex reference: `rmcp-client`. |
| Release pipeline, signing, SBOM and updater | PX-097 (update flow) and PX-098 (version policy), with M10.2 as recorded in the maintenance log | A tag-versus-manifest version check, a signing job that runs only after unsigned artifacts exist, published checksums and a signature, an SBOM, and an updater that refuses a tampered artifact. The signing key and platform identity are owner-supplied and are not part of this record. Codex reference: `rust-release.yml`. |
| Plugin install and upgrade pipeline | PX-087 and PX-088 (catalog, install, update, rollback) | Staging directory, atomic activation, remote-revision comparison for upgrades, an upgrade that widens permissions refused (the no-silent-widening rule), and rollback that leaves a consistent tree. Codex reference: `core-plugins` staging and upgrade. |
| Hook contract and hook trust | PX-092 (hook contract completion) and PX-117 (more hook points) | Exit-code blocking with a stderr reason, regex tool matchers with aliases, a payload that carries the working directory, model, run mode and tool-use id, published per-point JSON Schemas, hook trust bound to a digest of the declarations and of each command file with a visible modified state, and an administrator managed-hooks-only switch. Hooks stay tighten-only: an allow answer remains malformed. Codex reference: `hooks`. |
| Durable external-tool allow rules and labelled approval cards | PX-090 and REQ-PX-057 (durable allow rules) | A durable allow rule keyed by (server configuration fingerprint, tool) so an approved external call is not asked again while the fingerprint is unchanged, and an approval card that labels each parameter. A tool annotation may still only raise cost, never lower the ask. Codex reference: `mcp_tool_call`, approval templates. |
| Live native-model proof and graph reconcile | the live legs of PX-069, PX-071, PX-074 and PX-075, and the status of PX-069..076 | A retained live run of a real model driving a real native macOS app through the real actuator, with per-action approvals, a Stop mid-run and the unknown-outcome latch, run on the owner's machine with Accessibility and Screen Recording granted; and then each of PX-070..076 advanced one lifecycle step at a time on its own evidence refs (no bulk edit). Until then the GUI rows PX-174, PX-176..179 may be built and proven against the fixture actuator only, and their completion claims say so. |
| Failure diagnosis to harness revision proposals | REQ-PX-140 (DEFERRED, DR-PX-2026-10-19-014) | The diagnosis of a failed run into a proposal to change skill mappings, memory policy or tool exposure, rule-checked, version-controlled and human-reviewed, never applied on its own, is the activation of REQ-PX-140 and is not a row here. PX-189 (traces) is its input. |
