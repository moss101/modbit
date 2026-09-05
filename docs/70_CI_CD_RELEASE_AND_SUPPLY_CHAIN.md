# CI/CD, Release Engineering, and Supply Chain

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Completion rule:** code is not “done” until it is wired through the real runtime and passes the release-gate real-system test with evidence.  
> **No-placeholder rule:** production code paths may not contain fake implementations, TODO return values, hard-coded success, disabled security checks, or UI-only simulations of unavailable behavior.


## Branch policy

Protected `main`; short-lived PR branches; required signed CI status. Architecture-impact PRs include ADR link. Generated protocol code must match schema source.

## PR pipeline

1. formatting/lint;
2. Rust/TypeScript compile with warnings policy;
3. unit/property tests;
4. architecture dependency lint;
5. SQLite migration tests;
6. real local component integration (Git/SQLite/PTy/index);
7. protocol compatibility tests;
8. SAST/dependency/license/secret scans;
9. packaged desktop smoke against real local Core.

No live paid model call is required on every PR, but adapter changes trigger provider conformance in protected environment before merge where feasible.

## Nightly pipeline

- live OpenAI/Anthropic provider conformance;
- local agent E2E trials;
- real Chromium browser suite;
- restart/kill recovery matrix;
- retrieval/context benchmarks;
- cloud staging E2E with real MicroVM;
- security attack suite;
- performance trend collection.

## Release-candidate pipeline

Runs every required E2E scenario in `51_E2E_ACCEPTANCE_TEST_CATALOG.md` on signed candidate binaries and records evidence bundle. No skipped mandatory scenario allowed. Flaky tests are failures until root-caused; they are not silently retried to green without recording all attempts.

## Build reproducibility

- exact dependency lockfiles;
- Rust toolchain pinned in `rust-toolchain.toml`;
- Node/pnpm version pinned;
- Electron builder inputs pinned;
- sandbox guest image built from declarative Docker/VM image recipe and pinned by digest;
- SBOM for desktop, services and guest image;
- artifact checksums and signatures.

## Desktop update

Signed update manifests; staged rollout; rollback to previous compatible version. Core DB migration compatibility is checked before update install. Critical schema migration takes backup/checkpoint first.

## Cloud deploy

Blue/green or canary service rollout. DB migrations are expand→migrate→contract, never destructive in same release as code dependency. Cloud workers support current and previous protocol major/minor during rollout window.

## Supply-chain policy

New dependency requires owner, license, security/maintenance check, strategic justification and exit plan. GitHub/source URL is recorded in dependency inventory. Native binary dependencies require checksum/signature and reproducible provenance.

## Execution policy change pipeline

Dossier CI regenerates graph/manifest and runs `python3 tools/check_dossier.py --manifest` plus `python3 tools/test_dossier.py`. Product PR tests cover schemas/compiler hard exclusions/budget/fencing; nightly/RC runs doc 61's live provider, real Git/database, failure and holdout suites. Registry/profiler/Skill/gate/reward/policy bundles are immutable versioned artifacts with provenance and compatibility metadata. Privileged promotion requires named quality and controlled rollout evidence, tested rollback and no security regression. Client releases cannot silently publish routing policy.
