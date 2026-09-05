# Existing-Code Donor and Reuse Policy

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Completion rule:** code is not “done” until it is wired through the real runtime and passes the release-gate real-system test with evidence.  
> **No-placeholder rule:** production code paths may not contain fake implementations, TODO return values, hard-coded success, disabled security checks, or UI-only simulations of unavailable behavior.


## Principle

The new Modbit repository is authoritative. The old Code-OSS-era repository is frozen/read-only donor material. Do not “migrate the architecture”; extract only independently valuable Modbit-owned code after audit.

## Classification

For every donor module classify:
- **EXTRACT** — already independent and matches new contract/security expectations;
- **REFACTOR** — logic valuable but coupled to Code-OSS/old domain;
- **WRAP TEMPORARILY** — bounded transitional adapter with deletion issue/date;
- **REFERENCE ONLY** — reimplement from behavior/tests, do not copy architecture;
- **DROP** — obsolete, duplicate, IDE-specific or insecure.

## Likely retain/rebuild candidates

Mechanisms worth mining: provider adapter logic, Rust context/search code, Git/worktree helpers, terminal PTY code, secure IPC lessons, browser structural control, verification logic, evidence schemas, diagnostics/LSP helpers, sandbox/client code, tests that assert real behavior.

## Drop by default

- Code-OSS workbench shell and patch sets;
- VS Code Extension Host dependencies;
- Explorer/editor/SCM/debugger surface code;
- Monaco/editor-buffer state ownership;
- VS Code settings/accounts/telemetry integration;
- duplicate agent panels/harness graphs;
- Modbit Lite packaging/product forks;
- mocks wired as production providers;
- old v5 authority manifests that conflict with 2026-09-03 decisions.

## Extraction gate

Donor code enters new repo only if:
1. license/provenance is clear;
2. no proprietary external reference code/service dependency;
3. dependency direction fits new module layout;
4. domain IDs/events/state map to new canonical contracts;
5. security review passes;
6. a real integration test demonstrates behavior;
7. copied LOC does not drag in Code-OSS/old shell dependencies.

## Migration evidence

Each extraction PR contains `DONOR.md` entry with old path/commit, new module, classification, changes made, tests and reason. This preserves provenance without letting old architecture regain authority.


## AI-agent reuse warning

An agent must not copy an old module merely because it has the desired name. Before reuse, map every public entry point, dependency and side effect against the new owner contract. Extract only code that can be tested independently and then wire it through the new production path. Do not bring old service locators, UI ownership, policy bypasses or architecture generations along with useful algorithms.

## Routing donor migration

Classify any donor Agent Runtime as a real policy-routed model/tool executor with frontier quality backstop only after tracing Core → compiler → Gateway → governed tools → revision-bound verification/evidence. A direct model-call wrapper or TaskFingerprint switch is incomplete donor code, not a second implementation to preserve. Map TaskFingerprint intrinsic features into RequestProfile, external policy/economics into compiler inputs, and RouterDecisionRecord into canonical RoutingDecisionRecord/Trace without fabricating legacy provenance. No mandatory local model is added; any economical role is provider-neutral and evaluation-qualified. See DR-EPR-2026-09-05-v1.1, EPR-000..005 and EPR-014..019; map legacy topology only through validated conditional-slot migration and preserve missing empirical confidence as unknown.
