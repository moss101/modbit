# Canonical Tool and Capability Inventory

> **LOCKED:** Modbit implements capabilities, not external tool-name parity. Every executable operation has one canonical owner and one policy/evidence path. Compatibility aliases live only in import/adaptation layers.

## Model-visible strategy

Most turns see a small task-scoped direct surface. Procedural mode may expose `exec`, `wait`, and `request_user_input` while an isolated composition runtime invokes authorized `tools.*` bindings. Discovery never grants authority. Reviewer legs of a conditional plan see a further restricted projection: read-only canonical access plus scratch writes and bounded processes inside the disposable review worktree; effectful families (`change.apply`, `git.merge.*`, `git.worktree.*` on canonical trees, `browser.action`, `computer.action`, `external.call`, `web.*`, `memory.propose`) are excluded and kernel-denied under the `review_isolated` profile (`16_TOOL_CAPABILITY_AND_PROCEDURAL_RUNTIME.md`, `21_TERMINAL_EXECUTION_AND_SANDBOX.md`, EPR-018). No new tool family is introduced for review. The `forge.*` family (DR-PX-2026-09-05, PX-006) is admitted under the new-tool rule below because a forge API is a distinct effector with protected external effects that existing tools plus a skill cannot express safely; thin clients never call it directly, they issue canonical commands that the Core executes through it (`29_CLIENT_SURFACES_AND_SOURCE_CONTROL_INTEGRATION.md`).

| Canonical tool/family | Class | Purpose | Protected effect? | Owner |
|---|---|---|---|---|
| user.request_input | Interaction | typed question/confirmation | No | Question Service |
| wait | Control | yield on durable condition/child/process event | No | Agent Runtime |
| plan.get / plan.update | WorkGraph | durable plan/todo state | plan mutation | WorkGraph |
| context.query / resolve | Context | retrieve/hydrate bounded context | No | Context Engine |
| fs.list / fs.glob / fs.read | Filesystem | authorized workspace enumeration/read/media | No | Workspace/Media |
| search.grep / search.symbol | Search | exact/regex/symbol/definition/reference search | No | Context Engine/Graph |
| change.propose / change.apply | Change | revision-bound staged edit transaction | Yes | Change Engine |
| git.status / diff / log / blame | Git | read repository lineage | No | Git Service |
| git.worktree.create / close | Git | isolated worktree lifecycle | Yes | Worktree Manager |
| git.merge.prepare / commit | Git | reviewed merge transaction | Yes | Change Engine |
| shell.exec | Terminal | structured argv/cwd/env execution | Maybe | Terminal Broker |
| shell.attach / input / cancel | Terminal | durable PTY/process control | Maybe | Terminal Broker |
| test.run | Verification | execute configured real tests | code execution | Verification Plane |
| diagnostics.pull | Verification | bounded diagnostics after settle/on demand | No | Diagnostics Adapter |
| browser.navigate / snapshot | Browser | navigate/read semantic browser state | navigation may be protected | Browser Runtime |
| browser.action | Browser | semantic click/fill/select/submit/etc. | Yes by effect | Browser Runtime |
| browser.network / console / capture | Browser | bounded evidence/targeted visual region | No | Browser Runtime |
| computer.observe / action | Computer | approved native app state/action | action Yes | Computer Runtime |
| agent.spawn | Agents | transactional child admission | capacity/capability | Agent Runtime |
| agent.steer / park / resume / cancel | Agents | durable lifecycle controls | by policy | Agent Runtime |
| agent.wait / result | Agents | await/read typed result envelope | No | Agent Runtime |
| skill.list / load | Skills | discover/load approved procedural knowledge | No authority grant | Skill Registry |
| external.list | External tools | discover eligible tools/resources | No | External Tool Hub |
| external.call / cancel | External tools | normalized invocation/cancellation | by effect | External Tool Hub |
| web.search / fetch | Web | retrieve allowed public content | network protected | Web Gateway |
| artifact.get / range | Artifacts | content-addressed result/evidence range | No | Artifact Store |
| memory.query | Memory | retrieve scoped curated engineering memory | No | Engineering Memory |
| memory.propose | Memory | propose governed durable memory | promotion separate | Engineering Memory |
| forge.issue.read / forge.pr.comments.read / forge.ci.status | Forge | read issues, review comments and CI results from an allowed forge (GitHub first) as untrusted, provenance-bound data | No (network lease) | External Tool Hub |
| forge.pr.create / forge.pr.update | Forge | open or update a pull request for a reviewed candidate revision | Yes | External Tool Hub with Change Engine |

## Capability lifecycle

`SUPPORTED → DISCOVERABLE → TASK_RELEVANT → ACTIVATED → AUTHORIZED → EXECUTED → EVIDENCED`.

A capability without a production consumer/transport/effector is removed before model exposure. A denied capability is not intentionally left visible merely to waste a model tool call.

## New-tool admission

A new first-party tool is accepted only when existing tools plus a skill cannot express the operation safely; it has typed intent/result/failure contracts; a real effector exists; policy can decide before effect; timeout/cancel/idempotency semantics exist; large results use refs; every effect emits evidence; at least one real E2E path exists; and schema/context cost is measured.

## Skill vs tool

Use a **skill** for procedural knowledge over existing governed operations. Add a **tool** only for a new precise effector/protocol/stream/transaction/host-enforced capability.
