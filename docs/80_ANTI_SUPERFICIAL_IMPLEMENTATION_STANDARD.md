# Anti-Superficial Implementation Standard

## Why this exists

AI coding agents often over-credit existing code: they find a type or similarly named module, make a narrow change, add a unit test and infer that the larger product requirement is covered. Modbit forbids that completion model.

## The feature-depth equation

A production feature is complete only when all applicable layers are present:

`DOMAIN CONTRACT + OWNER + PRODUCTION WIRING + POLICY + PERSISTENCE + REAL EFFECTOR + FAILURE/RECOVERY + EVIDENCE + USER/MODEL PROJECTION + REAL TEST`.

If one applicable layer is missing, the feature is partial.

## Required existing-code audit

For each feature, document:

- production entry point;
- canonical owner;
- downstream dependencies;
- persistent state written/read;
- capability/policy decision;
- real effector boundary;
- success events/evidence;
- failure events/evidence;
- restart/reconnect behavior;
- user-visible/model-visible projection;
- tests that actually cross the boundary.

## Thin-implementation traps

### Durable state
Writing a table is not durability. Kill the process at relevant transition points, restart, rebuild projections and prove exact state/replay semantics.

### Browser
Rendering a browser pane is not agent browser control. Prove semantic state extraction, action targeting, postcondition verification, takeover without session restart, visual fallback and hostile-page isolation.

### Subagents
Creating a child task row is not subagent orchestration. Prove transactional admission, identity/lineage, isolated execution scope, lifecycle controls, durable resume, capacity, conflict handling and evidence handoff.

### Context
Embedding files is not a context engine. Prove query planning, freshness hydration, structural retrieval, provenance, token budgets, incremental indexing and retrieval-before-edit behavior.

### Memory
Saving text is not engineering memory. Prove scoped promotion, provenance, confidence, conflict/supersession, retrieval policy and isolation from recovery state.

### Terminal
Running a subprocess is not a durable terminal. Prove argv/cwd/env isolation, PTY, streaming, cancellation, output bounds/OutputRef, replay cursor, process lifecycle and command-failure semantics.

### Sandbox
Starting a container/VM is not governed isolated execution. Prove tenant identity, deny-by-default network/fs policy, typed guest RPC, secret handles, resource limits, kill/recovery and backend conformance.

### Skills
Loading Markdown into a prompt is not a skill system. Prove manifest/versioning, selection, provenance, capability independence, evaluation, signing/promotion and rollback where evolution is enabled.

### Protected effects
Displaying an approval dialog is not policy enforcement. Prove pre-effect authorization, idempotent decision binding, stale-decision rejection, receipt/evidence chain and crash behavior around dispatch ambiguity.

## No behavior deletion to pass tests

An agent may not remove an invariant, disable a safety gate, downgrade an acceptance criterion, skip a test, hide an error or narrow a test fixture merely to obtain green CI. Such changes require an explicit architecture decision.
