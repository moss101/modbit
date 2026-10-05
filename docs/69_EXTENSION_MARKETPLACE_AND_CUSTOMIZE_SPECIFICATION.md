# Extension marketplace and Customize specification (clean-room)

> **Authority:** DR-PX-2026-10-03-011 (`decisions/DR-PX-2026-10-03-011-extension-marketplace-and-customize.md`), status **proposed** until the owner ratifies it. Rows PX-087..PX-093 in `62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md` are NOT_STARTED and none may start before the record is accepted and DOC-PX-011 is COMPLETE.  
> **Nature:** a product specification in Modbit's own words and design. Provenance (clean-room posture), the four verification tags (LIVE, DISK, STATIC, UNVERIFIED) and the evidence key are those of `65_AGENT_FIRST_WORKSPACE_SPECIFICATION.md` sections 2, 3 and 12 and are not repeated. It records functional facts and says how well each is known; it is not an implementation and not proof that anything exists.

## 1. Purpose

Modbit already has a governed extension system: a signed package of hooks, tool servers, commands and providers that loads quarantined until a person trusts its exact digest, plus a skill registry with importers and a trust gate for external tool servers. What it lacks is a way to find, install, update and manage those packages and the other agent customisations (skills, subagent profiles, rules) from one place. The owner has made a marketplace a real goal. Doc 10 lists a marketplace-driven architecture in P0 as a non-goal; this record keeps that rule's intent (the Core never depends on a marketplace) and adds a marketplace as a client-side feature on top of the existing extension system, with every third-party artifact treated as hostile input. It also specifies the single Customize surface the research shows, because installing and managing customisations needs one.

## 2. Constraints and ownership

No canonical subsystem is added. The renderer holds no authority and every effect goes through the Core (doc 81, REQ-EV-0076). Decision: `MOD-EXT-001` in `02_AUTHORITY_AND_DECISIONS.md` (PROVISIONAL until the record is accepted). Common failure, cancellation, idempotency and restart semantics are those of doc 65 section 8.

| Row | Owner | Tier |
|---|---|---|
| PX-087 Plugin package format extension and signed catalog manifest with strict parsing and containment | extensions-hooks | release-critical |
| PX-088 Install, update and rollback pipeline with trust disclosure, digest-pinned fetch and hardened extraction | extensions-hooks | release-critical |
| PX-089 Plugin variables, secrets and hostile third-party content handling | extensions-hooks | release-critical |
| PX-090 External tool server trust by configuration hash, annotation classes, OAuth and connection state | external-tools | release-critical |
| PX-091 Agent self-extension write-protection map and organisation plugin and server policy | effects-security | release-critical |
| PX-092 Hook contract completion: merge order, exit-code blocking, fail-closed permission events, trusted-workspace gating and execution log | extensions-hooks | release-critical |
| PX-093 Customize surface: seven tabs, scopes, view options, catalog browsing, trust disclosure, update and the hook log | desktop | release-critical |

## 3. Existing implementation audit

Audit at `main` 5fdb47f, classes of doc 93.

| Area | Classification | What exists | First missing link |
|---|---|---|---|
| Extension package, signature, quarantine and trust | **PRODUCTION-WORKING** | `crates/tools/src/extensions.rs` and `services/modbit-core/src/extensions.rs` (IMP-EV-0138, IMP-EV-0225): a `modbit-extension.json` manifest with hooks, tools, commands and providers; ed25519 publisher signatures against trusted keys; `InspectExtension` shows publisher, source, digest, signature and every capability in words; `LoadExtension` with `expected_digest` (`EXTENSION_CHANGED`); unsigned or untrusted loads inert until `TrustExtension` names that digest; an invalid signature is never trusted; unloading removes handlers and providers. | Reused and extended: it is the package format and trust model. |
| Skills registry and importers | **PRODUCTION-WORKING** | `crates/skills`: manifest, selector, compiler, provenance and signing (M5.5), CLI install, list, revoke, remove; `ImportAgentConfig` importers with a migration report and quarantine of executable configuration (IMP-EV-0137, IMP-EV-0183). | Reused for skills and the import adapters. |
| External tool servers and trust | **IMPLEMENTED-PARTIAL** | `ProposeExternalServer` writes an inert proposal into the user configuration layer; `TrustExternalServer` is a person's decision held to the repository-trust capability and refused while a gate is unmet; credentials are brokered by handle. Trust is by server name; a hash of the effective configuration is not bound, per-tool read and write classes come from the host's own `read_only_tools` list and not from the server's annotations, and there is no OAuth flow, no deep-link install, no model-visible status note. | Config-hash approval, annotation classes and OAuth (PX-090). |
| Hook bus | **IMPLEMENTED-PARTIAL** | Typed lifecycle hooks with timeout, scope, audit and monotonic final deny (IMP-EV-0042, IMP-EV-0139, IMP-EV-0239, IMP-EV-0240). The reference's response-merge order, exit-code contract, fail-closed permission events, trusted-workspace gating of project hooks and a user-visible execution log were not found as a complete contract. | Complete the contract in place (PX-092). |
| Catalog, install pipeline, update policy, variables | **NOT-FOUND** | No catalog format, discovery, fetch-by-digest install with rollback, update policy, project-scope plugin list or plugin variables exist; an extension is loaded from a local directory. | PX-087 to PX-089. |
| Agent self-extension boundary | **IMPLEMENTED-PARTIAL** | The path policy denies protected configuration after symlink resolution (doc 23, M9.3). No explicit write-protection map separates data the agent may author (skills, rules, commands, agent profiles) from control files it may not (hooks, external-server configuration, permissions, trust markers, installed packages). | The map (PX-091). |
| Customize surface | **NOT-FOUND** | The CLI has skill and extension commands; the renderer has no skills, rules, hooks, MCP or plugin view. | PX-093. |

## 4. Requirements

### A. Package, catalog and discovery

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| EXM-A01 | A plugin is one package that may bundle any of: hooks, tool servers, commands, providers, skills, subagent profiles, rules, and external-server configuration, plus a variables schema. Modbit extends its existing `modbit-extension.json` manifest with the missing component paths, declared capabilities (the effect classes, hosts and hook events it will use), minimum client version, licence, publisher and repository. Components are discovered by convention when paths are omitted. Component names are normalised and de-duplicated. | STATIC (reference bundles the same seven primitives) | PX-087 |
| EXM-A02 | Package paths are contained: no parent segments, no absolute paths, no scheme, symlinks resolved and required to stay inside the package, symlinked manifests and hook files refused, 10 MiB per file and a package size cap. A manifest is validated whole and anything only the host may declare (an external server's trust, its read-only tools, the sites it serves) cannot be declared by a package. | STATIC (containment rules) | PX-087 |
| EXM-A03 | A catalog is a signed index a user adds as a source. Each entry has an id, publisher identity, versions, source (a Git URL pinned to a commit SHA, or a release asset with its SHA-256), a signature, declared capabilities, deprecation with a replacement, categories and, if the catalog operator supplies it, an install count. The client pins the catalog's signing key when the user adds it. A catalog is data: it cannot execute anything and cannot widen policy. | STATIC (reference marketplace manifest with SHA-pinned sources) | PX-087 |
| EXM-A04 | Discovery is search, curated categories and install count. There are no star ratings or reviews; the reference product has none either. Install counts come from the catalog operator, never from client telemetry. | STATIC (no ratings, install count and curated categories) | PX-087, PX-093 |
| EXM-A05 | Importers read other tools' plugin and skill formats into Modbit's format through the existing importer with a migration report. An import is quarantined like any package and is never activated by being imported. | STATIC (reference reads other tools' plugin formats); DISK (import gates and detected tools) | PX-087 |

### B. Install, update and integrity

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| EXM-B01 | Before activation Modbit shows the publisher, the source, the signature state, every declared capability in words, the external servers and hook events it adds, the variables it asks for and its size (REQ-EV-0225). An unsigned or unverified package is quarantined and inert until the person trusts that exact digest; an invalid signature is never trusted. An install that adds an external server or a hook shows an impact confirmation. | STATIC (impact confirmation and detail view; the disclosure itself is existing Modbit behaviour, REQ-EV-0225) | PX-088, PX-093 |
| EXM-B02 | Installation fetches by commit SHA or digest only. Git runs with prompts and askpass disabled, no repository hooks and a batch-mode transport; the artifact's SHA-256 and signature are verified before extraction; extraction refuses symlinks, special files, name collisions (case and Unicode-insensitive), encrypted or ZIP64 entries and exceeds neither 5,000 entries nor a 128 MiB expanded size. Install takes a lock, keeps the previous version for rollback and recovers from a crash mid-install to one whole version. A package never executes anything at install time. | STATIC (the reference's hardened install of its sidecar and clone path) | PX-088 |
| EXM-B03 | Updates never widen silently. A new version that changes declared capabilities, adds an external server or a hook, or changes the hash of an external server's effective configuration needs a fresh approval. Versions can be pinned. A deprecation shows its message and replacement. Rollback restores the previous version. Uninstall removes handlers, providers and caches and leaves no active component (REQ-EV-0240). | STATIC (pinning, deprecation, replacement); UNVERIFIED (Modbit's re-approval triggers) | PX-088 |
| EXM-B04 | Scopes are user and project; a project plugin list is committed with the repository and each user approves it by hash; plugin-supplied components show their source scope. Team scope with optional, default and required installs is deferred with team collaboration; organisation policy through the signed policy bundle can allow, block or require a package. | STATIC (reference scopes and install modes); UNVERIFIED (team scope never observed) | PX-088, PX-091 |

### C. Hostile input, secrets and MCP trust

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| EXM-C01 | Third-party content is hostile input: skill and rule text, descriptions, tool descriptions and hook output are scanned for injection, size-capped, stripped of control tags that imitate system messages, labelled with provenance, and never treated as instructions that alter policy or capabilities. Plugin code (hooks, stdio tool servers) runs outside the Core under the sandbox with no ambient secrets, and a hook cannot weaken a deny. | STATIC (hook-context hygiene, sandboxed stdio servers) | PX-089 |
| EXM-C02 | Plugin variables follow a restricted schema (types, enums, bounds). A variable whose name or schema marks it secret is write-only and goes to the secret broker by handle; it is never written into a package file, a log, an event or a prompt, and token-shaped strings are redacted from logs. A server configuration that references a variable derives its schema automatically. | STATIC (variables, write-only secrets, derived schema, token redaction) | PX-089 |
| EXM-C03 | An external tool server supplied by a project or a plugin is approved by a hash of its effective configuration (command, arguments, environment names, URL, header names, allowed tools); editing it re-prompts. Tools are classified read, write or unannotated from the server's annotations and unannotated counts as write; per-server and per-tool enable and a read and write group policy exist. OAuth uses PKCE and dynamic client registration with a refresh lock; a connection follows an explicit state machine with a single transport fallback, non-retryable errors are terminal and a needs-auth or errored server produces a status note telling the model what to do. An install deep link names a validated server and always opens a confirmation. | STATIC (hash approval, annotation classes, OAuth, state machine, deep link confirm) | PX-090, PX-095 |
| EXM-C04 | The agent may author data: skills, rules, commands and subagent profiles. It may never write hooks, external-server configuration, permissions or policy, trust markers, or the files of an installed package. The Kernel's protected-path map enforces this after symlink resolution, so an agent can extend itself without widening its own authority. | STATIC (the reference's sandbox write-protection map) | PX-091 |
| EXM-C05 | Cloud tasks use only the packages the tenant's policy allows; the plugin cache is per tenant; nothing crosses the tenant boundary. Organisation policy can set an external-server allow-list with wildcard and CIDR matching and import or export it with a fingerprint check for optimistic concurrency. | STATIC (admin MCP allow-list with wildcard and CIDR matching) | PX-091 |

### D. Hooks

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| EXM-D01 | The hook contract is complete: a typed event catalogue; a JSON request on standard input and a JSON response on standard output; exit code 2 blocks with a reason; permission events fail closed by default and others by opt-in; parallel responses merge deny over ask over allow; a timeout (5 s by default) and a loop limit; project hooks run only in trusted workspaces; every run is written to an execution log the user can read; a hook cannot grant beyond the Kernel and a session-end hook runs to completion or fails visibly (PX-049). The reference product's 21 events are a catalogue to study, not a list to copy. | STATIC (contract); DISK (default timeout 5000 ms, execution log shape, session-end failure) | PX-092 |

### E. Customize surface

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| EXM-E01 | One Customize surface has seven tabs in a fixed order: Plugins, MCP servers, Skills, Subagents, Rules, Commands, Hooks. Scopes are user, project and plugin (Team is shown only where a team exists and was never observed). Each tab has its own view options: MCP servers group by status or scope and sort by name or most used; the others group by source or author, filter all, local, workspace or plugin, and sort by name or author; there is no Installed filter. Each tab has an empty state, an Add menu with three sources (create here, import, install from the catalog) and a New action that asks for a name first. | LIVE (tabs, order, view options, empty states, Add menu, New prompt: signed-out instance); UNVERIFIED (Team scope) | PX-093 |
| EXM-E02 | The Plugins tab browses a catalog with search and categories, shows a detail view with the trust disclosure of EXM-B01, installs with a scope choice, and shows update, pin and remove. A load failure is typed (clone, install, parse, manifest, timeout, unknown) with copyable details, and a stale data indicator when the set is incomplete. The Hooks tab shows the execution log. | STATIC (failure taxonomy, detail view); DISK (hook execution log channel) | PX-093 |
| EXM-E03 | Rules have four types: Always, Auto-attached by file pattern, Requested by the agent, and Manual. A rule applies in the order its type implies and the context inspector shows which rules entered the prompt. | LIVE (the four labels and their precedence: signed-out instance) | PX-093 |

## 5. Rows and evidence tiers

Every row is release-critical: each changes effect-bearing behaviour, permissions or policy, execution, recovery, protocol or schema, a security boundary or evidence semantics (doc 83); none is eligible for the iteration tier.

| Row | Title | Owner | Prerequisites |
|---|---|---|---|
| PX-087 | Plugin package format extension and signed catalog manifest with strict parsing and containment | extensions-hooks | IMP-EV-0138, IMP-EV-0225, M5.5, DOC-PX-011 |
| PX-088 | Install, update and rollback pipeline with trust disclosure, digest-pinned fetch and hardened extraction | extensions-hooks | PX-087, M9.4, DOC-PX-011 |
| PX-089 | Plugin variables, secrets and hostile third-party content handling | extensions-hooks | PX-087, M9.3, M7.7, DOC-PX-011 |
| PX-090 | External tool server trust by configuration hash, annotation classes, OAuth and connection state | external-tools | M9.4, PX-089, DOC-PX-011 |
| PX-091 | Agent self-extension write-protection map and organisation plugin and server policy | effects-security | M9.3, PX-088, DOC-PX-011 |
| PX-092 | Hook contract completion: merge order, exit-code blocking, fail-closed permission events, trusted-workspace gating and execution log | extensions-hooks | IMP-EV-0139, IMP-EV-0239, PX-049, DOC-PX-011 |
| PX-093 | Customize surface: seven tabs, scopes, view options, catalog browsing, trust disclosure, update and the hook log | desktop | PX-044, PX-045, PX-052, PX-087, PX-088, PX-090, PX-092 |

## 6. Supersessions and clarifications

Nothing sealed changes until the record is accepted. The explicit records, also entered in docs 02 and 03:

| Earlier decision, row or text | Kind | Scope and effect | What survives |
|---|---|---|---|
| doc 10 non-goal: a marketplace-driven architecture in P0 | CLARIFIED | The non-goal's intent holds: the Core and every task run with no catalog reachable. A marketplace is a client-side feature on the existing extension system, added after P0. | Core independence from any marketplace. |
| REQ-EV-0225 (ADOPT, marketplace trust surfaced) and REQ-EV-0138 (ADAPT, unified plugins) | EXTENDED, NOT CHANGED | PX-087 to PX-093 extend the existing package, trust and quarantine model. | Row text, disposition and owner. |
| REQ-EV-0216 (REJECT, adopting an external skill-packaging framework) | UNCHANGED | Only formats are imported; no external plugin runtime is adopted. | The rejection. |

## 7. Traceability: requirement to row, tag and source

| Requirement | Rows | Verification tag | Source report section |
|---|---|---|---|
| EXM-A01 | PX-087 | STATIC (reference bundles the same seven primitives) | F04 §2.1, F04 §0, doc 16 |
| EXM-A02 | PX-087 | STATIC (containment rules) | F04 §2.1, F04 §10, doc 16 |
| EXM-A03 | PX-087 | STATIC (reference marketplace manifest with SHA-pinned sources) | F04 §2.3, F04 §2.2 |
| EXM-A04 | PX-087, PX-093 | STATIC (no ratings, install count and curated categories) | F04 §2.3, SYN §2.4 |
| EXM-A05 | PX-087 | STATIC (reference reads other tools' plugin formats); DISK (import gates and detected tools) | F04 §2.1, L12 §2.7, doc 40 REQ-EV-0137 |
| EXM-B01 | PX-088, PX-093 | STATIC (impact confirmation and detail view; the disclosure itself is existing Modbit behaviour, REQ-EV-0225) | doc 16, F04 §2.3 |
| EXM-B02 | PX-088 | STATIC (the reference's hardened install of its sidecar and clone path) | F04 §2.2, F03 §4 F-D1, F04 §10 |
| EXM-B03 | PX-088 | STATIC (pinning, deprecation, replacement); UNVERIFIED (Modbit's re-approval triggers) | F04 §2.2, doc 40 REQ-EV-0240 |
| EXM-B04 | PX-088, PX-091 | STATIC (reference scopes and install modes); UNVERIFIED (team scope never observed) | F04 §2.2, SYN §8 |
| EXM-C01 | PX-089 | STATIC (hook-context hygiene, sandboxed stdio servers) | F04 §10, F04 §7 |
| EXM-C02 | PX-089 | STATIC (variables, write-only secrets, derived schema, token redaction) | F04 §2.4, F04 §10 |
| EXM-C03 | PX-090, PX-095 | STATIC (hash approval, annotation classes, OAuth, state machine, deep link confirm) | F04 §7.3, F04 §7.4, F04 §7.6, F04 §10 |
| EXM-C04 | PX-091 | STATIC (the reference's sandbox write-protection map) | F04 §7.1, F04 §10 |
| EXM-C05 | PX-091 | STATIC (admin MCP allow-list with wildcard and CIDR matching) | F04 §7.7, doc 24 |
| EXM-D01 | PX-092 | STATIC (contract); DISK (default timeout 5000 ms, execution log shape, session-end failure) | F04 §1, L13 §2.4, L13 §3.8, doc 40 REQ-EV-0139 |
| EXM-E01 | PX-093 | LIVE (tabs, order, view options, empty states, Add menu, New prompt: signed-out instance); UNVERIFIED (Team scope) | SYN §8, L11 |
| EXM-E02 | PX-093 | STATIC (failure taxonomy, detail view); DISK (hook execution log channel) | F04 §2.2, F04 §2.3, L13 §2.7 |
| EXM-E03 | PX-093 | LIVE (the four labels and their precedence: signed-out instance) | SYN §8, F04 §4 |

### Row to requirements

| Row | Requirements |
|---|---|
| PX-087 | EXM-A01, EXM-A02, EXM-A03, EXM-A04, EXM-A05 |
| PX-088 | EXM-B01, EXM-B02, EXM-B03, EXM-B04 |
| PX-089 | EXM-C01, EXM-C02 |
| PX-090 | EXM-C03 |
| PX-091 | EXM-B04, EXM-C04, EXM-C05 |
| PX-092 | EXM-D01 |
| PX-093 | EXM-A04, EXM-B01, EXM-E01, EXM-E02, EXM-E03 |

## 8. Non-goals

| Not in scope | Why |
|---|---|
| A Modbit-operated public marketplace service | An operational decision; this record specifies the client and the catalog format (V02). |
| Plugin publishing, moderation, ratings and reviews | No evidence of need; the reference has no ratings. |
| Team marketplaces and team-required installs | Team collaboration is DEFERRED (REQ-PX-012, REQ-PX-013). |
| A marketplace dependency in the Core | The Core and every task run with no catalog reachable; doc 10's intent holds. |
| Adopting another product's plugin runtime | REQ-EV-0216 (REJECT); only formats are imported. |
| MCP Apps and design-mode integrations | Not in this record (V05). |

## 9. Still unverified

| ID | Unverified fact | Evidence that would close it |
|---|---|---|
| V01 | Behaviour of the reference product's Team scope, plugin install modes and moderation lifecycle | A team account; the sampled profile had no team (DISK). |
| V02 | Whether Modbit will operate an official catalog | Owner decision (question in the DR); the client works with any catalog the user adds and needs no operator to be tested. |
| V03 | Update re-approval triggers: which declared-capability changes warrant a prompt without prompt fatigue | Usage evidence; the triggers are policy data. |
| V04 | Skill index budget and progressive disclosure of skill text in the prompt | Not part of this record (context economy); a context-engine task with its own qualification. |
| V05 | MCP Apps (sandboxed interface cards from tool servers) | Not adopted here; a later record if wanted. |
