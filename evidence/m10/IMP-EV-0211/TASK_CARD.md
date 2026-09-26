# Task Card — IMP-EV-0211 Multi-level tests including real API call

## Identity

- Task ID: IMP-EV-0211 (owner label Qualification Suite; subsystem verification)
- Milestone: M10 Release candidate hardening
- Requirements:
  - REQ-EV-0211: qualification includes direct component, registry/integration and real external API tests when the integration requires them.
  - QUAL-EV-0211: staging integration uses a real test credential and a recorded safe fixture; mock-only cannot pass.
  - docs/50: test pyramid, levels 2, 3 and 6.
  - docs/15: "Live provider proof".
  - docs/82: no placeholder production evidence.
  - docs/83: definition of done.
- Qualification: real-system.
  - The live level runs against the real model gateway with the repository's own provider credentials (DR-M9-002).
  - The recordings are that run's exchanges, replayed through the real adapter on every platform.
  - The integration level runs a real Core process.
  - The gate reads the real workspace, workflows, fixtures and retained records.
- Evidence tier: release-critical, because it touches evidence semantics — what counts as proof of an integration.
- No new canonical owner:
  - the gate is a quality tool under `tools/`, like `tools/example_runner.py` (IMP-EV-0212);
  - the recording and replay plumbing is test support in `crates/providers/tests/`;
  - no production code changed.

## Goal

Every integration the product has is declared, with a test at each level it needs. An integration with an external API cannot be qualified on fakes alone: it needs a live test that a workflow actually runs with the real credential, and a recording of that run replayed offline. A live test that skipped is not a pass.

## Existing-code audit

- Classification: **IMPLEMENTED-PARTIAL**, with one BROKEN-DRIFTED part.
- What existed:
  - The provider gateway had a component level: `crates/providers/tests/conformance.rs` against wire-faithful local servers.
  - It had a live test, `live_streaming_tool_round_trip_and_cancellation_against_production_endpoints`, run nightly by `.github/workflows/live-providers.yml`.
  - The OpenAI wire had an integration level through the Core (`m2_6_…`, `m2_7_…`).
- First missing links:
  1. **A skipped live test was indistinguishable from a pass.**
     - The live test returned early and `ok` without `MODBIT_LIVE_PROVIDERS=1`.
     - The workflow exited 0 with a notice when the secrets were absent.
     - Nothing recorded which of the two had happened. Mock-only could pass.
  2. **No recorded fixture of any real exchange existed.** Every offline stream was hand-written.
  3. **The Anthropic wire had no integration-level test.**
     - No Core test streamed over it.
     - `qual_ev_0096_…` only shows it blocked by policy.
  4. **The GitHub forge adapter had no component-level test.** Only the Core's `forge.*` suites called it.
  5. **Nothing listed the integrations**, so nothing could say which ones lacked a level.
- BROKEN-DRIFTED:
  - The scheduled live proof had failed every day since 2026-09-22 (runs 35705593051, 35838173277, 35975604142, 36115236181).
  - The cancellation step cancelled on the first `MessageDelta` only.
  - `glm-5.3-flash` now reasons first. It spent the whole 256-token budget counting inside its reasoning, sent `Completed(max_tokens)` and no answer text, so the cancellation never fired.
  - This was a test defect, not a product one: the gateway cancels at any chunk.

## Change

- `crates/providers/tests/support/mod.rs` (new):
  - **Result records:** every live test writes one to `MODBIT_LIVE_RESULTS_DIR`. It is `passed` with the integrations it proved, or `skipped` with the reason. `passed` is written only after every assertion, so a panic leaves no record.
  - **Recording proxy:**
    - The adapter talks plain HTTP to 127.0.0.1. The proxy forwards over TLS with the adapter's own headers and streams the answer back as it arrives.
    - It keeps the request body, a request-header allowlist (`accept`, `anthropic-version`, `content-type`) and the response chunks.
    - It never keeps `authorization`, `x-api-key` or our per-request id.
    - Chunks are kept as UTF-8 text; a character split across chunks moves whole into the next one.
  - **Replay server:**
    - It answers only the recorded requests, in order. Method, path and body must equal the recording.
    - Any other request gets a 409 naming the first difference as a JSON pointer.
  - **`safety_findings`:** credential headers, plus anything the product's one redactor (`modbit_secrets`) would replace — held values and credential shapes.
- `crates/providers/tests/recorded.rs` (new):
  - `live_recorded_tool_round_trip_on_each_configured_wire`:
    - Drives the gateway's tool round trip through the recording proxy against every configured real endpoint.
    - Refuses an unsafe recording.
    - Writes `<wire>.json` to `MODBIT_LIVE_RECORD_DIR` with provenance: run, host, base path, model, auth scheme, extra body, and the stream ids and models the adapter read.
    - Leaves a `passed` record carrying each recording's sha256.
  - `replay_recorded_{openai,anthropic}_wire_tool_round_trip`: the real adapter must reproduce the round trip from the committed recording — the same tool call, and the same stream ids and models.
  - `replay_refuses_a_request_the_provider_never_accepted`.
  - `recorded_fixtures_carry_provenance_and_no_credential`: the check has teeth — a leaked header, a key-shaped token and an echoed held value are each found.
- `crates/providers/tests/fixtures/recorded/{openai,anthropic}-wire.json` (new): the two recordings from live run 36196671564 (z.ai `glm-5.3-flash`, both protocols).
- `crates/providers/tests/conformance.rs`:
  - The live test leaves its result record, and model selection moved to `support::live_model`.
  - Cancellation fires on the first streamed token of either kind, with `max_output_tokens` 4096 for the counting request.
- `services/modbit-core/tests/surface_protocol.rs`: `qual_ev_0211_the_core_routes_and_streams_over_the_anthropic_wire` (new). A real Core with `MODBIT_ANTHROPIC_BASE_URL` and a key:
  - lists the Anthropic catalog;
  - streams a text probe and a typed tool call over `/v1/messages`;
  - presents the key only in `x-api-key`;
  - counts health;
  - after the Core is killed, no file in its data directory holds the key.
- `crates/tools/tests/forge.rs` (new): `forge_adapter_reads_github_issues_directly_with_the_token_in_its_header_only`. The adapter is called directly against a GitHub-shaped server:
  - the token travels only in `Authorization` and never comes back, even when the forge echoes it;
  - GitHub's 404 is typed `FORGE_NOT_FOUND`;
  - another host is refused `EGRESS_DENIED` before any byte is sent.
- `tools/integrations.json` (new): the inventory. Eight integrations over the seven workspace members whose production source opens an outbound connection.
- `tools/integration_gate.py` (new): the gate. Structural mode runs in the `dossier integrity` job, `--live DIR` is the live workflow's last step, and `--release` fails on deferrals.
- `.github/workflows/live-providers.yml`:
  - fails without the secrets;
  - sets both directories;
  - runs the gate on the run's records;
  - retains log, records and recordings as the `live-qualification` artifact.
- `.github/workflows/ci.yml`: the dossier job gains the gate as its fourth step.
- `tools/test_dossier.py`: `IntegrationGateTests`, 20 cases on minimal packages.
- docs/15 "Live provider proof" and docs/50 "Multi-level integration qualification": as built.

## Inventory (what the gate enforces today)

| Integration | Boundary | Component | Integration | External |
|---|---|---|---|---|
| `provider.openai-wire` | external API | 5 conformance tests | `m2_6`, `m2_7` through the Core | 2 live tests, recording replayed |
| `provider.anthropic-wire` | external API | 2 conformance tests | `qual_ev_0211_…` through the Core (new) | 2 live tests, recording replayed |
| `forge.github` | external API | `forge_adapter_…` (new) | `qual_px_006`, `qual_px_010` through the Core | deferred by DR-M6-002: no token or test repository |
| `cloud.object-store` | real service (S3-compatible; SeaweedFS in the `cloud` job) | — | `qual_m8_1` approvals/outputs, `qual_m8_7` handoff | none |
| `cloud.worker-link` | in-tree | — | `qual_ev_0024`, `qual_m8_2` | none |
| `sandbox.gateway-link` | in-tree | — | `qual_m8_3` reference backend, `qual_m8_5` | none |
| `sandbox.egress` | in-tree | — | `qual_m8_3` reference and Firecracker | none |
| `sandbox.browser-relay` | in-tree | — | `qual_m8_8` | none |

## Verification

- **Live, on the real gateway** (workflow_dispatch on this branch; both runs used the repository's own provider secrets):
  - **Run 36196671564:**
    - both wires proven: streaming, the tool round trip and cancellation, on `api.z.ai`, model `glm-5.3-flash`;
    - both wires recorded (`fs.read({"path":"README.md"})`, then `demo`);
    - both result records `passed`;
    - the gate step failed only because the recordings were not yet committed — the first run cannot find a fixture that it is itself creating.
  - **Second run:** see `evidence.json`. With the recordings committed, the live tests and the gate step (`--live`) are all green.
- **Offline:**
  - both recordings replay through the real adapter on every CI platform;
  - the refusal test and the safety test pass;
  - the new Anthropic Core test and the forge component test pass, and `m2_6_…` still passes;
  - `python3 tools/integration_gate.py`: 8 integrations, 7 members, all declared and qualified;
  - `--live` on the retained records of run 36196671564 passes;
  - `--release` fails, naming `forge.github`, which is correct.
- **Failure injection** (logs in this directory):
  1. `mutation-1-request-body.log` — the OpenAI adapter sends `prompt_cache_key` changed. The replay is refused at `/body/prompt_cache_key` and the adapter reports the 409 as `PROVIDER_REJECTED`.
  2. `mutation-2-stream-reading.log` — the Anthropic decoder reads the provider id from the wrong field. The replay fails on the stream ids: `["message","message"]` against the recorded `msg_…` ids.
  3. `mutation-3-skipped-live-run.log` — both live tests run without the live switch. `cargo test` reports `ok`, exit 0. `integration_gate.py --live` reports four failures, exit 1: mock-only cannot pass.
  4. `IntegrationGateTests` covers 20 cases. Each case fails, or passes where the test says it should, for exactly one cause:
     - an undeclared Rust crate, and an undeclared TypeScript `fetch`;
     - a stale claim;
     - a missing test, and a helper that is not a test;
     - an external API with no live level or no component level;
     - a deferral: accepted, not accepted, or with no Decision Record;
     - `--release` against a deferral;
     - a live test no workflow runs, and an external test not named `live_`;
     - a credential header in a fixture, and a fixture recorded outside CI;
     - a fixture no retained run wrote, and a fixture edited after recording;
     - a skipped record, no record, and a pass that proved another integration;
     - a real service without its integration level, or with an external one.

## Limitations

- **GitHub forge:** `forge.github`'s real-API level is deferred under DR-M6-002 until the owner supplies `MODBIT_GITHUB_TOKEN` and a test repository (docs/77 §5 item 2). The gate says so: `--release` fails on it today. That is the requirement working, not a gap to paper over. No live forge test was written, because it could not be run here, and an unrun live test is exactly what this task refuses to count.
- **Production endpoints:** the live level runs on a compatible gateway (z.ai, DR-M9-002). It does not discharge docs/15's "production provider endpoint" clause, which needs `api.openai.com` / `api.anthropic.com` credentials.
- **Recording authenticity:** the gate cannot prove cryptographically that a fixture came from a real run. It binds each fixture by sha256 to the retained result record of a named CI run, whose artifact GitHub keeps. A hand-edited fixture fails; a forged record would have to be forged too.
- **Chunking:** each recorded answer arrived from the gateway as a single network chunk, so the recordings do not exercise the SSE parser's frame splitting. The component suite does, and the replay server keeps whatever boundaries a future recording has.
- **Re-recording cost:** a deliberate change to a request body the adapter builds fails the replay until the live workflow re-records it. That is the point, and it costs one `workflow_dispatch`. The procedure is in docs/15.
- **Detection scope:** coverage is detected by source patterns for outbound clients (HTTP client crates, `object_store`, hyper clients, websocket client connects, `TcpStream::connect`; `fetch`, `http.request`, `net.request`, `WebSocket` in TypeScript). A connection made through a crate not on that list would not be seen. The list is in the gate and extends with the dependency policy (docs/36).

## Evidence

- `evidence.json` in this directory.
- `live-run-36196671564/`: the retained result records and the log of the recording run.
- `mutation-{1,2,3}-*.log`.
- docs/50 "Multi-level integration qualification (REQ-EV-0211)"; docs/15 "Live provider proof" as built.
