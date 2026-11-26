# PX-137 seal (E2E_PROVEN → COMPLETE)

## Acceptance, QUAL-PX-137, and where each part is shown

- At least fifty cases, scored by the oracle in both profiles on one model: 77 cases (ripgrep 14.1.1, hand-labelled), baseline and treatment both on `gemini-3.8-flash` through the product's provider gateway. Live run 45607051313, `benchmarks/live-results/px137/45607051313/RUN.md`.
- The report gives accuracy, input tokens, tool calls and time per profile, with intervals and the baseline column: `summary.md` and `result.json` of the same run.
- Cold and incremental index times at 100 MB are recorded: index of 101 MB, cold start 403 s, incremental refresh of one file 13.2 ms (RUN.md).
- Rescoring the retained transcripts reproduces the report digest: test `live_agent.rs::rescoring_retained_transcripts_reproduces_the_digest_and_catches_any_edit`. Digest `4ada0e96…f824` is reproducible from the run's `live-evals-px137` artifact.
- A profile whose binary is missing fails the run: test `external_baseline.rs::a_missing_ripgrep_fails_the_run_instead_of_leaving_the_column_out`.

## CI and merge

- PR 253 (fusion top-1 fix) merged to main as c605c2e3.
- PR 255 (bench client replays `provider_data`, so Gemini's `thought_signature` survives; live results; graph E2E_PROVEN; px-203 E2E wait fix): CI run 45796403901 on head 95d9d786, conclusion success, 19 of 19 jobs green (`ci-run-45796403901.json`). Merged to main as d1e10f28 (squash), 2026-11-26.

## Findings that stay notes, not acceptance criteria

- Accuracy: treatment minus baseline was -0.013 [-0.052, 0.026], so no measurable difference. Input tokens -4914 per case [-8396, -1731] and tool calls -0.47 [-0.86, -0.10] are both significantly lower with retrieve.
- The model chose `retrieve` in only 10 of 77 cases. That makes this a weak test of the ranking itself.
- The run cannot be compared with October's run 38044424486 (`glm-5.3-flash`, different client).
- A rerun on this model needs a spend cap of at least 5 USD.
- No release gate is attested.
