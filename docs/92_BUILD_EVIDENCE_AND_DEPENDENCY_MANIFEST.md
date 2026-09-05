# Build Evidence and Dependency Manifest

## Evidence classes used by implementation

- architecture decision;
- requirement row;
- task card;
- source-code path/symbol at pinned revision;
- unit/property/integration result;
- qualification/E2E/security/performance run;
- event/effect/artifact/checkpoint evidence reference;
- build/environment/dependency digest.

When one of these is recorded on a project-graph node it is written as one `kind:value` reference using the grammar in `93_STATUS_VOCABULARY_AND_LIFECYCLE.md`; the graph tool and integrity check reject any other form.

Research provenance is deliberately not part of routine agent context. If an architect needs to revisit why a requirement exists, use the prior provenance dossier outside the implementation prompt.

## Exact dependency naming

Exact external dependency/provider names are confined to `35_DEPENDENCY_AND_BINDING_DECISIONS.md` or lockfiles/build configuration where implementation requires them. Agents must not use vendor/product names as architecture boundaries.
