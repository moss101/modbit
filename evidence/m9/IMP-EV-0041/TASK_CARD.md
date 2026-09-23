# Task Card — IMP-EV-0041 Policy generation + hot revalidation

## Identity

- Task ID: IMP-EV-0041 (REQ-EV-0041, ADOPT; owner Policy Kernel)
- Milestone: M9
- Qualification: QUAL-EV-0041 — change org policy while a run is active; the current authorized tool finishes, the next forbidden tool is absent.
- Evidence tier: release-critical (permissions and policy, security boundary)

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL. The admin/project/user layers resolved into a `ResolvedConfig` (REQ-EV-0039, M9.4) and the Capability Kernel had a step for its per-capability `DENY`/`ASK` — but the Core passed `config: None` to the kernel, so the step never ran; the configuration was resolved once per task and pinned for the task's life, so a change to organization policy reached no running task; nothing recorded a policy change.
- first missing link: the kernel was never given the configuration, and nothing refreshed it between rounds.
- production entry points: `crates/policy/src/config.rs` (`generation`, `permission_changes`, `denied`); `services/modbit-core/src/config.rs` (`Configurations::refresh`: re-resolve, replace the snapshot, return the one replaced when the generation moved); `tools.rs` (the kernel port carries the task's snapshot, which only a round boundary refreshes; a client's call outside a round uses it too, as M9.4 pins a task's configuration until its next round); `runtime.rs` (refresh at every round boundary, `PolicyGenerationChanged`, `POLICY_DENIED` withholding in the projection and the deferred catalog); `crates/domain` (`TaskEvent::PolicyGenerationChanged`).

## Verification

- `qual_ev_0041_a_tightened_policy_applies_from_the_next_round_and_the_call_in_flight_finishes` (services/modbit-core, real Core, real terminal broker): the organization layer is rewritten to deny `shell.exec` while a three-second `shell.exec` the policy allowed is running; that call finishes and its output reaches the next round; the next round records one `PolicyGenerationChanged` (tightened `shell.exec`, withheld `shell.exec`, a new generation); the round's projection no longer offers `shell.exec` and lists it withheld `POLICY_DENIED`; the model's next `shell.exec` is refused `TOOL_NOT_PROJECTED` at the policy stage and no process runs; a client's direct `InvokeTool shell.exec` after the run is `POLICY_DENIED` / `CAPABILITY_DENIED_BY_CONFIG` under the snapshot the last round installed, and nothing runs. `qual_ev_0224_…` (M9.4) still holds: a task with no round keeps the configuration it started with.
- `crates/policy` unit `a_generation_names_what_the_configuration_decides_and_a_change_says_which_way`.
- Regression: the full modbit-core suite (M9.4 external-server configuration, M5.1 projection, M2.5 kernel), policy/domain/core-runtime suites.

## Limitations

- "Can pause the next round": a tightening takes effect at the next round by withholding and refusing what it denies; it does not suspend the run on its own. A call already approved but not yet dispatched is decided again under the new snapshot (it is not in flight).
- The device/MDM layer above the admin layer is IMP-EV-0040's.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- docs/23, docs/16 and docs/30 carry the as-built paragraphs
