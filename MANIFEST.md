# Modbit Dossier Manifest — V3.3 EPR v1.1

> **Authority date:** 2026-09-05  
> **Generated:** 2026-09-08 by `tools/build_manifest.py`  
> **Scope:** every specification file in `docs/` plus the root governing files and tooling. The previous `99_MANIFEST.md` covered only 39 Part 2 files; this manifest covers all 89 docs.
> **Machine-readable twin:** `manifest.json` (same content, same hashes).

## Integrity rule

A dossier package is valid only if every path below exists with the listed SHA-256. `python3 tools/check_dossier.py --manifest` verifies this. Regenerate after any edit with `python3 tools/build_manifest.py`.

## Summary

| Section | Range | Files | Bytes |
|---|---|---:|---:|
| Authority and orientation | 00–09 | 8 | 65205 |
| Architecture and subsystems | 10–29 | 20 | 191488 |
| Implementation specifications | 30–39 | 10 | 80065 |
| Requirements, tasks and traceability | 40–49 | 10 | 351596 |
| Verification and testing | 50–69 | 15 | 201884 |
| Delivery and operations | 70–79 | 7 | 33831 |
| Agent process and governance | 80–97 | 18 | 109299 |
| Live state | 98–99 | 1 | 3745 |
| **Total docs** | | **89** | **1037113** |

## Specification files (`docs/`)

| # | File | Title | Section | Bytes | SHA-256 |
|---:|---|---|---|---:|---|
| 00 | `docs/00_MASTER_INDEX.md` | Modbit — AI-Agent Build Dossier V3.3 EPR v1.1 | authority | 16729 | `cac817ca1fd014438bbeab9e5c43b543f143c7fb137d8cd00a4c59d4cfc6b6cf` |
| 01 | `docs/01_START_HERE_FOR_BUILD_AGENTS.md` | Start Here for Build Agents | authority | 3036 | `e5ad750bd3c5c0a219f135bf3878c91c6230d417a66fca664becef682eb78fc2` |
| 02 | `docs/02_AUTHORITY_AND_DECISIONS.md` | Authority, Decision Register, and Conflict Resolution | authority | 16068 | `d56806f57926b7499ec7d3085208304ec679e7caaed50cc0422b15e346106160` |
| 03 | `docs/03_ARCHITECTURAL_CONFLICTS_AND_SUPERSESSIONS.md` | Architectural Conflicts and Supersessions | authority | 6175 | `6c80da9c6575a10848352f18c794b5896c2006a83d022b6513f7993fe217a95c` |
| 04 | `docs/04_REQUIREMENT_BASIS_AND_LIMITS.md` | Requirement Basis and Limits | authority | 1724 | `d9e07d840b4b006d79d00251d9525de503d5676e10e09b2229c7bfe5e2f04344` |
| 05 | `docs/05_EXECUTION_POLICY_ROUTER_ADOPTION_DECISION.md` | Execution policy router adoption decision | authority | 6756 | `ebb8376a8f733b5214f7e3ae7b14a83a865d3bfff6d2afe1a445a9f3a7763866` |
| 06 | `docs/06_EPR_V1_1_SUPERSESSION_DECISION.md` | EPR v1.1 supersession decision | authority | 8212 | `1444af3f025244525f537f5fdca31f50775f0f2857396705c306560dbd06ec10` |
| 07 | `docs/07_PRODUCT_EXTENSION_DECISION_RECORD.md` | Product extension decision record | authority | 6505 | `4c1d01b41e9aa50ec6b4d21bdcfd7aa98abee07be3e579b6e7202ad585ceb92a` |
| 10 | `docs/10_PRODUCT_PRD_AND_UX.md` | Product Requirements and UX Specification | architecture | 11316 | `ab27458161437b390eb75435877da8abade6fa7954603e0cb439022d24ebf4d9` |
| 11 | `docs/11_SYSTEM_ARCHITECTURE.md` | End-to-End System Architecture | architecture | 12638 | `34b51dd3be83dac06e6d2a50d9fc8baed4b9d7839597c9a3ebe1b38c66552233` |
| 12 | `docs/12_REPOSITORY_AND_MODULE_LAYOUT.md` | Clean Repository and Module Layout | architecture | 9439 | `9cf99c8535c14f16b216f2a477703fdc9c4a47a63867a616eea3ffcc3e3560b4` |
| 13 | `docs/13_DOMAIN_MODEL_AND_STATE_MACHINES.md` | Canonical Domain Model and State Machines | architecture | 6829 | `7c15fd3f525fe0f863ce28e00bb31d0eb8f0313babd53befd83148d18378f5c3` |
| 14 | `docs/14_AGENT_RUNTIME_AND_ORCHESTRATION.md` | Agent Runtime and Orchestration | architecture | 12027 | `af5f478cd395862bea94af024ad20747e980ed0b7c012d55a027d76b185ed429` |
| 15 | `docs/15_MODEL_ROUTER_AND_PROVIDER_GATEWAY.md` | Execution Policy Router and Provider Gateway | architecture | 7208 | `c2d264fb093ef1ac5b62685a082a5743015f6916fad49799ee529a6c91f277a3` |
| 16 | `docs/16_TOOL_CAPABILITY_AND_PROCEDURAL_RUNTIME.md` | Tool System, Capability Kernel, Procedural Runtime, and MCP | architecture | 5407 | `88f995ffc9687f2cc88f72e8f0a94c7aae602050db3dda997dc9d8bb9b052f3d` |
| 17 | `docs/17_CANONICAL_TOOL_AND_CAPABILITY_INVENTORY.md` | Canonical Tool and Capability Inventory | architecture | 5852 | `6e92b564da319488bdab3e4d17325799f5bda5e0c9e9fd0d316f0981c9b4bbe9` |
| 18 | `docs/18_CONTEXT_RETRIEVAL_AND_ENGINEERING_KNOWLEDGE.md` | Context, Retrieval, and Engineering Knowledge Engine | architecture | 5728 | `c6a362588d301b22d0f57bd11accecd6af33a9a5f7abc391afb15d7bef6f2303` |
| 19 | `docs/19_DURABLE_STATE_MEMORY_COMPACTION_CHECKPOINTS.md` | Durable State, Memory, Compaction, and Checkpoints | architecture | 4917 | `a2cd03cb090f1f19c330669becc67fcf87fe6e7c85e2524c803ad6eb432bc056` |
| 20 | `docs/20_WORKSPACE_GIT_AND_TRUSTED_CODE_SURFACE.md` | Workspace, Git, Worktrees, Diagnostics, and Trusted Code Surface | architecture | 4238 | `71e5a6df1723eb19bfa3816eeb1bee5a5b73f3250ad53a2055fb65a1c79e6129` |
| 21 | `docs/21_TERMINAL_EXECUTION_AND_SANDBOX.md` | Terminal, Execution Router, and Sandbox Architecture | architecture | 4273 | `993f563ecc713035e2ce4eb071c14fdd1cb3cd033ea0cc32a525ca542af2c64d` |
| 22 | `docs/22_BROWSER_AND_COMPUTER_USE.md` | Browser and Computer-Use Architecture | architecture | 3961 | `a7e441d9bc60785aea7dcc62fe8cef5946f3aa2551665368a360d4ec3519220b` |
| 23 | `docs/23_SECURITY_POLICY_EFFECT_LEDGER.md` | Security, Policy, Capabilities, Secrets, and Effect Ledger | architecture | 5169 | `f9f908c35490defc63d706e8748f8b3f1a0b707820f3a3b80da61268a8225500` |
| 24 | `docs/24_CLOUD_CONTROL_PLANE_AND_SYNC.md` | Cloud Control Plane, Remote Execution, and Sync | architecture | 5065 | `50516f6c15cc4353f2960c6a488cb706a706bdc6b2853b35dfb6c930df7c2eeb` |
| 25 | `docs/25_MULTIMODAL_MEDIA_AND_NOTEBOOK_RUNTIME.md` | Multimodal, Media and Notebook Runtime | architecture | 2960 | `e35d2b47f9ee33aa17e682f4226b26b8daa6ac8823a06722c3877fe416b02918` |
| 26 | `docs/26_SKILL_REGISTRY_AND_EVOLUTION.md` | Skill Registry and Evolution Integration — Skill Evolution Without a Second Runtime | architecture | 7450 | `3d7a2092543a186967759ebfcdcb4708afb73f96817d55989932b1908844b180` |
| 27 | `docs/27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md` | Execution policy router and verified orchestration | architecture | 57211 | `a95915dd93ad971360748c1fc05874aa4177793e6cfc6f7a8ba9baf3d2c6294b` |
| 28 | `docs/28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md` | Agent competence: planning, verification and repair | architecture | 12521 | `2dbf6183340ee6a0ba808eef2dd1eea147e41a75dc18e5a3ee77c5bfec117e57` |
| 29 | `docs/29_CLIENT_SURFACES_AND_SOURCE_CONTROL_INTEGRATION.md` | Client surfaces and source-control integration | architecture | 7279 | `4e378b45620945efedd7b8db3546d15ad72c247a894cb80cd650fddd70b5dae8` |
| 30 | `docs/30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md` | Protocol, APIs, and Event Schemas | implementation | 10356 | `b921e9340b85773b42433b169a6af7e8ed9b08b8557e44c871cdeea0fd1d4602` |
| 31 | `docs/31_DATABASE_AND_STORAGE_SCHEMA.md` | Database and Storage Schema | implementation | 10628 | `538226aff8cf1eddd80fe70daedfe7d4873ef2601fb0ae3a973551810c19d0a8` |
| 32 | `docs/32_DESKTOP_FRONTEND_IMPLEMENTATION.md` | Desktop Frontend Implementation | implementation | 5827 | `ac322efa1e408d2d7fb3655990d0a0828c269af317ac84d7fc459437123abaac` |
| 33 | `docs/33_CORE_AND_CLOUD_BACKEND_IMPLEMENTATION.md` | Core and Cloud Backend Implementation | implementation | 7476 | `09beec690bb13005db61c0a2b3ff60594741f49023fc8510ceb4b3e29b199f46` |
| 34 | `docs/34_OBSERVABILITY_COST_AND_OPERATIONS_DATA.md` | Observability, Cost, and Operations Data | implementation | 5702 | `4fbb53f587aca00d3b90626f3af21efc95f7241976877e45948ce6b2de5a9bf6` |
| 35 | `docs/35_DEPENDENCY_AND_BINDING_DECISIONS.md` | Dependency and Binding Decisions | implementation | 1492 | `96725161310f8c53975cc067e437c164e4d8def3c1efbfb40394573ec71ae4c6` |
| 36 | `docs/36_BUILD_BUY_DEPENDENCY_AND_LICENSE_POLICY.md` | Build / Buy / Dependency / License Decisions | implementation | 3363 | `538f768996bec4254231f7671517e68bea6cf529f51b60588a5b2dd91007071a` |
| 37 | `docs/37_EXISTING_CODE_DONOR_AND_REUSE_POLICY.md` | Existing-Code Donor and Reuse Policy | implementation | 3835 | `4dfad8325e6ed11e390952f46ff14caf6fa30fc72142f1ae58e32a22a620d4ee` |
| 38 | `docs/38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md` | Execution policy contracts and algorithms | implementation | 24554 | `4cf67ccec6005ec83478e6677a265d1fc79ce1241e4d8abce566784c01c1e4d3` |
| 39 | `docs/39_UX_FLOWS_ONBOARDING_AND_INTERACTION_BUDGETS.md` | UX flows, onboarding and interaction budgets | implementation | 6832 | `675100f531e76a11374f7b367f144214017ab12d7670daf9f21a30b131a6d562` |
| 40 | `docs/40_EVIDENCE_DERIVED_REQUIREMENT_LEDGER.md` | Evidence-Derived Requirement Ledger — Build Edition | requirements | 76383 | `d673606834f48960f015f4719c0b6fd956988469c39348aa859c4c0d91e20336` |
| 41 | `docs/41_EVIDENCE_DERIVED_IMPLEMENTATION_TASKS.md` | Evidence-Derived Implementation Tasks | requirements | 149169 | `90aadd632622877351db607f6c521b6b7d121ad55690ef0f313680b31dd26311` |
| 42 | `docs/42_EVIDENCE_DERIVED_QUALIFICATION_TEST_MATRIX.md` | Evidence-Derived Qualification Test Matrix | requirements | 49179 | `9bb8430c3b2cf0e06a7caf7caf1b40ae3ccd61dc07f36475029b557bb94ad292` |
| 43 | `docs/43_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md` | Implementation Roadmap and Verifiable Task Graph | requirements | 11018 | `051246e5acace3e6c7e35fb20d774bd1905bd0996517daadc9ec231fbe004c0e` |
| 44 | `docs/44_REQUIREMENTS_TRACEABILITY_MATRIX.md` | Requirements Traceability Matrix | requirements | 5473 | `36bb27a4cbbc50b24bed92e76d3145c286ed2a27943a8ba9509238c42c25fb31` |
| 45 | `docs/45_REQUIREMENT_TO_TASK_TO_TEST_TRACEABILITY.md` | Requirement → Task → Test Traceability | requirements | 1581 | `4c3822133dd045eeefc3da2da681d22bb02458ecec82b92478b9292a24d3a3fa` |
| 46 | `docs/46_REQUIREMENT_COVERAGE_FREEZE_GATE.md` | Requirement Coverage Freeze Gate | requirements | 1656 | `abce92a0c58412c47dcf9cfd75cea628883ad34aa876bfb36bce94291e968368` |
| 47 | `docs/47_REQUIREMENT_COVERAGE_AUDIT_REPORT.md` | Requirement Coverage Audit Report — Build Edition | requirements | 1999 | `6010cbab8bca22535ba7420326ff3dd2ff382cda8ddfa5194864ae2ba6e91455` |
| 48 | `docs/48_FEATURE_DEPTH_CONTRACTS.md` | Feature Depth Contracts | requirements | 5942 | `18b1996a420aeb09dd60da22fddfed18b77080635ebc88f7cbdb5c7dedffd7e6` |
| 49 | `docs/49_EXECUTION_POLICY_REQUIREMENTS_AND_TASKS.md` | Execution policy requirements and implementation tasks | requirements | 49196 | `e90bfba1d3f3594f1ff80bf629b2838be6d3423f4350f9c34c6fd8be4cf0b005` |
| 50 | `docs/50_TEST_STRATEGY_REAL_SYSTEM_GATES.md` | Test Strategy — Real-System Completion Gates | verification | 5886 | `9c25cf0c759ffb1c3edbe90c42e83b294c49a6b31a7dd43b6206752e2c0f65ba` |
| 51 | `docs/51_E2E_ACCEPTANCE_TEST_CATALOG.md` | End-to-End Acceptance Test Catalog | verification | 7941 | `94cb5e496f2fb7d39fa8bd18e9f94da048481c607ba4aabfcd750b29cb5b213c` |
| 52 | `docs/52_SECURITY_THREAT_MODEL_AND_TESTS.md` | Security Threat Model and Verification | verification | 5644 | `8d299543293dfdbf155e5609c4bf18022fe0f89399620191f0840852ab5684d1` |
| 53 | `docs/53_PERFORMANCE_AND_BENCHMARK_PLAN.md` | Performance, Context Economics, and Benchmark Plan | verification | 7144 | `a2fd331c6d0c824dcdb04b82766763f748ce44107884985122a12985c1d23d4b` |
| 54 | `docs/54_FAULT_INJECTION_AND_RECOVERY_CATALOG.md` | Fault Injection and Recovery Catalog | verification | 2266 | `3bb42df3f654adb39d146e74bee48b8cb9043b567027a59ee92aceca3181010f` |
| 55 | `docs/55_MUTATION_NEGATIVE_AND_CHAOS_TEST_POLICY.md` | Mutation, Negative and Chaos Test Policy | verification | 1995 | `ab51c19d80eded9dba1d000b1af5d390bf3463a6dbf92aba886812db7994dc24` |
| 56 | `docs/56_TOOL_CAPABILITY_CONFORMANCE.md` | Tool Parity and Capability Conformance — Real Effect Tests | verification | 4111 | `8692fb01100e5c7368cfb73ed06b587e8690e7bc58b2c15c8b98a20f98bc86df` |
| 57 | `docs/57_SKILL_EVOLUTION_REAL_TESTS.md` | Skill Evolution Real-System Tests | verification | 3466 | `fa5335a1367e464dc7c51cffbc972c803b26781b9e17163c81df25b4c2853f21` |
| 58 | `docs/58_MULTIMODAL_MEDIA_REAL_TESTS.md` | Multimodal / Media Real-System Tests | verification | 3391 | `f04a81fdf805ecac83242ff96948ad1467acd11308ac767029935efc1e7801d9` |
| 59 | `docs/59_RELEASE_ZERO_PROOF_SCENARIO.md` | Release Zero — Single Proof Scenario | verification | 3669 | `15fbb005f7a8196b4987468034f8de113dcd15a1ebd3f34dba822075012064fc` |
| 60 | `docs/60_RELEASE_ZERO_EXPANDED_PROOF.md` | Release Zero Expanded Proof — Clean-Slate V2 | verification | 4001 | `0a3cc3ce5329c15cafb402833cb6bd70c9c6e4afdbdb838d2a811176c975939e` |
| 61 | `docs/61_EXECUTION_POLICY_QUALIFICATION_AND_ROLLOUT_GATES.md` | Execution policy qualification and rollout gates | verification | 42076 | `27acefadb50e86b616bbe64818f9f32ef04df15a3ba7bd4c9ab5344651a4d15b` |
| 62 | `docs/62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md` | Product extension requirements, tasks and qualifications | verification | 88049 | `fe1839dd911208e91d35105f9fa82bec12d8a77a29d955324ae898226990b8b5` |
| 63 | `docs/63_AGENT_COMPETENCE_BENCHMARKS_AND_REGRESSION_SUITES.md` | Agent competence benchmarks and regression suites | verification | 6006 | `238ff74022c2a800de55badb2b7c67318b9746f90fcb717bdbd8e03fac0f516c` |
| 64 | `docs/64_VERIFICATION_EXECUTION_CONTRACTS.md` | Verification execution contracts: baseline, targeting, result normalization, flake handling and diff invariants | verification | 16239 | `2ebf3003e7f1059fe71db444eec5f1df677088d94bafa32e3c528df865768155` |
| 70 | `docs/70_CI_CD_RELEASE_AND_SUPPLY_CHAIN.md` | CI/CD, Release Engineering, and Supply Chain | delivery | 3502 | `a2cc865ecdd804938d638d5ffc3ba66581249787c1f452dfc69b45d27757a706` |
| 71 | `docs/71_OPERATIONS_RUNBOOK.md` | Operations and Incident Runbook | delivery | 4954 | `f633d3c4c1af5e81799c6497d603b448b239ae00b8169c79828d641d1f1b51be` |
| 72 | `docs/72_RISK_REGISTER_AND_OPEN_DECISIONS.md` | Risk Register and Open Technical Decisions | delivery | 9116 | `9e4f22e5d977b82b8d059a1a829e783a2dabbc1662569a3d581d0e2a6b4ba47e` |
| 73 | `docs/73_RELEASE_BLOCKERS_AND_STOP_THE_LINE_RULES.md` | Release Blockers and Stop-the-Line Rules | delivery | 1825 | `00103375774eed06e6afa60843bec3a02482d202ef1c37ad0d5f46abe79dddc6` |
| 74 | `docs/74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md` | Package Integrity and Build Coverage | delivery | 4468 | `b4e7687347e9ea62ef38e1e7e1b4c82669b487c15e6e0ef8a8ab8d8331cd3698` |
| 75 | `docs/75_PHASED_RELEASE_PLAN_AND_READINESS.md` | Phased release plan and readiness | delivery | 4279 | `6f753c623b64d361d02e590f9ee446d180d3a524b411b8f305183096e396d2ac` |
| 76 | `docs/76_LANGUAGE_AND_PLATFORM_SUPPORT_MATRIX.md` | Language and platform support matrix | delivery | 5687 | `753831f02bc261061c80249f4a7eca23e6e8891f886df25b52bb41331fc80b3f` |
| 80 | `docs/80_ANTI_SUPERFICIAL_IMPLEMENTATION_STANDARD.md` | Anti-Superficial Implementation Standard | governance | 3193 | `9d5ab7ddbe39110cff675b57fb75c3ec7fd3173480865674f3073489761fcb0f` |
| 81 | `docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md` | Architecture Guardrails and Forbidden Duplication | governance | 2671 | `b87503de04d9859fe6cc1694ac65653b430e9b52ff9cc1b5ed043f7a5c66dded` |
| 82 | `docs/82_NO_PLACEHOLDER_PRODUCTION_EVIDENCE_GATE.md` | No-Placeholder Production Evidence Gate | governance | 2418 | `f8ee13f8a255ed071aaa471d2c9a16dbe0e77900faa60d40222a299fe6a975d1` |
| 83 | `docs/83_DEFINITION_OF_DONE_AND_ACCEPTANCE.md` | Definition of Done and Acceptance Criteria | governance | 5587 | `19caf0963ae8bc2ef8940d84df3f6d2f34d69464916cfb2ea57aabdf2b930a1e` |
| 84 | `docs/84_EXISTING_CODE_FEATURE_AUDIT_PROTOCOL.md` | Existing-Code Feature Audit Protocol | governance | 1661 | `30747e11bc05265a7785c06be0864104f1c5f9cc58c60afb1c30830caace84b1` |
| 85 | `docs/85_AGENT_TASK_EXECUTION_PROTOCOL.md` | Agent Task Execution Protocol | governance | 1657 | `3ab0923199adda8c434022f7f7887a2307b64d2385c78f16a71b4d24488cde2b` |
| 86 | `docs/86_TASK_CARD_TEMPLATE.md` | Task Card Template | governance | 1524 | `49cc87943da0b4795f707e1963860fc729f5a1ec8f505c609cbc7cff71d52278` |
| 87 | `docs/87_HANDOFF_AND_MANIFEST_PROTOCOL.md` | Handoff and Manifest Protocol | governance | 1007 | `cec0530ae98473c30c74d527561f795bbab5f4e323c4ade9d0973d4f95e9d5df` |
| 88 | `docs/88_PARALLEL_AGENT_COORDINATION_RULES.md` | Parallel Agent Coordination Rules | governance | 1087 | `ddf65450d6d2974498c0fb63f6a12e80aeda3aae964d0b2f92c5a4caa4c2f4fc` |
| 89 | `docs/89_BUILD_AGENT_CONTEXT_LOADING_POLICY.md` | Build-Agent Context Loading Policy | governance | 1106 | `c754c7998ae1fe378b00259ce7adde4d09c242dba56e275ac85930a5cd1d5312` |
| 90 | `docs/90_PR_CHANGE_EVIDENCE_TEMPLATE.md` | PR / Change Evidence Template | governance | 1211 | `60feb8f87ef7d44982f8cb1e9734f10d885462abbb9a91701c60d88f3323c715` |
| 91 | `docs/91_FEATURE_COMPLETION_AUDIT.md` | Feature Completion Audit | governance | 1074 | `810c4488a24258cff4ed87cd0ba6cea6b21cda5834b44bc02bf4c9e0a4eda3f5` |
| 92 | `docs/92_BUILD_EVIDENCE_AND_DEPENDENCY_MANIFEST.md` | Build Evidence and Dependency Manifest | governance | 1048 | `69be7ff64cbfa647f40f0609af1194497da9f93e43f147378fc39ef8f1b5d37d` |
| 93 | `docs/93_STATUS_VOCABULARY_AND_LIFECYCLE.md` | Status Vocabulary and Lifecycle Reconciliation | governance | 9218 | `5121ef5959610b83b1e0f643a202c96cbb88e7c7e47d1b65149ca720f3f278f1` |
| 94 | `docs/94_EXECUTION_POLICY_DOSSIER_TASK_AND_HANDOFF.md` | Execution policy dossier task and handoff | governance | 11506 | `bc3c83f033db8ca5934751227cce2437f4cd7772a21917551618ae03f79b81ba` |
| 95 | `docs/95_EPR_V1_1_DOSSIER_TASK_AND_HANDOFF.md` | EPR v1.1 dossier task and handoff | governance | 10525 | `264069b600baf40d1e54d2f89be7e9804a8b77c0dbf3ddb0a9dca41111582413` |
| 96 | `docs/96_DOSSIER_GOVERNANCE_MAINTENANCE_TASK_AND_HANDOFF.md` | Dossier governance maintenance task and handoff | governance | 9070 | `af650309e48dd7fd60dd6d286c460ed7c3128a2201964078c7f50aea385bfb8c` |
| 97 | `docs/97_DOSSIER_MAINTENANCE_LOG.md` | Dossier maintenance log | governance | 43736 | `ae48c1832a920977def82d1ba4b7d492d4281c60a3723beaef613d3144c97073` |
| 98 | `docs/98_BUILD_MANIFEST.md` | Build Manifest | live-state | 3745 | `713222211a17c98c13f5528167c6e0df022dabdd8916659c356b4a1ae1bb0fa1` |

## Root governing files and tooling

| File | Role | Bytes | SHA-256 |
|---|---|---:|---|
| `AGENTS.md` | build-agent operating contract (highest authority) | 9512 | `52facabdca45abd2fcdaf10a7b0141057895217af95560395dc0d24a8793b64f` |
| `MODBIT-PATCH-EPR-v1.1-Supersession-and-Refinement.md` | source patch provenance | 20229 | `30dbacdb3a37b364f535f55ed7bf4ea8fa35d70cd6f85c9a61897ac9db72f2bd` |
| `MODBIT-PATCH-Execution-Policy-Router-and-Verified-Multi-Model-Orchestration.md` | source patch provenance | 54208 | `9e0cee7d49d442033b875e250a61a212b898dd6f4bf35826cdad5ffeb71edb66` |
| `README.md` | human orientation | 10200 | `575c1c947f4064ae19894a43916f66fca49974a3002a368c05314d80f3d7ccff` |
| `SKILLS.md` | governed procedures for agents | 18306 | `d6a62b1df940c10e1e4dec05af6a8ce7c94b65018c9e24a6bd76f88a7978d27c` |
| `evidence/dossier-epr-v1.1/baseline.json` | retained evidence | 11128 | `ae11c9be07272788d2957bff45bc1333f879e121d2b5205ebc01ce780ec62543` |
| `evidence/dossier-epr-v1.1/tests.log` | retained evidence | 2071 | `2f4bbebb6db1f0f0c6802f8dee65e5d6aa7933db6d1bda28a8acd70aa9bd88b3` |
| `evidence/dossier-epr-v1.1/validation.json` | retained evidence | 14010 | `288bf2fff83122240110c926d18d8ef616a9257fc62bc54eb70b536088dfbfb5` |
| `evidence/dossier-epr/baseline.json` | retained evidence | 9547 | `a5eb9628f914f0fde92da4cddf6f47a7628643d663823e96dca1352d55d22eb2` |
| `evidence/dossier-epr/tests.log` | retained evidence | 1516 | `76460cd8a33de170d5308d528f0437a8bfa0f4fc36999d92469f7e7f8a88b090` |
| `evidence/dossier-epr/validation.json` | retained evidence | 12563 | `ee2d386295ecb31c3b359fdba301db35b9586fb63defba4b1027f71b084a3efe` |
| `evidence/dossier-gov-002/baseline.json` | retained evidence | 12442 | `cfcc5c2fafd72b51c539c1dea7375f991f123c3ee4f4b80e70126a8de941501d` |
| `evidence/dossier-gov-002/tests.log` | retained evidence | 2384 | `a26eed18096f69be8e2ea1ede09a1b08926a1e34074c2c1ca2ebff06bf7b86fe` |
| `evidence/dossier-gov-002/validation.json` | retained evidence | 14887 | `1ed2daef187391d67483160273931e79c96bf7d3c0f09c42da46933d40318829` |
| `evidence/dossier-gov-003/baseline.json` | retained evidence | 12931 | `7e84a7abb4cebaed652a689f2921fbbae622708e87fa57372c79b801990bd013` |
| `evidence/dossier-gov-003/tests.log` | retained evidence | 2525 | `9a9a4f4965c44ac8e0f70eae2fba09f00d32ed99e5cb98bb4436666bec5b4666` |
| `evidence/dossier-gov-003/validation.json` | retained evidence | 15139 | `5758c95dfa054a9e402acbe287ac95fadea40fa5347ca1c52d1ca1cb41f4dbd6` |
| `evidence/dossier-gov-004/baseline.json` | retained evidence | 13157 | `d8eb58214c0c1a49dacf825699c8bfc063a8e453d2d200f6fc42d4323cabedb9` |
| `evidence/dossier-gov-004/tests.log` | retained evidence | 2592 | `ba61cc34aec9c28c5b3a2a27b40e67c1f65200d8f9076a837df250c081cb8cd1` |
| `evidence/dossier-gov-004/validation.json` | retained evidence | 14824 | `38b271df486c68ff1afcd7ce5779ea371f956d7583a85edb8d9e7b7dc5736876` |
| `evidence/dossier-gov/baseline.json` | retained evidence | 11772 | `f9c8db7cc997add9aaa708929bc14bde5a04a84147c87b73ed5a78be9a43b6f3` |
| `evidence/dossier-gov/tests.log` | retained evidence | 2307 | `8a0e53ed17231b88a8be6aec74cf98f98812bae46574c89f2c2ae7bac6368a58` |
| `evidence/dossier-gov/validation.json` | retained evidence | 14440 | `d1d5ddad3e4fee8e441610d48d1de489c788fb8e2b363b9336207bbf27dcbfc9` |
| `evidence/dossier-px-001/baseline.json` | retained evidence | 13517 | `d5b4e61277f1d231e41d95e36619d0cc22b3c9aaad8e04fb65b0d6ce08fd5ed7` |
| `evidence/dossier-px-001/tests.log` | retained evidence | 2815 | `4dc3b685e5625df8865ecd1ee59e2fe99a93e2a916812506187f69872a0e3588` |
| `evidence/dossier-px-001/validation.json` | retained evidence | 15374 | `053284b03b49af6e2c6cd0052c860dd8e8b459d9a5545dd98db004559c74566b` |
| `evidence/dossier-px-002/baseline.json` | retained evidence | 14341 | `5e85f2b06e899461ab764d6a9d92ad1c8193cce8966a682d09219750fe90222d` |
| `evidence/dossier-px-002/tests.log` | retained evidence | 2897 | `0d36a96d1f97bfc46d2dad0c583a197e873bfa962eaa5728b9e798d7d4a4ee3b` |
| `evidence/dossier-px-002/validation.json` | retained evidence | 15451 | `bf759b522fe3bf79e1c7003867450dab0590693d699ec3db3f65ba39ba16ecaf` |
| `evidence/dossier-px-003/baseline.json` | retained evidence | 14790 | `1259647e27ef73446cd508e5e23aec14e04689fd510de06004e5820533fb4592` |
| `evidence/dossier-px-003/tests.log` | retained evidence | 2978 | `384d9475e07882a5dbf9b34df997e4b38078150d629810c5285c6a43e1399f1b` |
| `evidence/dossier-px-003/validation.json` | retained evidence | 15714 | `22588ecdf39ddd1166502cbc316f3b62a63767f3e817e20822a469c20f8724b8` |
| `evidence/dossier-px-004/baseline.json` | retained evidence | 15399 | `54c87199a802af01c7bcdc7e17a95adbbad529c130f8c49e8bca5cf2176d6ee6` |
| `evidence/dossier-px-004/tests.log` | retained evidence | 3052 | `c63c87f76002f1ae67ebba70ea53977f352cf7ec4edda57833f4afc325e50679` |
| `evidence/dossier-px-004/validation.json` | retained evidence | 15847 | `019df047fb26509756b92d54cb3a82748d8f4fb5ed6f4958b2a511fd0d426e99` |
| `evidence/dossier-px-005/baseline.json` | retained evidence | 15853 | `584c79505481975c194fcf718edc5d97c9f532147c17d3e3aa074db043377e97` |
| `evidence/dossier-px-005/tests.log` | retained evidence | 3140 | `1bf71c1c7a133f4f714fe41f18b4d153c3de9b68f53ee41867c0e0311fd5eefb` |
| `evidence/dossier-px-005/validation.json` | retained evidence | 15961 | `8189e703354696361e179b539ea5bed5ed3f5d2b5e78ae3bf810823ad650bb24` |
| `evidence/dossier-px-006/baseline.json` | retained evidence | 16080 | `b0cd8948df0a7c91df18027fffb8d52ca28641144c8122818d23680733d17ac4` |
| `evidence/dossier-px-006/tests.log` | retained evidence | 3230 | `e45b9fac1be0036aefd60fb9f67ce07c9dde430dfb82b5f7e616d503a6fcf731` |
| `evidence/dossier-px-006/validation.json` | retained evidence | 16174 | `048c2969e4cd3424647d0e67c3937fe84a59584a22f7c34cfb51b348224736fa` |
| `evidence/m0/M0.1/TASK_CARD.md` | retained evidence | 4505 | `c597a9e74a5fb4bb02efad5915a74c526192cd26775d613df5f628a24f44522e` |
| `evidence/m0/M0.1/ci-run-34253119985.json` | retained evidence | 201 | `a200a78dc118e491331d8a0922f83ca924078dffa05665a157ef7a5350dd9854` |
| `evidence/m0/M0.1/evidence.json` | retained evidence | 2842 | `5f3e3bab51cfcf49a2e1413e4c2dde9d15050f8866f406b4e5bf491ec5f08a67` |
| `evidence/m0/M0.1/fresh-clone-linux-dossier-425d80d.log` | retained evidence | 209 | `f415eee1654ba75f6776bc97e9c95498ab9b5a33aa7b45e071c7bc89fe6a2b91` |
| `evidence/m0/M0.1/fresh-clone-linux-node-425d80d.log` | retained evidence | 733 | `d0b9bea9982714039d4de44675e0b62c5aadf658ba7bc431bd8d74cbfad900da` |
| `evidence/m0/M0.1/fresh-clone-linux-rust-425d80d.log` | retained evidence | 705 | `e4479b5b004531c95e0671d3f0fc123547a37ca272417c58d51d8170558e9d96` |
| `evidence/m0/M0.1/fresh-clone-macos-425d80d.log` | retained evidence | 34857 | `c85b7e2c0c538764ffab1fa9dd5a710809526af2f06ce6d30a57f09b65ad1cd8` |
| `evidence/m0/M0.1/inject-failure.log` | retained evidence | 235 | `dc63bccf2385b3a07a3212dab9d175503e979e91fa6a132d17197eb2765c0db0` |
| `graph/PROJECT_GRAPH.md` | human view of the graph | 40960 | `81e960a6f0b2ae5c46429d66317410b4b9916aa84afc39c72cbe50eaa018317c` |
| `graph/project-graph.json` | project driver graph with live status | 1008382 | `5d780627ecb4483a9215a4145e8c06a2fcb9a6a2443701de179eb946e094d7f3` |
| `tools/build_graph.py` | regenerates graph structure from docs | 45304 | `473c9881a97b045102a3f7e71e651553d6997491bb94dc0bc2bc90ad23ce93de` |
| `tools/build_manifest.py` | regenerates this manifest | 14792 | `8b162fc559327ea57f133ea269b165e37e2f3538e85166ca2d58cd6b02dd781b` |
| `tools/check_dossier.py` | integrity gate | 16615 | `17490633d9f5e4c25322c197cbbe765b6e3573cec639c152cd7744c792995e9c` |
| `tools/dossier_epr.py` | parses additive EPR authority and traceability | 9310 | `4b5e399ee1b4886662294e3780d7f0630195e29c8198256ff7719d5236591a18` |
| `tools/dossier_px.py` | parses the additive product-extension ledger and phased release rules | 6931 | `0a7dd5bd5872fb1d959c56fd017bd3916062d0f60d1bc51b9abceddc134202e2` |
| `tools/graph.py` | query/update graph | 36313 | `76f2a79fca3c305205f34573b94118c99511944833e8e602bf0eba7e2ac305cb` |
| `tools/test_dossier.py` | copied-package integration and negative tests | 30510 | `1bdaf62e9e4c332df96ecd5c994daffb2846f9c8bcbca69bb333ec17292d6ad4` |

## Rename map (V3 flat numbering → V3.1 `docs/`)

V3 reused numbers 17–29 for two different file sets. V3.1 assigns one number per file, grouped by section. Content was not changed by the move except cross-reference rewrites, removal of de-branding artifacts, and the merge noted below.

| V3 file | V3.1 location |
|---|---|
| `00_MASTER_INDEX.md` | `00_MASTER_INDEX.md` |
| `00_START_HERE_FOR_BUILD_AGENTS.md` | `01_START_HERE_FOR_BUILD_AGENTS.md` |
| `01_AUTHORITY_AND_DECISIONS.md` | `02_AUTHORITY_AND_DECISIONS.md` |
| `23_ARCHITECTURAL_CONFLICTS_AND_SUPERSESSIONS.md` | `03_ARCHITECTURAL_CONFLICTS_AND_SUPERSESSIONS.md` |
| `17_REQUIREMENT_BASIS_AND_LIMITS.md` | `04_REQUIREMENT_BASIS_AND_LIMITS.md` |
| `02_PRODUCT_PRD_AND_UX.md` | `10_PRODUCT_PRD_AND_UX.md` |
| `03_SYSTEM_ARCHITECTURE.md` | `11_SYSTEM_ARCHITECTURE.md` |
| `04_REPOSITORY_AND_MODULE_LAYOUT.md` | `12_REPOSITORY_AND_MODULE_LAYOUT.md` |
| `05_DOMAIN_MODEL_AND_STATE_MACHINES.md` | `13_DOMAIN_MODEL_AND_STATE_MACHINES.md` |
| `06_AGENT_RUNTIME_AND_ORCHESTRATION.md` | `14_AGENT_RUNTIME_AND_ORCHESTRATION.md` |
| `07_MODEL_ROUTER_AND_PROVIDER_GATEWAY.md` | `15_MODEL_ROUTER_AND_PROVIDER_GATEWAY.md` |
| `08_TOOL_CAPABILITY_AND_PROCEDURAL_RUNTIME.md` | `16_TOOL_CAPABILITY_AND_PROCEDURAL_RUNTIME.md` |
| `21_CANONICAL_TOOL_AND_CAPABILITY_INVENTORY.md` | `17_CANONICAL_TOOL_AND_CAPABILITY_INVENTORY.md` |
| `09_CONTEXT_RETRIEVAL_AND_ENGINEERING_KNOWLEDGE.md` | `18_CONTEXT_RETRIEVAL_AND_ENGINEERING_KNOWLEDGE.md` |
| `10_DURABLE_STATE_MEMORY_COMPACTION_CHECKPOINTS.md` | `19_DURABLE_STATE_MEMORY_COMPACTION_CHECKPOINTS.md` |
| `11_WORKSPACE_GIT_AND_TRUSTED_CODE_SURFACE.md` | `20_WORKSPACE_GIT_AND_TRUSTED_CODE_SURFACE.md` |
| `12_TERMINAL_EXECUTION_AND_SANDBOX.md` | `21_TERMINAL_EXECUTION_AND_SANDBOX.md` |
| `13_BROWSER_AND_COMPUTER_USE.md` | `22_BROWSER_AND_COMPUTER_USE.md` |
| `14_SECURITY_POLICY_EFFECT_LEDGER.md` | `23_SECURITY_POLICY_EFFECT_LEDGER.md` |
| `15_CLOUD_CONTROL_PLANE_AND_SYNC.md` | `24_CLOUD_CONTROL_PLANE_AND_SYNC.md` |
| `20_MULTIMODAL_MEDIA_AND_NOTEBOOK_RUNTIME.md` | `25_MULTIMODAL_MEDIA_AND_NOTEBOOK_RUNTIME.md` |
| `19_SKILL_REGISTRY_AND_EVOLUTION.md` | `26_SKILL_REGISTRY_AND_EVOLUTION.md` |
| `17_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md` | `30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md` |
| `18_DATABASE_AND_STORAGE_SCHEMA.md` | `31_DATABASE_AND_STORAGE_SCHEMA.md` |
| `19_DESKTOP_FRONTEND_IMPLEMENTATION.md` | `32_DESKTOP_FRONTEND_IMPLEMENTATION.md` |
| `20_CORE_AND_CLOUD_BACKEND_IMPLEMENTATION.md` | `33_CORE_AND_CLOUD_BACKEND_IMPLEMENTATION.md` |
| `21_OBSERVABILITY_COST_AND_OPERATIONS_DATA.md` | `34_OBSERVABILITY_COST_AND_OPERATIONS_DATA.md` |
| `27_DEPENDENCY_AND_BINDING_DECISIONS.md` | `35_DEPENDENCY_AND_BINDING_DECISIONS.md` |
| `30_BUILD_BUY_DEPENDENCY_AND_LICENSE_POLICY.md` | `36_BUILD_BUY_DEPENDENCY_AND_LICENSE_POLICY.md` |
| `28_EXISTING_CODE_DONOR_AND_REUSE_POLICY.md` | `37_EXISTING_CODE_DONOR_AND_REUSE_POLICY.md` |
| `29_OLD_REPO_DONOR_MIGRATION_RULES.md` | `37_EXISTING_CODE_DONOR_AND_REUSE_POLICY.md (merged; was a strict subset)` |
| `18_EVIDENCE_DERIVED_REQUIREMENT_LEDGER.md` | `40_EVIDENCE_DERIVED_REQUIREMENT_LEDGER.md` |
| `40_EVIDENCE_DERIVED_IMPLEMENTATION_TASKS.md` | `41_EVIDENCE_DERIVED_IMPLEMENTATION_TASKS.md` |
| `35_EVIDENCE_DERIVED_QUALIFICATION_TEST_MATRIX.md` | `42_EVIDENCE_DERIVED_QUALIFICATION_TEST_MATRIX.md` |
| `27_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md` | `43_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md` |
| `32_REQUIREMENTS_TRACEABILITY_MATRIX.md` | `44_REQUIREMENTS_TRACEABILITY_MATRIX.md` |
| `56_REQUIREMENT_TO_TASK_TO_TEST_TRACEABILITY.md` | `45_REQUIREMENT_TO_TASK_TO_TEST_TRACEABILITY.md` |
| `24_REQUIREMENT_COVERAGE_FREEZE_GATE.md` | `46_REQUIREMENT_COVERAGE_FREEZE_GATE.md` |
| `39_REQUIREMENT_COVERAGE_AUDIT_REPORT.md` | `47_REQUIREMENT_COVERAGE_AUDIT_REPORT.md` |
| `22_FEATURE_DEPTH_CONTRACTS.md` | `48_FEATURE_DEPTH_CONTRACTS.md` |
| `22_TEST_STRATEGY_REAL_SYSTEM_GATES.md` | `50_TEST_STRATEGY_REAL_SYSTEM_GATES.md` |
| `23_E2E_ACCEPTANCE_TEST_CATALOG.md` | `51_E2E_ACCEPTANCE_TEST_CATALOG.md` |
| `24_SECURITY_THREAT_MODEL_AND_TESTS.md` | `52_SECURITY_THREAT_MODEL_AND_TESTS.md` |
| `25_PERFORMANCE_AND_BENCHMARK_PLAN.md` | `53_PERFORMANCE_AND_BENCHMARK_PLAN.md` |
| `49_FAULT_INJECTION_AND_RECOVERY_CATALOG.md` | `54_FAULT_INJECTION_AND_RECOVERY_CATALOG.md` |
| `50_MUTATION_NEGATIVE_AND_CHAOS_TEST_POLICY.md` | `55_MUTATION_NEGATIVE_AND_CHAOS_TEST_POLICY.md` |
| `38_TOOL_CAPABILITY_CONFORMANCE.md` | `56_TOOL_CAPABILITY_CONFORMANCE.md` |
| `36_SKILL_EVOLUTION_REAL_TESTS.md` | `57_SKILL_EVOLUTION_REAL_TESTS.md` |
| `37_MULTIMODAL_MEDIA_REAL_TESTS.md` | `58_MULTIMODAL_MEDIA_REAL_TESTS.md` |
| `33_RELEASE_ZERO_PROOF_SCENARIO.md` | `59_RELEASE_ZERO_PROOF_SCENARIO.md` |
| `42_RELEASE_ZERO_EXPANDED_PROOF.md` | `60_RELEASE_ZERO_EXPANDED_PROOF.md` |
| `26_CI_CD_RELEASE_AND_SUPPLY_CHAIN.md` | `70_CI_CD_RELEASE_AND_SUPPLY_CHAIN.md` |
| `31_OPERATIONS_RUNBOOK.md` | `71_OPERATIONS_RUNBOOK.md` |
| `34_RISK_REGISTER_AND_OPEN_DECISIONS.md` | `72_RISK_REGISTER_AND_OPEN_DECISIONS.md` |
| `54_RELEASE_BLOCKERS_AND_STOP_THE_LINE_RULES.md` | `73_RELEASE_BLOCKERS_AND_STOP_THE_LINE_RULES.md` |
| `55_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md` | `74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md` |
| `16_ANTI_SUPERFICIAL_IMPLEMENTATION_STANDARD.md` | `80_ANTI_SUPERFICIAL_IMPLEMENTATION_STANDARD.md` |
| `26_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md` | `81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md` |
| `41_NO_PLACEHOLDER_PRODUCTION_EVIDENCE_GATE.md` | `82_NO_PLACEHOLDER_PRODUCTION_EVIDENCE_GATE.md` |
| `28_DEFINITION_OF_DONE_AND_ACCEPTANCE.md` | `83_DEFINITION_OF_DONE_AND_ACCEPTANCE.md` |
| `44_EXISTING_CODE_FEATURE_AUDIT_PROTOCOL.md` | `84_EXISTING_CODE_FEATURE_AUDIT_PROTOCOL.md` |
| `45_AGENT_TASK_EXECUTION_PROTOCOL.md` | `85_AGENT_TASK_EXECUTION_PROTOCOL.md` |
| `46_TASK_CARD_TEMPLATE.md` | `86_TASK_CARD_TEMPLATE.md` |
| `47_HANDOFF_AND_MANIFEST_PROTOCOL.md` | `87_HANDOFF_AND_MANIFEST_PROTOCOL.md` |
| `48_PARALLEL_AGENT_COORDINATION_RULES.md` | `88_PARALLEL_AGENT_COORDINATION_RULES.md` |
| `29_BUILD_AGENT_CONTEXT_LOADING_POLICY.md` | `89_BUILD_AGENT_CONTEXT_LOADING_POLICY.md` |
| `52_PR_CHANGE_EVIDENCE_TEMPLATE.md` | `90_PR_CHANGE_EVIDENCE_TEMPLATE.md` |
| `51_FEATURE_COMPLETION_AUDIT.md` | `91_FEATURE_COMPLETION_AUDIT.md` |
| `25_BUILD_EVIDENCE_AND_DEPENDENCY_MANIFEST.md` | `92_BUILD_EVIDENCE_AND_DEPENDENCY_MANIFEST.md` |
| `(new in V3.1)` | `93_STATUS_VOCABULARY_AND_LIFECYCLE.md` |
| `53_BUILD_MANIFEST.md` | `98_BUILD_MANIFEST.md` |
| `99_MANIFEST.md` | `MANIFEST.md + manifest.json at repository root (covers all files, not only Part 2)` |
