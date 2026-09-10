# Modbit Dossier Manifest — V3.3 EPR v1.1

> **Authority date:** 2026-09-05  
> **Generated:** 2026-09-10 by `tools/build_manifest.py`  
> **Scope:** every specification file in `docs/` plus the root governing files and tooling. The previous `99_MANIFEST.md` covered only 39 Part 2 files; this manifest covers all 89 docs.
> **Machine-readable twin:** `manifest.json` (same content, same hashes).

## Integrity rule

A dossier package is valid only if every path below exists with the listed SHA-256. `python3 tools/check_dossier.py --manifest` verifies this. Regenerate after any edit with `python3 tools/build_manifest.py`.

## Summary

| Section | Range | Files | Bytes |
|---|---|---:|---:|
| Authority and orientation | 00–09 | 8 | 65205 |
| Architecture and subsystems | 10–29 | 20 | 191488 |
| Implementation specifications | 30–39 | 10 | 80146 |
| Requirements, tasks and traceability | 40–49 | 10 | 351596 |
| Verification and testing | 50–69 | 15 | 202032 |
| Delivery and operations | 70–79 | 7 | 33831 |
| Agent process and governance | 80–97 | 18 | 109299 |
| Live state | 98–99 | 1 | 3983 |
| **Total docs** | | **89** | **1037580** |

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
| 30 | `docs/30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md` | Protocol, APIs, and Event Schemas | implementation | 10437 | `562de058295ce3e9b04f0c51c87d642595caeeafbcc54fbddebc2a9de53bcd67` |
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
| 62 | `docs/62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md` | Product extension requirements, tasks and qualifications | verification | 88197 | `1212dbd1e9e1bb505b6ae751c39236daece9d1c37eeba7d8e1dde41f5aca6016` |
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
| 98 | `docs/98_BUILD_MANIFEST.md` | Build Manifest | live-state | 3983 | `e6c9cbd61545aca7505d06edf0c0b861c7c84caa35f0f859347eba331d90d4ef` |

## Root governing files and tooling

| File | Role | Bytes | SHA-256 |
|---|---|---:|---|
| `AGENTS.md` | build-agent operating contract (highest authority) | 9512 | `52facabdca45abd2fcdaf10a7b0141057895217af95560395dc0d24a8793b64f` |
| `MODBIT-PATCH-EPR-v1.1-Supersession-and-Refinement.md` | source patch provenance | 20229 | `30dbacdb3a37b364f535f55ed7bf4ea8fa35d70cd6f85c9a61897ac9db72f2bd` |
| `MODBIT-PATCH-Execution-Policy-Router-and-Verified-Multi-Model-Orchestration.md` | source patch provenance | 54208 | `9e0cee7d49d442033b875e250a61a212b898dd6f4bf35826cdad5ffeb71edb66` |
| `README.md` | human orientation | 10200 | `575c1c947f4064ae19894a43916f66fca49974a3002a368c05314d80f3d7ccff` |
| `SKILLS.md` | governed procedures for agents | 18306 | `d6a62b1df940c10e1e4dec05af6a8ce7c94b65018c9e24a6bd76f88a7978d27c` |
| `docs/decisions/DR-M0-002-locked-architecture-file-guard.md` | source patch provenance | 2738 | `17609542de912b116bd113cc36d4c1fc5c863658bdd539e11e5f5b7c8772a240` |
| `docs/decisions/DR-M0-004-module-registration-and-canonical-systems.md` | source patch provenance | 2457 | `20217db18f46557448325ec23d75b1cf638cd9e4c8b96eddc8d1f512260ca992` |
| `docs/decisions/DR-M0-005-reschedule-runtime-dependent-m0-governance-tasks.md` | source patch provenance | 2912 | `7b76feae285e44195b47d33981d5167f73dfc718eebc49d289d6c473c2949fe0` |
| `docs/decisions/DR-M1-006-reschedule-m1-tasks-needing-later-runtime.md` | source patch provenance | 2722 | `f69bb610648eae11c9773485671ebb123c44c58422b0e678668978d2f760f991` |
| `docs/decisions/DR-M2-001-live-provider-proof-pending-credentials.md` | source patch provenance | 3090 | `4d881fa130b7b2a896f7b238924d0e796cc707b58f07e8ed07d606d165049a83` |
| `docs/decisions/DR-M2-002-reschedule-m2-tasks-needing-later-substrate.md` | source patch provenance | 4238 | `fd230f414f555a7d3ac5235f5ed5d45377ab678bd0216b1ae1ff6204f62a0a70` |
| `docs/decisions/DR-M3-001-single-search-stack-confinement.md` | source patch provenance | 2185 | `532b877ad071c02734609d5a85230a6f78cdde862b2eab71650e401f16077299` |
| `docs/decisions/README.md` | source patch provenance | 2312 | `b6b31751298aa4b6aa6f2de1c659882e19e310301d1428caba7658023967a4a2` |
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
| `evidence/m0/IMP-EV-0208/TASK_CARD.md` | retained evidence | 2676 | `2510e12a996033f4951c8a84a1adf68c6daca31607aa2695ebe53f8bcafb432a` |
| `evidence/m0/IMP-EV-0208/evidence.json` | retained evidence | 747 | `757c00fb86bc657d3d64089f599343a5a43c6f85249ed3463574780ce376726e` |
| `evidence/m0/M0.1/TASK_CARD.md` | retained evidence | 4582 | `9fbef0d723b9482b33064013d20ed743868d04ae71a795e7387aff0672eb0c7c` |
| `evidence/m0/M0.1/ci-run-34253119985.json` | retained evidence | 201 | `a200a78dc118e491331d8a0922f83ca924078dffa05665a157ef7a5350dd9854` |
| `evidence/m0/M0.1/ci-run-34255336543-summary.log` | retained evidence | 5031 | `42604365d8ed7a483e848829dcc22acc783e1ffb5242583c7f3b04370b62f427` |
| `evidence/m0/M0.1/ci-run-34255336543.json` | retained evidence | 752 | `aff3b7b6f155070126280daab467e3ccd896bec20e726f91dc5d7dad0b43d96d` |
| `evidence/m0/M0.1/evidence.json` | retained evidence | 3891 | `05b9b18523ae1d706ef96a559d077f12459fe6325c7f1ed7dabaeeaa56876166` |
| `evidence/m0/M0.1/fresh-clone-linux-dossier-425d80d.log` | retained evidence | 209 | `f415eee1654ba75f6776bc97e9c95498ab9b5a33aa7b45e071c7bc89fe6a2b91` |
| `evidence/m0/M0.1/fresh-clone-linux-node-425d80d.log` | retained evidence | 733 | `d0b9bea9982714039d4de44675e0b62c5aadf658ba7bc431bd8d74cbfad900da` |
| `evidence/m0/M0.1/fresh-clone-linux-rust-425d80d.log` | retained evidence | 705 | `e4479b5b004531c95e0671d3f0fc123547a37ca272417c58d51d8170558e9d96` |
| `evidence/m0/M0.1/fresh-clone-macos-425d80d.log` | retained evidence | 34857 | `c85b7e2c0c538764ffab1fa9dd5a710809526af2f06ce6d30a57f09b65ad1cd8` |
| `evidence/m0/M0.1/inject-failure.log` | retained evidence | 235 | `dc63bccf2385b3a07a3212dab9d175503e979e91fa6a132d17197eb2765c0db0` |
| `evidence/m0/M0.2/TASK_CARD.md` | retained evidence | 2958 | `68de49ba288220792905a019d3e19c47449dd382020d0e68fa471833850bf754` |
| `evidence/m0/M0.2/ci-run-34256442560-negative.json` | retained evidence | 794 | `daad25aaf3be9e16a5747cfe9733ef6ee663e004061450662915248214d4e17f` |
| `evidence/m0/M0.2/ci-run-34256442560-negative.json.log` | retained evidence | 612 | `68b5931b1a92a0a9e7f44a816ae9f89bc3c10a4be4192e1b275394442eced79f` |
| `evidence/m0/M0.2/evidence.json` | retained evidence | 1652 | `0e5f18a1d986c92794b845b902b54fa6523b0f4595ed0673a313c5ca5b60a327` |
| `evidence/m0/M0.2/inject-failure-silent-locked-change.log` | retained evidence | 490 | `a2d112b6cf3f72b6d16ea99650d82d531986822cb9e34b9f96ade11d5a104f01` |
| `evidence/m0/M0.3/TASK_CARD.md` | retained evidence | 3429 | `cd35519419404947c7b2f2848b8fab8c0433f4301572f4975e93bec637012eb9` |
| `evidence/m0/M0.3/ci-run-34263340707-roundtrip.log` | retained evidence | 1673 | `1d9ccdd465238a322a4fdb68e1b0dc340a2eccbf7ebe292beb43bd5c601603ee` |
| `evidence/m0/M0.3/ci-run-34263340707.json` | retained evidence | 904 | `077ca6f46143aeb0d7f42e53a091b6fb1fcd5933cd61b89e7f2fee391ee2a649` |
| `evidence/m0/M0.3/evidence.json` | retained evidence | 1292 | `7d676c993b8f7b5acf88be87dfc74a05352901463dec12bb2b3556b4dddee9d7` |
| `evidence/m0/M0.3/inject-failure-corrupted-ts-fixture.log` | retained evidence | 581 | `fffe98068a3edb9ebcbcdfb4c347a351ef3d314c4ed57d71478d3664bf9e5c31` |
| `evidence/m0/M0.4/TASK_CARD.md` | retained evidence | 3773 | `edf7380ae9d901960328e6fdf3e4751a65e9ed6abdf8f971acaf356d542f9668` |
| `evidence/m0/M0.4/evidence.json` | retained evidence | 1576 | `bdb12d74fc3c8e1f162a2a49b96d1006ba7d72c064614d6e8f1ee58a43a76f05` |
| `evidence/m0/M0.4/inject-failure-duplicate-canonical-owner.log` | retained evidence | 553 | `09e52f7b1f7ebe0d69d05e28f83df42ed227828a6ffdbb5406f76fa8659afefb` |
| `evidence/m1/IMP-EV-0010/TASK_CARD.md` | retained evidence | 1741 | `ec2337acf6cc4fc930a2ed98945dc9985ec2f3e7d002e5ee22d55e0e851527bb` |
| `evidence/m1/IMP-EV-0010/evidence.json` | retained evidence | 532 | `47eef5f6a216eaf6a6706d036a5f3a78d226123bcfe214a02f8c5e72a45744e6` |
| `evidence/m1/IMP-EV-0037/TASK_CARD.md` | retained evidence | 1605 | `2cf4397729f553dd0b3e299299483736a55d587a137b723c208c04f31ca6ce50` |
| `evidence/m1/IMP-EV-0037/evidence.json` | retained evidence | 474 | `4a0b3e2a2322b24d81ca2321a8ecfab230a3a2a913e6b0da540dd0d6f50506f3` |
| `evidence/m1/IMP-EV-0039/TASK_CARD.md` | retained evidence | 1693 | `77f07f13acd9dc988dbb6a88f3a582cf5114bc5b9b27ac1bf01a3fe4025cf997` |
| `evidence/m1/IMP-EV-0039/evidence.json` | retained evidence | 546 | `b9e92c9e85a96807135646b21ee886cb2c9943c4565deb467c6634abff6930b9` |
| `evidence/m1/IMP-EV-0054/TASK_CARD.md` | retained evidence | 1599 | `53d341471ce110e3e621850aff36f8729a31bde80195d297ce7dfc8ff5a4318a` |
| `evidence/m1/IMP-EV-0054/evidence.json` | retained evidence | 532 | `b98bcd361c9896cfafcc56aa1a434a0d86a98740777bf50f460b6fdae717827c` |
| `evidence/m1/IMP-EV-0101/TASK_CARD.md` | retained evidence | 1664 | `f956908518b9121515f68b0d313d4b3790d1b22296a979484c02f378707c19c3` |
| `evidence/m1/IMP-EV-0101/evidence.json` | retained evidence | 526 | `8aa2416114c060063f3721c05489789fa9218b095514db2b50d7ccdc92bfefa5` |
| `evidence/m1/IMP-EV-0102/TASK_CARD.md` | retained evidence | 1566 | `c222b87a8c94dc64fe3e10c5003db0ea3f599e1c482de118f780790dbefecb9a` |
| `evidence/m1/IMP-EV-0102/evidence.json` | retained evidence | 537 | `fd485932348178527a6bc1d1b12e5ce6b49b281924ed45334cb1f0dbe62b6435` |
| `evidence/m1/IMP-EV-0103/TASK_CARD.md` | retained evidence | 1706 | `131f237b455116f4b608b3d3a9d2604c38acdaf6c79e327c7ba7ad47732f891e` |
| `evidence/m1/IMP-EV-0103/evidence.json` | retained evidence | 515 | `70f44dc05ecdb0cd44d4a0695212f33b7dbeb68ef1799abc98f39075f2c731d1` |
| `evidence/m1/IMP-EV-0108/TASK_CARD.md` | retained evidence | 1544 | `3b12b8d0443a19dcc64cd67d02f5194df9792fd3a6a8b6e4564d4d30f677549d` |
| `evidence/m1/IMP-EV-0108/evidence.json` | retained evidence | 515 | `0838974a769f50799f05d1b4c029b693d971822183cdb134518c8d7e743c4f4a` |
| `evidence/m1/IMP-EV-0121/TASK_CARD.md` | retained evidence | 1541 | `717a4c2526e785ca4bad0d3d78f1cba8269325a5833c0a696cd2735d19a8d6ad` |
| `evidence/m1/IMP-EV-0121/evidence.json` | retained evidence | 527 | `02948356fbe093a5cd7678bc2bda28e957318a9162efc55cd9c197e0570fe96a` |
| `evidence/m1/IMP-EV-0143/TASK_CARD.md` | retained evidence | 1418 | `24db35de0fb56527e907aaaec1f297a2941ed2b1c6f719fd8784c1603e266483` |
| `evidence/m1/IMP-EV-0143/evidence.json` | retained evidence | 474 | `4be2116222e0aa4a38173f3eaf182dd2c87888f062c53b0c2c78d8aad9eef9af` |
| `evidence/m1/IMP-EV-0152/TASK_CARD.md` | retained evidence | 1426 | `0406b7c245725682ad3259294b9351ab655c72a4598f0205a74952b374bc9640` |
| `evidence/m1/IMP-EV-0152/evidence.json` | retained evidence | 474 | `5b32b1aed3d052020f183c417a5fbb22f59a58f7dd28746346f927cfde762ca4` |
| `evidence/m1/IMP-EV-0192/TASK_CARD.md` | retained evidence | 1633 | `040d80f83fdd852084628a8042ccc9e57ef2c346923b19680a6fc57ea9aebb41` |
| `evidence/m1/IMP-EV-0192/evidence.json` | retained evidence | 532 | `83d573d03b1b4f4b93b7cc35c9829f754c95bc6e9f5cb4d92322a9bd900fcd97` |
| `evidence/m1/IMP-EV-0262/TASK_CARD.md` | retained evidence | 1474 | `de6995866c5eae3ad1d15d76bab2c979ecc3583a8318822dca586fe2856b8905` |
| `evidence/m1/IMP-EV-0262/evidence.json` | retained evidence | 527 | `f56b377f5f6d55924583d62fd87d1a5fd73b119b38af4b11851b20c95f0c415b` |
| `evidence/m1/IMP-EV-0273/TASK_CARD.md` | retained evidence | 1457 | `fb74d2c59d92f677db0adff03492a0170795813880f3759ce845d4c3f37ede73` |
| `evidence/m1/IMP-EV-0273/evidence.json` | retained evidence | 532 | `bff42d0ed4830239957051038f946c5e3f53220b3991913c7ea42cdb1c6b94d5` |
| `evidence/m1/M1.1/TASK_CARD.md` | retained evidence | 3834 | `18bd02ec3507b85c7d6b3179a4171ad7db28b03725c6652561405060f56fd826` |
| `evidence/m1/M1.1/ci-run-34311147323-tests.log` | retained evidence | 3273 | `e7a9f998bfa13a0e9bfd3740a4bae800d105ae76efe0514862d04055cb187097` |
| `evidence/m1/M1.1/ci-run-34311147323.json` | retained evidence | 904 | `f63e14257f2dcb858be866e0c76746fa265e6581a59e995e19f8c3f6dd39719a` |
| `evidence/m1/M1.1/evidence.json` | retained evidence | 1102 | `d2204c954ad8d1acc7f508cc77ec47103aa3c86cf1cdee67be43892b1cc361fa` |
| `evidence/m1/M1.2/TASK_CARD.md` | retained evidence | 3752 | `80d5ebbd23d71593762e967df610c0b7e5ef96a34789e16d50cd69d14908aca4` |
| `evidence/m1/M1.2/evidence.json` | retained evidence | 1702 | `d2c2cb334d3bd3a523c4caefeeb6507bfa6147584472494b72b08af683f27c44` |
| `evidence/m1/M1.3/TASK_CARD.md` | retained evidence | 4520 | `05777b4a9fa1d468b6ba27ddd0aa93bc8e3e8cde51e15eb035762bf9dfd682b6` |
| `evidence/m1/M1.3/ci-run-34314359685-tests.log` | retained evidence | 2934 | `13f58690ec9ab40f98687244b0bb7b9e3aed4f3063e3d999854adc3a0c3bbfa1` |
| `evidence/m1/M1.3/ci-run-34314359685.json` | retained evidence | 904 | `804475ef1bbe3b8ab557425cdccb1b7af4c45995711bd2230a91c6a0a1d3b399` |
| `evidence/m1/M1.3/cli-transcript.log` | retained evidence | 851 | `85caf81cc127960b9534a887a3329469a0068b89746dd26b5fab74c5d3f0c66f` |
| `evidence/m1/M1.3/evidence.json` | retained evidence | 1186 | `3501c248434b0942fa4f291d1b00d2b53778d039e816e1838d10e64d69ed7c00` |
| `evidence/m1/M1.4/TASK_CARD.md` | retained evidence | 4480 | `e19f89503ff44ae1f3da5aebe823d7c0cdd517e1484cb7a646cb0a7313952a6b` |
| `evidence/m1/M1.4/ci-run-34316794416-e2e.log` | retained evidence | 1681 | `c2a5e30f1250e909a74b2890b3ddf99fd4aa8a34318ae5546579ab5ee87fdef3` |
| `evidence/m1/M1.4/ci-run-34316794416.json` | retained evidence | 1231 | `17ed0f7c9d4fd63f1abb1e6da65cbf0ddf6e2993e6c051ed1c322163d89d3dc6` |
| `evidence/m1/M1.4/evidence.json` | retained evidence | 1466 | `39649bc0e2563fff06fb134fb0d38bcf96fe10cc68fdbffa9018057405621b16` |
| `evidence/m1/M1.4/local-e2e.log` | retained evidence | 313 | `85975357f09871239436ab721ab8e7de79ac630698a8d75cd2374d8668c4cbdf` |
| `evidence/m1/M1.5/TASK_CARD.md` | retained evidence | 3696 | `71ce0451ca96dff7b2f1ffdd3b95ccbea2ee1e5ef74f56259ff88476fb091455` |
| `evidence/m1/M1.5/ci-run-34317667928-tests.log` | retained evidence | 1254 | `90d3f63b1fdc60b4cc52d7c209a39dc65ff9e6cdfc30ecb679e1a035be9af899` |
| `evidence/m1/M1.5/ci-run-34317667928.json` | retained evidence | 1231 | `f04d433a5ac15227f0b852fec3966e145a110018e72899338ed92b5546a68fec` |
| `evidence/m1/M1.5/evidence.json` | retained evidence | 1739 | `5e0a7f11ce1a4b72fd57b7a762dc17005284345c4d98cf5b7eb9d3dc31e24caf` |
| `evidence/m1/M1.5/local-desktop-e2e-with-recovery-banner.log` | retained evidence | 313 | `f1c58150b0ac188655b0c3440b9cbd549085d01e3e8fd5c031053f6a2bdfedfb` |
| `evidence/m1/ci-run-34347242710.json` | retained evidence | 1231 | `a238323591f3fc267ff1b4ee8095929379787e5ff43f8b18753446a77c8f4b03` |
| `evidence/m2/IMP-EV-0011/TASK_CARD.md` | retained evidence | 1620 | `22a7ddc2c80526c5aba876366bfeb862fac72742059962c5ad19deb8239fb5d2` |
| `evidence/m2/IMP-EV-0011/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0011/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0011/evidence.json` | retained evidence | 975 | `f578a8084891ba00e332dc590c40bba31d4ae4c2f6cafc39f2335326e1c380aa` |
| `evidence/m2/IMP-EV-0014/TASK_CARD.md` | retained evidence | 1529 | `52a5d24bec2fd16c0b950bc7af60b15c7ee514dd03fbc8480476bbb6fe7e18a6` |
| `evidence/m2/IMP-EV-0014/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0014/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0014/evidence.json` | retained evidence | 1090 | `033cee5384ccfeca9f0a12d55e195e2b554eb17a096bb80e49d1f57d6584760f` |
| `evidence/m2/IMP-EV-0015/TASK_CARD.md` | retained evidence | 1366 | `6e4869adb1a9ff0edf51e116b7db4786da6f623440615f4187fa902e58aaa2ea` |
| `evidence/m2/IMP-EV-0015/ci-run-34413048026-tests.log` | retained evidence | 84120 | `61db5b9e7ac9c0e84d068bd792c15be1accd9f43f994168675f1ffb6cf48f925` |
| `evidence/m2/IMP-EV-0015/ci-run-34413048026.json` | retained evidence | 36682 | `532cffa1b4de24f9bbdbdb4b8b42d91aa6ae773e7f3dfd7009cea7780b29313e` |
| `evidence/m2/IMP-EV-0015/evidence.json` | retained evidence | 923 | `9f486f4b85a0a59e36d75f05a38dcbd3f4b480757785a33135806106ab34f602` |
| `evidence/m2/IMP-EV-0016/TASK_CARD.md` | retained evidence | 1472 | `da756bcb364cfc2deb50804bada4a7c454026623ce0a0e2a8e8bd9fe23979f8a` |
| `evidence/m2/IMP-EV-0016/ci-run-34413048026-tests.log` | retained evidence | 84120 | `61db5b9e7ac9c0e84d068bd792c15be1accd9f43f994168675f1ffb6cf48f925` |
| `evidence/m2/IMP-EV-0016/ci-run-34413048026.json` | retained evidence | 36682 | `532cffa1b4de24f9bbdbdb4b8b42d91aa6ae773e7f3dfd7009cea7780b29313e` |
| `evidence/m2/IMP-EV-0016/evidence.json` | retained evidence | 984 | `63be0960c1048c400610a9fa969365ebdc4850e7ab7f6dd495c9de4b63461a01` |
| `evidence/m2/IMP-EV-0018/TASK_CARD.md` | retained evidence | 1681 | `1c7ebab3dfc2e46da44058ecaaa19fd0b4366bc2570495273b6aa31fb1649b4a` |
| `evidence/m2/IMP-EV-0018/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0018/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0018/evidence.json` | retained evidence | 975 | `9bc75d536f65fc873a4402d0fe174d6f1e6224dd82c5d69970d3016fba95a277` |
| `evidence/m2/IMP-EV-0019/TASK_CARD.md` | retained evidence | 1623 | `6bd04b2d1ee7066bba6c3e4a471c3701e9cdf9e6aa2dea407cb1fd7ab870c0a6` |
| `evidence/m2/IMP-EV-0019/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0019/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0019/evidence.json` | retained evidence | 949 | `a9ec77f6184be4024974b89c93873377598996cbc1eefb06640bb241c23e37cb` |
| `evidence/m2/IMP-EV-0022/TASK_CARD.md` | retained evidence | 1492 | `0665d56794f78abef6685bbc808aedb41f22b8946ac71e51ee26c418f2cfc76f` |
| `evidence/m2/IMP-EV-0022/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0022/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0022/evidence.json` | retained evidence | 901 | `811a291da4e46f463980e52dbe1a3c788e6a113d3916ecf2e6d9413a13004a88` |
| `evidence/m2/IMP-EV-0025/TASK_CARD.md` | retained evidence | 1442 | `ee3ad1537efb9fa51914438e4c070093b684d7b1f89e9af752118ae3217d9c6c` |
| `evidence/m2/IMP-EV-0025/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0025/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0025/evidence.json` | retained evidence | 873 | `396ba8e1673481551665cafb4c8faaa09df02aedecb57218ee58df1f8251817e` |
| `evidence/m2/IMP-EV-0026/TASK_CARD.md` | retained evidence | 1514 | `6fb6071c56aa69eec7c8969136749db3d5d319f6aa236cabba27b761b0236d48` |
| `evidence/m2/IMP-EV-0026/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0026/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0026/evidence.json` | retained evidence | 901 | `75d8dec413d0db9af181665823ba0909a8a9988308e512c8e84496ec76621260` |
| `evidence/m2/IMP-EV-0027/TASK_CARD.md` | retained evidence | 1489 | `72cbef5a150f4df6a15236bbe651c288f03a353bec1bebcbdb58bd0f79f9dc17` |
| `evidence/m2/IMP-EV-0027/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0027/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0027/evidence.json` | retained evidence | 896 | `f38b590bf91bddfa5c328506ca3a7e7c3ed7630286ff71e2a0d1dbc87c16a0d3` |
| `evidence/m2/IMP-EV-0028/TASK_CARD.md` | retained evidence | 1544 | `0c376923b061ead7b3d03fec4d9d6b5c4162eacd484fc9fc152043aad3b49ca6` |
| `evidence/m2/IMP-EV-0028/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0028/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0028/evidence.json` | retained evidence | 920 | `10f5facdcfb88cf2f246b247ed92b783cee0b5f5320613363f54b250c0625fd8` |
| `evidence/m2/IMP-EV-0031/TASK_CARD.md` | retained evidence | 1388 | `ad5ba1d0ae66c162f33454f91532e50dfd957ac6578f051d373ce98829596c7e` |
| `evidence/m2/IMP-EV-0031/ci-run-34413048026-tests.log` | retained evidence | 84120 | `61db5b9e7ac9c0e84d068bd792c15be1accd9f43f994168675f1ffb6cf48f925` |
| `evidence/m2/IMP-EV-0031/ci-run-34413048026.json` | retained evidence | 36682 | `532cffa1b4de24f9bbdbdb4b8b42d91aa6ae773e7f3dfd7009cea7780b29313e` |
| `evidence/m2/IMP-EV-0031/evidence.json` | retained evidence | 935 | `ae1c142789fa6ea0ff0d6cd103af54674a205d4cabbe7df1b3e8b09deaf13e14` |
| `evidence/m2/IMP-EV-0036/TASK_CARD.md` | retained evidence | 1632 | `eac5e90a61f56ae3600fe045c1845934177c7c7859b8969cb2db20c4645701b5` |
| `evidence/m2/IMP-EV-0036/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0036/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0036/evidence.json` | retained evidence | 955 | `04139e600e89a05a865017f791d64823e72faf92c4ed4d9f03382068684e1396` |
| `evidence/m2/IMP-EV-0044/TASK_CARD.md` | retained evidence | 1428 | `663a7e257064a2cb60722de4fec32e793bf6a892b3cae61b008e6afdc7a6aadc` |
| `evidence/m2/IMP-EV-0044/ci-run-34413048026-tests.log` | retained evidence | 84120 | `61db5b9e7ac9c0e84d068bd792c15be1accd9f43f994168675f1ffb6cf48f925` |
| `evidence/m2/IMP-EV-0044/ci-run-34413048026.json` | retained evidence | 36682 | `532cffa1b4de24f9bbdbdb4b8b42d91aa6ae773e7f3dfd7009cea7780b29313e` |
| `evidence/m2/IMP-EV-0044/evidence.json` | retained evidence | 913 | `a4a8375815bfae58287e77f9f037e53886264dc7396c21259a8794e8f055573e` |
| `evidence/m2/IMP-EV-0045/TASK_CARD.md` | retained evidence | 1467 | `ee6cd4a4eaca9767647c6aa360a4d598ce078fccffbfa31bd3ed2b78609351e7` |
| `evidence/m2/IMP-EV-0045/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0045/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0045/evidence.json` | retained evidence | 887 | `d27324a0351fffaedabc48e787ddb7ae3091e0f556de82ded2f5a544a89b4fdf` |
| `evidence/m2/IMP-EV-0064/TASK_CARD.md` | retained evidence | 1366 | `48e96a90312e82da195ecbab743c679238bedb037a4496139e1c504b3bce012c` |
| `evidence/m2/IMP-EV-0064/ci-run-34413048026-tests.log` | retained evidence | 84120 | `61db5b9e7ac9c0e84d068bd792c15be1accd9f43f994168675f1ffb6cf48f925` |
| `evidence/m2/IMP-EV-0064/ci-run-34413048026.json` | retained evidence | 36682 | `532cffa1b4de24f9bbdbdb4b8b42d91aa6ae773e7f3dfd7009cea7780b29313e` |
| `evidence/m2/IMP-EV-0064/evidence.json` | retained evidence | 930 | `a995dbd25d318cc553d4f467de4aa223738e76375411433f9e9a24dfc5085d09` |
| `evidence/m2/IMP-EV-0065/TASK_CARD.md` | retained evidence | 1229 | `15fc07c99337d3399482a790996e36638d8b9246db2c5d91e0589b6479e93d1f` |
| `evidence/m2/IMP-EV-0065/ci-run-34413048026-tests.log` | retained evidence | 84120 | `61db5b9e7ac9c0e84d068bd792c15be1accd9f43f994168675f1ffb6cf48f925` |
| `evidence/m2/IMP-EV-0065/ci-run-34413048026.json` | retained evidence | 36682 | `532cffa1b4de24f9bbdbdb4b8b42d91aa6ae773e7f3dfd7009cea7780b29313e` |
| `evidence/m2/IMP-EV-0065/evidence.json` | retained evidence | 850 | `f67f38011357b2c50a417f75d11802fd6703d5e94f8d00201ad6feabf71c047d` |
| `evidence/m2/IMP-EV-0067/TASK_CARD.md` | retained evidence | 1453 | `66b5b204453acb8baf31187eb36a79824818f394d001a60d9fd380d594556daa` |
| `evidence/m2/IMP-EV-0067/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0067/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0067/evidence.json` | retained evidence | 897 | `ccfe57e59ddb7f7969955d8314af87fe443eea440fe0eab6e2888572377c23ca` |
| `evidence/m2/IMP-EV-0068/TASK_CARD.md` | retained evidence | 1563 | `a606e69b02e6454cc6d72e1082f54a128186436a6f3cf342f6b1de22bd47f916` |
| `evidence/m2/IMP-EV-0068/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0068/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0068/evidence.json` | retained evidence | 982 | `90f00ab8bd7ef205e45f0265e7e012002730ecd17361c76f22e6e28e4711e911` |
| `evidence/m2/IMP-EV-0069/TASK_CARD.md` | retained evidence | 1494 | `e2a7d7193a5d1e5788e0f03c09e2d230800457e205cf61de4fe5a1c7348b1b24` |
| `evidence/m2/IMP-EV-0069/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0069/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0069/evidence.json` | retained evidence | 901 | `5b831b0c3c05143c78d489b74ae24d0ee5836840182eb931d8cf2fd7fe07e2a4` |
| `evidence/m2/IMP-EV-0071/TASK_CARD.md` | retained evidence | 1396 | `893a0b8d544eea5e57b243c7723cfeacf51f27e195df0cdb6bc721c8b6b695ba` |
| `evidence/m2/IMP-EV-0071/ci-run-34413048026-tests.log` | retained evidence | 84120 | `61db5b9e7ac9c0e84d068bd792c15be1accd9f43f994168675f1ffb6cf48f925` |
| `evidence/m2/IMP-EV-0071/ci-run-34413048026.json` | retained evidence | 36682 | `532cffa1b4de24f9bbdbdb4b8b42d91aa6ae773e7f3dfd7009cea7780b29313e` |
| `evidence/m2/IMP-EV-0071/evidence.json` | retained evidence | 979 | `518ffd074674395fbccfc798f4430b5f88a2d0b2a20080247cfe6a7ad03961fc` |
| `evidence/m2/IMP-EV-0079/TASK_CARD.md` | retained evidence | 1427 | `6ee5b1040c241a4fac2323217971dc1f8254df2d4ce9376f437aa8e135a692ee` |
| `evidence/m2/IMP-EV-0079/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0079/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0079/evidence.json` | retained evidence | 887 | `be931c2930923184f6edc549345481ae2a3b371497ee858d9ac09046f2f1697a` |
| `evidence/m2/IMP-EV-0080/TASK_CARD.md` | retained evidence | 1510 | `9869570db99355fc0053a76ac64446b72ca371ace8d5cf6515ca121f23c459fb` |
| `evidence/m2/IMP-EV-0080/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0080/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0080/evidence.json` | retained evidence | 931 | `d237a72bb276bf722a8c5fd06945adb9ea2385941d7b599cdc8b26342efb554a` |
| `evidence/m2/IMP-EV-0091/TASK_CARD.md` | retained evidence | 1344 | `78d02ef991c72fb623b21ae66cad0d27dd78f8c84a7b909fbe64843875d7a118` |
| `evidence/m2/IMP-EV-0091/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0091/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0091/evidence.json` | retained evidence | 833 | `50bc273c3cdce5faa5f13d0f9c4cb37525006d30392003d04dfa5bf32009d352` |
| `evidence/m2/IMP-EV-0093/TASK_CARD.md` | retained evidence | 1471 | `ca78d0f176aff70f8cac6b76b2536280109c6ecb84bb39851b673075a6576e58` |
| `evidence/m2/IMP-EV-0093/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0093/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0093/evidence.json` | retained evidence | 876 | `9eaba66ada74b5f3440f8b8f7c315d1ade5d61a432e8e58c699bf92439d47e07` |
| `evidence/m2/IMP-EV-0096/TASK_CARD.md` | retained evidence | 1336 | `31d22d9abb2fb728f937c40898abf3c755ad6dacf7be980d2fab6e4ee91db5ef` |
| `evidence/m2/IMP-EV-0096/ci-run-34413048026-tests.log` | retained evidence | 84120 | `61db5b9e7ac9c0e84d068bd792c15be1accd9f43f994168675f1ffb6cf48f925` |
| `evidence/m2/IMP-EV-0096/ci-run-34413048026.json` | retained evidence | 36682 | `532cffa1b4de24f9bbdbdb4b8b42d91aa6ae773e7f3dfd7009cea7780b29313e` |
| `evidence/m2/IMP-EV-0096/evidence.json` | retained evidence | 925 | `bddf94d032d788de379f0af192d8c079fc1a77a8f2fd8a6dcaa1be8ea659ce9c` |
| `evidence/m2/IMP-EV-0098/TASK_CARD.md` | retained evidence | 1521 | `1e15c889c1a53eeae020bad195977f440086007621693e5fbebbaaa6a66c930d` |
| `evidence/m2/IMP-EV-0098/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0098/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0098/evidence.json` | retained evidence | 930 | `af6ee7567cfed748b24ab2996d0dee28e0b8ebe84625d35d7eb48a452453789d` |
| `evidence/m2/IMP-EV-0100/TASK_CARD.md` | retained evidence | 1455 | `37faf2d964a5ac405616bb5d44240c81753693cc10e0abab8c2fe7a7fef7325e` |
| `evidence/m2/IMP-EV-0100/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0100/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0100/evidence.json` | retained evidence | 863 | `ecf124909b1f46e258299d34c888ff254dc2ff1c09d6cf86b632002f5492b0ef` |
| `evidence/m2/IMP-EV-0106/TASK_CARD.md` | retained evidence | 1430 | `aa0e3e103e0e499f80e057d90d585342ac5073bb219cd389be762a12d700267e` |
| `evidence/m2/IMP-EV-0106/ci-run-34413048026-tests.log` | retained evidence | 84120 | `61db5b9e7ac9c0e84d068bd792c15be1accd9f43f994168675f1ffb6cf48f925` |
| `evidence/m2/IMP-EV-0106/ci-run-34413048026.json` | retained evidence | 36682 | `532cffa1b4de24f9bbdbdb4b8b42d91aa6ae773e7f3dfd7009cea7780b29313e` |
| `evidence/m2/IMP-EV-0106/evidence.json` | retained evidence | 986 | `cc9b2062926b51a68fbd4c5a087f4139040a62ef9fa58f52b8e134b7f041cf9d` |
| `evidence/m2/IMP-EV-0107/TASK_CARD.md` | retained evidence | 1658 | `8c98a0d797892e228f98255630be41f4c7c9fb69cd742a7973449e4db0ad8d3e` |
| `evidence/m2/IMP-EV-0107/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0107/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0107/evidence.json` | retained evidence | 965 | `a85bec91b052cc97f6662f86bf696ee42b71e2e0184fd18a239ef53f5a769531` |
| `evidence/m2/IMP-EV-0112/TASK_CARD.md` | retained evidence | 1521 | `c512929b4169915c92953206f3e2b90bcdb7bd738a90479a1ddfcba4ef6a218b` |
| `evidence/m2/IMP-EV-0112/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0112/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0112/evidence.json` | retained evidence | 901 | `448c9aa51146c2a773c9055a80e0afe1284fb4e713d5b6961977efc5e7019140` |
| `evidence/m2/IMP-EV-0116/TASK_CARD.md` | retained evidence | 1219 | `88a7a2dffd759c8d1369515d81b4a116178835de98d2c5f965f83f14bb38761e` |
| `evidence/m2/IMP-EV-0116/ci-run-34413048026-tests.log` | retained evidence | 84120 | `61db5b9e7ac9c0e84d068bd792c15be1accd9f43f994168675f1ffb6cf48f925` |
| `evidence/m2/IMP-EV-0116/ci-run-34413048026.json` | retained evidence | 36682 | `532cffa1b4de24f9bbdbdb4b8b42d91aa6ae773e7f3dfd7009cea7780b29313e` |
| `evidence/m2/IMP-EV-0116/evidence.json` | retained evidence | 883 | `d65c6380825d0bf26e03763371da351c7d120fff56730317c5bcf82f2a55f91e` |
| `evidence/m2/IMP-EV-0119/TASK_CARD.md` | retained evidence | 1488 | `c2776b3fbcaed063fffd6bca74b46ce4d11723ede1b0f384ab30ef22be0e9933` |
| `evidence/m2/IMP-EV-0119/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0119/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0119/evidence.json` | retained evidence | 890 | `931ea3d0c2f1dd9232bf0aac32acf14dc50543459fc78982854e5a0a373170e5` |
| `evidence/m2/IMP-EV-0124/TASK_CARD.md` | retained evidence | 1557 | `792e8ebbc92069c77a32087dee92f770a1ade85983776f9ddf27981d9600ec94` |
| `evidence/m2/IMP-EV-0124/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0124/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0124/evidence.json` | retained evidence | 972 | `944d22947cba45c8d5c4a452f5e7c3d573804bd259ff2231f75d75d2029864a3` |
| `evidence/m2/IMP-EV-0125/TASK_CARD.md` | retained evidence | 1386 | `2c9d54f65ff1f9fa00147b843fc5cc441416721ef46b96c94ea2c5de1e312f73` |
| `evidence/m2/IMP-EV-0125/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0125/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0125/evidence.json` | retained evidence | 870 | `75d7fbb81416b72303e1f3548ebb692e98c54bf0c003b6bea34b0eca38491c42` |
| `evidence/m2/IMP-EV-0133/TASK_CARD.md` | retained evidence | 1223 | `7fb057ee4d8d4cc641e4a1833ed5b00d8359e944a1942ba029e7e9a352a68ad4` |
| `evidence/m2/IMP-EV-0133/ci-run-34413048026-tests.log` | retained evidence | 84120 | `61db5b9e7ac9c0e84d068bd792c15be1accd9f43f994168675f1ffb6cf48f925` |
| `evidence/m2/IMP-EV-0133/ci-run-34413048026.json` | retained evidence | 36682 | `532cffa1b4de24f9bbdbdb4b8b42d91aa6ae773e7f3dfd7009cea7780b29313e` |
| `evidence/m2/IMP-EV-0133/evidence.json` | retained evidence | 851 | `46870a59a556881af922174b8307c20c7b27b697070174d14ac32da745febc9e` |
| `evidence/m2/IMP-EV-0135/TASK_CARD.md` | retained evidence | 1491 | `8f790a7ac4702e1d32cb2cc3cd527ea99cf9bad40b21fcb3eec32204ffc98854` |
| `evidence/m2/IMP-EV-0135/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0135/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0135/evidence.json` | retained evidence | 948 | `2ea944456e4b5511f48c795e0276987231e6578251b68c9ec8efec5c2185eb0b` |
| `evidence/m2/IMP-EV-0136/TASK_CARD.md` | retained evidence | 1607 | `e9a6dbda7092ce5256d09f546a3d2fb51cabf214985a0e1fe4f6c7cd42fcd393` |
| `evidence/m2/IMP-EV-0136/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0136/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0136/evidence.json` | retained evidence | 969 | `8fc6439157cfba3b01cd68042dffa497d2c0cb072de668d9f4484de13821b00b` |
| `evidence/m2/IMP-EV-0189/TASK_CARD.md` | retained evidence | 1504 | `df6799025252374417d889614d829b5ece38cac05e1d00ee6f5b7e1a62048b09` |
| `evidence/m2/IMP-EV-0189/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0189/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0189/evidence.json` | retained evidence | 923 | `ca9cd4860891f7846b931e26dc5bf2fecfdbeda3f2f82824da145c6e928e5719` |
| `evidence/m2/IMP-EV-0190/TASK_CARD.md` | retained evidence | 2016 | `7b402e3980bf26142780229518dabdcdd5054b2c006ff63551b7ad8c8795e535` |
| `evidence/m2/IMP-EV-0190/ci-run-34437627213-tests.log` | retained evidence | 89460 | `77b0b9b704403e2e4fd814a458eadc5f940ec7fd82bab290882c5c184148d640` |
| `evidence/m2/IMP-EV-0190/ci-run-34437627213.json` | retained evidence | 36682 | `9080da190811d409f4b3d1310407f3a70fd6be7a11e2d26418b49f4f7e6d6a8f` |
| `evidence/m2/IMP-EV-0190/evidence.json` | retained evidence | 1341 | `1cb2a8dc8639506d6e1166772dec544c3ad199fdeb5bea7e8dc7fc2b958cca05` |
| `evidence/m2/IMP-EV-0191/TASK_CARD.md` | retained evidence | 1632 | `539270b4c2e47d1a55f67e82bbb95e80a380a7568d094b9efbd5eb6c4c695939` |
| `evidence/m2/IMP-EV-0191/ci-run-34437627213-tests.log` | retained evidence | 89460 | `77b0b9b704403e2e4fd814a458eadc5f940ec7fd82bab290882c5c184148d640` |
| `evidence/m2/IMP-EV-0191/ci-run-34437627213.json` | retained evidence | 36682 | `9080da190811d409f4b3d1310407f3a70fd6be7a11e2d26418b49f4f7e6d6a8f` |
| `evidence/m2/IMP-EV-0191/evidence.json` | retained evidence | 1010 | `0b5d97b4718124e045e57becadfeace3a4d9f8a07094610e09d7f31cd0a87329` |
| `evidence/m2/IMP-EV-0194/TASK_CARD.md` | retained evidence | 1532 | `8488d1d6919aaa55e02d27cde3595b3c453b5fb8c62a14c344db5829c87d19fc` |
| `evidence/m2/IMP-EV-0194/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0194/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0194/evidence.json` | retained evidence | 907 | `5909122791c59b748f8322a79d42fc3fbf45f5921b51419227ffe8576c99f0ad` |
| `evidence/m2/IMP-EV-0217/TASK_CARD.md` | retained evidence | 1281 | `d03d3272665214f51138721f2429a6fd0593e2c38b5d39f562c0965efade0746` |
| `evidence/m2/IMP-EV-0217/ci-run-34413048026-tests.log` | retained evidence | 84120 | `61db5b9e7ac9c0e84d068bd792c15be1accd9f43f994168675f1ffb6cf48f925` |
| `evidence/m2/IMP-EV-0217/ci-run-34413048026.json` | retained evidence | 36682 | `532cffa1b4de24f9bbdbdb4b8b42d91aa6ae773e7f3dfd7009cea7780b29313e` |
| `evidence/m2/IMP-EV-0217/evidence.json` | retained evidence | 920 | `8b50388c3b59b034380fca16627ab72ca0494df7e99c9b76e8f8a77fb6ff159b` |
| `evidence/m2/IMP-EV-0221/TASK_CARD.md` | retained evidence | 1574 | `7ceac06e47bf33d7841dfc030eeb2f8fa1e02ed224d254af92ed51fec8d43f3b` |
| `evidence/m2/IMP-EV-0221/ci-run-34437627213-tests.log` | retained evidence | 89460 | `77b0b9b704403e2e4fd814a458eadc5f940ec7fd82bab290882c5c184148d640` |
| `evidence/m2/IMP-EV-0221/ci-run-34437627213.json` | retained evidence | 36682 | `9080da190811d409f4b3d1310407f3a70fd6be7a11e2d26418b49f4f7e6d6a8f` |
| `evidence/m2/IMP-EV-0221/evidence.json` | retained evidence | 1144 | `6b0f0e262b8fe43bf0eb4f1a6a259f9b0334e34cf9a95c9466247703e0f058d0` |
| `evidence/m2/IMP-EV-0222/TASK_CARD.md` | retained evidence | 1433 | `a0b3971bf8e88dda1193fef959c27756d6b61a18417f0893ce0ec1820ec5dfdb` |
| `evidence/m2/IMP-EV-0222/ci-run-34437627213-tests.log` | retained evidence | 89460 | `77b0b9b704403e2e4fd814a458eadc5f940ec7fd82bab290882c5c184148d640` |
| `evidence/m2/IMP-EV-0222/ci-run-34437627213.json` | retained evidence | 36682 | `9080da190811d409f4b3d1310407f3a70fd6be7a11e2d26418b49f4f7e6d6a8f` |
| `evidence/m2/IMP-EV-0222/evidence.json` | retained evidence | 994 | `0daea80d2b5a101df32cddff26594f96484bf1fddf820eff5286e5928ce98f64` |
| `evidence/m2/IMP-EV-0239/TASK_CARD.md` | retained evidence | 1463 | `36d4b9f41137e36ee1185385011656d914189fb26edc95047ac2475ab025c17a` |
| `evidence/m2/IMP-EV-0239/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0239/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0239/evidence.json` | retained evidence | 879 | `d20e9088962aa604970165a5cbc5fa0efeaaa276e2b71e6126a9f698b228d5f0` |
| `evidence/m2/IMP-EV-0261/TASK_CARD.md` | retained evidence | 1336 | `f3c188651b1e87b36ce03a2274575c1b4af5179871ab1e07e0cba5487fb7b4e3` |
| `evidence/m2/IMP-EV-0261/ci-run-34437627213-tests.log` | retained evidence | 89460 | `77b0b9b704403e2e4fd814a458eadc5f940ec7fd82bab290882c5c184148d640` |
| `evidence/m2/IMP-EV-0261/ci-run-34437627213.json` | retained evidence | 36682 | `9080da190811d409f4b3d1310407f3a70fd6be7a11e2d26418b49f4f7e6d6a8f` |
| `evidence/m2/IMP-EV-0261/evidence.json` | retained evidence | 922 | `24c0b3f57a6c411dbfa443c4eb9ccace093bf09d843958b607ace7a7e1543c8d` |
| `evidence/m2/IMP-EV-0269/TASK_CARD.md` | retained evidence | 1626 | `6f32911110242cde0365f020291de66277ad3a79df2d88d1dc0294ddd6630123` |
| `evidence/m2/IMP-EV-0269/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0269/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0269/evidence.json` | retained evidence | 991 | `9dff691eeb8ee5353ca92e1893298f4495a4c03abcdcef3d47af8cf4ccc149b5` |
| `evidence/m2/IMP-EV-0271/TASK_CARD.md` | retained evidence | 1550 | `ebbd133356fd164cc0bf67b74f0456ae197a058521d6f637f4c3d685510e2e4f` |
| `evidence/m2/IMP-EV-0271/ci-run-34409173853-tests.log` | retained evidence | 80671 | `523c4dc423fa906a9c846d64d864d36c759cb9f98bd8f4e582600a39d7c14b05` |
| `evidence/m2/IMP-EV-0271/ci-run-34409173853.json` | retained evidence | 36682 | `49f5cd98348f61290daeee2920913a705bc385ed441cf7a15a9d9a357ad2ebdf` |
| `evidence/m2/IMP-EV-0271/evidence.json` | retained evidence | 1113 | `931f4ac6a59868402b3307f956ef2beba94bc02a87ecf6351c69d9d45ccacdc9` |
| `evidence/m2/M2.1/TASK_CARD.md` | retained evidence | 3257 | `7a47c4a50166c1cad115a58af4070788d7c64bb21aa37a1219aa2a022bb9318c` |
| `evidence/m2/M2.1/ci-run-34348031607-tests.log` | retained evidence | 3150 | `02a4c2c2751e4cb57c73ee1abb0a7d16da13f00fa7372f4d16585273a4cacff7` |
| `evidence/m2/M2.1/ci-run-34348031607.json` | retained evidence | 1231 | `c9bd3a2c7221084cf7cc697cb9e792075306439a6bab77ca58067022be38d174` |
| `evidence/m2/M2.1/evidence.json` | retained evidence | 864 | `7396e055a59f6808889f2c3c448fa19730850a61d3691dc01e80fbff82bff4be` |
| `evidence/m2/M2.10/TASK_CARD.md` | retained evidence | 3814 | `a7d3b2392719d0a3f9e1c76aea060cc6def9e2ff85822a34eb78e882c0d64db0` |
| `evidence/m2/M2.10/ci-run-34395436381-tests.log` | retained evidence | 18589 | `3bc238566c0d977becc08cd0f37853cc665c489120ea1c984056189f91312e1b` |
| `evidence/m2/M2.10/ci-run-34395436381.json` | retained evidence | 36717 | `ed6fc4e8d895eb2c5ba67d2f4acc8d28b6628f5a27daaa9885a0ad7ffd78ff48` |
| `evidence/m2/M2.10/evidence.json` | retained evidence | 2140 | `14547024d6ef98772d05db92de936fd25892076b7772fdd12ab802431ca5b883` |
| `evidence/m2/M2.2/TASK_CARD.md` | retained evidence | 2587 | `f3aca8a94ba9e33fa6ff071ee26df53acb6817df4a66d238bc1a316acfda4431` |
| `evidence/m2/M2.2/ci-run-34350179687-tests.log` | retained evidence | 2403 | `3c936b2a0ee4bd54241190daff76788e4d289654fd448efa79a6cadc3b68fe4e` |
| `evidence/m2/M2.2/ci-run-34350179687.json` | retained evidence | 1231 | `bbea444d906917fb3309c3f50605ccd13c7487aa88147ccf59d0a9cef63ca48e` |
| `evidence/m2/M2.2/evidence.json` | retained evidence | 907 | `84b0d8fc60c4692e33b5d13126690c0802365d1ca2925d30ec1c6de5a183537b` |
| `evidence/m2/M2.3/TASK_CARD.md` | retained evidence | 3281 | `b23f2415827c8605b2df6897dbdca8104201024fcf4d2e1a542aaa6b147120fc` |
| `evidence/m2/M2.3/ci-run-34353528415-tests.log` | retained evidence | 3321 | `a3283b8d6cedec9a9efe2ee210be8120f1d069dba5fab8f53a6598383010c150` |
| `evidence/m2/M2.3/ci-run-34353528415.json` | retained evidence | 1231 | `d6dfa39eb8de076857b2994d707da139371daa36334474884f87c6e0036ff6a5` |
| `evidence/m2/M2.3/evidence.json` | retained evidence | 1236 | `d5a1ccaf86d7e4bf981e352a5ead828ddf609593b74f44607c4e71426d3ef00c` |
| `evidence/m2/M2.4/TASK_CARD.md` | retained evidence | 3838 | `e861fe93d884c503a96c76b837faa007272ee1344891022b0d96224a249ca39d` |
| `evidence/m2/M2.4/ci-run-34359068152-tests.log` | retained evidence | 13746 | `2bae7f58bef53da51a35736b8bd4594ff550c55ad525a47882196cd1ae855f84` |
| `evidence/m2/M2.4/ci-run-34359068152.json` | retained evidence | 32955 | `d066d5e11a5fe8f72d98333b510919403bbce424b015ea1519d3b908591b9521` |
| `evidence/m2/M2.4/evidence.json` | retained evidence | 1586 | `f02a35fdebfe98e87f3d563bda83f4bdcb8e462dbfaa1278879f50cd0b9d6721` |
| `evidence/m2/M2.5/TASK_CARD.md` | retained evidence | 4575 | `a09d1c31ffa4aae21d7ebd05e55a122bd02db1ddda47ee050ba848f7c4c8669b` |
| `evidence/m2/M2.5/ci-run-34363127873-tests.log` | retained evidence | 19342 | `4b0565cf472772b46799d1c0b3b6bc6fea7dbbdf0d03876b096f50df053654f4` |
| `evidence/m2/M2.5/ci-run-34363127873.json` | retained evidence | 33633 | `da103667c9053f60e93552c76619a2dfb51096fb7163ad9b6c2166550753d8bf` |
| `evidence/m2/M2.5/evidence.json` | retained evidence | 1704 | `d119fa227b58033113ca516a86e2027aa0aaf05242df47f6661235d282720b33` |
| `evidence/m2/M2.6/TASK_CARD.md` | retained evidence | 3369 | `a2fd87279f0b59ebf4be421563b556211f86e88c18f879d8b8e82e9dc0114f8f` |
| `evidence/m2/M2.6/ci-run-34378814770-tests.log` | retained evidence | 19424 | `9834f177b0cc87171742d39a0235fd32542d9e861f791c3c343be20424fd6222` |
| `evidence/m2/M2.6/ci-run-34378814770.json` | retained evidence | 33633 | `305ed3732177b67c3ca6f9e333c84a78ff906101d0dac907f3daacb612d35f42` |
| `evidence/m2/M2.6/evidence.json` | retained evidence | 1845 | `9ca8adbe697bc707a441ae6ebf4bde88158e1dee4d3f2b887ab04286f9541be4` |
| `evidence/m2/M2.7/TASK_CARD.md` | retained evidence | 5014 | `188e5d6032670220f59c64f4a7e1e3728e758b8e88462343f2cdb544f91198db` |
| `evidence/m2/M2.7/ci-run-34381792069-tests.log` | retained evidence | 18455 | `ab6bfaf73891c8be92b245703c0a4c887eca004c11ef54b63cf8e6123e59ed4c` |
| `evidence/m2/M2.7/ci-run-34381792069.json` | retained evidence | 33633 | `bf01938391d6ed6e27509035986f8d03344d851706f2485a08b541697e8f8ad6` |
| `evidence/m2/M2.7/evidence.json` | retained evidence | 2053 | `305b7b356afa105ecb8e71b9c2976ac49ade938754ed81d90130d5ad209374df` |
| `evidence/m2/M2.8/TASK_CARD.md` | retained evidence | 4542 | `f0d2ca2aabb1245cbb9c459264734bbf2ce37b9e8e2508d25bd9583f443f1538` |
| `evidence/m2/M2.8/ci-run-34388863580-tests.log` | retained evidence | 19193 | `24ece43e9e7801fd7a26891d7f38895a76709f0c352224c335e7f3b5e6880076` |
| `evidence/m2/M2.8/ci-run-34388863580.json` | retained evidence | 36717 | `61da94b546e41a2b3ced86b03e5ff261f37a648c5aee2a2d053442d0ee7c2e72` |
| `evidence/m2/M2.8/evidence.json` | retained evidence | 2639 | `4734f9a260c71550c10cfddfaefc00016decb73e7c95d2e42493fc9b7b436626` |
| `evidence/m2/M2.9/TASK_CARD.md` | retained evidence | 4629 | `9b3782b2919a8b5bbc9d54afa949d24a14c4e937bd1b43546840a75e40dbc42c` |
| `evidence/m2/M2.9/ci-run-34391973639-tests.log` | retained evidence | 21185 | `1b7fe238d9de9eb873dd839d34dc85affd4d8d1e8b0ae921809782519cd48512` |
| `evidence/m2/M2.9/ci-run-34391973639.json` | retained evidence | 36717 | `33769422be07389ebdb41e22d37abf1247138614809e3d789d42d4a3984755a1` |
| `evidence/m2/M2.9/evidence.json` | retained evidence | 1845 | `dc82a5488947db73d94e0a18108e9ac1e2b1d178c5fe613a3124b3631116009e` |
| `evidence/m2/PX-000/TASK_CARD.md` | retained evidence | 2146 | `5cf9f39130596ddca5e806db7ef5a2787bab2695de0dfe33b466e7481a0e4a0e` |
| `evidence/m2/PX-000/ci-run-34437627213-tests.log` | retained evidence | 89460 | `77b0b9b704403e2e4fd814a458eadc5f940ec7fd82bab290882c5c184148d640` |
| `evidence/m2/PX-000/ci-run-34437627213.json` | retained evidence | 36682 | `9080da190811d409f4b3d1310407f3a70fd6be7a11e2d26418b49f4f7e6d6a8f` |
| `evidence/m2/PX-000/evidence.json` | retained evidence | 1257 | `bac1e118d98795fbb10da1a0328369e557857f087391dd95b8f0a07bfb8bb88e` |
| `evidence/m2/PX-014/TASK_CARD.md` | retained evidence | 1951 | `43502ff55ec4efd0baaeca80aca632bd585a186bf1b5bf23dd1b1f15f49874c7` |
| `evidence/m2/PX-014/ci-run-34437627213-tests.log` | retained evidence | 89460 | `77b0b9b704403e2e4fd814a458eadc5f940ec7fd82bab290882c5c184148d640` |
| `evidence/m2/PX-014/ci-run-34437627213.json` | retained evidence | 36682 | `9080da190811d409f4b3d1310407f3a70fd6be7a11e2d26418b49f4f7e6d6a8f` |
| `evidence/m2/PX-014/evidence.json` | retained evidence | 1182 | `6e3035d3ce3ab41c0f58671e7dd9986d1d352ef1cd8f630c02821850e79f4994` |
| `evidence/m2/PX-017/TASK_CARD.md` | retained evidence | 1842 | `5df44cf0ef8fb484fcb63eacee9523c28e35230943971481b644b86aed5140b5` |
| `evidence/m2/PX-017/ci-run-34439622680-tests.log` | retained evidence | 88693 | `415094676552a441dadce7af535f9dad399567274f0419b0411284771aab83b0` |
| `evidence/m2/PX-017/ci-run-34439622680.json` | retained evidence | 36682 | `f5e43f49b4e2fdd995012d346811e19a86794947785ca35caca6bf3f23ac89a6` |
| `evidence/m2/PX-017/evidence.json` | retained evidence | 1000 | `3d7db0e75f4509c2bd11fb88fcb51f99a7f51c76131be8d03e445bd6da12555b` |
| `evidence/m2/PX-019/TASK_CARD.md` | retained evidence | 1824 | `54f2e75112574d3c93112ba72baa1cbc191f13a13d2185926edb82768effa1df` |
| `evidence/m2/PX-019/ci-run-34439622680-tests.log` | retained evidence | 88693 | `415094676552a441dadce7af535f9dad399567274f0419b0411284771aab83b0` |
| `evidence/m2/PX-019/ci-run-34439622680.json` | retained evidence | 36682 | `f5e43f49b4e2fdd995012d346811e19a86794947785ca35caca6bf3f23ac89a6` |
| `evidence/m2/PX-019/ci-run-34471234251-tests.log` | retained evidence | 1431 | `b44f23c58e1b1bbaf2f7cf3c342a275a56d92c29c5db0b1547b822121f61e178` |
| `evidence/m2/PX-019/ci-run-34471234251.json` | retained evidence | 37317 | `51f49b4b673048323c61db1e141d974a431cb427a47f88cb0a36abeef7593911` |
| `evidence/m2/PX-019/evidence.json` | retained evidence | 1201 | `570205feeb78c3caf8d325e667f798c038ff3e6096661e5e5b09ff1936c859a8` |
| `evidence/m2/PX-032/TASK_CARD.md` | retained evidence | 1939 | `845fe18eceaffc2d278b6248f189fe9fe58b20aa2c045c87b63089e98aa7690b` |
| `evidence/m2/PX-032/ci-run-34439622680-tests.log` | retained evidence | 88693 | `415094676552a441dadce7af535f9dad399567274f0419b0411284771aab83b0` |
| `evidence/m2/PX-032/ci-run-34439622680.json` | retained evidence | 36682 | `f5e43f49b4e2fdd995012d346811e19a86794947785ca35caca6bf3f23ac89a6` |
| `evidence/m2/PX-032/evidence.json` | retained evidence | 924 | `e72e89e5d1e433a5edb1f9c4c927dd07edfaca3c8444e504d2892fde7b1f4e50` |
| `evidence/m2/PX-034/TASK_CARD.md` | retained evidence | 1875 | `09d907cad0ac7857232dd87a6420b8f0a1d949edd0d2dd45a2d7f95c7810a96b` |
| `evidence/m2/PX-034/ci-run-34439622680-tests.log` | retained evidence | 88693 | `415094676552a441dadce7af535f9dad399567274f0419b0411284771aab83b0` |
| `evidence/m2/PX-034/ci-run-34439622680.json` | retained evidence | 36682 | `f5e43f49b4e2fdd995012d346811e19a86794947785ca35caca6bf3f23ac89a6` |
| `evidence/m2/PX-034/ci-run-34469269565-tests.log` | retained evidence | 1176 | `ae3e1fe869beac530b5e06e0c8faada1d5f10f0572b535712a2e5283407d4ceb` |
| `evidence/m2/PX-034/ci-run-34469269565.json` | retained evidence | 37317 | `5208d78eda32115305b861e8f7bfb6635dc06ba13cc8d7e55edaabdc2b157bae` |
| `evidence/m2/PX-034/evidence.json` | retained evidence | 1164 | `ed8376378973ff07750af188031986d2bae6fbd144ae979f1b6ecc7460a7f898` |
| `evidence/m2/PX-036/TASK_CARD.md` | retained evidence | 2187 | `b80b890ddc045041b259e4a849bf6ebb787353d53dd32d84d1a4b22d7de203bb` |
| `evidence/m2/PX-036/ci-run-34439622680-tests.log` | retained evidence | 88693 | `415094676552a441dadce7af535f9dad399567274f0419b0411284771aab83b0` |
| `evidence/m2/PX-036/ci-run-34439622680.json` | retained evidence | 36682 | `f5e43f49b4e2fdd995012d346811e19a86794947785ca35caca6bf3f23ac89a6` |
| `evidence/m2/PX-036/ci-run-34469269565-tests.log` | retained evidence | 1251 | `0ea143498ef0c29e69aa934595f9dc1c4250ae83b06d0c2b5aef68abb77843de` |
| `evidence/m2/PX-036/ci-run-34469269565.json` | retained evidence | 37317 | `5208d78eda32115305b861e8f7bfb6635dc06ba13cc8d7e55edaabdc2b157bae` |
| `evidence/m2/PX-036/evidence.json` | retained evidence | 1422 | `8cfa03fd2d857b824707dd204927956d5bf70d850f5b82734f54dca8bcc87b38` |
| `evidence/m2/PX-037/TASK_CARD.md` | retained evidence | 2135 | `ad1e3ad8837591945754b80a9116924869a6251b5decedbbceadc11bf31c21bd` |
| `evidence/m2/PX-037/ci-run-34439622680-tests.log` | retained evidence | 88693 | `415094676552a441dadce7af535f9dad399567274f0419b0411284771aab83b0` |
| `evidence/m2/PX-037/ci-run-34439622680.json` | retained evidence | 36682 | `f5e43f49b4e2fdd995012d346811e19a86794947785ca35caca6bf3f23ac89a6` |
| `evidence/m2/PX-037/ci-run-34471234251-tests.log` | retained evidence | 1314 | `56675a6e0705bf79879c016a6f23968edc09bc0a17e2dabeccb1af7786db46bd` |
| `evidence/m2/PX-037/ci-run-34471234251.json` | retained evidence | 37317 | `51f49b4b673048323c61db1e141d974a431cb427a47f88cb0a36abeef7593911` |
| `evidence/m2/PX-037/evidence.json` | retained evidence | 1252 | `31e188b3d387f3ccf181c6663e7d9234f93b087db89147d204b15468007498f1` |
| `evidence/m3/IMP-EV-0001/TASK_CARD.md` | retained evidence | 1652 | `fc790704555fa43553234da82ddef7054723f6a3d7e42a70668248bdbd3ccca3` |
| `evidence/m3/IMP-EV-0001/ci-run-34462553237-tests.log` | retained evidence | 3015 | `a6747ac9129d4a10a2deeb97f392968e069c6ce73cb119199646fc0bfcb6baaa` |
| `evidence/m3/IMP-EV-0001/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0001/evidence.json` | retained evidence | 1246 | `e51b5a7d0de4a6f50cb9d640bffcac6c0cd7a6ff5ed6cf7735ec0069c955e2e8` |
| `evidence/m3/IMP-EV-0002/TASK_CARD.md` | retained evidence | 1281 | `abfc26a1c327da1185df20fac94bd1cb80e851f05d6eedb473ec2b1f692e75cd` |
| `evidence/m3/IMP-EV-0002/ci-run-34462553237-tests.log` | retained evidence | 636 | `eebd732084f8af60676bc06cb74f1c0abe100a8f130dd943aa726a885c225a54` |
| `evidence/m3/IMP-EV-0002/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0002/evidence.json` | retained evidence | 928 | `cf9fd47919b2ffa3daff60d919e1379bfc77d91ec332a47556130955c825027c` |
| `evidence/m3/IMP-EV-0003/TASK_CARD.md` | retained evidence | 1333 | `ab97cfa7fd09a38317d23d439b291e30fe0317c3e9c6db51cca222360ad238d1` |
| `evidence/m3/IMP-EV-0003/ci-run-34462553237-tests.log` | retained evidence | 1284 | `0cf56ed00b0acf9720e1183d4669211aeb56817b1c40b3ffa21c57409f567132` |
| `evidence/m3/IMP-EV-0003/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0003/evidence.json` | retained evidence | 992 | `1c9049b37897ab4b9ca578ea8ad3c520e4c5523299e0304dfedfd4f411d94103` |
| `evidence/m3/IMP-EV-0004/TASK_CARD.md` | retained evidence | 1568 | `ecd164138ea68472ca1fbf33392a7d4f11f17a3773528cc4416c07309f892b23` |
| `evidence/m3/IMP-EV-0004/ci-run-34462553237-tests.log` | retained evidence | 2361 | `986e052dbeb0e4a03e2c278adeb88e8b5cdeea4f537ecdb8b0c71459cd0c39e9` |
| `evidence/m3/IMP-EV-0004/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0004/evidence.json` | retained evidence | 1173 | `681004e8ec8d6dacd083889e8b35fbca63a48f77057e1e5584d248eb90dbaf7d` |
| `evidence/m3/IMP-EV-0005/TASK_CARD.md` | retained evidence | 1603 | `fc642e9be08e70921437dddb4e122c8be7ce3e7c1e8cc914dd85fcd890bcb091` |
| `evidence/m3/IMP-EV-0005/ci-run-34462553237-tests.log` | retained evidence | 3051 | `4e015d8675f24122010dd81ccc928c38acbe496b9049fa0e8f65baa1c39c5e13` |
| `evidence/m3/IMP-EV-0005/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0005/evidence.json` | retained evidence | 1177 | `6fcd1aa036f8ef72c2859c10f07ea7ef9160ef0b8f610ebbe0e540fd5f7214b4` |
| `evidence/m3/IMP-EV-0020/TASK_CARD.md` | retained evidence | 1422 | `24fb80792b02e4757aa352b36a5f9d0c519b3e2e5e975c148dbbff8b4d842f29` |
| `evidence/m3/IMP-EV-0020/ci-run-34462553237-tests.log` | retained evidence | 1869 | `3956b7d3d91bc50aeae405eeddca1e57014998fef0e4236b0c8e9ddeffa668ff` |
| `evidence/m3/IMP-EV-0020/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0020/evidence.json` | retained evidence | 1068 | `0c87102a16317870f8c58fbfaa5bc17c22b147aafde94a274af1e579fc85005c` |
| `evidence/m3/IMP-EV-0070/TASK_CARD.md` | retained evidence | 1598 | `815affd431b792e72da2d3dcf4e590cdc344febc86292e2214c9773b981c3beb` |
| `evidence/m3/IMP-EV-0070/ci-run-34464262601-tests.log` | retained evidence | 1845 | `9ab39942ed54897233aec56290f17bc25062c30162006a6902ad34770227b7c6` |
| `evidence/m3/IMP-EV-0070/ci-run-34464262601.json` | retained evidence | 36717 | `36ef0dcdf9b2a14fa490c68e8bf945d79b9136638786d577d6d9b0e1a692115b` |
| `evidence/m3/IMP-EV-0070/evidence.json` | retained evidence | 770 | `d69c1605f9efc1c6f489c8a93f5b878c424b4a5ad74c5b123bb458a356898660` |
| `evidence/m3/IMP-EV-0132/TASK_CARD.md` | retained evidence | 1827 | `62b0ae388684dcc322e21069ce59dd3a386588d1656e07c3abb9c2881a651b53` |
| `evidence/m3/IMP-EV-0132/ci-run-34466693450-tests.log` | retained evidence | 1206 | `fe570776d32a765b146b39654873957e0403d4dedf503e38cddb381d2eb77c67` |
| `evidence/m3/IMP-EV-0132/ci-run-34466693450.json` | retained evidence | 37317 | `3d2da0166c2a6b035052bd8ea4b1b2aaf914d4ec198416337d98983d99769ebf` |
| `evidence/m3/IMP-EV-0132/evidence.json` | retained evidence | 761 | `49f4e6161016c5a98df04bbe02fe5ca90938d4dd656c9fa0492ee8fe73dab798` |
| `evidence/m3/IMP-EV-0134/TASK_CARD.md` | retained evidence | 1886 | `680d6a4711545c09ab19020bf52cd32f20361acd6f7e29f3b45956f31bd4054c` |
| `evidence/m3/IMP-EV-0134/ci-run-34466693450-tests.log` | retained evidence | 1314 | `da3268f4ae3f5ec5ae1e0eff9a39e1bf2b95931e50bbc5975c978a54f074c862` |
| `evidence/m3/IMP-EV-0134/ci-run-34466693450.json` | retained evidence | 37317 | `3d2da0166c2a6b035052bd8ea4b1b2aaf914d4ec198416337d98983d99769ebf` |
| `evidence/m3/IMP-EV-0134/evidence.json` | retained evidence | 1247 | `1b780004e0c5c58832813332c5bca93f059974bb48fb706068be0b4c329acc72` |
| `evidence/m3/IMP-EV-0153/TASK_CARD.md` | retained evidence | 1295 | `e0cbd8d065bbb9d09e0a99ed620118841e4c901d75732f638b8622f3ab7df1e5` |
| `evidence/m3/IMP-EV-0153/ci-run-34462553237-tests.log` | retained evidence | 1842 | `a9c53589aa6cf770e2abb8bd08a7b64d202abb54ea27e2c4fbd7f3cb5df80e7d` |
| `evidence/m3/IMP-EV-0153/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0153/evidence.json` | retained evidence | 1021 | `efdd8d4f9b4fce66a11586ec07131ddb0f9f8b7fe5af91c2077155e188acb394` |
| `evidence/m3/IMP-EV-0154/TASK_CARD.md` | retained evidence | 1220 | `ef2c5a029b58ea0ac0a392ff1bcb7df9a65c26abdda7828a415681a421483c44` |
| `evidence/m3/IMP-EV-0154/ci-run-34462553237-tests.log` | retained evidence | 1260 | `e8d2a9b6125cd761950264060e9e908ffaab1e211ced206ef24d916970eef89b` |
| `evidence/m3/IMP-EV-0154/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0154/evidence.json` | retained evidence | 943 | `e7c33d1afa549fa15eff6a3f39f7ac310447017c16994cce904c3b44c8280235` |
| `evidence/m3/IMP-EV-0155/TASK_CARD.md` | retained evidence | 1303 | `3bc036c9b09ef5281e707bf81a46124657f8207f70d4a58752defe03d2621412` |
| `evidence/m3/IMP-EV-0155/ci-run-34462553237-tests.log` | retained evidence | 1875 | `6e1341f3359308a7560182e78f49ce89d312774f7cafa41cbee8539244ba6ccb` |
| `evidence/m3/IMP-EV-0155/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0155/evidence.json` | retained evidence | 1013 | `e10e841b06316a6e4b10f0c614213e623930644185a623a956990eae1a24ac33` |
| `evidence/m3/IMP-EV-0157/TASK_CARD.md` | retained evidence | 1297 | `84750b6a33379c26e76ec0f5e50764c038ff6c93a6cafcfa77fa95fcfcd088d6` |
| `evidence/m3/IMP-EV-0157/ci-run-34462553237-tests.log` | retained evidence | 1953 | `cb8db8482b87c1f8b268aa36f1063f37abfed80195c221380b34429c31d8bae6` |
| `evidence/m3/IMP-EV-0157/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0157/evidence.json` | retained evidence | 1024 | `b5655877742376c8ece334bc6a4e2001351ddc39a054bc4819290450935f5694` |
| `evidence/m3/IMP-EV-0158/TASK_CARD.md` | retained evidence | 1311 | `ddec5571f8e87a4da085b806355d10f788db5a0a0d28c4a3bcb29993abdd7717` |
| `evidence/m3/IMP-EV-0158/ci-run-34462553237-tests.log` | retained evidence | 1827 | `9d77e4d7bdd4e65240b8341e88f54d90b29163979fe968f7d590802aa8b7bdbf` |
| `evidence/m3/IMP-EV-0158/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0158/evidence.json` | retained evidence | 1016 | `cc11ae1c372bb274f94bd86b1b770935d6f8b562a6c7f65281bc998508e04107` |
| `evidence/m3/IMP-EV-0159/TASK_CARD.md` | retained evidence | 1239 | `bdbc3fb20d2e5d3304afee61de88f10dc3014a0ff303c467652b24b417929aed` |
| `evidence/m3/IMP-EV-0159/ci-run-34462553237-tests.log` | retained evidence | 1896 | `084d86d8040546fb7eb705379f2c9345e41ea43c5af138787ce308567bb52864` |
| `evidence/m3/IMP-EV-0159/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0159/evidence.json` | retained evidence | 948 | `fc8a78148b3dd3ed738f54876e549c0a17d839f54a7b9406a85d081a22601d32` |
| `evidence/m3/IMP-EV-0163/TASK_CARD.md` | retained evidence | 1224 | `8ddfee70a597bb925369f509a50a484e2778f4f6c603c4ec520dc7c2e02d86ac` |
| `evidence/m3/IMP-EV-0163/ci-run-34462553237-tests.log` | retained evidence | 1242 | `a639ec192a1aebdbca76d152258d6bce1facc55295395bcec5338edfe6ec4afc` |
| `evidence/m3/IMP-EV-0163/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0163/evidence.json` | retained evidence | 924 | `10e821d110c168c7cb48b7612dd198b6d4dc801d6508054f632b94a3f0298413` |
| `evidence/m3/IMP-EV-0164/TASK_CARD.md` | retained evidence | 1153 | `4fcae57831ebdfa235163bdedd10c8dc3498a902a998ee8c94f7ae1da17a2f00` |
| `evidence/m3/IMP-EV-0164/ci-run-34462553237-tests.log` | retained evidence | 1314 | `4de22029642529740f27e04f5eb97a101f65ab1b8f8f8c841994c5df09c5eb48` |
| `evidence/m3/IMP-EV-0164/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0164/evidence.json` | retained evidence | 896 | `38a4b9b2e024dfac9db84f8a0e94ab126a475b12b702aa911e0707b91eccd11b` |
| `evidence/m3/IMP-EV-0165/TASK_CARD.md` | retained evidence | 1218 | `466c289087d86c5084006eef6771d0b07ed4d152db509414c825f398bb11ab64` |
| `evidence/m3/IMP-EV-0165/ci-run-34462553237-tests.log` | retained evidence | 1212 | `72d51f6fd273565d664ff147d290b56beedf50e4bab2c6ed363f74bcac80ea20` |
| `evidence/m3/IMP-EV-0165/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0165/evidence.json` | retained evidence | 908 | `a2b8be68e5516827851bdc20cf87cf8dd8d0ac24f3c9dfbd2ba4ae0558bd2596` |
| `evidence/m3/IMP-EV-0166/TASK_CARD.md` | retained evidence | 1190 | `668a306ac0cd157af0b883a3d7ef0983c28889288168bc7d65e717fb1121de65` |
| `evidence/m3/IMP-EV-0166/ci-run-34462553237-tests.log` | retained evidence | 1272 | `97810828b1d394f1f9ef4d871473f098a35ea3744a6851ef962c49bbca74f36c` |
| `evidence/m3/IMP-EV-0166/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0166/evidence.json` | retained evidence | 917 | `580bc053e41776cd5357ce6769a0adb52656a571d684ed507a82b96adafad6b5` |
| `evidence/m3/IMP-EV-0167/TASK_CARD.md` | retained evidence | 1238 | `4b1d7b007f3913c2b46cb994d88e3d199c645fec983809b91a784312c8e32178` |
| `evidence/m3/IMP-EV-0167/ci-run-34462553237-tests.log` | retained evidence | 1284 | `0cf56ed00b0acf9720e1183d4669211aeb56817b1c40b3ffa21c57409f567132` |
| `evidence/m3/IMP-EV-0167/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0167/evidence.json` | retained evidence | 987 | `ce9aef35ece3a1960ceeb05fd2c7248cf23e226e617c10a03ea8a5db4b527881` |
| `evidence/m3/IMP-EV-0168/TASK_CARD.md` | retained evidence | 2714 | `02114e9701217fd7cf53220cf96aba31ee0748b9941d58c339debbc8a843ff66` |
| `evidence/m3/IMP-EV-0168/ci-run-34498383277-tests.log` | retained evidence | 2007 | `3020c6e11a914d02eba6a6cdcf64fff7435bd797f3a8e8d78f47c69365fd32cd` |
| `evidence/m3/IMP-EV-0168/ci-run-34498383277.json` | retained evidence | 37317 | `16c39bba70461d51e17f499c99673d31287afbb21f7329a51eae7d166968f36e` |
| `evidence/m3/IMP-EV-0168/evidence.json` | retained evidence | 1747 | `b4db381f8ce33aff65d40d4a76dd444880c8f1a224a0e1d43df04196c7ff7258` |
| `evidence/m3/IMP-EV-0169/TASK_CARD.md` | retained evidence | 2209 | `7b9985fde13a415e4d2a0a981ff976fa3843e4358dd845db85c48005c903f026` |
| `evidence/m3/IMP-EV-0169/evidence.json` | retained evidence | 631 | `957c4ab379ef34b278cb12d844835ca43a43355a843e9e89287b3427628bad26` |
| `evidence/m3/IMP-EV-0170/TASK_CARD.md` | retained evidence | 1177 | `79f105dcce7f4c8a45923201a4b08022179a09bdd2430520c3d7540d7d9154d2` |
| `evidence/m3/IMP-EV-0170/ci-run-34462553237-tests.log` | retained evidence | 1245 | `d1d7329d8fe5113979dcb82432f40adafa58d229a0cfe0726527d3c8129bd633` |
| `evidence/m3/IMP-EV-0170/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0170/evidence.json` | retained evidence | 882 | `47b502b0b587cf911534da0885e741c3c475175d33cf3166918cc3ccf0fde2c5` |
| `evidence/m3/IMP-EV-0171/TASK_CARD.md` | retained evidence | 1258 | `0ff370ce5bf02ef4c2ea5eca21247212b134ebb0099fdc2b1c5d3d865f44b5fa` |
| `evidence/m3/IMP-EV-0171/ci-run-34462553237-tests.log` | retained evidence | 1842 | `1c955e6734f2629cd1419f3fc1a4fce43269cb46e4c98a4e44f914ac1bb671ce` |
| `evidence/m3/IMP-EV-0171/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0171/evidence.json` | retained evidence | 921 | `584bbbf5d27124dd8026067d1084b0c7e5b99c09ba01af3cc663978f8e2bc1cc` |
| `evidence/m3/IMP-EV-0172/TASK_CARD.md` | retained evidence | 1291 | `04817704efa10b0fe636bc088a651dc95ce05b65b0fa1a3fd9c737f2d25884f4` |
| `evidence/m3/IMP-EV-0172/ci-run-34462553237-tests.log` | retained evidence | 1905 | `dc524e0b1d8064123674b42bfcf493b2e3e7eef409f645d33f38d7bc72cdc3cd` |
| `evidence/m3/IMP-EV-0172/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0172/evidence.json` | retained evidence | 981 | `2decb4ad544312e9e0f71ee80ad6f945e75de3e04fc7c29772024b2fcdda3a9b` |
| `evidence/m3/IMP-EV-0177/TASK_CARD.md` | retained evidence | 1705 | `fe6301f62b1f34a71f8b8b239969c8eba8179b091ad6eda6b9935f75790cf0ee` |
| `evidence/m3/IMP-EV-0177/ci-run-34466693450-tests.log` | retained evidence | 1314 | `da3268f4ae3f5ec5ae1e0eff9a39e1bf2b95931e50bbc5975c978a54f074c862` |
| `evidence/m3/IMP-EV-0177/ci-run-34466693450.json` | retained evidence | 37317 | `3d2da0166c2a6b035052bd8ea4b1b2aaf914d4ec198416337d98983d99769ebf` |
| `evidence/m3/IMP-EV-0177/evidence.json` | retained evidence | 1217 | `07c4a4068962146b39e5213b1ed24308a50e966eaa1d4d18d9b5ee406ef66b09` |
| `evidence/m3/IMP-EV-0229/TASK_CARD.md` | retained evidence | 1649 | `25463862be1c269c4e5c09f1970a39ad2b2ab8449a1e8f2226649425644f5db8` |
| `evidence/m3/IMP-EV-0229/ci-run-34466693450-tests.log` | retained evidence | 1314 | `da3268f4ae3f5ec5ae1e0eff9a39e1bf2b95931e50bbc5975c978a54f074c862` |
| `evidence/m3/IMP-EV-0229/ci-run-34466693450.json` | retained evidence | 37317 | `3d2da0166c2a6b035052bd8ea4b1b2aaf914d4ec198416337d98983d99769ebf` |
| `evidence/m3/IMP-EV-0229/evidence.json` | retained evidence | 1122 | `df04f22b5bbf2aebe001e43cd569571330ba958aebf631b6ca4bfa9fb9618ac8` |
| `evidence/m3/IMP-EV-0249/TASK_CARD.md` | retained evidence | 1115 | `4615126cc884c686d6920ac861a95847a8b33451c4d44a079cba5cc721c96af9` |
| `evidence/m3/IMP-EV-0249/ci-run-34462553237-tests.log` | retained evidence | 606 | `a840b32312940d485605140cd2297ede8ee84641df2d83cf7ac8ee06bf20084e` |
| `evidence/m3/IMP-EV-0249/ci-run-34462553237.json` | retained evidence | 36717 | `21620a89049f0926b25220a8376fad23a3242504355fa77799fb84fa7fc89106` |
| `evidence/m3/IMP-EV-0249/evidence.json` | retained evidence | 789 | `a05e0202141a9477c146b85e2f797ee8e78789f6eb5fbd1a2392e8b18c5075a7` |
| `evidence/m3/M3.1/TASK_CARD.md` | retained evidence | 3575 | `1ac323121367267e96fc8ae9bad301497ec26ed0ad0646b320dadc84b029695e` |
| `evidence/m3/M3.1/ci-run-34442392803-tests.log` | retained evidence | 59566 | `f4915fcacb8c1af4e218f6d0b2dc839242ce068c2e3ce5b1f004443fa193a32d` |
| `evidence/m3/M3.1/ci-run-34442392803.json` | retained evidence | 36682 | `02672015d974336fd27a6bb5a10c7dc08855d284c9bef7558e148c055d9cc329` |
| `evidence/m3/M3.1/evidence.json` | retained evidence | 923 | `7e9916c1d7fc8c1cedd895df9d216760c8489592d7fcd0a161f7924dec71b017` |
| `evidence/m3/M3.2/TASK_CARD.md` | retained evidence | 2615 | `0c254064cd9d205d74c959174786ab77aa03604f437375d638a52df584774382` |
| `evidence/m3/M3.2/ci-run-34443247904-tests.log` | retained evidence | 58907 | `5ec7eeaea73cdf0ec5df4404d77645956159ae9c2a77fb07d5a945705182e734` |
| `evidence/m3/M3.2/ci-run-34443247904.json` | retained evidence | 36682 | `d606062199e073b30e6ba1679d895b0e14fb6f221fd817cdae2a27d3f25447e5` |
| `evidence/m3/M3.2/evidence.json` | retained evidence | 650 | `7bae805146d689ba30cca5c38a971a67ce4176a14ee12ca0a3d81a7ca4e5da81` |
| `evidence/m3/M3.3/TASK_CARD.md` | retained evidence | 2837 | `a98fdcdf50450fcd7c8d94ba4c4d6e9b6d379a140c458c98a8a46f180152ab56` |
| `evidence/m3/M3.3/ci-run-34444133255-tests.log` | retained evidence | 60130 | `ff3687b3f4e13765020a818e846ee4e642cc9ec7e7a75e2cdf7faeb39b0ae052` |
| `evidence/m3/M3.3/ci-run-34444133255.json` | retained evidence | 36682 | `67b61a1a52165779ea2da8691f3953fcfc74a2ebb69ea56fdadc28d2db8187f4` |
| `evidence/m3/M3.3/evidence.json` | retained evidence | 696 | `e39b0490bc0fac81a9dd5875dddf92bf0ebc80c5610677c6ab6ca9fc3f389269` |
| `evidence/m3/M3.4/TASK_CARD.md` | retained evidence | 3232 | `aa73999748c8886b12244c4df91a0a23bbd85c9057a7ac0d07283d4076ce5584` |
| `evidence/m3/M3.4/ci-run-34461176500-tests.log` | retained evidence | 3732 | `d2324e8dd7d0aa24ee8fd6b9df97d9368f3c759bf5703dcf33cbc1c0706d71fe` |
| `evidence/m3/M3.4/ci-run-34461176500.json` | retained evidence | 36717 | `ca2e3fb22dd9789c70d00592226ac739aba339118b7fbcbaa0b2869e2d9f226b` |
| `evidence/m3/M3.4/evidence.json` | retained evidence | 1320 | `4bfeae1aa053f87501d77c559e449fc5d521c355ea93531acdbf7fdd7bd119c8` |
| `evidence/m3/M3.5/TASK_CARD.md` | retained evidence | 3391 | `d6cbc1db469f1e4ecc0ea973b5a5d23897c41c0d4bb0dbfe74c9e3f795c7eb10` |
| `evidence/m3/M3.5/ci-run-34461176500-tests.log` | retained evidence | 1902 | `4b501620b0709261e8c4c5ad3fda8aafa4684bdb5002e73c0e2c69c8eab5e0af` |
| `evidence/m3/M3.5/ci-run-34461176500.json` | retained evidence | 36717 | `ca2e3fb22dd9789c70d00592226ac739aba339118b7fbcbaa0b2869e2d9f226b` |
| `evidence/m3/M3.5/evidence.json` | retained evidence | 979 | `74100ad39b8b06c03b5f36d246975b27d3b14b9728f4b8e556531d3f0eea1497` |
| `evidence/m3/M3.6/TASK_CARD.md` | retained evidence | 4126 | `813fca4c54a0115f0da152ad6e82756f88ae6cce9e9f52c1fd54f20d6dfa6d7e` |
| `evidence/m3/M3.6/ci-run-34461176500-tests.log` | retained evidence | 4836 | `4785566fb633b7f8f15478a50d28beb226406ae360ff9844755a032eb860f549` |
| `evidence/m3/M3.6/ci-run-34461176500.json` | retained evidence | 36717 | `ca2e3fb22dd9789c70d00592226ac739aba339118b7fbcbaa0b2869e2d9f226b` |
| `evidence/m3/M3.6/evidence.json` | retained evidence | 1307 | `58cbc6474401787bfa2e20fbb4a3b6be2750cf8ad0ce548bb7e2e866931b7aad` |
| `evidence/m3/M3.7/TASK_CARD.md` | retained evidence | 4106 | `45eea59bdae09c3935bad5532ef721b65b4f8afca4fbc37670422c16db1202bf` |
| `evidence/m3/M3.7/ci-run-34461176500-tests.log` | retained evidence | 3759 | `a9c7c5f3395dff0307eba0fa4989910ec6045fac4b6cdc40aa9951aac7e9a87c` |
| `evidence/m3/M3.7/ci-run-34461176500.json` | retained evidence | 36717 | `ca2e3fb22dd9789c70d00592226ac739aba339118b7fbcbaa0b2869e2d9f226b` |
| `evidence/m3/M3.7/evidence.json` | retained evidence | 1291 | `3eb84ad9cb19b0f9e30e002a21598b1f1d4f9c4e3e219dbec2b9caeea5377184` |
| `evidence/m3/M3.8/TASK_CARD.md` | retained evidence | 4355 | `d25dc05e3440eff93872d13a39629983af23eeca67922476d466af45c0a5219c` |
| `evidence/m3/M3.8/ci-run-34461176500-tests.log` | retained evidence | 3099 | `336201c87526544e5102878b5b394a4b5f6eba07d2149796c744afdae25d6cd1` |
| `evidence/m3/M3.8/ci-run-34461176500.json` | retained evidence | 36717 | `ca2e3fb22dd9789c70d00592226ac739aba339118b7fbcbaa0b2869e2d9f226b` |
| `evidence/m3/M3.8/evidence.json` | retained evidence | 1124 | `5039ad18aa57a62d5c3a640095d9f905956c4e5b34c5914de6ace89554755d7f` |
| `evidence/m3/M3.9/TASK_CARD.md` | retained evidence | 4624 | `b5040f4c43a2b8886629c385e07665db2c3187e6c246724c7201d815865c4744` |
| `evidence/m3/M3.9/ci-run-34461176500-tests.log` | retained evidence | 1944 | `058be90dcb5bc7da540f25975c3958389c86aff1f04193898dac02300865deaa` |
| `evidence/m3/M3.9/ci-run-34461176500.json` | retained evidence | 36717 | `ca2e3fb22dd9789c70d00592226ac739aba339118b7fbcbaa0b2869e2d9f226b` |
| `evidence/m3/M3.9/evidence.json` | retained evidence | 1013 | `ca3d936ee57026d51f12a615cf69e73a3f37550fe3ed78ffc5be17473d090396` |
| `evidence/m3/M3.9/report.json` | retained evidence | 23092 | `5f37997802d678ab54adb91c49acd926120c65e1f4f2ad54fdd998c71102db23` |
| `evidence/m3/PX-015/TASK_CARD.md` | retained evidence | 3241 | `b6893a497e954894af48364366bca3fa234d8c5e021df75dda6aba46c1f535a8` |
| `evidence/m3/PX-015/ci-run-34498383277-tests.log` | retained evidence | 2517 | `a8f2d7e88e7e9aece42af677d8fd6dba6178cc4543e53ff3e2c609d983808895` |
| `evidence/m3/PX-015/ci-run-34498383277.json` | retained evidence | 37317 | `16c39bba70461d51e17f499c99673d31287afbb21f7329a51eae7d166968f36e` |
| `evidence/m3/PX-015/evidence.json` | retained evidence | 2073 | `11c4acd953047fb1837d7716584d3261ad0e16cd33f6ba0f6f1187969680764f` |
| `evidence/m3/PX-016/TASK_CARD.md` | retained evidence | 2762 | `6a54b9fb08551320ae1889b36ff78dcdc721cd0afc898cff076200c18a2a89f1` |
| `evidence/m3/PX-016/ci-run-34469269565-tests.log` | retained evidence | 2430 | `b7046bffb729567ce56b951431295e858687f53554bec52745bbabf978674660` |
| `evidence/m3/PX-016/ci-run-34469269565.json` | retained evidence | 37317 | `5208d78eda32115305b861e8f7bfb6635dc06ba13cc8d7e55edaabdc2b157bae` |
| `evidence/m3/PX-016/evidence.json` | retained evidence | 912 | `c120cf94ab4f58c6388cf0e739f45f07acc24707ea218011715a12649f92a095` |
| `evidence/m3/PX-018/TASK_CARD.md` | retained evidence | 3548 | `9d67374911c7a0f6784378bda07dff0773cd8b74a4f39b1ca1e3d53f59ed548d` |
| `evidence/m3/PX-018/ci-run-34471234251-tests.log` | retained evidence | 2136 | `8dc9071d8ac862071e95c060d2073f7f723acd8b14c1dd7b11a1a47cb814c430` |
| `evidence/m3/PX-018/ci-run-34471234251.json` | retained evidence | 37317 | `51f49b4b673048323c61db1e141d974a431cb427a47f88cb0a36abeef7593911` |
| `evidence/m3/PX-018/evidence.json` | retained evidence | 1049 | `5e419f2adfeff3fe438a6f5ab07078259ed7bef9de2b527eeff270bbc6ba71b4` |
| `evidence/m3/PX-026/TASK_CARD.md` | retained evidence | 3189 | `4615db25a626df74741f3e6de545ecd751addac484f128f8856ed33bd6464e4d` |
| `evidence/m3/PX-026/ci-run-34467756882-tests.log` | retained evidence | 65408 | `79c0f8192c5e3083c8fe7c12d39f4caed30c58161e53ee58974467a60e1d7281` |
| `evidence/m3/PX-026/ci-run-34467756882.json` | retained evidence | 37317 | `5f2418b7d2fca244d450172ef0671b0b08750b52cb7137f8aeb62e09d1f9cfea` |
| `evidence/m3/PX-026/evidence.json` | retained evidence | 1239 | `149fd313352adab868d722d4f4f2438204ce0ffb7a4aa830a437a94e5906a78a` |
| `evidence/m3/PX-033/TASK_CARD.md` | retained evidence | 2737 | `142f49bb8678a908cf07790bedb2e07c6b745ce61e2c30c6e2f15d230a88a27b` |
| `evidence/m3/PX-033/ci-run-34466693450-tests.log` | retained evidence | 2406 | `0ad691360e25f1bcce40efe49a56a9cbec52ba964fb8d21b37bf2ccb89d505bd` |
| `evidence/m3/PX-033/ci-run-34466693450.json` | retained evidence | 37317 | `3d2da0166c2a6b035052bd8ea4b1b2aaf914d4ec198416337d98983d99769ebf` |
| `evidence/m3/PX-033/evidence.json` | retained evidence | 1064 | `cd720b31c4bbf9b26a79876154ef5ec2611ad9fae461f943129e22d6b965c56a` |
| `evidence/m3/PX-035/TASK_CARD.md` | retained evidence | 3172 | `377c1b7311212a813c15a3a9054e592f33162b5e862eba157439f8b2e79c78c9` |
| `evidence/m3/PX-035/ci-run-34506031420-tests.log` | retained evidence | 2475 | `c3749bbece693b72275824bbe92c4d3384de5e52a0b6f24e7d7b48ca902c63e5` |
| `evidence/m3/PX-035/ci-run-34506031420.json` | retained evidence | 37317 | `2dda872217d637cc239830d540966e7ceea26638695b761005ae8130ead05733` |
| `evidence/m3/PX-035/evidence.json` | retained evidence | 1052 | `b9802499483e480a242f589fa2e57aa2fb05a977d436ff2526765540299466ea` |
| `evidence/m3/PX-038/TASK_CARD.md` | retained evidence | 3573 | `48cfc0e7710c822a6ad91fe7a7eff6f59625cabea668760b81419623d1d67d44` |
| `evidence/m3/PX-038/ci-run-34506031420-tests.log` | retained evidence | 1344 | `dd01e3782bdfd564fcef011097d4018e6b370811e957b83feeffa984b8825ffa` |
| `evidence/m3/PX-038/ci-run-34506031420.json` | retained evidence | 37317 | `2dda872217d637cc239830d540966e7ceea26638695b761005ae8130ead05733` |
| `evidence/m3/PX-038/evidence.json` | retained evidence | 822 | `a0c75c36ebb7c39a262fdbd87f815020ca7c4253672b8f60a819e0601200d28a` |
| `evidence/m3/PX-039/TASK_CARD.md` | retained evidence | 3867 | `3f69c9719f270edcf74374445342589e63c1f2924d8fa1f2e8759e76e84db5d7` |
| `evidence/m3/PX-039/ci-run-34506031420-tests.log` | retained evidence | 2610 | `8fc902968b8e5690c051b3f0ff8a2cf6700ad68cd7f6dfce5f5be3881ac8d3f0` |
| `evidence/m3/PX-039/ci-run-34506031420.json` | retained evidence | 37317 | `2dda872217d637cc239830d540966e7ceea26638695b761005ae8130ead05733` |
| `evidence/m3/PX-039/evidence.json` | retained evidence | 1004 | `63485b51f7968937de279637c1971fed433e6745b1b6899c4bca1e466a33e6bb` |
| `evidence/m3/PX-040/TASK_CARD.md` | retained evidence | 3796 | `b7ea1b6f7622e27c76dbc8d3f0537eb8288bc9e9aa59fff2bc34e884980cfa51` |
| `evidence/m3/PX-040/evidence.json` | retained evidence | 982 | `b272759ed67e637df4dd57cf0ea8728d71968451b2a0034e6f1717b1454e3706` |
| `graph/PROJECT_GRAPH.md` | human view of the graph | 40904 | `f77de324dbe84468e5fe29eac6f394ef9537224fe9cdabebd7165e7f36fb6ab1` |
| `graph/project-graph.json` | project driver graph with live status | 1223742 | `8c938f31261d75b0f5d8ee929c96539a55ea4dd7741037bcaa3c281c80b7961c` |
| `tools/build_graph.py` | regenerates graph structure from docs | 52844 | `13ea31cbf1ace263d36b4cc5ada8b19270a118e30e401114790652375b5c51d1` |
| `tools/build_manifest.py` | regenerates this manifest | 14871 | `b35f87f995ebea778ef5002f42c4dcb2f195f303ae121d7b4d73070d95d665b7` |
| `tools/check_dossier.py` | integrity gate | 17596 | `5c9b7a782d245223726f860ca583d766929a6f5662b644adebee64d257ad426b` |
| `tools/dossier_epr.py` | parses additive EPR authority and traceability | 9310 | `4b5e399ee1b4886662294e3780d7f0630195e29c8198256ff7719d5236591a18` |
| `tools/dossier_px.py` | parses the additive product-extension ledger and phased release rules | 6931 | `0a7dd5bd5872fb1d959c56fd017bd3916062d0f60d1bc51b9abceddc134202e2` |
| `tools/graph.py` | query/update graph | 36313 | `76f2a79fca3c305205f34573b94118c99511944833e8e602bf0eba7e2ac305cb` |
| `tools/test_dossier.py` | copied-package integration and negative tests | 33052 | `325773a518630a898fbe0579e2250c4404904beadf1c804f6dc594f39478f42d` |

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
