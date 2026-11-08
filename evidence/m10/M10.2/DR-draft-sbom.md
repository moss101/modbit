---
id: DR-M10-2026-11-07-001
title: SBOM format, coverage, generation, license policy and reproducibility checks for the desktop and its bundled services
status: proposed
date: 2026-11-07
supersedes: none (formalises decision 2 of the evidence-only record DR-M10-2-2026-10-02 and its recorded deviation)
approved_by: not approved; awaiting the owner
intended_path: docs/decisions/DR-M10-2026-11-07-001-sbom.md
---

# DR-M10-2026-11-07-001 — SBOM for Modbit's desktop and bundled services (DRAFT, proposed)

> **Draft for the owner's decision. Status `proposed`.** This file is in
> `evidence/m10/M10.2/` because `docs/decisions/DR-*.md` is a locked path
> (`tools/architecture-lint/rules.toml`): a commit that touches it must carry a
> `Decision-Record:` trailer naming a record with `status: accepted` in that
> commit. Earlier proposed records (DR-PX-2026-10-03-009, -011, -012) entered
> `docs/decisions/` only inside a commit whose trailer named a record the owner
> had just accepted (DR-PX-2026-10-05-013, commit 912a633f). No accepted record
> covers this one, and naming an unrelated accepted record would pass the lint
> without authorising the change. When the owner accepts it, the record moves to
> `intended_path` with `status: accepted` and `approved_by` filled in, in a
> commit whose trailer names it, and a register row is added to doc 02 and
> `docs/decisions/README.md`.
>
> Numbering: no `DR-M10-*` record exists under `docs/decisions/`, so this is the
> first of the milestone's dated series (`001`). `DR-M10-2-2026-10-02` lives in
> evidence only and has no sequence number.

## 1. Trigger and evidence

- docs/77 section 5 item 4 names "the SBOM/provenance tooling decision (doc 70)"
  as an owner input, to be recorded "as a Decision Record under
  `36_BUILD_BUY_DEPENDENCY_AND_LICENSE_POLICY.md`". docs/70 "Build
  reproducibility" requires an "SBOM for desktop, services and guest image" and
  "artifact checksums and signatures". docs/36 "License gate" requires a CI
  license scanner and shipped notices "generated from lockfiles/SBOM".
- Owner decision of 2026-11-07: produce an UNSIGNED packaged desktop build for
  the packaged-E2E and render-budget legs, and draft this record for approval.
- What exists and runs today (branch `wip/m10-2-unsigned`, evidence in this
  directory): per-source CycloneDX 1.5 SBOMs for `modbit-core` (416
  components), `modbit-execd` (120) and the desktop (11, including Electron);
  a per-package SBOM that names the bytes of one built package by SHA-256 and
  carries its UNSIGNED label; a license gate that passes on all of them; a
  reproducibility check (`tools/release/repro.mjs`).

## 2. Current behaviour

- `node tools/release/release.mjs sbom --out-dir D` runs `cargo cyclonedx`
  (0.5.9) over the Rust workspace and keeps the SBOMs of the shipped binaries
  (`modbit-core`, `modbit-execd`), and `pnpm sbom --sbom-format cyclonedx
  --prod` for `@modbit/desktop`, adding an explicit `electron` framework
  component for the runtime the packager copies in.
- `release.mjs package-sbom` (added 2026-11-07) describes one built package:
  the bundled `modbit-core` and `modbit-execd`, `app.asar`, the install helper,
  the pinned update files, `build-info.json` and the Electron executable, each
  by SHA-256 and size; the distributables (dmg, zip, NSIS, AppImage, tar.gz) by
  SHA-256; links to the per-source SBOMs by digest; and the package's own label
  (`modbit:label=UNSIGNED`, `modbit:signed=false`, `modbit:notarized=false`).
  It refuses a package without `build-info.json` or missing a file it must ship.
- `release.mjs licenses` applies `tools/release/license-policy.json`.
- `release.mjs verify --sbom-dir` checks each SBOM against the digest the
  signed update manifest carries.
- The guest image has no SBOM yet (see section 5).

## 3. Proposed decision

### 3.1 Format: CycloneDX 1.5 JSON

| Considered | Why / why not |
|---|---|
| **CycloneDX 1.5 JSON (chosen)** | Both generators already in use emit it natively (`cargo-cyclonedx`, `pnpm sbom`), so no conversion step can lose data. It carries hashes, `externalReferences` of type `bom` (linking package SBOM to source SBOMs by digest), free-form `properties` (where the UNSIGNED label lives) and component types for `application`, `framework` and `file`. Supported by the common vulnerability scanners. |
| SPDX 2.3 JSON | Equally accepted for licence compliance and the better fit for a legal notices file. Neither generator in use emits it, so it would need a converter, and its file-level relationships are heavier for the per-package view. Can be produced later from CycloneDX for a consumer that requires SPDX, as a derived artifact, never as the source of truth. |
| Both, side by side | Doubles what has to be signed and verified for no present consumer. Rejected until a consumer asks. |

The spec version is pinned (1.5) in the tooling; moving to a newer version is a
change to this record.

### 3.2 What the SBOMs cover

| Scope | Document | Generator | Status |
|---|---|---|---|
| Rust crates of `modbit-core` | `modbit-core.cdx.json` | `cargo-cyclonedx` 0.5.9 (`--describe binaries`) | runs |
| Rust crates of `modbit-execd` | `modbit-execd.cdx.json` | `cargo-cyclonedx` 0.5.9 | runs |
| npm packages shipped in the desktop (`--prod`) | `desktop.cdx.json` | `pnpm sbom` (pnpm 11.8.0, built in) | runs |
| Electron runtime (Chromium and Node.js inside) | `electron` component in `desktop.cdx.json`; the executable by digest in the package SBOM | `release.mjs` | runs; Chromium and Node.js are covered as part of the pinned Electron release, not itemised |
| Bundled binaries and files of one built package, and its distributables | `modbit-desktop-<version>-<os>-<arch>[-UNSIGNED].package.cdx.json` | `release.mjs package-sbom` | runs on every OS in CI |
| Other workspace binaries (`modbit-cli`, `modbit-cloud-*`, `modbit-sandbox-gateway`) | one per binary, same generator | `cargo-cyclonedx` | not shipped in the desktop package today; added when a release ships them |
| Native-control helper (`modbit-actuator-macos`, PX-072) | its own source SBOM plus a package-SBOM entry | `cargo-cyclonedx`, `package-sbom` | not bundled: it ships only inside a signed bundle (PX-072), which needs the owner's identity |
| Sandbox guest image | image SBOM | `syft` (already approved in DR-M10-2-2026-10-02) | not wired; needs the image build in CI |

### 3.3 How it is generated and attached

1. CI job `release-sbom` (Linux) generates the per-source SBOMs, runs the
   license gate, the release-tooling tests and `repro.mjs`, and uploads them
   as the `supply-chain-evidence` artifact.
2. CI job `desktop-packaged` (macOS, Linux, Windows) builds the package,
   runs the packaged suite against it, and `tools/release/unsigned-bundle.mjs`
   writes the package SBOM, the distributables and `SHA256SUMS-UNSIGNED` into
   one upload per OS (`modbit-desktop-<os-arch>-UNSIGNED`).
3. For a release (owner-only, section 6): the per-source and package SBOMs are
   listed with their SHA-256 in the signed update manifest (`release.mjs
   manifest`; the manifest signature already covers the SBOM digests), attached
   to the release next to the artifacts and `SHA256SUMS`, and checked with
   `release.mjs verify --sbom-dir` before publication. An SBOM is never edited
   after its digest is signed.

### 3.4 License policy

- Allowed by default (`tools/release/license-policy.json`): 0BSD, Apache-2.0,
  BSD-2-Clause, BSD-3-Clause, BSL-1.0, CDLA-Permissive-2.0, ISC,
  LLVM-exception, MIT, MIT-0, Unicode-3.0, Unlicense, Zlib.
- Copyleft, network-copyleft and unknown or undeclared licences fail the gate
  until a review is recorded in the policy's `reviewed` list with the reviewer
  and date (docs/36). An `OR` expression passes when one alternative is
  allowed; `AND` and `WITH` need every term allowed.
- First-party packages are recognised from the workspace (`cargo metadata`,
  `apps/*`, `packages/*`), never from the policy file, so a third-party
  `UNLICENSED` crate still fails.
- The Rust side is checked twice: by this gate over the SBOMs and by
  `cargo deny check` (PX-167, `deny.toml`). A disagreement between the two is a
  failure to investigate, not a reason to drop one.
- Shipped notices (docs/36) are to be generated from these SBOMs; not built yet.

### 3.5 Reproducibility checks

- Inputs pinned: `rust-toolchain.toml`, `.node-version`, `packageManager`
  (pnpm 11.8.0), `Cargo.lock`, `pnpm-lock.yaml`, exact Electron and
  electron-builder versions, `SOURCE_DATE_EPOCH` from the commit.
- `tools/release/repro.mjs` builds the shipped Rust binaries and the desktop
  bundles twice from clean target directories at one fixed, remapped path and
  compares SHA-256; it fails on any difference and writes `repro.json`.
- Proven so far: identical desktop bundles, and identical Rust binaries at a
  fixed remapped path on macOS arm64, in PR #60 and again on this branch at
  92dd23a4 (12 of 12 artifacts identical, `unsigned/repro.json`). The package
  today carries the ordinary, not path-remapped `target/release` build, so the
  shipped binaries are not yet the reproducible ones; the release pipeline
  should package the remapped build. Not proven: identical packaged distributables (zip, dmg, NSIS,
  AppImage), reproducibility across checkout paths and across machines. These
  remain open and are not claimed.
- Every distributable carries a SHA-256 in `SHA256SUMS[-UNSIGNED]` and in the
  package SBOM.

## 4. Migration and compatibility

Additive. No change to SurfaceProtocol, the Core schema, the update manifest
format or any store. The UNSIGNED artifact naming (`artifactName` in
`apps/desktop/electron-builder.yml`) is overridden by the owner's signing
pipeline once it signs.

## 5. Security impact

Positive: what ships is described by digest, licences are gated, and the SBOM
digests are covered by the update manifest signature once a signing key exists.
Remaining gaps: guest-image SBOM (syft), shipped notices, SAST and secret
scanning (each a new dependency needing its own record), and signing of the
SBOMs themselves outside the manifest.

## 6. What stays owner-only

- The code-signing identity for Windows, and the Apple Developer ID and
  notarization for macOS. Until supplied every package is UNSIGNED by name
  (`*-UNSIGNED.*`), by `build-info.json` (`signed: false`, `notarized: false`)
  and by its package SBOM; macOS bundles carry only the linker's ad-hoc
  signature (`TeamIdentifier=not set`), which names nobody.
- The update-manifest signing key (`release.mjs keygen`, stored as
  `MODBIT_UPDATE_SIGNING_KEY`). Until a public key is pinned in a package, that
  package verifies nothing and refuses every update.
- The release feed and its hosting.
- Acceptance of this record.

## 7. Test impact

`release-sbom` and `desktop-packaged` CI jobs (both in `ci-compatible`'s
needs); `sbom-cli.test.ts` covers the licence gate, the SBOM shape check, the
manifest-digest check and `package-sbom`'s refusals and UNSIGNED label.

## 8. Rollback

Remove the SBOM steps and `package-sbom`; no stored state depends on them.

## 9. Explicit user approval

Not given. Requested from the owner with this draft (PR description and this
file).
