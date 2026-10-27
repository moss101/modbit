# Task Card — PX-130 Credential broker unification: one broker interface and handle path for tools, providers, MCP, browser and cloud

## Identity

- Task ID: PX-130 (REQ-PX-130; owner effects-security); qualification QUAL-PX-130 / PX-E2E-130; Decision Record DR-PX-2026-10-05-013
- Evidence tier: release-critical (the secret-handling boundary)
- Prior state: IMPLEMENTING since 2026-10-17 ("only the provider consumer is proven ... no lint that fails a direct credential read"). That note is stale.

## Existing-code audit (2026-10-27, main c48afabf)

- One interface: `crates/secrets/src/broker.rs` (`CredentialBroker`, `SecretHandle`, scoped grants with principal, audience, purpose, expiry and call cap; rotation, revocation, counted and audited use; `process_broker` / `env_secret` for process-start credentials); the one redactor in `crates/secrets/src/redact.rs`.
- Consumers on it: providers (`crates/providers/src/gateway.rs` acquires per request), MCP (`services/modbit-core/src/mcp.rs`), browser fill (`services/modbit-core/src/browser.rs`, the host holds the value, the broker counts and authorises), forge (`crates/tools/src/forge.rs`, `services/modbit-core/src/forge.rs`), cloud (`apps/cloud-api`, `apps/cloud-worker`, `apps/sandbox-gateway`, #120).
- Lint: `architecture-lint credentials` (`tools/architecture-lint/src/credentials.rs`, rule in `rules.toml`, #119) fails a credential read outside the broker; two permanent CLI forwarding reads are listed with reasons.
- Classification: PRODUCTION-WORKING for the local consumers; first missing link for COMPLETE is external (below). No product code changed in this step.

## Proof (local, macOS arm64, 2026-10-27)

- `services/modbit-core/tests/px_130_broker.rs`: provider key counted, revoked on the next request and rotated; a planted key is in no request, event, broker view or file the Core wrote, and no child process sees it; an MCP server's credential comes from the broker, is counted and is refused once revoked. 3 of 3 pass.
- `services/modbit-core/tests/surface_protocol.rs` (IMP-EV-0088 browser fill): the broker's counter equals the fills the host performed and its view carries no value.
- `crates/secrets` unit tests: expiry and TTL clamp, revocation, nonce and call cap, rotation, counter, no value printed or serialised.
- `tools/architecture-lint/tests/credentials.rs`: a fixture read outside the broker is reported; this repository passes.
- `apps/cloud-worker/tests/cloud_worker.rs`: the forge token reaches the forge through the broker and never the guest.
- Mutation check (QUAL: "a mutation that logs a resolved value must fail the artifact search"): `mutation-check.txt` — the resolved key copied into the request body, and the resolved key written to a log file in the Core's data dir; both KILLED.

## Not proven (why the row stops at REAL_TESTING)

- QUAL-PX-130 names "a real provider call on the compatible gateway" and "the staging cloud": the provider consumer is proven against a local server recording real HTTP requests, and the cloud consumers in the CI-hosted cloud job, not a live gateway or a deployed staging cloud. Both need the owner's credentials.
- The `crates/effects` stub documents where receipts live (FIX-08) rather than being retired; the task text asks for one or the other, and that choice is left to the owner.
