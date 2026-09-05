# Fault Injection and Recovery Catalog

At minimum the release suite must exercise:

1. Core killed before event commit.
2. Core killed after event commit before response.
3. renderer disconnected during streaming.
4. provider stream interrupted mid-event.
5. tool transport drops after dispatch with outcome unknown.
6. approval persisted, Core killed before effect dispatch.
7. effect dispatched, response lost, retry arrives.
8. stale session/kernel lease attempts write.
9. stale checkpoint writer races newer epoch.
10. asynchronous compaction returns after fork/revert.
11. terminal process outlives client disconnect.
12. terminal broker killed with active PTY.
13. browser renderer/process crashes mid-task.
14. human takes browser/computer control during agent action.
15. browser page navigates between plan and action.
16. hostile web content attempts instruction/policy injection.
17. sandbox guest is killed/unreachable.
18. cloud worker loses lease/network.
19. object/artifact digest corruption.
20. database migration interrupted.
21. disk full during event/checkpoint/artifact write.
22. duplicate subagent spawn request after retry.
23. conflicting subagent edits race.
24. context index stale relative to active worktree.
25. MCP/external tool server disconnects during call.
26. secret broker token expires during protected operation.
27. cancellation races tool completion.
28. user edit races staged patch apply.
29. event replay cursor is too old/compacted.
30. clock/ordering skew between cloud control and worker events.

Each test specifies expected final state, allowed retry behavior, evidence, and whether recovery is automatic or requires user action. “No crash” is not sufficient; invariants must be checked.

## Execution policy fault extension

Existing FI-01..30 remain mandatory. EPR-FI-000..013 in doc 61 additionally cover profiler/registry loss, cyclic/invalid plans, missing gates/critics, inherited-budget exhaustion, provider stream/usage ambiguity, cancel/restart at leg boundaries, stale routing/compaction generations, critic privilege attempts, candidate apply races, replay leakage and incompatible policy rollback. Faults assert exact state/effect/budget invariants and retained evidence, not merely lack of crash.
