# PX-136 paired benchmark: DIRECT / CASCADE / CRITIQUE

direct: 25/30 verified (0.66-0.93), $0.0197 per run, 377 s per run; cascade: 27/30 verified (0.74-0.97), $0.0172 per run, 299 s per run; critique: 26/30 verified (0.70-0.95), $0.0231 per run, 472 s per run; cascade against direct over 30 pairs: quality NO_DIFFERENCE_DETECTED, cost INDETERMINATE; critique against direct over 30 pairs: quality NO_DIFFERENCE_DETECTED, cost INDETERMINATE; the default route stays DIRECT; nothing here attests a gate

- task set: `px136-paired-30` v1.0.0 (30 tasks, digest `6a093fbb51df`)
- opener: `openai/glm-5.3-flash`; second binding: `anthropic/glm-5.3-flash`
- registry: `paired-17989265` (digest `d93800aad98c`)
- live: true
- spend: $1.7992 of a $7.00 cap over 90 calls

| arm | runs | verified success (95% Wilson) | cost/run USD (95% CI) | accounting total/run USD | cost/verified success | latency s (95% CI) | escalation | review | needed a person |
|---|---|---|---|---|---|---|---|---|---|
| direct | 30 | 25/30 = 0.833 (0.664-0.927) | 0.0197 (0.0143-0.0261) | 0.0219 | 0.0237 | 377 (248-528) | 0/30 | 11/30 | 3/30 |
| cascade | 30 | 27/30 = 0.900 (0.744-0.965) | 0.0172 (0.0133-0.0212) | 0.0187 | 0.0191 | 299 (218-385) | 0/30 | 8/30 | 4/30 |
| critique | 30 | 26/30 = 0.867 (0.703-0.947) | 0.0231 (0.0196-0.0266) | 0.0256 | 0.0267 | 472 (376-577) | 0/30 | 28/30 | 3/30 |

## Against DIRECT (paired)

- cascade: 30 pairs; success delta +0.067 (-0.100 to +0.267) => quality NO_DIFFERENCE_DETECTED; cost delta -0.0026 USD (-0.0099 to +0.0041) => cost INDETERMINATE; latency delta -78 s
- critique: 30 pairs; success delta +0.033 (-0.133 to +0.200) => quality NO_DIFFERENCE_DETECTED; cost delta +0.0034 USD (-0.0035 to +0.0099) => cost INDETERMINATE; latency delta +95 s

## Gate errors against the independent check

- all: 93 observations; false accepts 0/4 (0.000-0.490); false rejects 6/89 (0.031-0.139); no verdict 8
- cascade: 33 observations; false accepts 0/1 (0.000-0.793); false rejects 5/32 (0.069-0.318); no verdict 2
- critique: 30 observations; false accepts 0/1 (0.000-0.793); false rejects 0/29 (0.000-0.117); no verdict 3
- direct: 30 observations; false accepts 0/2 (0.000-0.658); false rejects 1/28 (0.006-0.177); no verdict 3

## Outcome statistics samples by leg family (REQ-PX-133)

- solver: 0 observations over 0 runs, 0 verified, 0 with unreported cost, total priced cost 0 minor units; keys: 
- escalation: 0 observations over 0 runs, 0 verified, 0 with unreported cost, total priced cost 0 minor units; keys: 
- reviewer: 46 observations over 46 runs, 40 verified, 0 with unreported cost, total priced cost 5479 minor units; keys: reviewer|glm-5.3-flash

## Method

30 tasks x 1 repeats x 3 arms, each trial a fresh repository, a fresh profile and a fresh Core driven through modbit-cli under the same signed two-binding registry (openai/glm-5.3-flash opens, anthropic/glm-5.3-flash is the stronger-solver and reviewer binding). BOTH BINDINGS ARE THE SAME MODEL behind two endpoints, so CASCADE measures a second attempt on a second wire and CRITIQUE a review by the same model, not escalation to a stronger one. Verified success needs the Core's own completion gating, the task's pristine check.py passing on the final tree and check.py untouched. Cost is priced from the usage the gateway returned for every model invocation of the session (the review task's included) at the catalog list prices, no cache discount. Gate errors compare the gate's verdict at the end of a leg with the independent check of the same tree. Sub-agent spawning is forbidden on every arm (`--forbid-spawn`): a child's budget is clamped to the parent leg's small plan reservation and, in the first pilot, children exhausted it and dominated the cost. Wilson intervals for proportions, deterministic bootstrap for means and paired differences. The provider was live; the numbers are this model's on this small task set and say nothing about other models or tasks. A ceiling effect is likely on tasks this small.

Rule: For each of CASCADE and CRITIQUE against DIRECT, over the tasks both arms ran: QUALITY is BETTER when the 95% bootstrap interval of the paired verified-success difference lies entirely above zero, WORSE when it lies entirely below zero, NO_DIFFERENCE_DETECTED otherwise; COST is MORE when the interval of the paired total-cost difference (every leg priced from gateway usage) lies entirely above zero, LESS when entirely below, INDETERMINATE otherwise. The harness decides nothing about the default route: it stays DIRECT, and changing it is an EPR Decision Record after this evidence.

Report digest: `75f3734eff9f34b986f92abc3544d8e9a1e91ac86f20b4b3311806a7ce4262a4`
