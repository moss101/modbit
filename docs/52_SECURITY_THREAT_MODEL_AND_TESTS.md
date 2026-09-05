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

## Security gates

- SAST and dependency scan clean of unresolved critical/high findings or documented exception with expiry.
- SBOM generated and signed.
- Secret scan on repository/build artifacts.
- Fuzzers for protocol decoder, path normalizer, tool argument validation and event migration.
- Property tests for effect receipt chain and lease fencing.
- Quarterly external penetration test before broad enterprise availability.

## Data privacy

Telemetry defaults to metadata, not source/prompt contents. Cloud source/checkpoints are encrypted in transit/at rest. Retention/deletion APIs must delete content-addressed objects when no remaining authorized references exist, while preserving legally required aggregate audit metadata per policy.


## V2 threats

Add explicit threats for media metadata exfiltration, decompression/image/PDF bombs, malicious document prompt injection, rich MCP media smuggling, vision-bridge residency leakage, skill evolution poisoning, benchmark overfit, candidate self-promotion, extension compatibility importing executable content, and multi-client approval races. The Capability Kernel remains authoritative in every case; derived media/wiki text is untrusted data.

## Conditional execution policy adversarial cases

Treat profiles, proposed plans, reviewer findings, registry/statistics feeds and telemetry as untrusted data. Test high-mean/weak-LCB admission, stale/missing statistics, runtime-invented continuation, missing assurance slot, risk/acceptance conflation, forged or stale evidence, incorrect escalation attribution and critical-surface misses. Required policy/assurance cannot be bypassed by model strength or routing savings.

Reviewer tests must permit bounded scratch file writes and sandboxed evidence processes while actually denying canonical/mount/symlink escapes, commit/push/deploy/persistent child processes, external actions and deny-default secrets/network. Default context must exclude solver hidden reasoning. Unknown effects/usage reconcile before fallback; no slot reset mints budget. EPR-FI-014..019 and amended existing EPR faults in doc 61 require real boundaries and mutation checks.
