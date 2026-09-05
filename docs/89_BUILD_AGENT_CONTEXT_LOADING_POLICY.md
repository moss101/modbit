# Build-Agent Context Loading Policy

Large agent prompts encourage superficial synthesis. Load only task-relevant authority.

## Minimum context bundle per task

1. `AGENTS.md` and current task card.
2. Authority/decision file.
3. Target subsystem spec.
4. Direct dependency specs.
5. Relevant `REQ-EV-*` rows only.
6. Linked qualification/E2E tests.
7. Current manifest/handoff for touched modules.
8. Existing source files discovered by code search/AST, not arbitrary repository dumps.

## Retrieval rules

- exact/symbol search first for known identifiers;
- hydrate source bytes from active revision before reasoning;
- expand to structural/dependency search only when needed;
- include interfaces **and their implementations/callers/tests** when assessing completeness;
- never conclude from snippets alone that a feature is implemented;
- record revision/hash for evidence used in a task decision.

## Context freshness

If source changes after an agent's plan, refresh the affected files and dependency graph before applying edits. Stale plan execution is a merge/conflict risk and must fail safe.
