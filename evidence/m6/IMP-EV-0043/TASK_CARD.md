# Task Card — IMP-EV-0043 ProtocolCapabilitySet

## Identity

- Task ID: IMP-EV-0043
- Milestone: M6 Subagents/fleet (P0→P1)
- Requirement: REQ-EV-0043; owner label: Capability Kernel; subsystem: protocol / core server
- Qualification: QUAL-EV-0043 — Headless client lacks UI-only capabilities while Core task remains valid.
- Evidence tier: real-system (the real Core with three client kinds on live connections, against a scripted OpenAI-compatible server)

## Goal

Separate client/transport capabilities from per-round execution authority.

## Existing-code audit

- classification: MISSING before: the hello carried a client kind the Core did not act on; every connected client could ask for every command, and only the task's leases and policy stood between a client and an effect.
- production entry points: `crates/protocol/proto/modbit/v1/negotiation.proto` (`HelloAck.client_capabilities`), `crates/protocol/src/client.rs` (`Client.capabilities`), `services/modbit-core/src/server.rs` (`client_capabilities` by kind, `required_client_capability` per command, `CLIENT_CAPABILITY` refusal in the connection loop before `handle_command`).
- proof: a CLI client's negotiated set holds `task.author`, `events.subscribe`, the human decisions and provider/repository setup but neither `ui.selection` nor `ui.code_view`: its editor-source selection and its code-view request are refused `CLIENT_CAPABILITY` naming the capability and the kind, while its `source: cli` selection is recorded; a desktop client records an editor selection on the same task; a sandbox guest (events only) can neither start the task nor resolve an approval, but reads the same snapshot; the task the CLI authored starts and reaches ReadyForReview — its authority is its own.

## Limitations

- The set is fixed per connection from the client kind; a per-client negotiation of finer capabilities (an IDE without a code view) is not modelled.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0043_a_headless_client_lacks_ui_only_capabilities_while_the_task_stays_valid`
- `qual_ev_0141_0160_a_selection_steers_retrieval_is_visible_and_grants_no_write`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
