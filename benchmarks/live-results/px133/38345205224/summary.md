# PX-136 paired benchmark: DIRECT / CASCADE / CRITIQUE

cascade: 27/30 verified (0.74-0.97), $0.0183 per run, 338 s per run; critique: 24/30 verified (0.63-0.90), $0.0188 per run, 400 s per run; the default route stays DIRECT; nothing here attests a gate

- task set: `px136-paired-30` v1.0.0 (30 tasks, digest `6a093fbb51df`)
- opener: `openai/glm-5.3-flash`; second binding: `anthropic/glm-5.3-flash`
- registry: `paired-2c06678f` (digest `58a4b4a8cf9b`)
- live: true
- spend: $1.1144 of a $4.00 cap over 60 calls

| arm | runs | verified success (95% Wilson) | cost/run USD (95% CI) | accounting total/run USD | cost/verified success | latency s (95% CI) | escalation | review | needed a person |
|---|---|---|---|---|---|---|---|---|---|
| cascade | 30 | 27/30 = 0.900 (0.744-0.965) | 0.0183 (0.0132-0.0240) | 0.0213 | 0.0204 | 338 (223-473) | 0/30 | 7/30 | 6/30 |
| critique | 30 | 24/30 = 0.800 (0.627-0.905) | 0.0188 (0.0151-0.0223) | 0.0213 | 0.0235 | 400 (303-513) | 0/30 | 25/30 | 5/30 |

## Against DIRECT (paired)


## Gate errors against the independent check

- all: 64 observations; false accepts 0/8 (0.000-0.324); false rejects 1/56 (0.003-0.094); no verdict 12
- cascade: 34 observations; false accepts 0/5 (0.000-0.434); false rejects 1/29 (0.006-0.172); no verdict 7
- critique: 30 observations; false accepts 0/3 (0.000-0.562); false rejects 0/27 (0.000-0.125); no verdict 5

## Outcome statistics samples by leg family (REQ-PX-133)

- solver: 0 observations over 0 runs, 0 verified, 0 with unreported cost, total priced cost 0 minor units; keys: 
- escalation: 0 observations over 0 runs, 0 verified, 0 with unreported cost, total priced cost 0 minor units; keys: 
- reviewer: 32 observations over 32 runs, 26 verified, 0 with unreported cost, total priced cost 3347 minor units; keys: reviewer|glm-5.3-flash

## Method

30 tasks x 1 repeats x 2 arms, each trial a fresh repository, a fresh profile and a fresh Core driven through modbit-cli under the same signed two-binding registry (openai/glm-5.3-flash opens, anthropic/glm-5.3-flash is the stronger-solver and reviewer binding). BOTH BINDINGS ARE THE SAME MODEL behind two endpoints, so CASCADE measures a second attempt on a second wire and CRITIQUE a review by the same model, not escalation to a stronger one. Verified success needs the Core's own completion gating, the task's pristine check.py passing on the final tree and check.py untouched. Cost is priced from the usage the gateway returned for every model invocation of the session (the review task's included) at the catalog list prices, no cache discount. Gate errors compare the gate's verdict at the end of a leg with the independent check of the same tree. Sub-agent spawning is forbidden on every arm (`--forbid-spawn`): a child's budget is clamped to the parent leg's small plan reservation and, in the first pilot, children exhausted it and dominated the cost. Wilson intervals for proportions, deterministic bootstrap for means and paired differences. The provider was live; the numbers are this model's on this small task set and say nothing about other models or tasks. A ceiling effect is likely on tasks this small.

Rule: For each of CASCADE and CRITIQUE against DIRECT, over the tasks both arms ran: QUALITY is BETTER when the 95% bootstrap interval of the paired verified-success difference lies entirely above zero, WORSE when it lies entirely below zero, NO_DIFFERENCE_DETECTED otherwise; COST is MORE when the interval of the paired total-cost difference (every leg priced from gateway usage) lies entirely above zero, LESS when entirely below, INDETERMINATE otherwise. The harness decides nothing about the default route: it stays DIRECT, and changing it is an EPR Decision Record after this evidence.

Report digest: `a8a59e272ab3b2bc0ae9d755e720611d69000150523c2cde86cb179767509cf3`

## INCOMPLETE: missing measurements

- flatten-nested#0/cascade: the cost $0.005562 priced from usage does not reconcile with the Core's accounting $0.006500 (of which $0.000000 held for unknown usage): a leg's cost is missing or double counted
- parse-duration#0/cascade: the cost $0.008179 priced from usage does not reconcile with the Core's accounting $0.009100 (of which $0.000000 held for unknown usage): a leg's cost is missing or double counted
- 0 escalation samples, 1 required
