# Task Card — M0.1 Create monorepo, Rust workspace, pnpm workspace, CI, architecture-lint

## Identity

- Task ID: M0.1
- Milestone: M0 — Repository and authority
- Canonical owner: Core/runtime team (repository root, Rust workspace, architecture lint); Surface team (pnpm workspace packages)
- Requirement IDs: roadmap task `docs/43` M0.1; prerequisite of PX-030 (REQ-PX-030, platform CI matrix from M0) and of M0.2
- Risk class: medium (build and CI substrate; no runtime behavior)
- Evidence tier: release-critical (touches execution substrate for every later task; uncertain → release-critical)
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

A fresh clone builds and tests on macOS, Linux and Windows CI; forbidden dependency edges between workspace crates are rejected by a real `cargo metadata` based lint with a negative test proving the rejection.

## Non-goals

- Any product behavior (no runtime, protocol, store, UI). Binaries refuse to run rather than simulate success.
- ADR metadata enforcement (M0.2) and protobuf generation (M0.3).
- Release-grade platform claims: CI results are `CI_COMPATIBLE` only (docs/76, PX-030).

## Required reading

`AGENTS.md`, `docs/02`, `docs/11`, `docs/12` (layout, dependency direction), `docs/35` (bindings), `docs/70` (CI/CD, reproducibility), `docs/76` (platform matrix), `docs/81` (forbidden directions), `docs/82`, `docs/83`, `docs/93`.

## Existing-code audit

- classification: NOT-FOUND (repository was docs-only: dossier, graph, Python tooling, evidence)
- production entry point: none
- current owner: none
- real effector/storage boundary: none
- tests found: dossier self-tests only (`tools/test_dossier.py`)
- first missing/broken link: no `Cargo.toml`, `pnpm-workspace.yaml`, CI workflow or architecture lint
- duplicate/drift risks: none (clean slate); later tasks must extend these crates in place, never create parallel workspaces

## Invariants

- Dependency direction of docs/12 and docs/81 encoded as data in `tools/architecture-lint/rules.toml`; violations fail CI.
- `modbit-domain` depends on no workspace crate; sandbox/browser never import `modbit-core-runtime`; providers never reach workspace/git/tools/effects/terminal; protocol-state/checkpoint never depend on memory; CLI stays a thin client.
- Toolchains pinned (`rust-toolchain.toml` 1.97.1, `.node-version` 26.5.1, `packageManager` pnpm 11.8.0); lockfiles committed and enforced with `--locked` / `--frozen-lockfile`.
- CI actions pinned by commit SHA.

## Implementation slice

- domain/API: none
- persistence/migrations: none
- policy/capabilities: none
- service/effector: `tools/architecture-lint` (Rust, runs real `cargo metadata`), CI workflow `.github/workflows/ci.yml`
- UI/model projection: none
- failure/recovery: lint exits 1 on violation, 2 on operational error; binaries exit non-zero with an explicit "no runtime wired" message
- evidence/observability: CI step summary records `CI_COMPATIBLE only`; evidence bundle in this directory

## Verification

- unit/property: `architecture-lint` rule engine tests (transitive path, glob targets, confine, invalid glob)
- integration: `tools/architecture-lint/tests/real_workspace.rs` — real `cargo metadata` on the Modbit workspace (pass) and on a disposable workspace with an injected forbidden edge (fail), plus CLI exit codes
- qualification test IDs: none linked directly (QUAL-PX-030 is attached to PX-030, which depends on this task)
- E2E: fresh-clone CI on macOS, Linux, Windows (`rust`, `node`, `dossier` jobs)
- security/fault: injected forbidden edge `modbit-browser -> modbit-core-runtime` in the real workspace rejected by the lint (see `inject-failure.log`)
- performance budget: n/a

## Completion evidence

- commit/revision: 425d80d4b80534f2066a6c9d1a68d725778dcd97 (scaffold), evidence commit follows
- test run IDs: `fresh-clone-macos-425d80d`, `fresh-clone-linux-rust-425d80d`, `fresh-clone-linux-node-425d80d`, `fresh-clone-linux-dossier-425d80d`, `inject-failure-browser-core-runtime` (all in `evidence.json`)
- artifacts/effect/event refs: `evidence/m0/M0.1/evidence.json`, logs in this directory
- environment/build digest: see `evidence.json` `environment`
- hosted CI: run 34253119985 did not start (GitHub billing); Windows unproven

## Remaining acceptance criteria

- green `ci` workflow on macOS, Linux and Windows runners (blocked on GitHub Actions billing; see `evidence.json` `blocker`)

## Status

Tracked on graph node M0.1 (`python3 tools/graph.py show M0.1`).
