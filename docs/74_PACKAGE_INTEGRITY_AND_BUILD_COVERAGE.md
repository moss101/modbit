# Package Integrity and Build Coverage

## Documentation package checks

- all payload files are `.md`;
- master manifests enumerate every file;
- exactly 291 evidence-derived requirement rows are present;
- every ADOPT/ADAPT row has an implementation task and qualification test;
- no `UNREVIEWED`, `UNKNOWN`, `TBD IMPLEMENTATION`, fake-completion or placeholder acceptance states;
- build-agent docs contain no external-product feature shorthand;
- exact dependency/provider names appear only where implementation binding requires them.

## Product CI coverage checks to implement

CI must parse requirement/task/test metadata and fail when a production requirement has no owner, task or real qualification. It must also fail on production fake adapters, forbidden dependency edges, skipped protected-effect tests and incomplete manifest evidence.

## V3.3 EPR v1.1 package coverage and regeneration

The 291 REQ-EV rows remain frozen; DR-EPR-2026-09-05-v1.1 establishes 20 REQ-EPR, 20 EPR tasks, 20 QUAL-EPR, 40 real/fault scenarios, 18 ADR-R decisions and seven gate nodes. These must resolve to existing owner boundaries and acyclic prerequisites. Both original root patches and retained dossier evidence are package payloads alongside Markdown and standard-library Python/JSON tooling. Generated MANIFEST.md/manifest.json exclude their own hashes to avoid recursion; transient caches and Git internals are not payload.

After edits run the required initial manifest build, graph build, then refresh hashes for the final graph before full validation:

```bash
python3 tools/build_manifest.py
python3 tools/build_graph.py
python3 tools/build_manifest.py
python3 tools/check_dossier.py --manifest
python3 tools/test_dossier.py
```

`graph.py set` also refreshes the human graph view; regenerate manifest hashes after any status change. Integrity includes current source/graph equivalence, dependency cycles, evidence/prerequisite guards and manifest coverage. Test faults mutate disposable copied packages, never weaken the delivered policy or assertions.

## Pinned integrity constants and evidence grammar (DR-GOV-2026-09-05)

The tooling pins the sealed surface on purpose. `tools/check_dossier.py` expects exactly 291 REQ-EV rows, 265 IMP-EV tasks and 291 QUAL-EV tests. `tools/dossier_epr.py` expects exactly the EPR-000..019 triplets, ADR-R-039..056 with the ten recorded scoped supersession pairs, gates A–G and their critical task coverage. Any addition (a new EPR triplet, ADR, gate or supersession) requires an approved Decision Record and, in the same reseal, a coordinated change to those constants and to `tools/test_dossier.py`; the summary counts printed by the check are computed from the parsed documents and graph, not typed by hand.

Evidence references on graph nodes must follow the `kind:value` grammar in `93_STATUS_VOCABULARY_AND_LIFECYCLE.md`. The check rejects malformed references and `artifact:` paths that do not exist in the package (G3), and rejects decision statuses outside the doc 93 ladder (D7). Governance maintenance of this kind is tracked as a `dossier_task` with its own handoff and `evidence/` bundle (`96_DOSSIER_GOVERNANCE_MAINTENANCE_TASK_AND_HANDOFF.md`).
