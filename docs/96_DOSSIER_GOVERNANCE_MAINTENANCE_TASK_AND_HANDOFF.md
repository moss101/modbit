# Dossier governance maintenance task and handoff

## Identity and authority

- Task: DOC-GOV-001 (`dossier_task`); owner: governance; prerequisite: DOC-EPR-002 COMPLETE; outside product milestone roll-ups.
- Decision Record: DR-GOV-2026-09-05, approved for dossier adoption. On 2026-09-05 the user reviewed the sealed V3.3 EPR v1.1 dossier, received a gap report, and explicitly requested that four of its items be applied as one resealed dossier change: stale governing files, an evidence-reference grammar, a decision status outside the vocabulary, and undocumented pinned tool constants.
- Scope: governing files, status/evidence vocabulary, one decision-status label, dossier tooling and its tests. No requirement row, canonical owner, ADR clause, EPR task or product status changes. Product implementation remains NOT_STARTED.
- Revision before change: `../evidence/dossier-gov/baseline.json` (SHA-256 of every package file; initial `check_dossier --manifest` exit 0). No Git repository exists, so no commit anchor is available.

## Decision Record (change-control fields)

| Field | Content |
|---|---|
| Trigger / evidence | Dossier review on 2026-09-05: `../AGENTS.md` hash unchanged since before EPR v1.0 while its layout and regeneration sequence disagreed with doc 74 and `../SKILLS.md`; the graph accepted any evidence string and DOC-EPR-001/DOC-EPR-002 used different formats; MOD-SKILL-001 carried the compound status "PROVISIONAL / EXPERIMENT-GATED", outside the doc 93 ladder, and no tool validated decision statuses; the tools pin exact sealed counts without saying so anywhere. |
| Current behavior | The highest-authority file described a three-step regeneration that leaves the graph hash stale and skips the copied-package tests; `graph.py set` and `check_dossier.py` accepted free-text evidence; decision statuses were unvalidated; count pins were implicit. |
| Replacement | `../AGENTS.md`, `../README.md` and `../SKILLS.md` describe the actual package, the five-step reseal, the dossier-task pattern and the pinned constants. Doc 93 defines the `kind:value` evidence grammar; `graph.py set` rejects malformed references and `check_dossier.py` G3 validates every stored reference (`artifact:` package paths must exist). Doc 93 states that decision statuses are single values; `build_graph.py` and `check_dossier.py` D7 enforce the ladder. MOD-SKILL-001 is `EXPERIMENT`. Doc 74 documents the pins; the check prints computed counts. |
| Migration | The two DOC-EPR-002 references stored as bare paths and one stored as `sha256:` were rewritten in place to `artifact:` and `revision:sha256:`; the referenced files and digest are unchanged. DOC-EPR-001 references already conformed. No other node's live state changed. |
| Compatibility | Graph schema stays 1.2; node and edge types are unchanged. Two governance nodes (DR-GOV-2026-09-05, DOC-GOV-001) with six edges are added; this document adds its own `doc` node and reference edges. Existing `run:`, `artifact:`, `revision:` and `commit:` references remain valid. |
| Security impact | None on the product. Tooling becomes stricter: completion cannot be recorded against non-referential evidence. |
| Test impact | Three tests added to `../tools/test_dossier.py`: malformed and missing-file evidence rejected without graph mutation; compound decision status rejected by builder and check; governance task/edges generated and every stored reference conforms. Existing fixtures now use valid grammar. |
| Rollback | Restore the files in the inventory below to the hashes in `baseline.json` and rerun the reseal. Because doc 02 and doc 93 are governing text, reversal requires another approved Decision Record. |
| Explicit user approval | Given in chat on 2026-09-05 ("implement this") for exactly items 1, 2, 5 and 6 of the review. |

## Stage applicability

| Stage | DOC-GOV-001 execution |
|---|---|
| SELECT / READ / TRACE / AUDIT / MAP / PLAN | Review of all 78 prior docs, graph, manifest, tools and evidence; baseline hashes; first missing links listed above |
| IMPLEMENT VERTICAL SLICE | Governing text, vocabulary/grammar, tool validation, tests, regenerated graph and manifests |
| TEST LOCALLY / INTEGRATION | Actual Python CLIs and copied-package positive/negative tests |
| TEST REAL EFFECT | Real filesystem regeneration and SHA-256 verification of both manifest forms |
| INJECT FAILURE | Copied packages with free-text evidence, a missing `artifact:` path and a compound decision status must be rejected; graph bytes must not change on rejection |
| Product provider/sandbox/SQLite/Git qualification | Non-applicable: no product source exists and no product claim is made |
| CAPTURE / UPDATE / HANDOFF | Evidence retained under `../evidence/dossier-gov/`; only DOC-GOV-001 status set via graph.py; graph and manifests regenerated |

## Changes applied

1. **Governing files.** `../AGENTS.md` layout now lists both root patches, `evidence/` and all six tools; its "changing the dossier" rule is the five-step reseal of doc 74; steps 4 and 5 name REQ-EPR/QUAL-EPR and doc 06; dossier-only work is defined as a `dossier_task`. `../README.md` layout, doc count and examples are updated. `../SKILLS.md` evidence-capture, graph-and-manifest-update and dossier-maintenance skills carry the grammar and the pins, with a governance-maintenance note.
2. **Evidence grammar.** Doc 93 section "Evidence reference grammar" (ten kinds); doc 92 cross-reference; `../tools/graph.py` `evidence_ref_error` used by `set`; `../tools/check_dossier.py` G3 validates all stored references.
3. **Decision status.** Doc 02 MOD-SKILL-001 is `EXPERIMENT` with a provenance note; doc 93 states single values; `../tools/build_graph.py` raises on any other label; `../tools/check_dossier.py` D7 flags docs and graph.
4. **Pinned constants.** Doc 74 section; `../tools/dossier_epr.py` docstring; `../tools/check_dossier.py` summary counts computed from parsed data.

## Status and handoff

- **Task and requirements:** DOC-GOV-001 under DR-GOV-2026-09-05. No REQ/IMP/QUAL/EPR/ADR content changed; the 291 REQ-EV rows, both root patches and every prior evidence bundle are byte-identical to the baseline.
- **Revision:** direct workspace, no Git repository or branch. The exact revision is `source_revision_sha256` in `../evidence/dossier-gov/validation.json`, computed as for DOC-EPR-002: SHA-256 over the sorted source hash index of root `*.md` except `MANIFEST.md`, `docs/*.md` and `tools/*.py`.
- **Interfaces:** graph schema 1.2 unchanged; two new governance nodes with six edges plus the doc 96 node; `graph.py set` validates `--evidence`; `check_dossier.py` adds D7 and extends G3.
- **Evidence:** `../evidence/dossier-gov/baseline.json`, `../evidence/dossier-gov/tests.log`, `../evidence/dossier-gov/validation.json`. This is a SHA-256 integrity reseal, not a product certification or digital signature.
- **Checks:** Python syntax parse of all six tools; `python3 tools/test_dossier.py` (29 tests) all passed; `build_manifest`, `build_graph`, `build_manifest`, `check_dossier --manifest` all exit 0. Exact commands, outputs and digests are in validation.json. The graph and both manifest forms are regenerated again after DOC-GOV-001 completion so status and hashes are current.
- **Faults exercised:** free-text evidence, non-existent `artifact:` path, compound decision status, plus every prior negative case (source tampering, unlisted payload, stale views, cycles, duplicates, missing ADR/requirement/supersession, evidence-free and upstream-blocked completion). All run in disposable package copies with the real CLIs.
- **Remaining dossier acceptance:** none after the final integrity check.
- **Remaining product work/blockers:** unchanged from doc 95. Production source is absent; all milestones and EPR-000..019 remain NOT_STARTED; numerical EPR thresholds still need approved measured profiles.
- **Next safe action:** `python3 tools/graph.py ready`, take M0.1 on the critical path. Do not treat this reseal as product progress.

## Exact file inventory for this change

| Action | Path |
|---|---|
| Added | `docs/96_DOSSIER_GOVERNANCE_MAINTENANCE_TASK_AND_HANDOFF.md` |
| Added | `evidence/dossier-gov/baseline.json` |
| Added | `evidence/dossier-gov/tests.log` |
| Added | `evidence/dossier-gov/validation.json` |
| Changed | `AGENTS.md` |
| Changed | `MANIFEST.md` |
| Changed | `README.md` |
| Changed | `SKILLS.md` |
| Changed | `docs/00_MASTER_INDEX.md` |
| Changed | `docs/01_START_HERE_FOR_BUILD_AGENTS.md` |
| Changed | `docs/02_AUTHORITY_AND_DECISIONS.md` |
| Changed | `docs/74_PACKAGE_INTEGRITY_AND_BUILD_COVERAGE.md` |
| Changed | `docs/92_BUILD_EVIDENCE_AND_DEPENDENCY_MANIFEST.md` |
| Changed | `docs/93_STATUS_VOCABULARY_AND_LIFECYCLE.md` |
| Changed | `docs/98_BUILD_MANIFEST.md` |
| Changed | `graph/PROJECT_GRAPH.md` |
| Changed | `graph/project-graph.json` |
| Changed | `manifest.json` |
| Changed | `tools/build_graph.py` |
| Changed | `tools/check_dossier.py` |
| Changed | `tools/dossier_epr.py` |
| Changed | `tools/graph.py` |
| Changed | `tools/test_dossier.py` |
