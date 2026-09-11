# Task Card — IMP-EV-0253 Do not force retrieval tool usage in treatment

## Identity

- Task ID: IMP-EV-0253
- Milestone: M3
- Requirement: REQ-EV-0253; owner label: Benchmark Method; subsystem: eval-bench
- Qualification: `QUAL-EV-0253` — benchmark prompts are identical except the available capability profile.
- Evidence tier: real-system or production-equivalent
- Decision Record: `docs/decisions/DR-M3-003-benchmark-and-conformance-live-model-halves.md` (the live-model half)

## Goal

Keep the benchmark's treatment unbiased: the agent chooses its tools naturally, so the prompts the variants see must be identical, the only permitted difference is the capability profile, and nothing in a prompt may tell the agent which tools to use.

## Existing-code audit

- classification: ABSENT before this task. Nothing checked what each variant was asked. The harness happened to send the same prompt, but nothing would have noticed a treatment prompt that steered the agent toward the machinery under test.
- production entry points:
  - `benchmarks/context-economics::prompt_parity` — compares the first prompt each variant saw, with the workspace's location scrubbed (every trial runs in its own copy of the repository, and where that copy lives is not an instruction). The system prompt and the request must be byte-identical; the only difference allowed is the tool list, reported as the capability difference; and any phrase that tells the agent which tools to use (`FORCING_PHRASES`) invalidates the comparison, whichever variant carries it.
  - `PairedReport::prompt_parity` — the verdict travels with the report.
  - The real benchmark through Core records what each variant saw from the model server's request bodies and asserts the verdict: identical system prompt, identical request, no forcing instruction, and — here — no capability difference either, because both variants had `context.pack` and the baseline was free to use it and chose not to.
- proof: on the real benchmark the verdict is `unbiased`; the unit tests show a treatment prompt that says "you must use the context.pack tool" flagged as steering with the variant named, and a different request recognized as a different task rather than a variant of the same one.

## Limitations

- Whether a model left to itself reaches for retrieval is the live-model half: on the wire-faithful local provider the agent's choices are its script's. DR-M3-003 defers that run to `.github/workflows/live-providers.yml`.
- The forcing-phrase list is a list. It catches the instructions a benchmark author would plausibly write; it is not a proof that no steering sentence exists, and the byte-identity check is what carries the weight.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0250_0252_0274_paired_context_economics_benchmark_publishes_savings_with_confidence` — the real benchmark, now with the parity verdict.
- `a_benchmark_is_unbiased_only_when_its_prompts_are_identical_and_force_nothing`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
