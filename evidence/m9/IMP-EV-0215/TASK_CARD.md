# Task Card — IMP-EV-0215 Tool credentials outside tool arguments (part of M9.3)

## Identity

- Task ID: IMP-EV-0215
- Milestone: M9 Engineering memory / effects / security hardening (P0/P1); part of M9.3 "protected-path/secret redaction/broker hardening".
- Requirements: REQ-EV-0215 (required/optional secrets resolved by the host, never exposed as model parameter values); QUAL-EV-0215 (schema inspection contains no API key field; secret redaction test passes); docs/23 "Secret broker".
- Qualification: the whole registered tool surface's input schemas, and the pipeline's secret-in-arguments guard.
- Evidence tier: real-system (registry + pipeline).

## Goal

No tool exposes a secret as a model parameter: a tool's input schema declares no secret-value field, and a credential in the Core's custody can never be carried through a tool call's arguments — the call is refused before any effect and the record names the field, not the value.

## Existing-code audit

- classification: PARTIAL before this task: the secret custody and injection were built in M7.7/M7.8 and PX-006 — the forge broker supplies the token (a credential in the arguments is refused, `crates/tools/src/forge.rs`), the browser fills a credential by handle (never the value, `crates/tools/src/browser.rs`), and the pipeline refuses any call whose arguments carry a custody secret (`carries_secret` → `SECRET_EXFILTRATION_BLOCKED`, before the write-ahead journal and before policy, `crates/tools/src/pipeline.rs`). What was missing was the QUAL-EV-0215 assertion over the whole surface: that no registered tool's input schema declares a secret-value field, and a direct proof that a custody secret cannot pass through arguments with the value never in the record.
- production entry points: `crates/tools/src/pipeline.rs` (`carries_secret`, the `SECRET_EXFILTRATION_BLOCKED` denial), the tool specs' `input_schema` (registry), `crates/tools/src/forge.rs` / `browser.rs` (host-resolved token / credential handle).
- proof: `crates/tools/tests/pipeline_and_direct.rs::qual_ev_0215_no_tool_schema_declares_a_secret_and_secrets_never_enter_arguments` — walks every registered tool's input schema (direct, forge, browser; recursively through nested properties and array items) and asserts no property is a secret-value field name (api_key, secret, password, private_key, access_key, *_token, …), while allowing credential handles and reference fields (the sanctioned mechanism) and counts like `token_budget`; then, with a custody secret set, a `change.apply` whose argument embeds the secret is refused `SECRET_EXFILTRATION_BLOCKED` before any effector, the record names the field (`content`) but never the value, and the same shape without the secret is not blocked.

## Limitations

- The check is over the built-in tool families; a skill or plugin that added a tool would be checked the same way at registration but is not exercised here.
- The secret-in-arguments guard matches a custody secret of at least 8 characters as a substring; a very short secret is not treated as one (by design, to avoid false positives).
- Output redaction of a secret echoed back by an external service is covered where it occurs (the forge redacts its token from responses; the browser proves the key never enters the transcript, M7.7); this task covers the argument path and the schema surface.

## Verification

- `crates/tools/tests/pipeline_and_direct.rs::qual_ev_0215_no_tool_schema_declares_a_secret_and_secrets_never_enter_arguments`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
