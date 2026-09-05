# Release Zero Expanded Proof — Clean-Slate V2

This supersets the original Release Zero scenario. Passing it does not prove every P1/P2 feature, but it proves that the product is a real integrated system rather than a set of panels and service stubs.

## Scenario

Using the packaged desktop application and a fixture software repository:

1. authenticate to the configured real staging model gateway;
2. open/trust the real Git repository;
3. start a coding task from Work view;
4. Context Planner retrieves current source and displays provenance in Context Inspector;
5. an applicable approved skill is selected from real Skill Registry;
6. the model uses the real task-scoped tool projection or Procedural Tool Runtime;
7. spawn one bounded child agent for a separable research/test subtask and continue parent work safely;
8. child result returns as typed `AgentResultEnvelope`;
9. stage and apply a real multi-file change through Change Engine into isolated worktree;
10. run real project tests in durable terminal; first run is intentionally failing and the agent repairs it without treating command failure as run failure;
11. read a real image or PDF artifact relevant to the task through Media Pipeline using a real eligible model/vision bridge path;
12. launch the changed app and attach the **same live Chromium session** used by the agent and the user-visible browser pane;
13. validate a UI behavior structurally; use targeted visual fallback only if the fixture requires it;
14. reach a protected browser/effect operation and create a real pending `DecisionRequest`;
15. terminate the desktop renderer while approval is pending, reopen and verify exact state from Core;
16. hard-kill/restart Core before resolving approval; recover pending protocol state from stores without transcript reconstruction;
17. approve or deny and verify effect receipt semantics; no duplicate effect after replay;
18. disconnect/reconnect another supervision client from an event offset and verify lossless replay;
19. complete verification: tests, Git diff, browser evidence, context provenance and effect receipts bound to exact revision;
20. produce final CompletionContract result **VERIFIED** only if all required evidence passes.

## Required fault variants

- stale async compaction result arrives after new turn: rejected;
- stale checkpoint epoch write races current checkpoint: rejected;
- duplicate subagent spawn request after retry: reattaches to same child;
- user edits a file before revert: optimistic revert refuses destructive overwrite;
- browser worker disconnects: session/recovery path is explicit and Core remains sound;
- terminal output exceeds model budget: OutputRef spill retains complete log;
- prompt injection in repo/web/media asks for forbidden tool: capability does not expand;
- cloud variant loses sandbox: task recovers from checkpoint or fails with exact recovery state, never fabricated success.

## Pass criteria

- zero mock/fake production effectors in the executed path;
- every effect is observable in canonical events/evidence;
- restart/resume reproduces exact pending state;
- no provider/browser/sandbox secrets appear in prompt/event/evidence bodies;
- final Git revision and evidence hashes are consistent;
- verifier cannot fail open;
- requirement qualification test IDs associated with exercised capabilities are marked PASS with run/evidence IDs.

## Execution policy release evidence

Preserve this scenario and add current-version DIRECT non-regression and enabled CASCADE/CRITIQUE proofs from doc 61. The release must reconstruct request → profile/envelope → plan/leg/attempt → actual tools/effects → revision-bound gate/risk/critic evidence → complete cost/outcome. Include actual Core kill/restart, budget exhaustion, denied canonical/external reviewer effects and previous-good policy rollback. Applicable EPR-GATE-A..G evidence accompanies the RC; documentation or disabled experimental code cannot support an enabled workflow claim.
