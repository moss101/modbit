---
id: DR-PX-2026-10-03-011
title: An extension marketplace as a client-side catalog over the existing extension system, hostile-input handling and the Customize surface (PX-087..093, doc 69)
status: proposed
date: 2026-10-03
supersedes: none
approved_by: pending owner ratification; basis is the owner's statement on 2026-10-03 that a marketplace is a real goal
---

# DR-PX-2026-10-03-011 — Extension marketplace and Customize

## Problem and goal

Modbit already has a governed extension system: a signed package of hooks, tool servers, commands and providers that loads quarantined until a person trusts its exact digest, a skill registry with importers, and a trust gate for external tool servers. It has no way to find, install, update and manage them, and no single place for the other agent customisations (skills, subagent profiles, rules). The owner has made a marketplace a real goal. Doc 10 lists "a marketplace-driven architecture in P0" as a non-goal, and the owner's hard carry-overs are explicit: the marketplace must keep the secret broker, the tenant boundary, provenance and the injection scanner; third-party skills and plugins are hostile input; installs are signed and hash-pinned; MCP configuration is approved by its hash. The goal is a catalog, install and update pipeline and one management surface that meet those constraints.

## Trigger and evidence

- **Owner's statement of 2026-10-03** and the carry-overs above.
- **Existing sealed requirements.** REQ-EV-0225 (ADOPT): publisher, source, signature and capabilities shown before activation, unsigned or untrusted extensions quarantined; REQ-EV-0138, 0137, 0114, 0181, 0224, 0239, 0240 (extensions, importers, skills, trusted servers, monotonic hooks, reversible handlers); REQ-EV-0216 (REJECT): no external skill-packaging runtime.
- **What exists** (audit, doc 69 section 3): the extension package, signatures, quarantine and trust (`crates/tools/src/extensions.rs`, `services/modbit-core/src/extensions.rs`) are production-working; the skill registry and importers are production-working; external-server proposal and trust are partial (trust by name, no configuration hash, no annotation classes, no OAuth); the hook contract is partial; catalog, install pipeline, update policy, variables and the Customize surface are NOT-FOUND.
- **Research (tags per doc 65 section 3).** LIVE (signed-out instance): the Customize editor has seven tabs in a fixed order, per-tab view options, empty states, an Add menu with three sources and a New action that asks for a name, and four rule types with their precedence (`SYN` §8). STATIC: plugins are packages bundling the same seven primitives with a manifest, convention discovery and containment rules; catalogs with SHA-pinned sources, curated categories and install counts but no star ratings; installs by commit SHA or artifact digest with a hardened Git environment; update pinning, deprecation and replacement; scopes user, team and project; plugin variables with write-only secrets; MCP project servers approved by a hash of their configuration, annotation-based read and write classes, OAuth with PKCE and dynamic client registration, a refresh lock, a connection state machine; a sandbox write-protection map that lets an agent author skills but not hooks, MCP configuration or permissions; an admin allow-list with wildcard and CIDR matching; and deep-link install that opens a confirmation (`F04`, `F03` §4). DISK: default hook timeout 5000 ms and an execution log (`L13`). UNVERIFIED: Team scope, moderation and install modes (the sampled profile had no team).

## Current behavior

Extensions load from a local directory; skills install from the CLI; external servers are proposed and trusted through commands; there is no catalog, no fetch-by-digest install, no update or rollback, no variables, no Customize view, and no explicit map of what an agent may write about itself.

## Proposed replacement

Specified in doc 69 (18 requirements) and rows PX-087 to PX-093, all release-critical:

1. **PX-087** the package format extension and a signed catalog manifest with strict parsing and containment.
2. **PX-088** the install, update and rollback pipeline: digest-pinned fetch, signature verification, hardened extraction, trust disclosure before activation, updates that never widen silently, scopes user and project.
3. **PX-089** variables and secrets (write-only, brokered) and hostile third-party content handling with sandboxed plugin code.
4. **PX-090** external-server trust by configuration hash, annotation classes, OAuth and the connection state machine.
5. **PX-091** the agent self-extension write-protection map and organisation policy for packages and servers.
6. **PX-092** hook contract completion in place.
7. **PX-093** the Customize surface with marketplace browsing.

## Independent Modbit design

The marketplace is a **client-side feature on the existing extension system**: the Core and every task run with no catalog reachable. Modbit's own `modbit-extension.json` manifest grows the missing component kinds; the existing signature and quarantine model is the trust model. A catalog is a signed index the user adds as a source, with the key pinned at that moment; it is data and cannot execute or widen policy. Install fetches by commit SHA or digest only, verifies digest and signature before extraction, extracts under hardened rules, takes a lock and keeps the previous version. Activation shows the exact capabilities in words first. An update that changes declared capabilities, adds a server or a hook, or changes an external server's configuration hash needs a fresh approval. Discovery is search, categories and an operator-supplied install count; no ratings or reviews. Customize is one surface with seven tabs; nothing is activated by browsing or importing.

## Security model

- **Hostile input.** Skill and rule text, descriptions, tool descriptions and hook output are scanned for injection, size-capped, stripped of control tags that imitate system messages, labelled with provenance, and never alter policy or capabilities. Plugin code (hooks, stdio servers) runs outside the Core under the sandbox with no ambient secrets; hooks cannot weaken a deny.
- **Secrets.** Variables marked secret are write-only, held by the secret broker by handle, never in package files, logs, events or prompts; token-shaped strings are redacted.
- **Integrity.** Digest and signature before extraction; no symlinks, special files, collisions, encrypted or ZIP64 entries, bombs; no mutable refs; no install-time execution; lock, rollback, crash recovery.
- **MCP.** Approval by configuration hash; unannotated tools count as write; OAuth with PKCE, dynamic registration and a refresh lock; terminal non-retryable errors; status note for the model.
- **Self-extension.** The agent may author skills, rules, commands and profiles as inert data; it may never write hooks, server configuration, permissions or policy, trust markers or installed package files, in any run mode.
- **Tenant boundary.** Cloud tasks use only packages the tenant's policy allows; the plugin cache is per tenant; organisation allow-lists support wildcard and CIDR matching with fingerprint-checked import and export.

## Canonical owner mapping (doc 81)

`extensions-hooks` owns packages, catalog, install and hooks (PX-087, 088, 089, 092); `external-tools` owns external-server trust (PX-090); `effects-security` owns the write-protection map and organisation policy (PX-091); `desktop` owns Customize (PX-093). No second extension runtime, registry, policy engine or secret store is introduced; the catalog adds data, not authority.

## Alternatives rejected

- **A Modbit-operated public marketplace service as part of this record.** Not specified: an operational decision (owner question below). The client works with any catalog the user adds.
- **Star ratings, reviews, publisher moderation workflows.** Rejected: no evidence of need, and the reference has no ratings.
- **Auto-update plugins to the latest version.** Rejected: updates never widen silently; versions can be pinned.
- **Installing from a branch or tag name.** Rejected: only commit SHAs and digests.
- **Adopting another product's plugin runtime or MCP Apps.** Rejected: REQ-EV-0216; only formats are imported and MCP Apps are not adopted here.
- **Marketplace-first architecture.** Rejected: the Core must not depend on a catalog (doc 10's intent).
- **Team marketplaces with required installs.** Deferred with team collaboration (REQ-PX-012, REQ-PX-013).

## Supersessions

Recorded in docs 02 and 03 and in doc 69 section 6, effective only on acceptance: doc 10's non-goal of a marketplace-driven architecture in P0 is clarified, not superseded (the intent holds; a marketplace is added as an optional client feature after P0); REQ-EV-0225 and REQ-EV-0138 are extended, not changed; REQ-EV-0216 (REJECT) is unchanged. No sealed row, EPR clause or pinned count changes.

## Migration

Additive. The manifest extension is backward compatible: existing `modbit-extension.json` packages and signatures stay valid.

## Compatibility

Rows are M10, RELEASE_ZERO, depending on IMP-EV-0138, IMP-EV-0225, M5.5, M9.3, M9.4, and for the surface on PX-044, PX-045 and PX-052 of DR-PX-2026-10-03-007. PX-095 of DR-PX-2026-10-03-012 (deep links) depends on PX-088 and PX-090 here.

## Security impact

Adds the largest hostile-input surface in the product. Each control above has a negative proof, including mutation checks that fail when digest verification, symlink resolution, control-tag stripping or the hash binding is removed.

## Test impact

Seven qualifications and scenarios in doc 62 on a real Core, real Git, a local HTTPS release fixture, hostile and malicious-archive fixtures, the real MCP test server and a local OAuth server; cross-tenant checks.

## Rollback

Revert the commits adding the rows and spec and rerun the reseal; a new record is needed for the locked paths.

## Consequences and owner questions

- **Question 1.** Will Modbit operate an official catalog? This record specifies the client and the catalog format only.
- **Question 2.** Which declared-capability changes should force re-approval on update? The triggers are policy data; the defaults here are conservative.
- RELEASE_ZERO grows by seven work items.

## Explicit user approval

Pending. Acceptance flips `status` to `accepted` in the commit that lands the rows.
