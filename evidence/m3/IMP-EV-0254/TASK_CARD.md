# Task Card — IMP-EV-0254 Modbit structural advantage hypothesis

## Identity

- Task ID: IMP-EV-0254
- Milestone: M3
- Requirement: REQ-EV-0254 (disposition EXPERIMENT); owner label: Context Eval; subsystem: eval-bench
- Qualification: `QUAL-EV-0254` — Profile A baseline, B hybrid, C structural with paired trials.
- Evidence tier: real-system or production-equivalent

## Goal

Test AST/symbol/call/dependency/Git/test signals over a hybrid-only baseline.

## Existing-code audit

- classification: PARTIAL before this task. M3.9 built the harness and the three profiles and compared their means; a mean cannot say where a profile won and where it lost, and nothing tested the hypothesis case by case.
- production entry point: `benchmarks/retrieval` (`compare`, `Measure`, `Comparison`), over the harness in `crates/retrieval/src/bench.rs`; the statistics are the shared ones in `benchmarks/context-economics`.
- proof: every case runs under both profiles on the same corpus at the same revision, and the comparison is per case: the baseline value, the treatment value and the delta for each, then wins, losses, ties, the mean and median delta and a deterministic bootstrap interval. It is set up to be able to say no, and on the first run it did: the structural profile lost a case where graph expansion filled the top K and pushed the answer out. That is a real defect, and it is fixed — a candidate the query itself produced now ranks above one that is only a neighbour of a seed, and the expansion keeps a small reserved tail so a caller still surfaces on a repository full of text matches.

## Finding

On this corpus — eight engineering questions about this repository, K = 5:

- Against profile A (exact only), the structural profile is clearly better on recall: +0.875 mean, seven wins, no losses, interval [0.625, 1.000].
- Against profile B (hybrid), it is not: recall is identical on all eight cases, precision is within noise, and it spends 2.25 more retrieval steps on average (three cases worse, interval [0.75, 4.50]). It tends to return a smaller top-K context (-219 tokens mean) but the interval crosses zero.

The hypothesis is therefore not supported on this corpus for retrieval quality: the structural signals earn their place over exact-only retrieval, not over hybrid retrieval. The escalation is bounded and never retrieves worse, which is the invariant the experiment now guards.

## Limitations

The corpus is this repository, so the cases are engineering questions about it and not a public benchmark; nothing here is evidence about other repositories. Eight cases and one revision give a wide interval. The measures are retrieval quality and cost, not task outcome: whether the structural profile helps an agent finish a task needs the competence benchmark (PX-020) and a live provider.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `the_structural_profile_is_compared_with_the_hybrid_baseline_case_by_case`
- `harness_measures_the_three_profiles_on_a_real_corpus_and_reports_them`
- `m3_7_retrieval_planner_starts_cheap_escalates_on_short_coverage_and_fuses_with_boosts`
- `a_paired_report_measures_the_difference_and_says_what_it_cannot`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json/log copies alongside
