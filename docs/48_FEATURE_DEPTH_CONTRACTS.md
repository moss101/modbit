# Feature Depth Contracts

> These contracts define the **minimum depth** required before a subsystem can be called implemented. They supplement individual requirement/test rows.

| Subsystem | Domain/API | Persistence | Policy | Real effector | Recovery | Evidence | UI/model projection | Mandatory real proof |
|---|---|---|---|---|---|---|---|---|
| Session/task/run | Typed IDs/state machines | Event + projection stores | tenant/session ownership | Core command handler | replay/rebuild | transition log | fleet/timeline | kill Core and restore exact task |
| Agent/subagent | AgentNode + lineage + control events | protocol + node state | capability/capacity | scheduler/executor | resume/park/cancel | child result envelope | fleet/attention | kill during child tool call and resume |
| Model provider | normalized request/event contract | request metadata/cost refs | model/tool entitlement | real provider stream | retry/fallback policy | raw/normalized event refs | streaming turn | live provider call + interrupted stream |
| Tool runtime | intent/result/failure schema | tool/protocol event refs | capability/effect gate | non-test effector | idempotency/cancel | ToolCall + effect evidence | tool state | call through production router to real effect |
| Procedural runtime | exec/wait/input API | script/output refs | no ambient authority | isolate + tools bindings | cancel/timeout | composed-call provenance | bounded result | script uses real tools under same policy |
| Context/retrieval | QueryPlan/ContextPack | versioned indexes/ledger | scope/provenance | real repo/index/LSP | stale-index rebuild | retrieval trace | context inspector | mutate repo after indexing; no stale bytes |
| Engineering memory | scoped memory record | durable scoped store | promotion/visibility | memory service | migration/conflict handling | provenance/supersession | memory inspector | transcript cannot silently become memory |
| Compaction | epoch/manifests | durable compressed epoch | context policy | compactor | stale rejection/fallback | input/output digests | transparent | race async compactions + restart |
| Checkpoints | epoch/baseline/delta refs | object store + metadata | workspace ownership | checkpoint engine | fencing/restore | digest/provenance | recovery status | restore worktree+protocol cursor after crash |
| Workspace/files | revision/path contracts | filesystem/Git state | path/protected-path | real filesystem | concurrent edit protection | change journal | code review | real multi-file edit with precondition race |
| Git/change | transaction/patch contracts | repo/worktree | protected operations | real Git | rollback/merge conflict | revision-bound diff | review/accept | ambiguous edit fails without mutation |
| Terminal/process | ExecRequest/PTY/process | replay/output refs | command/env policy | OS process broker | reattach/cancel | exit/output digests | live terminal | >10MB output + restart/replay |
| Browser | semantic entity/action/state | session/events/artifacts | origin/effect/control lease | actual Chromium | reconnect/takeover | snapshots/deltas/network refs | same live surface | act structurally, takeover, resume same session |
| Computer use | app target/action contract | control state | app/semantic risk gate | native automation bridge | watchdog/preemption | action receipt | live observe/takeover | human activity revokes controller immediately |
| Sandbox/cloud exec | sandbox/session/RPC | cloud session/checkpoint | tenant/capability/network/fs | actual isolated guest | loss/recreate/restore | guest/effect logs | run state | real cloud guest killed and recovered |
| Skills | manifest/version/select/load | registry/artifacts | provenance/signature | compiler/selector | rollback/version pin | selection trace | skill indicator | selected skill changes procedure, not authority |
| Skill evolution | candidate/eval/promotion | immutable experiment trace | promotion gate | evaluator | rollback | score/provenance | admin only | candidate cannot self-promote; regression rejected |
| Media | MediaEnvelope/ref | artifact/media store | size/type/content rules | real parsers/vision path | bounded fallback | digest/transform trace | preview/context | real image/PDF/audio/video fixtures |
| MCP/external tools | discover/call/cancel | connection/tool metadata | task-scoped activation | real transport | reconnect/cancel | external call evidence | tool UI | real local server discovery+call+cancel |
| Protected effects | EffectIntent/Decision/Receipt | immutable ledger | capability/approval | effect dispatcher | ambiguous dispatch recovery | receipt chain | approval/attention | crash between approval and dispatch; no double effect |
| Desktop surface | typed SurfaceProtocol | projection caches only | no privileged bypass | real Core/browser/terminal bridges | reconnect/replay | UI action IDs | fleet/work/code | renderer crash/restart with active run |

## Conditional execution policy depth across existing owners

Completion requires versioned conditional plan/slot validation reaching actual Core/provider/tool routes, separate statistics/confidence-feasibility, durable slot/leg/attempt/budget/epoch state, policy-owned factual assurance and independent acceptance, disposable non-committing reviewer isolation, atomic candidate apply, independent request/leg/gate accounting and authorized projections. Kill/restart, missing/undeclared slots, weak-bound cheap rejection, infeasible quality, denied canonical reviewer effects versus allowed ephemeral tests, cleanup and compatible rollback require real proof.

Eval Harness must qualify representative LCB/posterior quality and independently safe acceptance/risk/critical-surface rates, with holdout and controlled rollout. A routing gain does not waive gate failure. No second executor, statistics memory or patch-application engine is permitted. Tasks and qualification: docs 49/61; current authority: doc 06.
