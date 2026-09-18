# Task Card — IMP-EV-0224 MCP config conversational management (M9.4)

## Identity

- Task ID: IMP-EV-0224 (part of milestone task M9.4 "external MCP gateway")
- Milestone: M9 Engineering memory / effects / security hardening (P0/P1)
- Requirements: REQ-EV-0224 (UI/agent may propose config changes but the host validates/trusts/authorizes); QUAL-EV-0224 (a proposed MCP install cannot execute until trust/credential gates pass); docs/16 "MCP / external tools"; docs/30 protocol additions.
- Qualification: the real Core, real configuration files and a real MCP server binary that must never be started until it is trusted.
- Evidence tier: real-system.

## Goal

Let a person (or an agent through a person's client) add an external tool server in conversation, without ever letting a proposal be an installation.

## Existing-code audit

- classification: ABSENT before this task. A server's configuration reached the Core only from files on disk (IMP-EV-0128) and a boot environment variable; there was no way for a client or an agent to propose one, no trust transition, and no way to hand the Core a credential for an external server at run time. The `Trust::Proposed` state and the hub's refusal to start an untrusted server existed from IMP-EV-0104 and are what a proposal lands in.
- production entry points: `crates/protocol/proto/modbit/v1/surface.proto` (`ProposeExternalServer`, `TrustExternalServer`, `ConfigureExternalCredential`, `ExternalServerConfigured`, `ExternalCredentialConfigured`); `services/modbit-core/src/server.rs` (the three handlers, their client capabilities and the supported-command list); `services/modbit-core/src/mcp.rs` (`propose`, `set_trust`, `stop_named`, `has_credential`, `clear_credential`); `services/modbit-core/src/config.rs` (`put_user_server` / `user_server` / `denied_above_user` — the user layer written whole through a temporary file, preserving every other key including ones this build does not know).

## What the gates are

- **Proposing is not installing.** A proposal is stored `PROPOSED` whatever the definition says, in the user configuration layer — the same file the resolver reads and a person edits, so there is no second store to disagree with the configuration. It is `UNTRUSTED` in the listing with no tools, `EXTERNAL_SERVER_UNTRUSTED` on a call, and its process is never spawned.
- **The host validates.** A definition that does not validate is refused `BAD_EXTERNAL_SERVER`; a name a higher configuration layer denied is refused `EXTERNAL_SERVER_DENIED` at propose time rather than silently at resolve time.
- **The credential gate.** Trusting a server whose definition names a credential the Core does not hold is refused `EXTERNAL_CREDENTIAL_UNAVAILABLE`. `ConfigureExternalCredential` puts the value in the Core's memory only — not journaled, never returned — and then the trust succeeds.
- **The host authorizes.** Proposing needs `task.author`; trusting needs `repository.trust` — letting a program the host did not write run against this workspace is the same class of decision as trusting the repository. A sandbox guest holds neither.
- **Trust is revocable and stops the program.** `TrustExternalServer { trust: false }` returns the server to `PROPOSED` and drops every pooled transport of that name, so the child process ends rather than merely becoming unreachable.
- **A task keeps the configuration it started with.** The task that saw the proposal still sees it untrusted after the trust; the next task sees the server ready.

## Limitations

- An agent cannot call these commands: it proposes by asking the person, whose client sends the command. Adding an `external.propose` tool would put a fourth name in a namespace docs/17 defines as `external.list / call / cancel`, so it is deliberately not done.
- Proposals live in the user layer only. A project- or admin-layer server is edited in its own file by whoever owns it.
- The three commands are not journaled; the configuration file is the durable record, and `ExternalServerConfigured` is the answer rather than an event. A task-scoped audit trail of who proposed what would need an event, which this task does not add.

## Verification

- `services/modbit-core/tests/surface_protocol.rs::qual_ev_0224_a_proposed_external_server_is_inert_until_the_host_trusts_it_and_holds_its_credential`
- `services/modbit-core` `config::proposal_tests::a_proposal_is_written_into_the_user_layer_without_disturbing_the_rest_of_it`, `::a_file_that_is_not_json_is_never_overwritten`, `::a_name_a_higher_layer_denied_is_known_before_it_is_proposed`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
