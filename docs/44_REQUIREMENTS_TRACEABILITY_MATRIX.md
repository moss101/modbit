# Requirements Traceability Matrix

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Completion rule:** code is not “done” until it is wired through the real runtime and passes the release-gate real-system test with evidence.  
> **No-placeholder rule:** production code paths may not contain fake implementations, TODO return values, hard-coded success, disabled security checks, or UI-only simulations of unavailable behavior.


| Requirement | Architecture owner | Primary implementation | Release proof |
|---|---|---|---|
| Single agent-first Modbit/no IDE | SurfaceProtocol + desktop | `apps/desktop`, `packages/ui` | E2E-001 + UI audit |
| Durable session/task | Core domain/event store | `domain`, `event-store`, `protocol-state` | E2E-003/004 |
| Exact restart/resume | recovery spine | `checkpoint`, `compaction`, protocol state | E2E-004..008 |
| Memory separate from recovery | memory + state | `memory`, event/checkpoint stores | promotion/restart tests |
| Real local coding | workspace/tool/runtime | `workspace`, `git`, `terminal`, providers | E2E-001/002 |
| Dynamic tool projection | Prompt Compiler/tools | `prompt-compiler`, `tools` | E2E-011 |
| Procedural code-mode runtime | tools/procedural | `procedural-runtime` | E2E-012 |
| Subagents transactionally isolated | Core scheduler | `core-runtime`, `git`, `policy` | E2E-009/010 |
| Hybrid structural retrieval | Context Engine | `retrieval`, `context`, `diagnostics` | retrieval benchmark |
| Live same-session browser/takeover | browser/main | `browser`, Electron main | E2E-013..015 |
| Screenshot as fallback | browser semantic compiler | `browser` | E2E-014 + fallback-rate metric |
| Prompt injection isolation | context/policy/browser | `browser`, `context`, `policy` | E2E-016 |
| Protected effects/receipts | policy/effects | `effects`, `policy` | E2E-005/023 + chain test |
| Durable terminal/replay | exec broker | `modbit-execd`, `terminal` | E2E-008/021 |
| MicroVM cloud isolation | gateway/sandbox | cloud worker, gateway, guest | E2E-017/018/024 |
| No raw secrets in guest/renderer | secrets/policy | `secrets`, gateway/main | security leak suite |
| Same local/cloud runtime semantics | Core | shared Rust crates | local/cloud parity test |
| Real completion evidence | verification/release | `verification`, evidence-check | RC evidence bundle |
| Retrieval benchmark discipline | context/bench | `benchmarks/retrieval` | A/B/C report |

## Traceability rule

Every implementation task/PR names at least one requirement/decision ID; every locked requirement has at least one automated proof. An orphan feature without requirement owner is architecture drift and should not merge.


## V2 traceability extension

Source research traceability is normalized by `40_EVIDENCE_DERIVED_REQUIREMENT_LEDGER.md`: each source row carries owner + `IMP-EV-*` task + `QUAL-EV-*` qualification. Do not duplicate all rows in this high-level matrix; CI joins the two files and fails on missing IDs.

## Execution policy coverage extension

| Requirement group | Canonical owner | Implementation | Proof |
|---|---|---|---|
| REQ-EPR-000/002/003/004 | model-gateway | existing provider routing/registry modules | matching QUAL-EPR and gates A/B/C |
| REQ-EPR-001 | domain-events | Run/protocol/event contracts and migration | actual database/restart proof |
| REQ-EPR-005/006/007/009 | core-runtime | existing bounded leg executor and routing epoch | real provider/Git/cancel/restart; gates B/D/E/F |
| REQ-EPR-008 | effects-security | PolicyEnvelope and factual RealizedRisk, capability/effect checks | protected-path and reviewer denial mutation tests |
| REQ-EPR-010 | observability | complete attempt accounting and raw outcomes | provider usage/event reconciliation |
| REQ-EPR-011/012 | eval-bench | isolated replay and offline policy promotion | scratch snapshot/effect isolation and rollback; gate G |
| REQ-EPR-013 | skills | qualified model/Skill profiles and role prompts | real Skill/provider/holdout/provenance; gate G |

Exact additive REQ→EPR→QUAL and dependency rows are in doc 49; real scenarios/gates in doc 61. This implements a policy-routed model/tool executor with frontier quality backstop; no fixed model name or mandatory local SLM is architecture.

## v1.1 delta ownership and proof

| Requirement | Existing owner | Distinct implementation slice | Proof |
|---|---|---|---|
| REQ-EPR-014 | core-runtime | conditional migration/slot admission/fencing shared by compiler and runtime | actual Core/SQLite activation/restart; gate A |
| REQ-EPR-015 | eval-bench | separate versioned derived Outcome Statistics interface/materialization | real artifact/store and compiler read; gate C |
| REQ-EPR-016 | model-gateway | conservative feasibility/cold-start/infeasible fallback component | mean-versus-LCB and hard-empty cases; C/F |
| REQ-EPR-017 | verification | assurance-input versus evidence acceptance contract | independent actual checks/approval cases; A/D |
| REQ-EPR-018 | effects-security | disposable review process/path/egress/cleanup boundary | allowed ephemeral and denied canonical/external effects; E |
| REQ-EPR-019 | eval-bench | independent gate/risk calibration and critical-surface corpus | release rejection despite router gains; D/E/G |

Existing EPR-000..013 are amended in place by doc 06. Matching QUAL-EPR/EPR-E2E/EPR-FI and exact dependencies are in docs 49/61; no new owner is introduced.
