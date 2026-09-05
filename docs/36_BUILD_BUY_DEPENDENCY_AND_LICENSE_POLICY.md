# Build / Buy / Dependency / License Decisions

> **Authority date:** 2026-09-03  
> **Product:** Modbit — clean-slate implementation dossier  
> **Completion rule:** code is not “done” until it is wired through the real runtime and passes the release-gate real-system test with evidence.  
> **No-placeholder rule:** production code paths may not contain fake implementations, TODO return values, hard-coded success, disabled security checks, or UI-only simulations of unavailable behavior.


## Decision framework

Evaluate in order: dependency → integration → fork → reimplement → build → reject. Strategic Core state/policy/context/effect semantics remain Modbit-owned even when libraries provide mechanics.

## Selected dependencies / mechanisms

| Capability | Decision | Reason / boundary |
|---|---|---|
| Desktop Chromium shell | **DEPEND: Electron** (PROVISIONAL) | Best fit for visible Chromium session and TS surface; Core independent via SurfaceProtocol |
| UI | **DEPEND: React/TypeScript** | Surface only |
| Core/services | **BUILD on Rust ecosystem** | Own domain/runtime; memory safety/perf for PTY/index/network |
| Local DB | **DEPEND: SQLite** | Durable local transactional state |
| Cloud DB | **DEPEND: Postgres** | Durable multi-tenant events/projections/leases |
| Object storage | **INTEGRATE: S3-compatible** | Immutable blobs/checkpoints/outputs; provider abstracted |
| Lexical search | **DEPEND: Tantivy** | Embedded BM25 |
| Semantic ANN | **DEPEND: USearch** (PROVISIONAL) | Embedded HNSW; benchmark/licensing validation required before lock |
| Syntax | **DEPEND: tree-sitter** | Deterministic parse/symbol anchors |
| Language semantics | **INTEGRATE: LSP servers** | Headless diagnostics/refs; not IDE dependence |
| Git | **DEPEND: libgit2/git CLI where semantics require** | Prefer library for typed operations; CLI allowed for exact Git behavior with argv and evidence |
| Procedural isolate | **DEPEND: QuickJS via maintained Rust binding** | No ambient authority; tiny embeddable runtime |
| Cloud sandbox | **INTEGRATE: MicroVM-class sandbox substrate** | Modbit Gateway owns tenancy/policy/secrets; do not fork unless upstream gap forces it |
| Browser automation | **BUILD semantic layer on Chromium/CDP** | Do not build browser engine; can borrow/reference AX/semantic patterns |
| Model providers | **INTEGRATE APIs** | Modbit owns normalized gateway/router, not model service |
| MCP | **INTEGRATE protocol** | External tools remain capability-gated/untrusted |

## Fork policy

Fork only if upstream is strategically necessary, cannot accept required security/performance changes, and maintenance cost is justified. A fork requires upstream sync plan, security owner and quarterly divergence review. Code-OSS fork is specifically not part of Modbit.

## License gate

Allowed by default: permissive licenses compatible with commercial distribution after notice obligations. Copyleft/network-copyleft/native binary licenses require legal review before merge. License scanner runs in CI and shipped notices are generated from lockfiles/SBOM.

## Dependency health

Quarterly score: release activity, bus factor, security advisories, open critical bugs, platform support, binary provenance and replacement cost. Strategic dependency degradation triggers an exit ADR, not silent accumulation of patches.
