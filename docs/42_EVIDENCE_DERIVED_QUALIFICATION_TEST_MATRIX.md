# Evidence-Derived Qualification Test Matrix

> Every retained mechanism has a qualification. These tests are evidence gates; production features cannot be closed by unit-only proof.

| Test ID | Requirement | Disposition | Owner | Qualification | Required evidence class |
|---|---|---|---|---|---|
| QUAL-EV-0001 | REQ-EV-0001 | ADAPT | Context Engine | Fixed-revision repository benchmark proves planner choice, recall, latency and token cost. | real-system or production-equivalent |
| QUAL-EV-0002 | REQ-EV-0002 | ADOPT | Context Engine | Mutate indexed file after indexing; stale bytes must never reach model context. | real-system or production-equivalent |
| QUAL-EV-0003 | REQ-EV-0003 | ADAPT | Context Engine | Context budget test proves hydration occurs only when requested and provenance survives. | real-system or production-equivalent |
| QUAL-EV-0004 | REQ-EV-0004 | ADAPT | Repository Index | Large-repo incremental edit updates only affected index segments and stays revision-correct. | real-system or production-equivalent |
| QUAL-EV-0005 | REQ-EV-0005 | ADOPT | Context Graph | Cross-language fixture proves symbol identity is consistent across index/query/impact paths. | real-system or production-equivalent |
| QUAL-EV-0006 | REQ-EV-0006 | ADOPT | Agent Runtime | Kill Core mid-child run; restart resumes exact child identity and parent linkage. | real-system or production-equivalent |
| QUAL-EV-0007 | REQ-EV-0007 | ADOPT | Agent Runtime | Replay identical spawn call after transport retry; exactly one child exists. | real-system or production-equivalent |
| QUAL-EV-0008 | REQ-EV-0008 | ADOPT | Agent Runtime | Move real running child background then foreground without restart or lost event offset. | real-system or production-equivalent |
| QUAL-EV-0009 | REQ-EV-0009 | ADOPT | Agent Runtime | Steer during live model/tool cycle and verify deterministic cancellation boundary and replay. | real-system or production-equivalent |
| QUAL-EV-0010 | REQ-EV-0010 | ADOPT | Event Protocol | Disconnect desktop, produce events, reconnect from offset and compare exact stream. | real-system or production-equivalent |
| QUAL-EV-0011 | REQ-EV-0011 | ADAPT | Artifact Store | Restart and retrieve multi-MB artifact by digest; digest mismatch fails closed. | real-system or production-equivalent |
| QUAL-EV-0012 | REQ-EV-0012 | ADOPT | Checkpoint Store | Race two checkpoint writes; old epoch cannot overwrite newer state. | real-system or production-equivalent |
| QUAL-EV-0013 | REQ-EV-0013 | ADOPT | Checkpoint Store | Restore edited worktree and protocol cursor after process restart from baseline+delta. | real-system or production-equivalent |
| QUAL-EV-0014 | REQ-EV-0014 | ADOPT | Change Engine | Concurrent user edit causes precondition failure without data loss. | real-system or production-equivalent |
| QUAL-EV-0015 | REQ-EV-0015 | ADOPT | Change Engine | Ambiguous duplicated target must fail and leave worktree unchanged. | real-system or production-equivalent |
| QUAL-EV-0016 | REQ-EV-0016 | ADAPT | Change Engine | Injected failure at edit N rolls back transaction or emits explicit partial state by contract. | real-system or production-equivalent |
| QUAL-EV-0017 | REQ-EV-0017 | ADAPT | Error Service | Secret-bearing internal error is redacted for both surfaces according to policy. | real-system or production-equivalent |
| QUAL-EV-0018 | REQ-EV-0018 | ADOPT | Verification Plane | Fixture with pre-existing errors proves baseline issues are not blamed on change. | real-system or production-equivalent |
| QUAL-EV-0019 | REQ-EV-0019 | ADOPT | Terminal Broker | Generate >10MB output; UI can replay, model receives bounded view, full artifact remains retrievable. | real-system or production-equivalent |
| QUAL-EV-0020 | REQ-EV-0020 | ADOPT | Diagnostics Adapter | High-churn editor/test fixture proves no unsolicited diagnostic prompt traffic. | real-system or production-equivalent |
| QUAL-EV-0021 | REQ-EV-0021 | ADAPT | Workspace Fabric | Change environment definition and verify run pins old revision until explicit rebuild. | real-system or production-equivalent |
| QUAL-EV-0022 | REQ-EV-0022 | ADAPT | Workspace Fabric | Cloud run reconstructs dirty state exactly and cleanup removes temporary refs safely. | real-system or production-equivalent |
| QUAL-EV-0023 | REQ-EV-0023 | ADOPT | Observability | Staging run emits all timestamps and derived cold/warm latency metrics. | real-system or production-equivalent |
| QUAL-EV-0024 | REQ-EV-0024 | EXPERIMENT | Worker Gateway | VPC test with no inbound ports proves identity, reconnect, revocation and tenant isolation. | experiment report |
| QUAL-EV-0025 | REQ-EV-0025 | ADAPT | Terminal Broker | Two concurrent workspaces cannot leak cwd/env or aliases. | real-system or production-equivalent |
| QUAL-EV-0026 | REQ-EV-0026 | ADOPT | Terminal Broker | Noisy build shows token reduction while raw output digest remains complete. | real-system or production-equivalent |
| QUAL-EV-0027 | REQ-EV-0027 | ADOPT | Terminal Broker | Start long test, detach, restart UI, reattach and cancel successfully. | real-system or production-equivalent |
| QUAL-EV-0028 | REQ-EV-0028 | ADOPT | Model Gateway | Provider conformance probes detect capability mismatch and route away. | real-system or production-equivalent |
| QUAL-EV-0029 | REQ-EV-0029 | ADAPT | Model Router | Replay benchmark corpus and verify deterministic hard exclusions plus auditable scores. | real-system or production-equivalent |
| QUAL-EV-0030 | REQ-EV-0030 | ADAPT | Model Router | Primary outage triggers approved fallback and records RouterDecisionRecord. | real-system or production-equivalent |
| QUAL-EV-0031 | REQ-EV-0031 | ADOPT | Policy Kernel | Blocked provider remains unavailable despite task/profile request. | real-system or production-equivalent |
| QUAL-EV-0032 | REQ-EV-0032 | ADOPT | Usage Ledger | Reconcile provider invoice sample against canonical usage events within tolerance. | real-system or production-equivalent |
| QUAL-EV-0033 | REQ-EV-0033 | DEFERRED | Learning/Eval | When enabled, hunk feedback maps to immutable generation IDs without changing policy. | architecture/absence guard |
| QUAL-EV-0034 | REQ-EV-0034 | DEFERRED | Learning/Eval | Client renders versioned taxonomy and old feedback remains interpretable. | architecture/absence guard |
| QUAL-EV-0035 | REQ-EV-0035 | ADOPT | Workspace UI | E2E task compares UI inspector against actual PromptEnvelope context IDs. | real-system or production-equivalent |
| QUAL-EV-0036 | REQ-EV-0036 | ADOPT | Workspace UI + Change Engine | Reject one hunk and accept another; resulting Git diff matches user choices exactly. | real-system or production-equivalent |
| QUAL-EV-0037 | REQ-EV-0037 | ADAPT | Workspace UI | State transitions update attention buckets after desktop reconnect with no client-local truth. | real-system or production-equivalent |
| QUAL-EV-0038 | REQ-EV-0038 | REJECT | Agent Runtime | Admission test denies unsafe overlapping write sets or depth/capacity excess. | architecture/absence guard |
| QUAL-EV-0039 | REQ-EV-0039 | ADOPT | Configuration Service | Conflicting admin/project/user configs resolve deterministically; lower authority cannot widen. | real-system or production-equivalent |
| QUAL-EV-0040 | REQ-EV-0040 | ADAPT | Policy Kernel | Project file attempting to disable device requirement is rejected. | real-system or production-equivalent |
| QUAL-EV-0041 | REQ-EV-0041 | ADOPT | Policy Kernel | Change org policy while run active; current authorized tool finishes, next forbidden tool is absent. | real-system or production-equivalent |
| QUAL-EV-0042 | REQ-EV-0042 | ADOPT | Hook Bus | Slow/failing hook follows configured fail policy and cannot bypass monotonic guard. | real-system or production-equivalent |
| QUAL-EV-0043 | REQ-EV-0043 | ADOPT | Capability Kernel | Headless client lacks UI-only capabilities while Core task remains valid. | real-system or production-equivalent |
| QUAL-EV-0044 | REQ-EV-0044 | ADOPT | Capability Kernel | Remove browser consumer; browser tool disappears rather than failing after model selects it. | real-system or production-equivalent |
| QUAL-EV-0045 | REQ-EV-0045 | ADOPT | Policy Kernel | Autonomous run cannot request higher privilege than profile ceiling. | real-system or production-equivalent |
| QUAL-EV-0046 | REQ-EV-0046 | ADOPT | Agent Runtime | Background child reaches protected effect and transitions to parent attention state. | real-system or production-equivalent |
| QUAL-EV-0047 | REQ-EV-0047 | ADOPT | Agent Runtime | Core restart preserves child status, task, tool cursor and private context refs. | real-system or production-equivalent |
| QUAL-EV-0048 | REQ-EV-0048 | ADOPT | Agent Runtime | Child cannot access a parent-only secret/tool or hidden transcript. | real-system or production-equivalent |
| QUAL-EV-0049 | REQ-EV-0049 | ADOPT | Agent Runtime | Park child during parent intervention, restart Core, resume from same state. | real-system or production-equivalent |
| QUAL-EV-0050 | REQ-EV-0050 | ADAPT | Agent Runtime | Resume completed research child; new result is a new attempt with prior evidence linked. | real-system or production-equivalent |
| QUAL-EV-0051 | REQ-EV-0051 | ADOPT | Agent Runtime | Depth N+1 launch rejected with typed admission failure. | real-system or production-equivalent |
| QUAL-EV-0052 | REQ-EV-0052 | ADOPT | WorkGraph | Compaction and model restart cannot alter canonical plan state. | real-system or production-equivalent |
| QUAL-EV-0053 | REQ-EV-0053 | ADAPT | Agent Runtime | Seed repeated read/search loop; watchdog moves run to STALLED with evidence. | real-system or production-equivalent |
| QUAL-EV-0054 | REQ-EV-0054 | ADOPT | Session Store | Simulate dual resume; only current lease can append mutation events. | real-system or production-equivalent |
| QUAL-EV-0055 | REQ-EV-0055 | ADOPT | Protocol Store | Crash while tool awaits approval; restart reconstructs exact pending state. | real-system or production-equivalent |
| QUAL-EV-0056 | REQ-EV-0056 | ADOPT | Context Engine | Fork/revert invalidates incompatible compaction output and retains canonical history. | real-system or production-equivalent |
| QUAL-EV-0057 | REQ-EV-0057 | ADOPT | Context Engine | Compaction fidelity test checks labeled instructions/decisions/approvals survive. | real-system or production-equivalent |
| QUAL-EV-0058 | REQ-EV-0058 | ADOPT | Context Engine | Delay compactor while adding events; stale result cannot install. | real-system or production-equivalent |
| QUAL-EV-0059 | REQ-EV-0059 | ADOPT | Instruction Compiler | Unrelated module rule stays absent until matching file is touched. | real-system or production-equivalent |
| QUAL-EV-0060 | REQ-EV-0060 | ADAPT | Repository Knowledge | Edit source after wiki generation; stale claim flagged and never treated as authority. | real-system or production-equivalent |
| QUAL-EV-0061 | REQ-EV-0061 | ADOPT | Skill Compiler | Malicious skill requesting admin capability cannot widen task authority. | real-system or production-equivalent |
| QUAL-EV-0062 | REQ-EV-0062 | ADOPT | Workspace Fabric | Resume detects unavailable environment revision and follows explicit rebuild/fail path. | real-system or production-equivalent |
| QUAL-EV-0063 | REQ-EV-0063 | ADOPT | Workspace Fabric | Local→cloud handoff preserves state but never embeds secret values. | real-system or production-equivalent |
| QUAL-EV-0064 | REQ-EV-0064 | ADOPT | Change Engine | Undo created/deleted/modified files while preserving unrelated user changes. | real-system or production-equivalent |
| QUAL-EV-0065 | REQ-EV-0065 | ADOPT | Change Engine | User edit after agent change blocks destructive revert. | real-system or production-equivalent |
| QUAL-EV-0066 | REQ-EV-0066 | ADOPT | Effect Ledger | External API effect is never labeled fully undoable; compensation receipt is distinct. | real-system or production-equivalent |
| QUAL-EV-0067 | REQ-EV-0067 | ADOPT | Change Engine | Injected conflict + failed test leaves merge transaction inspectable and recoverable. | real-system or production-equivalent |
| QUAL-EV-0068 | REQ-EV-0068 | ADOPT | Verification Plane | Verifier crash yields INDETERMINATE, never success. | real-system or production-equivalent |
| QUAL-EV-0069 | REQ-EV-0069 | EXPERIMENT | Verification Plane | Disabled configuration emits zero verifier model calls. | experiment report |
| QUAL-EV-0070 | REQ-EV-0070 | ADOPT | Verification Plane | High-noise repo proves only relevant post-change window is evaluated. | real-system or production-equivalent |
| QUAL-EV-0071 | REQ-EV-0071 | ADAPT | Verification Plane | Seed secret in patch; merge blocked with evidence. | real-system or production-equivalent |
| QUAL-EV-0072 | REQ-EV-0072 | ADOPT | Worker Protocol | Older worker missing capability receives compatible task projection or explicit rejection. | real-system or production-equivalent |
| QUAL-EV-0073 | REQ-EV-0073 | ADOPT | Reliability Layer | Fault injection verifies no generic success on timeout/corrupt state. | real-system or production-equivalent |
| QUAL-EV-0074 | REQ-EV-0074 | REJECT | Policy Kernel | Malformed enterprise policy prevents widening and produces typed operator error. | architecture/absence guard |
| QUAL-EV-0075 | REQ-EV-0075 | ADOPT | Worker Fabric | Renderer compromise test cannot invoke privileged effect without capability token. | real-system or production-equivalent |
| QUAL-EV-0076 | REQ-EV-0076 | ADOPT | Core + Worker Fabric | Kill renderer and reconnect; session truth unchanged. | real-system or production-equivalent |
| QUAL-EV-0077 | REQ-EV-0077 | ADOPT | Execution Timeline | Fork produces independent revision lineage without copying invalid pending effects. | real-system or production-equivalent |
| QUAL-EV-0078 | REQ-EV-0078 | ADOPT | Agent Runtime | Child prompt dump lacks parent-only memory and secrets. | real-system or production-equivalent |
| QUAL-EV-0079 | REQ-EV-0079 | ADOPT | Tool Runtime | Invalid alias/schema is repaired or rejected before effector. | real-system or production-equivalent |
| QUAL-EV-0080 | REQ-EV-0080 | ADOPT | Capability Kernel | Prompt injection asking to bypass policy fails. | real-system or production-equivalent |
| QUAL-EV-0081 | REQ-EV-0081 | ADAPT | Worker Fabric | Browser worker crash does not corrupt Core and restart reattaches session. | real-system or production-equivalent |
| QUAL-EV-0082 | REQ-EV-0082 | ADOPT | Browser/Computer Runtime | Accessible form completes with zero screenshot dependency; canvas case escalates explicitly. | real-system or production-equivalent |
| QUAL-EV-0083 | REQ-EV-0083 | ADOPT | Computer Runtime | Window title spoof cannot substitute a different process identity. | real-system or production-equivalent |
| QUAL-EV-0084 | REQ-EV-0084 | ADOPT | Computer Runtime | Two workers contend; second receives lease conflict. | real-system or production-equivalent |
| QUAL-EV-0085 | REQ-EV-0085 | ADOPT | Computer Runtime | Emergency stop halts input within safety bound and records reason. | real-system or production-equivalent |
| QUAL-EV-0086 | REQ-EV-0086 | ADOPT | Computer Runtime + Evidence | Raw input fallback without post-check is rejected by completion verifier. | real-system or production-equivalent |
| QUAL-EV-0087 | REQ-EV-0087 | ADOPT | Computer Runtime | Inject real mouse/keyboard event; automation stops and requires reacquisition. | real-system or production-equivalent |
| QUAL-EV-0088 | REQ-EV-0088 | ADOPT | Policy Kernel | Destructive/credential UI action requests approval even if click tool normally allowed. | real-system or production-equivalent |
| QUAL-EV-0089 | REQ-EV-0089 | ADOPT | Computer Runtime | Fault fixtures trigger each code and verify recovery guidance. | real-system or production-equivalent |
| QUAL-EV-0090 | REQ-EV-0090 | ADOPT | Computer Runtime | Clipboard secret is restored and never enters model/evidence body. | real-system or production-equivalent |
| QUAL-EV-0091 | REQ-EV-0091 | ADOPT | Policy Kernel | Project instruction cannot weaken org deny rule. | real-system or production-equivalent |
| QUAL-EV-0092 | REQ-EV-0092 | ADOPT | Context + Session Stores | Restart after multiple compactions and reconstruct exact task/protocol state. | real-system or production-equivalent |
| QUAL-EV-0093 | REQ-EV-0093 | ADOPT | Policy + Workspace Fabric | Trusted repo still cannot use denied network/secret capability. | real-system or production-equivalent |
| QUAL-EV-0094 | REQ-EV-0094 | REJECT | Clean-room governance | Dependency/SBOM gate rejects external reference binary/package inclusion. | architecture/absence guard |
| QUAL-EV-0095 | REQ-EV-0095 | DEFERRED | Enterprise Networking | Future enterprise test must verify configured trust root and no silent MITM. | architecture/absence guard |
| QUAL-EV-0096 | REQ-EV-0096 | ADOPT | Tool Runtime | Snapshot tool schemas across modes and verify denied/irrelevant tools absent. | real-system or production-equivalent |
| QUAL-EV-0097 | REQ-EV-0097 | ADOPT | Procedural Tool Runtime | Real coding task completes through procedural mode and every nested effect is policy/evidence-tracked. | real-system or production-equivalent |
| QUAL-EV-0098 | REQ-EV-0098 | ADOPT | Event Protocol | Provider replay preserves tool-call/result pairing across restart. | real-system or production-equivalent |
| QUAL-EV-0099 | REQ-EV-0099 | ADOPT | Agent Runtime | Compile failure followed by fix/test succeeds in same run without task crash. | real-system or production-equivalent |
| QUAL-EV-0100 | REQ-EV-0100 | ADOPT | Terminal Broker | Conformance suite exercises each field against real processes. | real-system or production-equivalent |
| QUAL-EV-0101 | REQ-EV-0101 | ADOPT | Persistence | Hard-kill backend and resume pending run without transcript inference. | real-system or production-equivalent |
| QUAL-EV-0102 | REQ-EV-0102 | ADOPT | Domain Model | State-transition tests reject impossible conflation such as command failure=thread failure. | real-system or production-equivalent |
| QUAL-EV-0103 | REQ-EV-0103 | ADOPT | Desktop Security | Malicious renderer message without schema/capability rejected. | real-system or production-equivalent |
| QUAL-EV-0104 | REQ-EV-0104 | ADOPT | MCP Hub | Real MCP test server supports list/call/cancel and audit correlation. | real-system or production-equivalent |
| QUAL-EV-0105 | REQ-EV-0105 | ADOPT | Instruction Compiler | Prompt trace proves rules/skills present only when selected and survive compaction. | real-system or production-equivalent |
| QUAL-EV-0106 | REQ-EV-0106 | ADOPT | Change Engine | Apply real patch and verify UI/evidence sees identical diff. | real-system or production-equivalent |
| QUAL-EV-0107 | REQ-EV-0107 | ADOPT | Agent Runtime | Seed failing test; model receives bounded failure and repairs without losing raw log. | real-system or production-equivalent |
| QUAL-EV-0108 | REQ-EV-0108 | ADOPT | Transport | Multi-MB terminal/browser result remains responsive and memory-bounded. | real-system or production-equivalent |
| QUAL-EV-0109 | REQ-EV-0109 | ADOPT | ExecutionBackend | Same fixture passes on local and cloud with equivalent effect/event semantics. | real-system or production-equivalent |
| QUAL-EV-0110 | REQ-EV-0110 | ADOPT | Browser Runtime | Forged peer cannot attach/take over session. | real-system or production-equivalent |
| QUAL-EV-0111 | REQ-EV-0111 | ADAPT | Context Economy | Benchmark reports cached-prefix hit/miss and compaction invalidation correctness. | real-system or production-equivalent |
| QUAL-EV-0112 | REQ-EV-0112 | ADAPT | Model Gateway | Routing record shows requested vs resolved values and policy reason. | real-system or production-equivalent |
| QUAL-EV-0113 | REQ-EV-0113 | REJECT | Architecture Governance | Architecture coverage audit rejects source claims without evidence basis. | architecture/absence guard |
| QUAL-EV-0114 | REQ-EV-0114 | ADOPT | Skill Registry | Add/remove skill on disk; registry refreshes with hash/provenance and invalid metadata fails. | real-system or production-equivalent |
| QUAL-EV-0115 | REQ-EV-0115 | ADOPT | Agent Profile Registry | Profile requesting forbidden tool receives narrowed surface. | real-system or production-equivalent |
| QUAL-EV-0116 | REQ-EV-0116 | ADOPT | Tool Runtime | Tool-schema token benchmark vs eager all-tools baseline. | real-system or production-equivalent |
| QUAL-EV-0117 | REQ-EV-0117 | ADOPT | Agent Runtime | Attempt write in Plan mode is absent/denied before execution. | real-system or production-equivalent |
| QUAL-EV-0118 | REQ-EV-0118 | ADOPT | WorkGraph UI | Edit/review plan then resume; exact version ID is recorded in execution. | real-system or production-equivalent |
| QUAL-EV-0119 | REQ-EV-0119 | ADAPT | Task Runtime | Model says done while acceptance fails; run remains incomplete. | real-system or production-equivalent |
| QUAL-EV-0120 | REQ-EV-0120 | ADOPT | WorkGraph | Restart preserves task statuses independent of chat compaction. | real-system or production-equivalent |
| QUAL-EV-0121 | REQ-EV-0121 | ADOPT | Session Store | Resume after Core crash reproduces pending state exactly. | real-system or production-equivalent |
| QUAL-EV-0122 | REQ-EV-0122 | ADOPT | Execution Timeline | New branch gets selected decisions/evidence but no stale pending approval. | real-system or production-equivalent |
| QUAL-EV-0123 | REQ-EV-0123 | ADOPT | Execution Timeline | Preview is non-mutating; revert honors optimistic hash checks. | real-system or production-equivalent |
| QUAL-EV-0124 | REQ-EV-0124 | ADOPT | Workspace Fabric | Two sessions modify separately and merge transaction detects conflict. | real-system or production-equivalent |
| QUAL-EV-0125 | REQ-EV-0125 | ADOPT | Worktree Manager | Parallel E2E verifies no cross-worktree writes. | real-system or production-equivalent |
| QUAL-EV-0126 | REQ-EV-0126 | ADOPT | CLI/API Surface | Identical task run via desktop and headless yields same canonical states. | real-system or production-equivalent |
| QUAL-EV-0127 | REQ-EV-0127 | ADOPT | Agent Runtime | Kill client; worker continues safely and reconnects. | real-system or production-equivalent |
| QUAL-EV-0128 | REQ-EV-0128 | ADOPT | MCP Hub | User/project MCP conflict resolves deterministically; credentials never enter model prompt. | real-system or production-equivalent |
| QUAL-EV-0129 | REQ-EV-0129 | ADOPT | Instruction + Memory | Conflicting rules show explicit winner and source. | real-system or production-equivalent |
| QUAL-EV-0130 | REQ-EV-0130 | ADOPT | Context Engine | Critical-fact compaction corpus meets fidelity threshold. | real-system or production-equivalent |
| QUAL-EV-0131 | REQ-EV-0131 | ADOPT | Context Inspector | Inspector totals match actual provider request envelope. | real-system or production-equivalent |
| QUAL-EV-0132 | REQ-EV-0132 | ADOPT | Evidence Index | Search returns evidence by run/step and respects tenant scope. | real-system or production-equivalent |
| QUAL-EV-0133 | REQ-EV-0133 | ADOPT | Capability Kernel | Disable consumer adapter and verify schema disappears. | real-system or production-equivalent |
| QUAL-EV-0134 | REQ-EV-0134 | ADOPT | Tool Registry | Search tool catalog then activate; permission still enforced. | real-system or production-equivalent |
| QUAL-EV-0135 | REQ-EV-0135 | ADOPT | Terminal Broker | Background process survives UI restart and output cap. | real-system or production-equivalent |
| QUAL-EV-0136 | REQ-EV-0136 | ADAPT | Policy Profiles | Mode switch requiring user action cannot be triggered by model tool call. | real-system or production-equivalent |
| QUAL-EV-0137 | REQ-EV-0137 | ADAPT | Importers | Malicious executable config is quarantined until user trusts. | real-system or production-equivalent |
| QUAL-EV-0138 | REQ-EV-0138 | ADAPT | Extension System | Extension crash/timeout cannot bypass Core or corrupt run state. | real-system or production-equivalent |
| QUAL-EV-0139 | REQ-EV-0139 | ADOPT | Hook Bus | Mutating hook cannot override final monotonic deny. | real-system or production-equivalent |
| QUAL-EV-0140 | REQ-EV-0140 | DEFERRED | Learning/Eval | Promotion requires explicit/eval gate and provenance. | architecture/absence guard |
| QUAL-EV-0141 | REQ-EV-0141 | ADAPT | Workspace Context Bridge | Review selection affects context but cannot mutate canonical source. | real-system or production-equivalent |
| QUAL-EV-0142 | REQ-EV-0142 | ADOPT | Operations | Export can replay evidence metadata and contains no credential values. | real-system or production-equivalent |
| QUAL-EV-0143 | REQ-EV-0143 | ADOPT | Workspace UI | Reconnect test verifies buckets from Core state. | real-system or production-equivalent |
| QUAL-EV-0144 | REQ-EV-0144 | ADAPT | Coordinator + TaskContract | Builder attempts out-of-scope file write and is denied. | real-system or production-equivalent |
| QUAL-EV-0145 | REQ-EV-0145 | ADOPT | Task Isolation Bundle | Parallel tasks prove isolation across every bound resource. | real-system or production-equivalent |
| QUAL-EV-0146 | REQ-EV-0146 | ADOPT | Workspace Fabric | Rebuild and pin exact environment digest. | real-system or production-equivalent |
| QUAL-EV-0147 | REQ-EV-0147 | ADOPT | Evidence Archive | Visual/browser test artifact links to revision and verification claim. | real-system or production-equivalent |
| QUAL-EV-0148 | REQ-EV-0148 | DEFERRED | Integration Broker | Connector cannot bypass task policy or tenant scope. | architecture/absence guard |
| QUAL-EV-0149 | REQ-EV-0149 | DEFERRED | Automation | Forged webhook rejected; valid trigger creates canonical task. | architecture/absence guard |
| QUAL-EV-0150 | REQ-EV-0150 | ADAPT | Parallel Change Coordinator | Overlapping writes are serialized/denied before execution. | real-system or production-equivalent |
| QUAL-EV-0151 | REQ-EV-0151 | ADOPT | Attention Manager | Each attention reason is actionable and clears from canonical event. | real-system or production-equivalent |
| QUAL-EV-0152 | REQ-EV-0152 | ADOPT | Workspace UI | UI reload derives all task state from Core APIs. | real-system or production-equivalent |
| QUAL-EV-0153 | REQ-EV-0153 | ADOPT | Context Engine | Repo-QA benchmark measures recall@K and precision. | real-system or production-equivalent |
| QUAL-EV-0154 | REQ-EV-0154 | ADOPT | Context Engine | A/B benchmark vs lexical and semantic-only baselines. | real-system or production-equivalent |
| QUAL-EV-0155 | REQ-EV-0155 | ADOPT | Context Graph | Cross-file query resolves structural path correctly. | real-system or production-equivalent |
| QUAL-EV-0156 | REQ-EV-0156 | DEFERRED | Context Engine | Multi-repo fixture prevents identity collisions. | architecture/absence guard |
| QUAL-EV-0157 | REQ-EV-0157 | ADOPT | Context Graph | Impact benchmark checks affected file/test recall. | real-system or production-equivalent |
| QUAL-EV-0158 | REQ-EV-0158 | ADOPT | Context Engine | Old commit cannot override current code truth. | real-system or production-equivalent |
| QUAL-EV-0159 | REQ-EV-0159 | ADOPT | Context Engine | Deprecated doc is downranked after source change. | real-system or production-equivalent |
| QUAL-EV-0160 | REQ-EV-0160 | ADAPT | Workspace Context Bridge | Selection influences retrieval and is visible in inspector. | real-system or production-equivalent |
| QUAL-EV-0161 | REQ-EV-0161 | ADOPT | Context Connectors | Prompt injection in ticket remains untrusted data and cannot grant tools. | real-system or production-equivalent |
| QUAL-EV-0162 | REQ-EV-0162 | ADOPT | Engineering Memory | Memory conflict/supersession is inspectable and no raw transcript auto-promotes. | real-system or production-equivalent |
| QUAL-EV-0163 | REQ-EV-0163 | ADOPT | Context Query Planner | Planner benchmark records subqueries and coverage. | real-system or production-equivalent |
| QUAL-EV-0164 | REQ-EV-0164 | ADOPT | Context Graph | Budget cap prevents runaway expansion. | real-system or production-equivalent |
| QUAL-EV-0165 | REQ-EV-0165 | ADOPT | Context Engine | Rerank improves relevant-file recall without unacceptable latency. | real-system or production-equivalent |
| QUAL-EV-0166 | REQ-EV-0166 | ADOPT | Context Pack Compiler | Budget never exceeded and required critical facts retained. | real-system or production-equivalent |
| QUAL-EV-0167 | REQ-EV-0167 | ADOPT | Context Engine | Compression fidelity corpus and handle hydration pass. | real-system or production-equivalent |
| QUAL-EV-0168 | REQ-EV-0168 | ADOPT | Context + Change Engine | Blind edit attempt with missing context is blocked/surfaced. | real-system or production-equivalent |
| QUAL-EV-0169 | REQ-EV-0169 | ADOPT | Context Engine | Prompt envelope validates provenance on every non-ephemeral fragment. | real-system or production-equivalent |
| QUAL-EV-0170 | REQ-EV-0170 | ADOPT | Context Engine | Stale cache never labeled current. | real-system or production-equivalent |
| QUAL-EV-0171 | REQ-EV-0171 | ADOPT | Context Engine | Architecture test prevents duplicate search stacks in production modules. | real-system or production-equivalent |
| QUAL-EV-0172 | REQ-EV-0172 | ADOPT | Repository Index | Cold vs incremental index benchmarks reported. | real-system or production-equivalent |
| QUAL-EV-0173 | REQ-EV-0173 | ADOPT | Context Economy | Benchmark dashboard reports quality and economics together. | real-system or production-equivalent |
| QUAL-EV-0174 | REQ-EV-0174 | ADAPT | Context Engine | Specialist has no mutation tools and produces provenance-complete pack. | real-system or production-equivalent |
| QUAL-EV-0175 | REQ-EV-0175 | ADOPT | Workspace UI | Inspector ids match PromptEnvelope. | real-system or production-equivalent |
| QUAL-EV-0176 | REQ-EV-0176 | ADOPT | Workspace Fabric | Handoff verifies no authority/secret values smuggled in capsule. | real-system or production-equivalent |
| QUAL-EV-0177 | REQ-EV-0177 | ADOPT | Tool Runtime | Large MCP catalog token benchmark proves lazy behavior. | real-system or production-equivalent |
| QUAL-EV-0178 | REQ-EV-0178 | ADAPT | Agent Runtime | Launch two real specialized children and verify independent state/tool ceilings. | real-system or production-equivalent |
| QUAL-EV-0179 | REQ-EV-0179 | ADOPT | Agent Runtime | Complete child, restart parent, send follow-up and verify lineage/state. | real-system or production-equivalent |
| QUAL-EV-0180 | REQ-EV-0180 | ADAPT | Scheduler | Dependency-sensitive scheduler keeps blocking child foreground and separable child background. | real-system or production-equivalent |
| QUAL-EV-0181 | REQ-EV-0181 | ADOPT | Skill Registry | Install extension skill, validate hash, activate without capability escalation. | real-system or production-equivalent |
| QUAL-EV-0182 | REQ-EV-0182 | ADAPT | Agent Profile Registry | Invalid/unsafe tool declaration is narrowed or rejected. | real-system or production-equivalent |
| QUAL-EV-0183 | REQ-EV-0183 | ADAPT | Extension System | Compatibility fixture imports and migration report labels mapped/skipped/conflicts. | real-system or production-equivalent |
| QUAL-EV-0184 | REQ-EV-0184 | ADOPT | Media + File Tool | Read actual PNG/PDF/audio/video with capable and incapable models; unsupported modality is explicit. | real-system or production-equivalent |
| QUAL-EV-0185 | REQ-EV-0185 | ADOPT | Media Pipeline | Scanned PDF fixture triggers bounded vision path; page range/source/model recorded. | real-system or production-equivalent |
| QUAL-EV-0186 | REQ-EV-0186 | ADAPT | Artifact/Notebook Adapter | Real ipynb read/edit preserves unrelated cells and execution metadata policy. | real-system or production-equivalent |
| QUAL-EV-0187 | REQ-EV-0187 | ADOPT | MCP Hub + Media | MCP test server returns image+text; both reach vision-capable model and evidence store. | real-system or production-equivalent |
| QUAL-EV-0188 | REQ-EV-0188 | ADOPT | Provider Adapter | Strict OpenAI-compatible test rejects embedded media but passes split follow-up representation. | real-system or production-equivalent |
| QUAL-EV-0189 | REQ-EV-0189 | ADOPT | Model Gateway | Routing refuses unsupported media model and selects eligible endpoint. | real-system or production-equivalent |
| QUAL-EV-0190 | REQ-EV-0190 | ADAPT | Input Gateway | Upload image/file through desktop/API and verify same canonical envelope. | real-system or production-equivalent |
| QUAL-EV-0191 | REQ-EV-0191 | ADOPT | Input Queue | Concurrency test sends messages mid-run and verifies exact ordering/cancellation semantics. | real-system or production-equivalent |
| QUAL-EV-0192 | REQ-EV-0192 | ADAPT | Core API | Desktop + web test observe same run; reconnect from event cursor is lossless. | real-system or production-equivalent |
| QUAL-EV-0193 | REQ-EV-0193 | ADAPT | MCP Hub | Two sessions reuse transport; config/tenant change creates separate pool entry. | real-system or production-equivalent |
| QUAL-EV-0194 | REQ-EV-0194 | ADAPT | Approval Service | Conflicting client approvals follow configured policy and are auditable. | real-system or production-equivalent |
| QUAL-EV-0195 | REQ-EV-0195 | ALREADY COVERED | Core subsystems | Requirement coverage audit proves owner/test links exist. | architecture/absence guard |
| QUAL-EV-0196 | REQ-EV-0196 | ADAPT | Skill Evolution Lab | Delete/reject candidate skill; raw traces and wiki knowledge remain intact. | real-system or production-equivalent |
| QUAL-EV-0197 | REQ-EV-0197 | ADAPT | Skill Evolution Lab | Rollback candidate and verify wiki head unchanged unless separately reverted. | real-system or production-equivalent |
| QUAL-EV-0198 | REQ-EV-0198 | EXPERIMENT | Skill Evolution Lab | Seed contradictory traces; maintainer records both with provenance/confidence rather than overwriting. | experiment report |
| QUAL-EV-0199 | REQ-EV-0199 | EXPERIMENT | Skill Evolution Lab | Candidate diff references motivating evidence IDs and changes one bounded behavior. | experiment report |
| QUAL-EV-0200 | REQ-EV-0200 | ADOPT | Skill Registry + Eval | Candidate regressing safety/quality is rejected and previous active skill remains byte-identical. | real-system or production-equivalent |
| QUAL-EV-0201 | REQ-EV-0201 | ADOPT | Skill Registry | Audit can reconstruct why each skill version was accepted/rejected. | real-system or production-equivalent |
| QUAL-EV-0202 | REQ-EV-0202 | ADAPT | Skill Package | Runtime loads purpose summary; detailed evolution wiki remains inaccessible by default. | real-system or production-equivalent |
| QUAL-EV-0203 | REQ-EV-0203 | ADOPT | Context Policy | Prompt audit confirms evolution store is absent during normal run. | real-system or production-equivalent |
| QUAL-EV-0204 | REQ-EV-0204 | ADAPT | Skill Evolution Lab | Large evolution corpus stays within token budget and provenance remains complete. | real-system or production-equivalent |
| QUAL-EV-0205 | REQ-EV-0205 | EXPERIMENT | Eval Harness | Nightly matrix reports baseline vs skill deltas per model and rejects hidden regression. | experiment report |
| QUAL-EV-0206 | REQ-EV-0206 | EXPERIMENT | Eval Harness | A/B benchmark uses same tasks/environment and records confidence intervals. | experiment report |
| QUAL-EV-0207 | REQ-EV-0207 | EXPERIMENT | Eval Harness | Promotion of evolution-lab mechanism requires statistically/practically meaningful lift vs simpler skill refinement. | experiment report |
| QUAL-EV-0208 | REQ-EV-0208 | ADOPT | Architecture Governance | Architecture dependency test shows production runtime has one Engineering Memory interface. | real-system or production-equivalent |
| QUAL-EV-0209 | REQ-EV-0209 | ADAPT | Skill Package | Package parser validates metadata/resources and rejects malformed/oversized package. | real-system or production-equivalent |
| QUAL-EV-0210 | REQ-EV-0210 | ADOPT | Skill/Tool Developer Kit | Test plugin registers, lists, invokes real effector and passes removal/reload. | real-system or production-equivalent |
| QUAL-EV-0211 | REQ-EV-0211 | ADOPT | Qualification Suite | Staging integration uses real test credential and recorded safe fixture; mock-only cannot pass. | real-system or production-equivalent |
| QUAL-EV-0212 | REQ-EV-0212 | ADOPT | Quality Gate | Docs/example runner executes declared examples and fails release on drift. | real-system or production-equivalent |
| QUAL-EV-0213 | REQ-EV-0213 | ADAPT | Skill Compiler | Token benchmark compares eager package vs compiled skill projection. | real-system or production-equivalent |
| QUAL-EV-0214 | REQ-EV-0214 | ADAPT | Skill Registry | Model cannot invoke a skill marked non-model-invocable. | real-system or production-equivalent |
| QUAL-EV-0215 | REQ-EV-0215 | ADOPT | Secret Broker | Schema inspection contains no API key field; secret redaction test passes. | real-system or production-equivalent |
| QUAL-EV-0216 | REQ-EV-0216 | REJECT | Product Scope | Dependency/SBOM and product-scope audit show no scientific-tool runtime dependency. | architecture/absence guard |
| QUAL-EV-0217 | REQ-EV-0217 | ADAPT | Tool Runtime | Compatibility matrix has canonical owner/effect/test for each source capability. | real-system or production-equivalent |
| QUAL-EV-0218 | REQ-EV-0218 | ADAPT | Agent Runtime | Explore child cannot mutate; coder child mutation requires worktree/capability. | real-system or production-equivalent |
| QUAL-EV-0219 | REQ-EV-0219 | ADOPT | Agent Admission | Leaf profile lacks spawn capability. | real-system or production-equivalent |
| QUAL-EV-0220 | REQ-EV-0220 | ADOPT | Agent Runtime | Restart and resume child with same lineage. | real-system or production-equivalent |
| QUAL-EV-0221 | REQ-EV-0221 | ADOPT | Task Runtime | Run long task, list/status/read full output/stop after UI restart. | real-system or production-equivalent |
| QUAL-EV-0222 | REQ-EV-0222 | ADAPT | Approval/Question Service | Headless run returns NEEDS_INPUT rather than hanging. | real-system or production-equivalent |
| QUAL-EV-0223 | REQ-EV-0223 | ADOPT | Media Pipeline | Oversized image/video is bounded; explicit crop improves targeted recognition. | real-system or production-equivalent |
| QUAL-EV-0224 | REQ-EV-0224 | ADAPT | MCP Hub | Proposed MCP install cannot execute until trust/credential gates pass. | real-system or production-equivalent |
| QUAL-EV-0225 | REQ-EV-0225 | ADOPT | Extension System | Unsigned/untrusted extension is quarantined. | real-system or production-equivalent |
| QUAL-EV-0226 | REQ-EV-0226 | ALREADY COVERED | Hook Bus | Coverage audit maps source to existing hook conformance tests. | architecture/absence guard |
| QUAL-EV-0227 | REQ-EV-0227 | DEFERRED | External Client Adapter | Future adapter cannot own canonical state or policy. | architecture/absence guard |
| QUAL-EV-0228 | REQ-EV-0228 | ALREADY COVERED | OutputRef | Large-output conformance test. | architecture/absence guard |
| QUAL-EV-0229 | REQ-EV-0229 | ADAPT | Tool Registry | Toolset enablement cannot expose denied tool. | real-system or production-equivalent |
| QUAL-EV-0230 | REQ-EV-0230 | ADOPT | Skill Compiler | Build/buy lint requires justification for every new tool namespace. | real-system or production-equivalent |
| QUAL-EV-0231 | REQ-EV-0231 | ADAPT | Procedural Tool Runtime | Script attempts unauthorized tool and is denied by same Kernel. | real-system or production-equivalent |
| QUAL-EV-0232 | REQ-EV-0232 | ALREADY COVERED | Agent Runtime | Existing subagent isolation E2E covers. | architecture/absence guard |
| QUAL-EV-0233 | REQ-EV-0233 | ADAPT | ExecutionBackend | Backend conformance suite runs same fixture on supported backends. | real-system or production-equivalent |
| QUAL-EV-0234 | REQ-EV-0234 | ADAPT | Browser Runtime | Accessible and canvas fixtures validate escalation hierarchy. | real-system or production-equivalent |
| QUAL-EV-0235 | REQ-EV-0235 | ALREADY COVERED | Session Index + Engineering Memory | Raw session content never auto-promotes to Engineering Memory. | architecture/absence guard |
| QUAL-EV-0236 | REQ-EV-0236 | REJECT | Product Scope | Scope audit rejects unrelated integration package in core release. | architecture/absence guard |
| QUAL-EV-0237 | REQ-EV-0237 | EXPERIMENT | Skill Evolution Lab | Skill cannot self-promote without eval/promotion transaction. | experiment report |
| QUAL-EV-0238 | REQ-EV-0238 | ADAPT | Agent Runtime | Kill/restart test proves durability beyond process-local baseline. | real-system or production-equivalent |
| QUAL-EV-0239 | REQ-EV-0239 | ADAPT | Tool Runtime | Hook tries to override deny after guard; execution remains denied. | real-system or production-equivalent |
| QUAL-EV-0240 | REQ-EV-0240 | ADAPT | Hook Bus | Unload extension removes handlers without stale mutation path. | real-system or production-equivalent |
| QUAL-EV-0241 | REQ-EV-0241 | ADAPT | Agent/Tool Profiles | Profile validation rejects unknown/unsafe capability expansion. | real-system or production-equivalent |
| QUAL-EV-0242 | REQ-EV-0242 | ADOPT | Core Architecture | Restart loses no durable truth even though hook process resets. | real-system or production-equivalent |
| QUAL-EV-0243 | REQ-EV-0243 | ALREADY COVERED | Core owners | Architecture CI blocks duplicate authority service. | architecture/absence guard |
| QUAL-EV-0244 | REQ-EV-0244 | EXPERIMENT | Adaptive Profile Evaluator | Shadow candidate never controls production run. | experiment report |
| QUAL-EV-0245 | REQ-EV-0245 | ADOPT | Reliability Layer | Fault corpus produces stable diagnostic features. | real-system or production-equivalent |
| QUAL-EV-0246 | REQ-EV-0246 | EXPERIMENT | Adaptive Profile Evaluator | Third repair attempt rejected; fallback run remains functional. | experiment report |
| QUAL-EV-0247 | REQ-EV-0247 | EXPERIMENT | Eval Registry | Rejected candidate remains audit artifact but never active. | experiment report |
| QUAL-EV-0248 | REQ-EV-0248 | EXPERIMENT | Eval Harness | Cheap but lower-correctness profile cannot promote. | experiment report |
| QUAL-EV-0249 | REQ-EV-0249 | ADOPT | Context Engine | Run same frozen retrieval benchmark profile with and without Modbit retrieval. | real-system or production-equivalent |
| QUAL-EV-0250 | REQ-EV-0250 | ADOPT | Benchmark Harness | Benchmark report includes paired distribution/confidence. | real-system or production-equivalent |
| QUAL-EV-0251 | REQ-EV-0251 | ADOPT | Benchmark Harness | Same model/task/environment across variants. | real-system or production-equivalent |
| QUAL-EV-0252 | REQ-EV-0252 | ADOPT | Benchmark Harness | Both warm agent time and cold time-to-first-use reported. | real-system or production-equivalent |
| QUAL-EV-0253 | REQ-EV-0253 | ADOPT | Benchmark Method | Benchmark prompts are identical except available capability profile. | real-system or production-equivalent |
| QUAL-EV-0254 | REQ-EV-0254 | EXPERIMENT | Context Eval | Profile A baseline, B hybrid, C structural with paired trials. | experiment report |
| QUAL-EV-0255 | REQ-EV-0255 | ADOPT | Agent Runtime | Typical task creates one primary; unnecessary swarm is not spawned. | real-system or production-equivalent |
| QUAL-EV-0256 | REQ-EV-0256 | ADOPT | Agent Profile Registry | Fallback model switch preserves agent/run identity and lineage. | real-system or production-equivalent |
| QUAL-EV-0257 | REQ-EV-0257 | ALREADY COVERED | WorkGraph | Existing plan-mode conformance test. | architecture/absence guard |
| QUAL-EV-0258 | REQ-EV-0258 | ALREADY COVERED | Session Store | Restart/resume release test. | architecture/absence guard |
| QUAL-EV-0259 | REQ-EV-0259 | ALREADY COVERED | Execution Timeline | Existing fork/revert test. | architecture/absence guard |
| QUAL-EV-0260 | REQ-EV-0260 | ALREADY COVERED | Context Engine | Existing compaction fidelity test. | architecture/absence guard |
| QUAL-EV-0261 | REQ-EV-0261 | ADAPT | Input Queue | Ask side question mid-run; main state/event cursor remains unchanged. | real-system or production-equivalent |
| QUAL-EV-0262 | REQ-EV-0262 | ADOPT | Input Queue | Multiple queued inputs preserve ordering across reconnect. | real-system or production-equivalent |
| QUAL-EV-0263 | REQ-EV-0263 | ADOPT | Agent/Task Runtime | Client disconnect/restart does not lose background task. | real-system or production-equivalent |
| QUAL-EV-0264 | REQ-EV-0264 | DEFERRED | Automation | Future schedule survives restart and preserves principal/policy. | architecture/absence guard |
| QUAL-EV-0265 | REQ-EV-0265 | ADAPT | Workspace UX | Low-risk task skips ceremony; high-risk configured task requires plan/review. | real-system or production-equivalent |
| QUAL-EV-0266 | REQ-EV-0266 | ALREADY COVERED | Usage Ledger | Usage reconciliation test. | architecture/absence guard |
| QUAL-EV-0267 | REQ-EV-0267 | ADOPT | Agent Runtime | Injected failure during admission leaves no orphan child/worktree/capacity leak. | real-system or production-equivalent |
| QUAL-EV-0268 | REQ-EV-0268 | ADOPT | Context Economy | Benchmark cache hits/misses and verify no stale context after fork/revert. | real-system or production-equivalent |
| QUAL-EV-0269 | REQ-EV-0269 | ADOPT | Artifact Store | 10MB+ tool result is paged without context overflow and digest matches raw output. | real-system or production-equivalent |
| QUAL-EV-0270 | REQ-EV-0270 | ADOPT | Effect Ledger | Tamper/delete/reorder receipt causes chain verification failure. | real-system or production-equivalent |
| QUAL-EV-0271 | REQ-EV-0271 | ADOPT | Terminal Broker | Restart desktop, replay exact terminal tail and continue input. | real-system or production-equivalent |
| QUAL-EV-0272 | REQ-EV-0272 | ADOPT | Resource Governor | Capacity exhaustion denies launch without partial side effects. | real-system or production-equivalent |
| QUAL-EV-0273 | REQ-EV-0273 | ADOPT | Session Store | Two clients attempt mutation; stale lease rejected. | real-system or production-equivalent |
| QUAL-EV-0274 | REQ-EV-0274 | ADOPT | Context Economy | Paired benchmark publishes verified-outcome economics. | real-system or production-equivalent |
| QUAL-EV-0275 | REQ-EV-0275 | ADAPT | Attention Manager | Attention item is created/cleared solely from canonical unresolved state. | real-system or production-equivalent |
| QUAL-EV-0276 | REQ-EV-0276 | ADOPT | Browser Runtime | Modern SaaS fixture executes with standard Chromium compatibility. | real-system or production-equivalent |
| QUAL-EV-0277 | REQ-EV-0277 | ADOPT | Semantic Browser Compiler | Accessible workflow completes without screenshot/OCR dependency. | real-system or production-equivalent |
| QUAL-EV-0278 | REQ-EV-0278 | ADAPT | Semantic Browser Compiler | DOM mutation makes stale ref return TARGET_STALE, never click wrong element. | real-system or production-equivalent |
| QUAL-EV-0279 | REQ-EV-0279 | ADOPT | Browser Event Protocol | Long navigation flow shows token reduction and state equivalence. | real-system or production-equivalent |
| QUAL-EV-0280 | REQ-EV-0280 | ADAPT | Browser Runtime | Known flow can reuse verified transition; changed page invalidates fingerprint. | real-system or production-equivalent |
| QUAL-EV-0281 | REQ-EV-0281 | ADAPT | MCP/Browser Gateway | Same task selects native tool when trust/policy allow; fallback works otherwise. | real-system or production-equivalent |
| QUAL-EV-0282 | REQ-EV-0282 | ADOPT | Browser Runtime | Canvas/image button fixture escalates locally; standard form does not. | real-system or production-equivalent |
| QUAL-EV-0283 | REQ-EV-0283 | ADOPT | Workspace Browser Surface | User takes over mid-run with no browser restart and agent resumes after reacquisition. | real-system or production-equivalent |
| QUAL-EV-0284 | REQ-EV-0284 | ADOPT | Context/Policy | Seed hostile page instructions; forbidden tool remains unavailable. | real-system or production-equivalent |
| QUAL-EV-0285 | REQ-EV-0285 | ADOPT | SandboxBackend | Real cloud sandbox boots fixture and passes backend conformance. | real-system or production-equivalent |
| QUAL-EV-0286 | REQ-EV-0286 | ADOPT | Sandbox Gateway | Cross-tenant sandbox handle use is denied and audited. | real-system or production-equivalent |
| QUAL-EV-0287 | REQ-EV-0287 | ADOPT | Sandbox Policy | Guest cannot reach internal/control-plane endpoints by default. | real-system or production-equivalent |
| QUAL-EV-0288 | REQ-EV-0288 | ADOPT | Secret Broker | Inspect guest image/env/artifacts; long-lived provider secret absent. | real-system or production-equivalent |
| QUAL-EV-0289 | REQ-EV-0289 | ADOPT | Guest RPC | Unknown/stale RPC version/capability is rejected. | real-system or production-equivalent |
| QUAL-EV-0290 | REQ-EV-0290 | ADOPT | Sandbox Policy | Attempt protected host/control path write fails. | real-system or production-equivalent |
| QUAL-EV-0291 | REQ-EV-0291 | ADOPT | ExecutionBackend | Backend contract test can run against local reference backend and MicroVM substrate backend. | real-system or production-equivalent |

## EPR additive qualifications

The 291 QUAL-EV rows remain unchanged. `61_EXECUTION_POLICY_QUALIFICATION_AND_ROLLOUT_GATES.md` adds QUAL-EPR-000..019 plus exact real/fault scenarios and gates A–G for the tasks in doc 49. Both qualification sets remain required; neither a new plan schema nor a dossier integrity test closes product behavior.
