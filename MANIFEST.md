# Modbit Dossier Manifest — V3.3 EPR v1.1

> **Authority date:** 2026-09-05  
> **Generated:** 2026-09-05 by `tools/build_manifest.py`  
> **Scope:** every specification file in `docs/` plus the root governing files and tooling. The previous `99_MANIFEST.md` covered only 39 Part 2 files; this manifest covers all 87 docs.
> **Machine-readable twin:** `manifest.json` (same content, same hashes).

## Integrity rule

A dossier package is valid only if every path below exists with the listed SHA-256. `python3 tools/check_dossier.py --manifest` verifies this. Regenerate after any edit with `python3 tools/build_manifest.py`.

## Summary

| Section | Range | Files | Bytes |
|---|---|---:|---:|
| Authority and orientation | 00–09 | 8 | 64029 |
| Architecture and subsystems | 10–29 | 20 | 181159 |
| Implementation specifications | 30–39 | 10 | 77578 |
| Requirements, tasks and traceability | 40–49 | 10 | 351091 |
| Verification and testing | 50–69 | 14 | 144049 |
| Delivery and operations | 70–79 | 6 | 26903 |
| Agent process and governance | 80–97 | 18 | 98548 |
| Live state | 98–99 | 1 | 3523 |
| **Total docs** | | **87** | **946880** |

## Specification files (`docs/`)

| # | File | Title | Section | Bytes | SHA-256 |
|---:|---|---|---|---:|---|
| 00 | `docs/00_MASTER_INDEX.md` | Modbit — AI-Agent Build Dossier V3.3 EPR v1.1 | authority | 15553 | `cb874c5c5ea8fd33b9fbe7058ac0ba5161c8a07eddb2495d83fceac126cfd6a3` |
| 01 | `docs/01_START_HERE_FOR_BUILD_AGENTS.md` | Start Here for Build Agents | authority | 3036 | `e5ad750bd3c5c0a219f135bf3878c91c6230d417a66fca664becef682eb78fc2` |
| 02 | `docs/02_AUTHORITY_AND_DECISIONS.md` | Authority, Decision Register, and Conflict Resolution | authority | 16068 | `d56806f57926b7499ec7d3085208304ec679e7caaed50cc0422b15e346106160` |
| 03 | `docs/03_ARCHITECTURAL_CONFLICTS_AND_SUPERSESSIONS.md` | Architectural Conflicts and Supersessions | authority | 6175 | `6c80da9c6575a10848352f18c794b5896c2006a83d022b6513f7993fe217a95c` |
| 04 | `docs/04_REQUIREMENT_BASIS_AND_LIMITS.md` | Requirement Basis and Limits | authority | 1724 | `d9e07d840b4b006d79d00251d9525de503d5676e10e09b2229c7bfe5e2f04344` |
| 05 | `docs/05_EXECUTION_POLICY_ROUTER_ADOPTION_DECISION.md` | Execution policy router adoption decision | authority | 6756 | `ebb8376a8f733b5214f7e3ae7b14a83a865d3bfff6d2afe1a445a9f3a7763866` |
| 06 | `docs/06_EPR_V1_1_SUPERSESSION_DECISION.md` | EPR v1.1 supersession decision | authority | 8212 | `1444af3f025244525f537f5fdca31f50775f0f2857396705c306560dbd06ec10` |
| 07 | `docs/07_PRODUCT_EXTENSION_DECISION_RECORD.md` | Product extension decision record | authority | 6505 | `4c1d01b41e9aa50ec6b4d21bdcfd7aa98abee07be3e579b6e7202ad585ceb92a` |
| 10 | `docs/10_PRODUCT_PRD_AND_UX.md` | Product Requirements and UX Specification | architecture | 10754 | `8d227d45b322e757fc372590fda8b4b11b61471f751099ea7635b7c04214a162` |
| 11 | `docs/11_SYSTEM_ARCHITECTURE.md` | End-to-End System Architecture | architecture | 12638 | `34b51dd3be83dac06e6d2a50d9fc8baed4b9d7839597c9a3ebe1b38c66552233` |
| 12 | `docs/12_REPOSITORY_AND_MODULE_LAYOUT.md` | Clean Repository and Module Layout | architecture | 9439 | `9cf99c8535c14f16b216f2a477703fdc9c4a47a63867a616eea3ffcc3e3560b4` |
| 13 | `docs/13_DOMAIN_MODEL_AND_STATE_MACHINES.md` | Canonical Domain Model and State Machines | architecture | 6653 | `463d1e76cf1eaaec70a50bba0c2b948d50f89135cb4c166eb8e40621d024741f` |
| 14 | `docs/14_AGENT_RUNTIME_AND_ORCHESTRATION.md` | Agent Runtime and Orchestration | architecture | 7826 | `8e0bfe8f01e09d94c8498a0042528f7cdbf9d1f350043cff77627b66bfd769a8` |
| 15 | `docs/15_MODEL_ROUTER_AND_PROVIDER_GATEWAY.md` | Execution Policy Router and Provider Gateway | architecture | 7208 | `c2d264fb093ef1ac5b62685a082a5743015f6916fad49799ee529a6c91f277a3` |
| 16 | `docs/16_TOOL_CAPABILITY_AND_PROCEDURAL_RUNTIME.md` | Tool System, Capability Kernel, Procedural Runtime, and MCP | architecture | 5407 | `88f995ffc9687f2cc88f72e8f0a94c7aae602050db3dda997dc9d8bb9b052f3d` |
| 17 | `docs/17_CANONICAL_TOOL_AND_CAPABILITY_INVENTORY.md` | Canonical Tool and Capability Inventory | architecture | 5702 | `91f3b05092f8f90242a60422ea0f620e2bcc078f83e49cc3210155ba07020c5e` |
| 18 | `docs/18_CONTEXT_RETRIEVAL_AND_ENGINEERING_KNOWLEDGE.md` | Context, Retrieval, and Engineering Knowledge Engine | architecture | 5660 | `3755bbb880d958321ab80cbf48092cdfbd7ddc34cb30450c235ac8a60562564e` |
| 19 | `docs/19_DURABLE_STATE_MEMORY_COMPACTION_CHECKPOINTS.md` | Durable State, Memory, Compaction, and Checkpoints | architecture | 4917 | `a2cd03cb090f1f19c330669becc67fcf87fe6e7c85e2524c803ad6eb432bc056` |
| 20 | `docs/20_WORKSPACE_GIT_AND_TRUSTED_CODE_SURFACE.md` | Workspace, Git, Worktrees, Diagnostics, and Trusted Code Surface | architecture | 3963 | `a0a333e5258935caa4b106f845f801400862d286fa001c8ca8d861b05621ba07` |
| 21 | `docs/21_TERMINAL_EXECUTION_AND_SANDBOX.md` | Terminal, Execution Router, and Sandbox Architecture | architecture | 4273 | `993f563ecc713035e2ce4eb071c14fdd1cb3cd033ea0cc32a525ca542af2c64d` |
| 22 | `docs/22_BROWSER_AND_COMPUTER_USE.md` | Browser and Computer-Use Architecture | architecture | 3961 | `a7e441d9bc60785aea7dcc62fe8cef5946f3aa2551665368a360d4ec3519220b` |
| 23 | `docs/23_SECURITY_POLICY_EFFECT_LEDGER.md` | Security, Policy, Capabilities, Secrets, and Effect Ledger | architecture | 5169 | `f9f908c35490defc63d706e8748f8b3f1a0b707820f3a3b80da61268a8225500` |
| 24 | `docs/24_CLOUD_CONTROL_PLANE_AND_SYNC.md` | Cloud Control Plane, Remote Execution, and Sync | architecture | 5065 | `50516f6c15cc4353f2960c6a488cb706a706bdc6b2853b35dfb6c930df7c2eeb` |
| 25 | `docs/25_MULTIMODAL_MEDIA_AND_NOTEBOOK_RUNTIME.md` | Multimodal, Media and Notebook Runtime | architecture | 2960 | `e35d2b47f9ee33aa17e682f4226b26b8daa6ac8823a06722c3877fe416b02918` |
| 26 | `docs/26_SKILL_REGISTRY_AND_EVOLUTION.md` | Skill Registry and Evolution Integration — Skill Evolution Without a Second Runtime | architecture | 7450 | `3d7a2092543a186967759ebfcdcb4708afb73f96817d55989932b1908844b180` |
| 27 | `docs/27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md` | Execution policy router and verified orchestration | architecture | 57211 | `a95915dd93ad971360748c1fc05874aa4177793e6cfc6f7a8ba9baf3d2c6294b` |
| 28 | `docs/28_AGENT_COMPETENCE_PLANNING_VERIFICATION_AND_REPAIR.md` | Agent competence: planning, verification and repair | architecture | 7624 | `181448de6de2ed0d322c455428f881e1167286f2bbc81e88d9a603d21d0335cf` |
| 29 | `docs/29_CLIENT_SURFACES_AND_SOURCE_CONTROL_INTEGRATION.md` | Client surfaces and source-control integration | architecture | 7279 | `4e378b45620945efedd7b8db3546d15ad72c247a894cb80cd650fddd70b5dae8` |
| 30 | `docs/30_PROTOCOL_APIS_AND_EVENT_SCHEMAS.md` | Protocol, APIs, and Event Schemas | implementation | 9871 | `232e49314bfb3c654e425c15e0cd5e68208323aadff4ed48f9b29fcf18ac8213` |
| 31 | `docs/31_DATABASE_AND_STORAGE_SCHEMA.md` | Database and Storage Schema | implementation | 9637 | `4e6c0ecf0fb9a4a88d7d894401cfaa4b20ea611dfa8bf1f2bbb9bd11704f06f2` |
| 32 | `docs/32_DESKTOP_FRONTEND_IMPLEMENTATION.md` | Desktop Frontend Implementation | implementation | 5827 | `ac322efa1e408d2d7fb3655990d0a0828c269af317ac84d7fc459437123abaac` |
| 33 | `docs/33_CORE_AND_CLOUD_BACKEND_IMPLEMENTATION.md` | Core and Cloud Backend Implementation | implementation | 6465 | `dc5ca338def3fb0c1d9750352c3ae49ea85874334d4a4e8b0493ac7359b9693d` |
| 34 | `docs/34_OBSERVABILITY_COST_AND_OPERATIONS_DATA.md` | Observability, Cost, and Operations Data | implementation | 5702 | `4fbb53f587aca00d3b90626f3af21efc95f7241976877e45948ce6b2de5a9bf6` |
| 35 | `docs/35_DEPENDENCY_AND_BINDING_DECISIONS.md` | Dependency and Binding Decisions | implementation | 1492 | `96725161310f8c53975cc067e437c164e4d8def3c1efbfb40394573ec71ae4c6` |
| 36 | `docs/36_BUILD_BUY_DEPENDENCY_AND_LICENSE_POLICY.md` | Build / Buy / Dependency / License Decisions | implementation | 3363 | `538f768996bec4254231f7671517e68bea6cf529f51b60588a5b2dd91007071a` |
| 37 | `docs/37_EXISTING_CODE_DONOR_AND_REUSE_POLICY.md` | Existing-Code Donor and Reuse Policy | implementation | 3835 | `4dfad8325e6ed11e390952f46ff14caf6fa30fc72142f1ae58e32a22a620d4ee` |
| 38 | `docs/38_EXECUTION_POLICY_CONTRACTS_AND_ALGORITHMS.md` | Execution policy contracts and algorithms | implementation | 24554 | `4cf67ccec6005ec83478e6677a265d1fc79ce1241e4d8abce566784c01c1e4d3` |
| 39 | `docs/39_UX_FLOWS_ONBOARDING_AND_INTERACTION_BUDGETS.md` | UX flows, onboarding and interaction budgets | implementation | 6832 | `675100f531e76a11374f7b367f144214017ab12d7670daf9f21a30b131a6d562` |
| 40 | `docs/40_EVIDENCE_DERIVED_REQUIREMENT_LEDGER.md` | Evidence-Derived Requirement Ledger — Build Edition | requirements | 76383 | `d673606834f48960f015f4719c0b6fd956988469c39348aa859c4c0d91e20336` |
| 41 | `docs/41_EVIDENCE_DERIVED_IMPLEMENTATION_TASKS.md` | Evidence-Derived Implementation Tasks | requirements | 149169 | `90aadd632622877351db607f6c521b6b7d121ad55690ef0f313680b31dd26311` |
| 42 | `docs/42_EVIDENCE_DERIVED_QUALIFICATION_TEST_MATRIX.md` | Evidence-Derived Qualification Test Matrix | requirements | 49179 | `9bb8430c3b2cf0e06a7caf7caf1b40ae3ccd61dc07f36475029b557bb94ad292` |
| 43 | `docs/43_IMPLEMENTATION_ROADMAP_AND_TASK_GRAPH.md` | Implementation Roadmap and Verifiable Task Graph | requirements | 10513 | `56f3f0cbba1682f2920416ffca956f5c85755fc9c669b10cb22068cdafd2531e` |
| 44 | `docs/44_REQUIREMENTS_TRACEABILITY_MATRIX.md` | Requirements Traceability Matrix | requirements | 5473 | `36bb27a4cbbc50b24bed92e76d3145c286ed2a27943a8ba9509238c42c25fb31` |
| 45 | `docs/45_REQUIREMENT_TO_TASK_TO_TEST_TRACEABILITY.md` | Requirement → Task → Test Traceability | requirements | 1581 | `4c3822133dd045eeefc3da2da681d22bb02458ecec82b92478b9292a24d3a3fa` |
| 46 | `docs/46_REQUIREMENT_COVERAGE_FREEZE_GATE.md` | Requirement Coverage Freeze Gate | requirements | 1656 | `abce92a0c58412c47dcf9cfd75cea628883ad34aa876bfb36bce94291e968368` |
| 47 | `docs/47_REQUIREMENT_COVERAGE_AUDIT_REPORT.md` | Requirement Coverage Audit Report — Build Edition | requirements | 1999 | `6010cbab8bca22535ba7420326ff3dd2ff382cda8ddfa5194864ae2ba6e91455` |
| 48 | `docs/48_FEATURE_DEPTH_CONTRACTS.md` | Feature Depth Contracts | requirements | 5942 | `18b1996a420aeb09dd60da22fddfed18b77080635ebc88f7cbdb5c7dedffd7e6` |
| 49 | `docs/49_EXECUTION_POLICY_REQUIREMENTS_AND_TASKS.md` | Execution policy requirements and implementation tasks | requirements | 49196 | `e90bfba1d3f3594f1ff80bf629b2838be6d3423f4350f9c34c6fd8be4cf0b005` |
| 50 | `docs/50_TEST_STRATEGY_REAL_SYSTEM_GATES.md` | Test Strategy — Real-System Completion Gates | verification | 5380 | `15ce306959ad831318d55c9f0dd6325a19a65a3e8842256668cf89ec977e9967` |
| 51 | `docs/51_E2E_ACCEPTANCE_TEST_CATALOG.md` | End-to-End Acceptance Test Catalog | verification | 7941 | `94cb5e496f2fb7d39fa8bd18e9f94da048481c607ba4aabfcd750b29cb5b213c` |
| 52 | `docs/52_SECURITY_THREAT_MODEL_AND_TESTS.md` | Security Threat Model and Verification | verification | 5644 | `8d299543293dfdbf155e5609c4bf18022fe0f89399620191f0840852ab5684d1` |
| 53 | `docs/53_PERFORMANCE_AND_BENCHMARK_PLAN.md` | Performance, Context Economics, and Benchmark Plan | verification | 6843 | `cb132235360b0ac36e1f7e13dbaa672be9b6a4ab278b0d279e2c354df58a63aa` |
| 54 | `docs/54_FAULT_INJECTION_AND_RECOVERY_CATALOG.md` | Fault Injection and Recovery Catalog | verification | 2266 | `3bb42df3f654adb39d146e74bee48b8cb9043b567027a59ee92aceca3181010f` |
| 55 | `docs/55_MUTATION_NEGATIVE_AND_CHAOS_TEST_POLICY.md` | Mutation, Negative and Chaos Test Policy | verification | 1995 | `ab51c19d80eded9dba1d000b1af5d390bf3463a6dbf92aba886812db7994dc24` |
| 56 | `docs/56_TOOL_CAPABILITY_CONFORMANCE.md` | Tool Parity and Capability Conformance — Real Effect Tests | verification | 3070 | `69e4737b3086c3ad7517a992fabb7b158cb9e208c5d6d34f09bcbbc0154c1f7d` |
| 57 | `docs/57_SKILL_EVOLUTION_REAL_TESTS.md` | Skill Evolution Real-System Tests | verification | 3466 | `fa5335a1367e464dc7c51cffbc972c803b26781b9e17163c81df25b4c2853f21` |
| 58 | `docs/58_MULTIMODAL_MEDIA_REAL_TESTS.md` | Multimodal / Media Real-System Tests | verification | 3391 | `f04a81fdf805ecac83242ff96948ad1467acd11308ac767029935efc1e7801d9` |
| 59 | `docs/59_RELEASE_ZERO_PROOF_SCENARIO.md` | Release Zero — Single Proof Scenario | verification | 3669 | `15fbb005f7a8196b4987468034f8de113dcd15a1ebd3f34dba822075012064fc` |
| 60 | `docs/60_RELEASE_ZERO_EXPANDED_PROOF.md` | Release Zero Expanded Proof — Clean-Slate V2 | verification | 4001 | `0a3cc3ce5329c15cafb402833cb6bd70c9c6e4afdbdb838d2a811176c975939e` |
| 61 | `docs/61_EXECUTION_POLICY_QUALIFICATION_AND_ROLLOUT_GATES.md` | Execution policy qualification and rollout gates | verification | 42076 | `27acefadb50e86b616bbe64818f9f32ef04df15a3ba7bd4c9ab5344651a4d15b` |
| 62 | `docs/62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md` | Product extension requirements, tasks and qualifications | verification | 50365 | `311ee9ae68f0f35662c761234b9e6255b8f6b15eb0d12c58dc976e58ec570c6e` |
| 63 | `docs/63_AGENT_COMPETENCE_BENCHMARKS_AND_REGRESSION_SUITES.md` | Agent competence benchmarks and regression suites | verification | 3942 | `79a7fbb6439cdf4a0cc75bd71d5016c85947af607bf3ef16604c0bcc68dfec9f` |
| 70 | `docs/70_CI_CD_RELEASE_AND_SUPPLY_CHAIN.md` | CI/CD, Release Engineering, and Supply Chain | delivery | 3502 | `a2cc865ecdd804938d638d5ffc3ba66581249787c1f452dfc69b45d27757a706` |
| 71 | `docs/71_OPERATIONS_RUNBOOK.md` | Operations and Incident Runbook | delivery | 4954 | `f633d3c4c1af5e81799c6497d603b448b239ae00b8169c79828d641d1f1b51be` |
| 72 | `docs/72_RISK_REGISTER_AND_OPEN_DECISIONS.md` | Risk Register and Open Technical Decisions | delivery | 8446 | `d8f50beffbb989e82f2f4ed68504bdbef460d5a37032171d97b509173279b1f9` |
| 73 | `docs/73_RELEASE_BLOCKERS_AND_STOP_THE_LINE_RULES.md` | Release Blockers and Stop-the-Line Rules | delivery | 1825 | `00103375774eed06e6afa60843bec3a02482d202ef1c37ad0d5f46abe79dddc6` |
| 74 | `docs/74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md` | Package Integrity and Build Coverage | delivery | 3938 | `7753011c640a56cdeed7f16f19dc1861fef81a51be8ac16c087f30d0326e4d42` |
| 75 | `docs/75_PHASED_RELEASE_PLAN_AND_READINESS.md` | Phased release plan and readiness | delivery | 4238 | `059da8ffb8cf6c9b019408a37666366eb21cd821fc9702fa9f536d6fb4a3bcd0` |
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
| 97 | `docs/97_DOSSIER_MAINTENANCE_LOG.md` | Dossier maintenance log | governance | 32985 | `d8e84a6b9eb0a87558941c473aa70e5281477bb851120a3bc9ed57e498ecf33f` |
| 98 | `docs/98_BUILD_MANIFEST.md` | Build Manifest | live-state | 3523 | `c318aa5ebd9253cb4ee2207a2a4dcae4d2b57f0d7d8f3ea7dfc30ef7656e5be5` |

## Root governing files and tooling

| File | Role | Bytes | SHA-256 |
|---|---|---:|---|
| `AGENTS.md` | build-agent operating contract (highest authority) | 9512 | `52facabdca45abd2fcdaf10a7b0141057895217af95560395dc0d24a8793b64f` |
| `MODBIT-PATCH-EPR-v1.1-Supersession-and-Refinement.md` | source patch provenance | 20229 | `30dbacdb3a37b364f535f55ed7bf4ea8fa35d70cd6f85c9a61897ac9db72f2bd` |
| `MODBIT-PATCH-Execution-Policy-Router-and-Verified-Multi-Model-Orchestration.md` | source patch provenance | 54208 | `9e0cee7d49d442033b875e250a61a212b898dd6f4bf35826cdad5ffeb71edb66` |
| `README.md` | human orientation | 8707 | `cab8e955cfad61c103130dce19918858af9a36ea20f2742c4fbf94ee7481ee87` |
| `SKILLS.md` | governed procedures for agents | 17860 | `e964270eeb61d64f43cc95cceafc798f1300ccbb72d02cf54db393fb601bec5a` |
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
| `graph/PROJECT_GRAPH.md` | human view of the graph | 39237 | `c00357d27459fef17a28d7f0ce3eb5c202bd488b3437e3f1bb8f2e1ec7058023` |
| `graph/project-graph.json` | project driver graph with live status | 920121 | `4dd593531bbd751d791024b93a7270ff10a51f52f93cfd435dae997a5f661d18` |
| `tools/build_graph.py` | regenerates graph structure from docs | 43614 | `eed1405877e2512bcb13b1440fdcaf17f0f7c040a906fce4ebbfdfca9514f5f8` |
| `tools/build_manifest.py` | regenerates this manifest | 14792 | `8b162fc559327ea57f133ea269b165e37e2f3538e85166ca2d58cd6b02dd781b` |
| `tools/check_dossier.py` | integrity gate | 16615 | `17490633d9f5e4c25322c197cbbe765b6e3573cec639c152cd7744c792995e9c` |
| `tools/dossier_epr.py` | parses additive EPR authority and traceability | 9310 | `4b5e399ee1b4886662294e3780d7f0630195e29c8198256ff7719d5236591a18` |
| `tools/dossier_px.py` | parses the additive product-extension ledger and phased release rules | 6931 | `0a7dd5bd5872fb1d959c56fd017bd3916062d0f60d1bc51b9abceddc134202e2` |
| `tools/graph.py` | query/update graph | 36313 | `76f2a79fca3c305205f34573b94118c99511944833e8e602bf0eba7e2ac305cb` |
| `tools/test_dossier.py` | copied-package integration and negative tests | 26807 | `e63c752fa1fc6b5c41fc78b247091b5781e07b1892a3834b34638943fac1105c` |

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
