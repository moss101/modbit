# Security Threat Model and Verification

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Completion rule:** code is not “done” until it is wired through the real runtime and passes the release-gate real-system test with evidence.  
> **No-placeholder rule:** production code paths may not contain fake implementations, TODO return values, hard-coded success, disabled security checks, or UI-only simulations of unavailable behavior.


## Assets

Source code, Git history, provider credentials, user credentials, cloud tenant data, browser sessions/cookies, secrets, model context, terminal processes, artifacts, effect approvals and audit/evidence integrity.

## Adversaries

- malicious repository/package/install script;
- prompt-injected web page/document;
- malicious MCP/external tool;
- compromised model response;
- compromised sandbox guest;
- local unprivileged process trying to attach to Core/IPC;
- cross-tenant cloud attacker;
- dependency/supply-chain compromise.

## Major threats and controls

### IPC privilege escalation
**Control:** OS-local socket/pipe permissions, boot secret, process identity checks where available, renderer cannot connect directly.  
**Test:** independent local process attempts replay/connection/command injection; must fail.

### Path traversal/symlink escape
**Control:** canonical path resolution + root/protected path policy on every operation.  
**Test:** `../`, Unicode variants, junction/symlink race, rename-after-check. Use open-at/handle-based safe operations where platform supports.

### Shell injection
**Control:** argv-first execution; shell mode explicit; model arguments validated.  
**Test:** hostile filenames/arguments containing shell metacharacters do not execute unintended commands in argv mode.

### Prompt injection/data exfiltration
**Control:** untrusted provenance lanes, capability kernel, secret handles, domain egress policy.  
**Test:** repository/browser/MCP content requests secret upload; no secret is released and tool capability remains unchanged.

### Secret leakage
**Control:** raw secrets excluded from renderer/model/sandbox image, scoped broker use, output redaction.  
**Test:** dump guest env/proc, terminal logs, crash report and UI state; raw secret absent.

### TOCTOU capability bypass
**Control:** policy checks at actual open/dispatch with revision/generation.  
**Test:** mutate symlink/path/browser target between proposal and execution; effect blocked or re-approved if intent changes.

### Duplicate external side effects
**Control:** stable ToolCallId, intent hash, receipt chain, unknown-outcome reconciliation.  
**Test:** kill network/Core at each dispatch/ack boundary and assert at-most-once or explicit reconciliation.

### Sandbox escape/internal SSRF
**Control:** MicroVM boundary, deny internal network, gateway egress allowlist, no cloud metadata endpoint.  
**Test:** guest attempts RFC1918/link-local/metadata/control-plane endpoints and host mounts.

### Cross-tenant access
**Control:** tenant-bound auth, DB object ownership checks, scoped URLs, sandbox lease binding.  
**Test:** full IDOR matrix across sessions/events/artifacts/outputs/approvals/sandboxes.

### Malicious skill/plugin/MCP
**Control:** signing/provenance, capability ceilings, schema/size limits, no instruction privilege.  
**Test:** tool returns oversized recursive schema/content and instruction injection; gateway bounds/isolates it.  
**As built (M9.4):** the hostile server of `tools/mcp-testserver` declares a forged native name, an unusable name, an oversize schema, a 40-deep schema, a `$ref` pointing off the machine, an escape-sequence description carrying an instruction, smuggled `requiredCapabilities` / `systemPrompt` / `effectClass` fields, and 200 tools. `qual_ev_0104_0193_a_real_mcp_server_lists_calls_and_cancels_while_two_sessions_share_one_transport` runs the real Core against it and asserts each one refused by name with the rest of the list standing, the count bounded, the smuggled fields absent from what the host keeps, and the server unable to make any call a read.

## Security gates

- SAST and dependency scan clean of unresolved critical/high findings or documented exception with expiry.
- SBOM generated and signed.
- Secret scan on repository/build artifacts.
- Fuzzers for protocol decoder, path normalizer, tool argument validation and event migration.
- Property tests for effect receipt chain and lease fencing.
- Quarterly external penetration test before broad enterprise availability.

As built (M9.6): the fuzzers and property tests are `proptest` suites over the real components, run by `cargo test --workspace` on every platform, each with a docs/55 control mutation beside it. Protocol decoder — `crates/protocol/tests/property_framing.rs`: arbitrary bytes never panic the frame reader, a declared length above the 4 MiB ceiling is refused before it is read, every written frame reads back identical, and a corrupted length prefix never yields a frame. Path normalizer — `crates/workspace/tests/property_paths.rs`: paths built from parent references, dot variants, Unicode confusables, NUL, absolute and drive prefixes, both separators and 300-character names are refused or resolve inside the root, a real link to the outside is refused wherever it sits, a protected name is refused under any prefix, and (the TOCTOU case) a target swapped for an escaping link between two operations is refused at the second because every operation resolves anew; the fuzzer found that on Unix a backslash resolved to a literal file name while the root-relative record said a forward-slash path, which the normalizer now refuses on non-Windows platforms. Tool argument validation — `crates/tools/tests/property_arguments.rs`: every registered tool's schema accepts or refuses arbitrary JSON without panicking, refuses an undeclared key (every tool closes its top-level schema) and holds every declared string bound. Effect receipt chain — `crates/policy/tests/property_chain.rs`: arbitrary chains verify, every single-field tamper, reorder and interior deletion is detected, and a verifier with the hash check removed accepts tampers while one with the link check removed accepts reorders (each check is load-bearing); a truncated tail is invisible to the chain alone and is what the store's `last_receipt_hash` is for. Lease fencing and event migration — `crates/event-store/tests/property_fencing.rs` on the real SQLite store: for arbitrary interleavings of lease acquisitions and fenced appends only the current generation advances state and a stale one writes nothing; migrations are idempotent across reopen; and a projection rebuilt from the log reproduces the one projected while appending. Shell injection — `services/modbit-execd/tests/broker.rs`: hostile arguments (metacharacters, command substitution, quoting, newlines, a leading dash, environment references) reach the process as single literal tokens through the real broker, nothing named inside them runs, and the exit code is the child's. SAST/dependency/secret scanning, the signed SBOM and the external penetration test belong to M10.2 and the release procedure.

## Data privacy

Telemetry defaults to metadata, not source/prompt contents. Cloud source/checkpoints are encrypted in transit/at rest. Retention/deletion APIs must delete content-addressed objects when no remaining authorized references exist, while preserving legally required aggregate audit metadata per policy.


## V2 threats

Add explicit threats for media metadata exfiltration, decompression/image/PDF bombs, malicious document prompt injection, rich MCP media smuggling, vision-bridge residency leakage, skill evolution poisoning, benchmark overfit, candidate self-promotion, extension compatibility importing executable content, and multi-client approval races. The Capability Kernel remains authoritative in every case; derived media/wiki text is untrusted data.

## Conditional execution policy adversarial cases

Treat profiles, proposed plans, reviewer findings, registry/statistics feeds and telemetry as untrusted data. Test high-mean/weak-LCB admission, stale/missing statistics, runtime-invented continuation, missing assurance slot, risk/acceptance conflation, forged or stale evidence, incorrect escalation attribution and critical-surface misses. Required policy/assurance cannot be bypassed by model strength or routing savings.

Reviewer tests must permit bounded scratch file writes and sandboxed evidence processes while actually denying canonical/mount/symlink escapes, commit/push/deploy/persistent child processes, external actions and deny-default secrets/network. Default context must exclude solver hidden reasoning. Unknown effects/usage reconcile before fallback; no slot reset mints budget. EPR-FI-014..019 and amended existing EPR faults in doc 61 require real boundaries and mutation checks.
