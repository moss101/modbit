# Task Card — IMP-EV-0212 No fake test examples

## Identity

- Task ID: IMP-EV-0212 (owner label Quality Gate; subsystem verification)
- Milestone: M10 Release candidate hardening
- Requirements: REQ-EV-0212 (examples used for evaluation must be executable/verified, not illustrative placeholders); QUAL-EV-0212 (a docs/example runner executes declared examples and fails release on drift); docs/83 definition of done; docs/74 package integrity.
- Qualification: real-system — the runner executes the dossier's own declared commands against a real copied package and against the real tools' own argument parsers, and CI fails the merge on drift.
- Evidence tier: release-critical (evidence semantics: what the project's own instructions claim can be done).
- No new canonical owner: the runner is a governance/quality tool under `tools/`, not a second verification engine. It asserts nothing about product behavior; it asserts that the commands the dossier declares still work.

## Goal

Every shell example the dossier gives an agent is either executed for real or checked against the tool's own parser, none may sit undeclared, and a release fails when one drifts.

## Existing-code audit

- classification: **NOT-FOUND**. Nothing verifies a documentation example. `tools/` has no example runner, `.github/workflows/ci.yml` has no example step, and `tools/check_dossier.py` checks requirement/reference/hash integrity but never executes a command the docs print. The dossier declares 50 shell command lines across 8 governing files (`AGENTS.md`, `README.md`, `SKILLS.md`, `graph/PROJECT_GRAPH.md`, `docs/74`, `docs/75`, `docs/77`, `docs/98`) and every one of them is an instruction an agent is expected to follow.
- first missing link: nothing executes them, so a renamed subcommand or a dropped flag would silently leave the highest-priority instructions in the package wrong. It also means nothing distinguishes an example that is meant to run from one that cannot: `AGENTS.md`'s own `graph.py set IMP-EV-0012 REAL_TESTING` names a task that is long since `COMPLETE`, so executing it verbatim would be a backward move the tool refuses without `--note` — it is a legitimate illustration, and today nothing says so or checks that its subcommand and flags are still real.
- the benchmark side is already covered and is recorded here so the requirement is not read too narrowly: the only committed evaluation corpora are `benchmarks/gate-calibration/corpora/{acceptance-holdout,risk-holdout}.json`, and EPR-019's qualification executes every case in both against hidden oracles and pins every verdict, so no case there is illustrative. The gap is the documentation, not the corpora.

## Change

- `tools/example_runner.py` (new): discovers every `bash` block in `AGENTS.md`, `README.md`, `SKILLS.md`, `graph/PROJECT_GRAPH.md` and `docs/*.md`; requires each to be declared in `tools/examples.json` and pinned by the sha256 of its body; applies a per-command policy — `run` (executed for real in a throwaway copy of the package, against a declared exit status), `shape` (not executed, but checked against the tool it invokes), `synopsis` (a usage template, checked against the tool's subcommands and flags). `--list` prints every block and its policies; `--declare` rewrites the hashes and leaves a new command undeclared so an author must choose.
- `tools/examples.json` (new): 11 blocks, 55 commands — 40 `run`, 14 `shape`, 1 `synopsis`.
- `tools/graph.py`: `main()` split so the command surface is available as `build_parser()`. This is what makes a `shape` or `synopsis` check the tool's own opinion rather than a guess.
- `.github/workflows/ci.yml`: the `dossier integrity` job gains a third step, so drift fails the merge.
- `tools/test_dossier.py`: `ExampleRunnerTests`, 13 negative proofs on minimal fixture packages.
- `docs/74`: the gate as built.

## Verification

- `python3 tools/example_runner.py` on the real package: **11 blocks, 40 commands executed in a copied package, 15 checked against the tool they invoke**, ~2 minutes. The 40 executed commands include the whole five-step reseal of docs/74 (`build_manifest` → `build_graph` → `build_manifest` → `check_dossier --manifest` → `test_dossier`), every read-only `graph.py` subcommand the docs show (`ready`, `ready --all`, `ready --release ALPHA`, `show` for a milestone task, an `IMP-EV`, an `EPR` and a subsystem, `status`, `gates`, `releases`, `goal`, `goal --json`, `goal ALPHA`, `path`, `stats`, `render`, `render --write`), both `check_dossier` forms and both `jq` queries of the graph.
- documented non-zero contracts are pinned rather than hidden: `graph.py goal` and `goal --json` are declared to exit 1 (docs/77: 1 means work remains toward RELEASE_ZERO) with a reason, while `goal ALPHA` exits 0 because Alpha is READY. A non-zero expectation without a reason is refused.
- the 15 non-executed commands are the ones that must not run in a gate, each naming why: the four `graph.py set` / `attest` examples carry illustrative ids and transitions (`graph.py`'s own parser still accepts every invocation); the `cargo` and `pnpm` lines are CI's own rust and node jobs; `SKILLS.md`'s second copy of the reseal would run the copied-package suite twice for no new information; `SKILLS.md`'s `set <id> <STATE> [--evidence <ref> ...]` is a synopsis, checked against `graph.py set`'s real flags.
- `ExampleRunnerTests` (13 cases, 1.8 s, minimal fixture packages so each names one cause): an undeclared block; a command hidden in a `text` block where the gate cannot see it; a block that changed after its declaration (drift); a declaration whose block is gone; a `run` command that stopped working (`exited 3, not the declared 0`); an unjustified non-zero expectation, then the same declaration with a reason passing; a `shape` without a reason; a program with no shape checker (`git push`); a flag its tool no longer parses; an invocation `graph.py`'s own parser rejects; a synopsis naming an unknown subcommand; a synopsis naming a flag the subcommand does not accept; a synopsis carrying a real argument instead of a metavariable.
- failure injection beyond the suite, found and fixed while building it: `docs/74`'s declared example runs `tools/test_dossier.py`, and a first draft put the real-package gate run inside that suite — the gate then ran itself, each level copying the package, until the 600-second timeout. The runner now marks its `run` subprocesses and refuses to start inside one, so that cycle is one legible failure instead of a hang; the fixture tests clear the marker because the runner is their subject.
- regression: `python3 tools/test_dossier.py` 56 tests OK; `check_dossier.py --manifest` exit 0.

## Limitations

- Scope is `bash` blocks in the governing root files, `graph/PROJECT_GRAPH.md` and `docs/*.md`. A command in another block kind is refused rather than exempt, and today no other block kind holds one (73 `text` blocks scanned, none with a command line), but a shell construct the splitter does not model — a heredoc, a pipeline whose parts are separate commands, a `for` loop — would be handed to `sh` whole as one `run` command, or need `shape`.
- Shape checkers exist for `python3 tools/*.py`, `cargo` and `pnpm`. Any other program must be declared `run`, because a `shape` declaration for an unknown program is refused; that is deliberate, and it means a new program in an example needs a checker before it can be illustrative.
- The tools that read `sys.argv` directly (`check_dossier.py`, `build_manifest.py`, `build_graph.py`, `test_dossier.py`) are shape-checked by looking for the flag literal in their source, which catches a removed or renamed flag but not a flag whose meaning changed. `graph.py` is checked by its real parser.
- The gate costs about two minutes, most of it the nested `test_dossier.py` run that `docs/74`'s example declares.
- `run` examples execute in a copy of the working tree, so they prove the commands work on the package as it stands; they are not a second opinion on the package's content, which `check_dossier.py` owns.
- The requirement is read here as covering the documentation's examples, which had no gate. The other examples the project evaluates itself with — `benchmarks/gate-calibration/corpora/{acceptance-holdout,risk-holdout}.json` — are already executed case by case against hidden oracles by EPR-019's qualification, so none of them is illustrative; nothing new was needed there and nothing was changed.

## Evidence

- `evidence.json` in this directory
- docs/74 "Documentation package checks" as built
