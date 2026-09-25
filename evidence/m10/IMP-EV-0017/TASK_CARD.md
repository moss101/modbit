# Task Card — IMP-EV-0017 Dual user/model error channels

## Identity

- Task ID: IMP-EV-0017 (REQ-EV-0017, ADAPT; owner Error Service, subsystem observability; the redactor is effects-security's `crates/secrets`)
- Milestone: M10 (wave 1)
- Qualification: QUAL-EV-0017 — a secret-bearing internal error is redacted for both surfaces according to policy.
- Evidence tier: release-critical (a security boundary: secrets on the person's, the model's and the log's surfaces; evidence semantics)
- No second redactor: the five that existed (provider gateway word filter, forge token replace, external-tool result scrub, Cloud API shape list, and the Core's lack of one on error text) become one implementation in `modbit_secrets`, which each former site now calls.

## Existing-code audit

- classification: IMPLEMENTED-PARTIAL. `FailureDiagnostic` (REQ-EV-0073) was the one failure identity, but `render()` was a single text for "a model or a person" with no user explanation and no structured repair payload. Redaction was heuristic and scattered: the gateway split on spaces and missed a quoted `"sk-…"` and a bearer after a colon; it truncated before redacting; a provider's in-stream error went out unredacted; an external server's *error* (as opposed to its result) was never scrubbed; `TaskNeedsAttention.reason`, `FailureDiagnostic.detail`, the model's `INFRA_FAILURE` text and every `CommandAck.error_message` carried whatever the source said; `crates/secrets` was an empty scaffold.
- first missing link: nothing checked error text against the Core's held secrets before it reached the log, a person or the model.
- production entry points: `crates/secrets/src/redact.rs` (`Redactor`: `data`, `error`, `data_json`, `error_json`, `event_payload`; `shape_of`, `error_text`); `crates/domain/src/failure.rs` (`user_explanation`, `model_repair`, `render` with the `repair:` line); `crates/core-runtime/src/diagnostics.rs` (`bounded` shape-redacts `detail`); `crates/providers/src/gateway.rs` (one redactor per attempt over the endpoint's own key: transport errors, the HTTP body — redacted whole, then bounded — and in-stream errors); `crates/tools/src/forge.rs` (bodies lose the token as data; errors shape-redacted); `services/modbit-core/src/mcp.rs` (`redact` over the shared redactor, `redact_error` on every server failure and health message); `services/modbit-core/src/tools.rs` (`redactor()` over custody; every tool result redacted before it is stored or read; `SECRET_IN_TOOL_RESULT`); `crates/event-store/src/store.rs` (`PayloadFilter`, applied to every append before hashing); `services/modbit-core/src/server.rs` (the filter installed at boot; rejections redacted at the transport; `TaskStatus.attention_reason` redacted as read; `TaskStatus.user_explanation`); `services/modbit-core/src/runtime.rs` (`INFRA_FAILURE` text redacted); `apps/cloud-api/src/routes.rs` (`secret_shaped` is `modbit_secrets::shape_of`).

## Verification

- `qual_ev_0017_a_secret_bearing_internal_error_is_redacted_for_the_person_and_the_model` (real Core, real MCP test server over stdio, scripted OpenAI-compatible provider):
  - the model's side: a host-declared server holding a brokered credential fails `refuse` with a JSON-RPC error repeating the credential (as a token and as a bearer); the agent runs through the Core to `ReadyForReview`; no request the model received contains the credential; the tool result it read keeps the server's words (`token [redacted] is not valid`) under `failure_class`/`retryable` and a `repair:` JSON payload with class, code, retryability and recovery path; the task's log holds no credential; after the Core stops, no file in its data directory does.
  - the person's side: a provider answers 401 with the presented key echoed quoted and as a bearer; the task needs attention typed `PROVIDER`/`AUTH_REJECTED`; `attention_reason` keeps the provider's words with `[redacted]` and no key; `user_explanation` reads "The model provider failed or refused the request (AUTH_REJECTED). …" with no key; `ProbeModel` of the same model reports the same error without the key; an `ImportAgentConfig` rejection that repeats a pasted path holding the key leaves as `/nonexistent/[redacted]/agents`; the `TaskNeedsAttention` diagnosis keeps its `detail` as redacted evidence; no file in the data directory holds the key.
- Mutations, each run against the test (each fails at the named assertion):
  - M1: no redaction on the external server's error nor at the Core's tool-result boundary → "the credential never reaches the model".
  - M2: no redaction in the gateway → the probe reports the key (the persisted reason is still covered by the store filter).
  - M3/M4: gateway and read-time redaction off, store filter on → the reason is still redacted (fails later, at the probe); filter off too → "the reason keeps the provider's words, not its key".
  - M5: rejections not redacted at the transport → the import rejection repeats the key.
- Unit: `modbit_secrets::redact::tests::*` (held values replaced wherever they appear including quoted; longest first; every credential shape replaced in error text and kept in data; an event payload loses held values everywhere and shapes only under error keys; a sealed receipt is untouched), `modbit_domain::failure::tests::one_identity_renders_for_a_person_and_for_a_model`.

## Limitations

- Stdout/stderr objects a process spills are stored as the process wrote them; what the model pages from them (`artifact.range`) passes the result boundary and is redacted there, but the stored object is not rewritten.
- No original is kept: docs/23's protected diagnostic vault does not exist, so a redacted value is gone from the record.
- Shapes are a fixed list (provider, forge, worker, bearer, AWS, Google, Slack, credential parameters, PEM keys); a secret of another shape that the Core does not hold is not recognised.
- Events already in a log written before this change are not rewritten; `attention_reason` and `user_explanation` are redacted as read, other historical payloads are not.
- An effect receipt is sealed by its own hash and is never rewritten by the filter; receipts carry ids and hashes, not free text.

## Evidence

- `evidence.json` in this directory
- docs/23 "Secrets", docs/30 "Security/effects", docs/34 "Logs" as built
