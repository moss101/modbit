//! Shared sample messages for the Rust <-> TypeScript protocol round trip.
//!
//! Each sample is one message with deterministic contents, its canonical
//! binary encoding from Rust, and a plain JSON description of the field values
//! that the TypeScript test compares against after decoding.

use std::path::{Path, PathBuf};

use modbit_protocol::v1::*;
use prost::Message;
use serde_json::{Value, json};

/// A sample message.
pub struct Sample {
    /// File stem, e.g. `command_envelope`.
    pub name: &'static str,
    /// Fully qualified protobuf type name.
    pub type_name: &'static str,
    /// Rust binary encoding.
    pub bytes: Vec<u8>,
    /// Field values (bytes as lowercase hex, ids as hex, enums as names).
    pub expected: Value,
    decode: fn(&[u8]) -> anyhow::Result<Vec<u8>>,
}

impl Sample {
    /// Decode `bytes` as this sample's type and re-encode canonically.
    pub fn reencode(&self, bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
        (self.decode)(bytes)
    }
}

fn id(b: u8) -> Option<Id> {
    Some(Id { value: vec![b; 16] })
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn idhex(b: u8) -> String {
    hex(&[b; 16])
}

fn reencode<M: Message + Default>(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    Ok(M::decode(bytes)?.encode_to_vec())
}

/// Repository root (this crate lives in `tools/protocol-fixtures`).
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

/// Fixture directory for one side (`rust` or `ts`).
pub fn fixture_dir(side: &str) -> PathBuf {
    repo_root().join("tests/fixtures/protocol/v1").join(side)
}

/// Every sample, in a stable order.
pub fn samples() -> Vec<Sample> {
    let command = CommandEnvelope {
        command_id: id(0x11),
        tenant_id: id(0x22),
        user_id: id(0x33),
        session_id: id(0x44),
        aggregate_id: None,
        expected_generation: Some(42),
        command_type: "CreateTask".into(),
        schema_version: 1,
        payload: vec![0xde, 0xad, 0xbe, 0xef],
        issued_at: Some(prost_types::Timestamp {
            seconds: 1_757_289_600,
            nanos: 123_456_789,
        }),
    };
    let event = EventEnvelope {
        event_id: id(0x55),
        tenant_id: id(0x22),
        session_id: id(0x44),
        run_id: id(0x66),
        sequence: 9_007_199_254_740_993, // > 2^53: exercises 64-bit handling in TypeScript
        causation_id: None,
        event_type: "TaskCreated".into(),
        schema_version: 1,
        payload: vec![],
        recorded_at: Some(prost_types::Timestamp {
            seconds: 1_757_289_601,
            nanos: 0,
        }),
        aggregate_type: "task".into(),
        aggregate_id: id(0x77),
        task_id: id(0x77),
    };
    let request = ToolCallRequest {
        tool_call_id: id(0x77),
        tool_name: "fs.read".into(),
        tool_version: "1".into(),
        arguments_json: r#"{"path":"README.md"}"#.into(),
        capability_lease_id: id(0x88),
        execution_profile: "local_default".into(),
        expected_workspace_revision: Some(0),
        timeout_ms: 30_000,
        output_budget_bytes: 1 << 20,
    };
    let result = ToolCallResult {
        tool_call_id: id(0x77),
        status: ToolCallStatus::UnknownOutcome as i32,
        structured_output_json: None,
        stdout_ref: id(0x99),
        stderr_ref: None,
        produced_artifact_ids: vec![
            Id {
                value: vec![0xaa; 16],
            },
            Id {
                value: vec![0xbb; 16],
            },
        ],
        effect_receipt_ids: vec![],
        workspace_revision_after: Some(u64::MAX),
    };
    let read = OutputRefReadResponse {
        output_ref_id: id(0x99),
        checksum_sha256: (0u8..32).collect(),
        total_bytes: 1_048_576,
        content_type: "text/plain; charset=utf-8".into(),
        offset: 4096,
        data: "héllo\n".as_bytes().to_vec(),
    };
    // REQ-EPR-001: the routing state as a client reads it. It carries an
    // attempt whose provider reported nothing, so both languages have to agree
    // that unknown usage is a flag and not a zero.
    let routing = RoutingPlanView {
        plan_id: "direct:11111111111111111111111111111111".into(),
        schema_version: 2,
        routing_epoch: 0,
        lease_generation: 3,
        content_digest: "a".repeat(64),
        plan_ref: "b".repeat(64),
        total_budget_minor: 0,
        currency: "USD".into(),
        scale: 2,
        legacy_source: String::new(),
        path_label: "DIRECT".into(),
        slots: vec![RoutingSlotView {
            slot_id: "initial".into(),
            predecessor: String::new(),
            trigger: "INITIAL".into(),
            max_activations: 1,
            activations: 1,
            endpoint: "openai".into(),
            model: "gpt-5-mini".into(),
            role: "solver".into(),
            timeout_ms: 120_000,
            max_output_tokens: 4096,
            max_retries: 0,
            reserved_minor: 0,
        }],
        attempts: vec![
            RoutingAttemptView {
                slot_id: "initial".into(),
                attempt: 1,
                outcome: "SUCCEEDED".into(),
                usage_known: true,
                input_tokens: 9_007_199_254_740_993, // > 2^53: 64-bit in TypeScript
                output_tokens: 374,
                provider_request_id: "req_01HZ".into(),
            },
            RoutingAttemptView {
                slot_id: "initial".into(),
                attempt: 2,
                outcome: "CANCELLED".into(),
                usage_known: false,
                input_tokens: 0,
                output_tokens: 0,
                provider_request_id: String::new(),
            },
        ],
        not_claimed: vec!["unknown cost stays unknown".into()],
    };
    let hello = Hello {
        protocol_version: Some(modbit_protocol::PROTOCOL_VERSION),
        client_kind: ClientKind::Cli as i32,
        client_build: "0.0.0".into(),
        supported_command_types: vec!["CreateSession".into(), "CreateTask".into()],
    };
    vec![
        Sample {
            name: "command_envelope",
            type_name: "modbit.v1.CommandEnvelope",
            bytes: command.encode_to_vec(),
            expected: json!({
                "commandId": idhex(0x11), "tenantId": idhex(0x22), "userId": idhex(0x33),
                "sessionId": idhex(0x44), "aggregateId": null, "expectedGeneration": "42",
                "commandType": "CreateTask", "schemaVersion": 1, "payload": "deadbeef",
                "issuedAt": {"seconds": "1757289600", "nanos": 123456789}
            }),
            decode: reencode::<CommandEnvelope>,
        },
        Sample {
            name: "event_envelope",
            type_name: "modbit.v1.EventEnvelope",
            bytes: event.encode_to_vec(),
            expected: json!({
                "eventId": idhex(0x55), "tenantId": idhex(0x22), "sessionId": idhex(0x44),
                "runId": idhex(0x66), "sequence": "9007199254740993", "causationId": null,
                "eventType": "TaskCreated", "schemaVersion": 1, "payload": "",
                "recordedAt": {"seconds": "1757289601", "nanos": 0},
                "aggregateType": "task", "aggregateId": idhex(0x77), "taskId": idhex(0x77)
            }),
            decode: reencode::<EventEnvelope>,
        },
        Sample {
            name: "tool_call_request",
            type_name: "modbit.v1.ToolCallRequest",
            bytes: request.encode_to_vec(),
            expected: json!({
                "toolCallId": idhex(0x77), "toolName": "fs.read", "toolVersion": "1",
                "argumentsJson": "{\"path\":\"README.md\"}", "capabilityLeaseId": idhex(0x88),
                "executionProfile": "local_default", "expectedWorkspaceRevision": "0",
                "timeoutMs": "30000", "outputBudgetBytes": "1048576"
            }),
            decode: reencode::<ToolCallRequest>,
        },
        Sample {
            name: "tool_call_result",
            type_name: "modbit.v1.ToolCallResult",
            bytes: result.encode_to_vec(),
            expected: json!({
                "toolCallId": idhex(0x77), "status": "TOOL_CALL_STATUS_UNKNOWN_OUTCOME",
                "structuredOutputJson": null, "stdoutRef": idhex(0x99), "stderrRef": null,
                "producedArtifactIds": [idhex(0xaa), idhex(0xbb)], "effectReceiptIds": [],
                "workspaceRevisionAfter": "18446744073709551615"
            }),
            decode: reencode::<ToolCallResult>,
        },
        Sample {
            name: "output_ref_read_response",
            type_name: "modbit.v1.OutputRefReadResponse",
            bytes: read.encode_to_vec(),
            expected: json!({
                "outputRefId": idhex(0x99), "checksumSha256": hex(&(0u8..32).collect::<Vec<_>>()),
                "totalBytes": "1048576", "contentType": "text/plain; charset=utf-8",
                "offset": "4096", "data": hex("héllo\n".as_bytes())
            }),
            decode: reencode::<OutputRefReadResponse>,
        },
        Sample {
            name: "routing_plan_view",
            type_name: "modbit.v1.RoutingPlanView",
            bytes: routing.encode_to_vec(),
            expected: json!({
                "planId": "direct:11111111111111111111111111111111",
                "schemaVersion": 2, "routingEpoch": "0", "leaseGeneration": "3",
                "contentDigest": "a".repeat(64), "planRef": "b".repeat(64),
                "totalBudgetMinor": "0", "currency": "USD", "scale": 2,
                "legacySource": "", "pathLabel": "DIRECT",
                "slots": [{
                    "slotId": "initial", "predecessor": "", "trigger": "INITIAL",
                    "maxActivations": 1, "activations": 1, "endpoint": "openai",
                    "model": "gpt-5-mini", "role": "solver", "timeoutMs": "120000",
                    "maxOutputTokens": 4096, "maxRetries": 0, "reservedMinor": "0"
                }],
                "attempts": [
                    {
                        "slotId": "initial", "attempt": 1, "outcome": "SUCCEEDED",
                        "usageKnown": true, "inputTokens": "9007199254740993",
                        "outputTokens": "374", "providerRequestId": "req_01HZ"
                    },
                    {
                        "slotId": "initial", "attempt": 2, "outcome": "CANCELLED",
                        "usageKnown": false, "inputTokens": "0", "outputTokens": "0",
                        "providerRequestId": ""
                    }
                ],
                "notClaimed": ["unknown cost stays unknown"]
            }),
            decode: reencode::<RoutingPlanView>,
        },
        Sample {
            name: "hello",
            type_name: "modbit.v1.Hello",
            bytes: hello.encode_to_vec(),
            expected: json!({
                "protocolVersion": {"major": 1, "minor": 0}, "clientKind": "CLIENT_KIND_CLI",
                "clientBuild": "0.0.0", "supportedCommandTypes": ["CreateSession", "CreateTask"]
            }),
            decode: reencode::<Hello>,
        },
    ]
}
