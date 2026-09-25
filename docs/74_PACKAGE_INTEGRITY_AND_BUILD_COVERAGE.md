# Package Integrity and Build Coverage

## Documentation package checks

- specification payload is Markdown; tooling is standard-library Python and JSON; evidence bundles are JSON and logs;
- master manifests enumerate every file;
- exactly 291 evidence-derived requirement rows are present;
- every ADOPT/ADAPT row has an implementation task and qualification test;
- no `UNREVIEWED`, `UNKNOWN`, `TBD IMPLEMENTATION`, fake-completion or placeholder acceptance states;
- build-agent docs contain no external-product feature shorthand;
- exact dependency/provider names appear only where implementation binding requires them.

As built (REQ-EV-0212, `tools/example_runner.py`): every `bash` block in a governing root file or a numbered doc is an instruction this package gives an agent, so none of them may be a placeholder. The runner discovers each block in `AGENTS.md`, `README.md`, `SKILLS.md`, `graph/PROJECT_GRAPH.md` and `docs/*.md`, requires it to be declared in `tools/examples.json`, and pins it by the sha256 of its body — a block added, edited or deleted fails the gate until its declaration is updated, which is what "fails release on drift" means here. Policy is per command, not per block, so one illustrative line does not excuse the runnable ones beside it. A `run` command is executed for real in a throwaway copy of the package and must exit with the declared status, which is how a documented non-zero contract is pinned rather than hidden: `graph.py goal` is declared to exit 1 while work remains toward RELEASE_ZERO (docs/77), so the day the goal is met that declaration is updated deliberately. A `shape` command is not executed — its declaration says why — but it is still checked against the real thing it invokes: `graph.py` parses the invocation with its own `build_parser()`, the tools that read `sys.argv` are checked for the flag literals they parse, `cargo -p` must name a real workspace member — cargo's own subcommand list is cargo's business, and asking for it would make this gate need a Rust toolchain in a job that deliberately has none — and `pnpm` a real `package.json` script. A `synopsis` is a usage template rather than a command (`graph.py set <id> <STATE> [--evidence <ref> ...]`): its subcommand and every flag it names are checked against that parser, and a template carrying a real argument is refused as a command in disguise. A program with no shape checker cannot be declared `shape`, so there is no policy meaning "trust me", and a declared example that invoked the gate itself is refused rather than left to recurse. Today: 11 blocks, 40 commands executed in a copied package, 15 checked against the tool they invoke. Wired as the `dossier integrity` job's third step. Negative proofs in `tools/test_dossier.py` (`ExampleRunnerTests`, on minimal fixture packages): an undeclared block, a block that changed after its declaration, a declaration whose block is gone, a `run` command that stopped working, an unjustified non-zero expectation, a `shape` or `synopsis` without a reason, a program with no checker, a flag its tool no longer parses, an invocation the tool's own parser rejects, a synopsis naming an unknown subcommand or flag, and a synopsis carrying a real argument — each fails, and each names its one cause.

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

The tooling pins the sealed surface on purpose. `tools/check_dossier.py` expects exactly 291 REQ-EV rows, 265 IMP-EV tasks and 291 QUAL-EV tests. `tools/dossier_epr.py` expects exactly the EPR-000..019 triplets, ADR-R-039..056 with the ten recorded scoped supersession pairs, gates A–G and their critical task coverage. Any addition (a new EPR triplet, ADR, gate or supersession) requires an approved Decision Record and, in the same reseal, a coordinated change to those constants and to `tools/test_dossier.py`; the summary counts printed by the check are computed from the parsed documents and graph, not typed by hand. `IMP-EV-*` milestones derive from the owner label map in `tools/build_graph.py`; the one recorded exception is `MILESTONE_OVERRIDES`, which schedules `IMP-EV-0107` (bounded failure evidence for repair) in M2 under DR-PX-2026-09-05-006 because the M2 repair loop depends on it. The override is documented on the node (`milestone_override`), leaves docs 40/41/42 byte-identical, and is load-bearing: without it the graph builder rejects the dependency as a milestone cycle, which `tools/test_dossier.py` proves on a copied package.

Evidence references on graph nodes must follow the `kind:value` grammar in `93_STATUS_VOCABULARY_AND_LIFECYCLE.md`. The check rejects malformed references and `artifact:` paths that do not exist in the package (G3), and rejects decision statuses outside the doc 93 ladder (D7). Release-gate attestations are validated and may not precede their required tasks (G7); lifecycle transitions move one step at a time and `BLOCKED`/backward moves require a note, enforced by `tools/graph.py set`. Check D8 fails when any top-level section of either root source patch is missing from the coverage map in `27_EXECUTION_POLICY_ROUTER_AND_VERIFIED_ORCHESTRATION.md`. Check D9 validates the additive PX ledger structurally without pinning a count, and G8 requires the releases ALPHA, BETA and RELEASE_ZERO to exist, nest and cover every product work item; effective totals in the check summary are computed from base plus EPR plus PX. Governance maintenance of this kind is tracked as a `dossier_task` with its own handoff and `evidence/` bundle (`96_DOSSIER_GOVERNANCE_MAINTENANCE_TASK_AND_HANDOFF.md`).
