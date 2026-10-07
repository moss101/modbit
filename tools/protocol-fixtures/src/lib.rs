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
    let ts_row = |ordinal: u32,
                  row_id: &str,
                  kind: TranscriptRowKind,
                  text: &str,
                  children: Vec<TranscriptRow>| TranscriptRow {
        ordinal,
        row_id: row_id.into(),
        kind: kind as i32,
        offset: 50 + u64::from(ordinal),
        last_offset: 60 + u64::from(ordinal),
        at: Some(prost_types::Timestamp {
            seconds: 1_700_000_000 + i64::from(ordinal),
            nanos: 0,
        }),
        turn_id: "turn-1".into(),
        hints: Some(RowHints {
            renderable: true,
            groupable: kind == TranscriptRowKind::ToolCard,
            has_reasoning: false,
            duration_ms: 250,
            short_text: text.into(),
            lines_added: 0,
            lines_removed: 0,
            status: "COMPLETE".into(),
        }),
        text: text.into(),
        text_truncated: false,
        text_ref: String::new(),
        children,
        facts: None,
    };
    let transcript_page = TranscriptPage {
        task_id: id(0x21),
        density: TranscriptDensity::Balanced as i32,
        rows: vec![
            ts_row(
                1,
                "unread",
                TranscriptRowKind::UnreadDivider,
                "2 new",
                vec![],
            ),
            ts_row(
                2,
                "group:explore:tool:a",
                TranscriptRowKind::WorkGroup,
                "Explored 2 items",
                vec![
                    ts_row(
                        1,
                        "tool:a",
                        TranscriptRowKind::ToolCard,
                        "fs.read a.txt",
                        vec![],
                    ),
                    ts_row(
                        2,
                        "tool:b",
                        TranscriptRowKind::ToolCard,
                        "fs.read b.txt",
                        vec![],
                    ),
                ],
            ),
        ],
        total_rows: 2,
        next_after_row: 0,
        has_more: false,
        as_of_offset: 90,
        last_offset: 90,
        task_state: "Running".into(),
        read_offset: 40,
        events_read: 12,
    };
    let row_json = |ordinal: u32,
                    row_id: &str,
                    kind: &str,
                    text: &str,
                    group: bool,
                    children: Vec<Value>| {
        json!({
            "ordinal": ordinal, "rowId": row_id, "kind": kind,
            "offset": (50 + u64::from(ordinal)).to_string(),
            "lastOffset": (60 + u64::from(ordinal)).to_string(),
            "at": {"seconds": (1_700_000_000 + i64::from(ordinal)).to_string(), "nanos": 0},
            "turnId": "turn-1",
            "hints": {
                "renderable": true, "groupable": kind == "TRANSCRIPT_ROW_KIND_TOOL_CARD" && !group,
                "hasReasoning": false, "durationMs": "250", "shortText": text,
                "linesAdded": 0, "linesRemoved": 0, "status": "COMPLETE"
            },
            "text": text, "textTruncated": false, "textRef": "", "children": children,
            "user": null, "stream": null, "tool": null, "approval": null,
            "group": null, "footer": null, "tail": null, "boundary": null
        })
    };
    let agent_headers = AgentHeaders {
        session_id: id(0x10),
        headers: vec![AgentHeader {
            task_id: id(0x21),
            session_id: id(0x10),
            workspace_root: "/repo".into(),
            title: "make the check pass".into(),
            subtitle: "repo".into(),
            created_at: Some(prost_types::Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            }),
            updated_at: Some(prost_types::Timestamp {
                seconds: 1_700_000_100,
                nanos: 500_000_000,
            }),
            status_class: AgentStatusClass::ReadyForReviewUnseen as i32,
            status_label: "Ready for review".into(),
            unread: true,
            pending_approval: false,
            pending_plan: false,
            context_percent: 37,
            files_changed: 2,
            lines_added: 14,
            lines_removed: 3,
            last_checkpoint_at: None,
            subagent: false,
            archived: false,
            execution_location: "local".into(),
            origin: "cli".into(),
            task_state: "ReadyForReview".into(),
            last_offset: 90,
            read_offset: 40,
            attention_items: 0,
        }],
        last_offset: 90,
        events_read: 0,
        objects_read: 0,
    };

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
        admission: Some(RoutingAdmissionView {
            admitted: true,
            plan_id: "direct:11111111111111111111111111111111".into(),
            validation_digest: "c".repeat(64),
            reserved_minor: 0,
            currency: "USD".into(),
            scale: 2,
            routing_epoch: 0,
            refusal_code: String::new(),
            refusal_detail: String::new(),
            activations: vec![RoutingActivationView {
                slot_id: "initial".into(),
                activation: 1,
                reserved_minor: 0,
            }],
            feasibility: "QUALITY_FLOOR_UNKNOWN".into(),
            quality_lcb_bp: 0,
            stats_version: "none".into(),
            thresholds_version: "none".into(),
            target_met: false,
        }),
    };
    let session_tree = SessionTreeView {
        session_id: id(0x10),
        branch_generation: 1,
        tasks: vec![
            SessionTreeNode {
                task_id: id(0x11),
                goal_text: "make the check pass".into(),
                state: "Waiting(Approval)".into(),
                origin: "cli".into(),
                workspace_root: "/repo".into(),
                forked_from_task: None,
                forked_from_checkpoint: String::new(),
                forked_from_epoch: 0,
                forked_from_offset: 0,
                capsule_ref: String::new(),
                branch_generation: 0,
                runs: vec![TaskRunView {
                    run_id: id(0x12),
                    state: "Suspended".into(),
                }],
                checkpoints: vec![CheckpointView {
                    checkpoint_id: "01a09072-1262-70f2-b103-63d3e8f0feda".into(),
                    epoch: 1,
                    kind: "BASELINE".into(),
                    base_checkpoint_id: String::new(),
                    status: "CURRENT".into(),
                    workspace_revision: 3,
                    manifest_ref: "a".repeat(64),
                    integrity_hash: "b".repeat(64),
                    git_head: "c".repeat(40),
                    files: 1,
                    removed: 0,
                    event_offset: 120,
                    index_generation: 2,
                    reason: "before the fork".into(),
                    created_at_ms: 1_700_000_000_000,
                    committed_at_ms: 1_700_000_000_500,
                }],
                restores: vec![RestoreView {
                    checkpoint_id: "01a09072-1262-70f2-b103-63d3e8f0feda".into(),
                    epoch: 1,
                    offset: 140,
                    files_written: 1,
                    files_reverted: 1,
                    preconditions_checked: 2,
                }],
            },
            SessionTreeNode {
                task_id: id(0x13),
                goal_text: "make the check pass".into(),
                state: "Queued".into(),
                origin: "fork".into(),
                workspace_root: "/profile/worktrees/13".into(),
                forked_from_task: id(0x11),
                forked_from_checkpoint: "01a09072-1262-70f2-b103-63d3e8f0feda".into(),
                forked_from_epoch: 1,
                forked_from_offset: 120,
                capsule_ref: "d".repeat(64),
                branch_generation: 1,
                runs: vec![],
                checkpoints: vec![],
                restores: vec![],
            },
        ],
        branches: vec![BranchEventView {
            branch_generation: 1,
            kind: "fork".into(),
            reason: "task 13 forked from task 11 at checkpoint 01a09072 (epoch 1)".into(),
            offset: 141,
        }],
    };
    let protocol_state = ProtocolStateView {
        task_id: id(0x22),
        version: "protocol-state-1".into(),
        boundary: "AWAITING_APPROVAL".into(),
        calls: vec![
            PendingCallView {
                tool_call_id: id(0x33),
                tool_name: "git.worktree.close".into(),
                effect_class: "Destructive".into(),
                phase: "AWAITING_APPROVAL".into(),
                arguments_hash: "d".repeat(64),
                call_id: "call_1_0".into(),
                run_id: id(0x44),
                approval_id: idhex(0x55),
                reason: String::new(),
            },
            PendingCallView {
                tool_call_id: id(0x66),
                tool_name: "shell.exec".into(),
                effect_class: "ReversibleWrite".into(),
                phase: "UNKNOWN_OUTCOME".into(),
                arguments_hash: "e".repeat(64),
                call_id: String::new(),
                run_id: None,
                approval_id: String::new(),
                reason: "core restarted (boot generation 7) after the call was dispatched and before its result was acknowledged".into(),
            },
        ],
        approvals: vec![PendingApprovalView {
            approval_id: id(0x55),
            tool_call_id: id(0x33),
            tool_name: "git.worktree.close".into(),
            effect_class: "Destructive".into(),
            intent_hash: "d".repeat(64),
            expires_at: 1_700_000_000_000,
            expired: false,
        }],
        question_id: String::new(),
        active_leases: 1,
        digest: "f".repeat(64),
        terminals: vec![TerminalCursorView {
            handle_id: "0011223344556677".into(),
            request_id: "bg-1".into(),
            argv: vec!["sh".into(), "-c".into(), "tail -f log".into()],
            replay_generation: 3,
            last_acknowledged_cursor: 4096,
            running: true,
            output_ref: String::new(),
            exit_code: 0,
            exit_known: false,
            tool_call_id: "01a08f33-0d97-7d40-b299-f8a7bc5cda60".into(),
        }],
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
            // PX-043: the registry row a client lists (terminal.proto).
            name: "terminal_view",
            type_name: "modbit.v1.TerminalView",
            bytes: TerminalView {
                session_id: "5e55f00d".into(),
                task_id: id(0x77),
                owner: "agent".into(),
                argv: vec!["sh".into(), "-c".into(), "sleep 30".into()],
                title: "sh -c sleep 30".into(),
                state: "EXITED".into(),
                exit_code: Some(-1),
                started_at_ms: 1_757_289_600_123,
                elapsed_ms: 30_001,
                bytes_so_far: 9_007_199_254_740_993,
                oldest_cursor: 4_194_288,
                replay_window_bytes: 67_108_864,
                pty: true,
                rows: 40,
                cols: 120,
                input_lease_holder: "user:3-ab12cd".into(),
                output_ref: "ab".repeat(32),
                cwd: "/work".into(),
                timed_out: false,
            }
            .encode_to_vec(),
            expected: json!({
                "sessionId": "5e55f00d", "taskId": idhex(0x77), "owner": "agent",
                "argv": ["sh", "-c", "sleep 30"], "title": "sh -c sleep 30",
                "state": "EXITED", "exitCode": -1, "startedAtMs": "1757289600123",
                "elapsedMs": "30001", "bytesSoFar": "9007199254740993",
                "oldestCursor": "4194288", "replayWindowBytes": "67108864",
                "pty": true, "rows": 40, "cols": 120,
                "inputLeaseHolder": "user:3-ab12cd", "outputRef": "ab".repeat(32),
                "cwd": "/work", "timedOut": false
            }),
            decode: reencode::<TerminalView>,
        },
        Sample {
            // PX-043: attaching from a cursor, with an acknowledgement window.
            name: "attach_terminal",
            type_name: "modbit.v1.AttachTerminal",
            bytes: AttachTerminal {
                task_id: id(0x77),
                session_id: "5e55f00d".into(),
                after_cursor: 18_446_744_073_709_551_615,
                window_bytes: 262_144,
                take_input_lease: true,
                steal_input_lease: false,
            }
            .encode_to_vec(),
            expected: json!({
                "taskId": idhex(0x77), "sessionId": "5e55f00d",
                "afterCursor": "18446744073709551615", "windowBytes": "262144",
                "takeInputLease": true, "stealInputLease": false
            }),
            decode: reencode::<AttachTerminal>,
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
                "notClaimed": ["unknown cost stays unknown"],
                "admission": {
                    "admitted": true,
                    "planId": "direct:11111111111111111111111111111111",
                    "validationDigest": "c".repeat(64),
                    "reservedMinor": "0", "currency": "USD", "scale": 2,
                    "routingEpoch": "0", "refusalCode": "", "refusalDetail": "",
                    "activations": [{
                        "slotId": "initial", "activation": 1, "reservedMinor": "0"
                    }],
                    "feasibility": "QUALITY_FLOOR_UNKNOWN", "qualityLcbBp": 0,
                    "statsVersion": "none", "thresholdsVersion": "none", "targetMet": false
                }
            }),
            decode: reencode::<RoutingPlanView>,
        },
        Sample {
            name: "protocol_state_view",
            type_name: "modbit.v1.ProtocolStateView",
            bytes: protocol_state.encode_to_vec(),
            expected: json!({
                "taskId": idhex(0x22), "version": "protocol-state-1",
                "boundary": "AWAITING_APPROVAL",
                "calls": [
                    {
                        "toolCallId": idhex(0x33), "toolName": "git.worktree.close",
                        "effectClass": "Destructive", "phase": "AWAITING_APPROVAL",
                        "argumentsHash": "d".repeat(64), "callId": "call_1_0",
                        "runId": idhex(0x44), "approvalId": idhex(0x55), "reason": ""
                    },
                    {
                        "toolCallId": idhex(0x66), "toolName": "shell.exec",
                        "effectClass": "ReversibleWrite", "phase": "UNKNOWN_OUTCOME",
                        "argumentsHash": "e".repeat(64), "callId": "",
                        "runId": null, "approvalId": "",
                        "reason": "core restarted (boot generation 7) after the call was dispatched and before its result was acknowledged"
                    }
                ],
                "approvals": [{
                    "approvalId": idhex(0x55), "toolCallId": idhex(0x33),
                    "toolName": "git.worktree.close", "effectClass": "Destructive",
                    "intentHash": "d".repeat(64), "expiresAt": "1700000000000", "expired": false
                }],
                "questionId": "", "activeLeases": 1, "digest": "f".repeat(64),
                "terminals": [{
                    "handleId": "0011223344556677", "requestId": "bg-1",
                    "argv": ["sh", "-c", "tail -f log"], "replayGeneration": "3",
                    "lastAcknowledgedCursor": "4096", "running": true, "outputRef": "",
                    "exitCode": 0, "exitKnown": false,
                    "toolCallId": "01a08f33-0d97-7d40-b299-f8a7bc5cda60"
                }]
            }),
            decode: reencode::<ProtocolStateView>,
        },
        Sample {
            name: "session_tree_view",
            type_name: "modbit.v1.SessionTreeView",
            bytes: session_tree.encode_to_vec(),
            expected: json!({
                "sessionId": idhex(0x10), "branchGeneration": "1",
                "tasks": [
                    {
                        "taskId": idhex(0x11), "goalText": "make the check pass",
                        "state": "Waiting(Approval)", "origin": "cli", "workspaceRoot": "/repo",
                        "forkedFromTask": null, "forkedFromCheckpoint": "", "forkedFromEpoch": 0,
                        "forkedFromOffset": "0", "capsuleRef": "", "branchGeneration": "0",
                        "runs": [{"runId": idhex(0x12), "state": "Suspended"}],
                        "checkpoints": [{
                            "checkpointId": "01a09072-1262-70f2-b103-63d3e8f0feda", "epoch": 1,
                            "kind": "BASELINE", "baseCheckpointId": "", "status": "CURRENT",
                            "workspaceRevision": "3", "manifestRef": "a".repeat(64),
                            "integrityHash": "b".repeat(64), "gitHead": "c".repeat(40),
                            "files": 1, "removed": 0, "eventOffset": "120",
                            "indexGeneration": "2", "reason": "before the fork",
                            "createdAtMs": "1700000000000", "committedAtMs": "1700000000500"
                        }],
                        "restores": [{
                            "checkpointId": "01a09072-1262-70f2-b103-63d3e8f0feda", "epoch": 1,
                            "offset": "140", "filesWritten": 1, "filesReverted": 1,
                            "preconditionsChecked": 2
                        }]
                    },
                    {
                        "taskId": idhex(0x13), "goalText": "make the check pass",
                        "state": "Queued", "origin": "fork", "workspaceRoot": "/profile/worktrees/13",
                        "forkedFromTask": idhex(0x11),
                        "forkedFromCheckpoint": "01a09072-1262-70f2-b103-63d3e8f0feda",
                        "forkedFromEpoch": 1, "forkedFromOffset": "120", "capsuleRef": "d".repeat(64),
                        "branchGeneration": "1", "runs": [], "checkpoints": [], "restores": []
                    }
                ],
                "branches": [{
                    "branchGeneration": "1", "kind": "fork",
                    "reason": "task 13 forked from task 11 at checkpoint 01a09072 (epoch 1)",
                    "offset": "141"
                }]
            }),
            decode: reencode::<SessionTreeView>,
        },
        Sample {
            name: "transcript_page",
            type_name: "modbit.v1.TranscriptPage",
            bytes: transcript_page.encode_to_vec(),
            expected: json!({
                "taskId": idhex(0x21),
                "density": "TRANSCRIPT_DENSITY_BALANCED",
                "rows": [
                    row_json(1, "unread", "TRANSCRIPT_ROW_KIND_UNREAD_DIVIDER", "2 new", false, vec![]),
                    row_json(2, "group:explore:tool:a", "TRANSCRIPT_ROW_KIND_WORK_GROUP", "Explored 2 items", true, vec![
                        row_json(1, "tool:a", "TRANSCRIPT_ROW_KIND_TOOL_CARD", "fs.read a.txt", false, vec![]),
                        row_json(2, "tool:b", "TRANSCRIPT_ROW_KIND_TOOL_CARD", "fs.read b.txt", false, vec![]),
                    ]),
                ],
                "totalRows": 2, "nextAfterRow": 0, "hasMore": false,
                "asOfOffset": "90", "lastOffset": "90", "taskState": "Running",
                "readOffset": "40", "eventsRead": "12"
            }),
            decode: reencode::<TranscriptPage>,
        },
        Sample {
            name: "agent_headers",
            type_name: "modbit.v1.AgentHeaders",
            bytes: agent_headers.encode_to_vec(),
            expected: json!({
                "sessionId": idhex(0x10),
                "headers": [{
                    "taskId": idhex(0x21), "sessionId": idhex(0x10),
                    "workspaceRoot": "/repo", "title": "make the check pass", "subtitle": "repo",
                    "createdAt": {"seconds": "1700000000", "nanos": 0},
                    "updatedAt": {"seconds": "1700000100", "nanos": 500000000},
                    "statusClass": "AGENT_STATUS_CLASS_READY_FOR_REVIEW_UNSEEN",
                    "statusLabel": "Ready for review", "unread": true, "pendingApproval": false,
                    "pendingPlan": false, "contextPercent": 37, "filesChanged": 2,
                    "linesAdded": 14, "linesRemoved": 3, "lastCheckpointAt": null,
                    "subagent": false, "archived": false, "executionLocation": "local",
                    "origin": "cli", "taskState": "ReadyForReview", "lastOffset": "90",
                    "readOffset": "40", "attentionItems": 0
                }],
                "lastOffset": "90", "eventsRead": "0", "objectsRead": "0"
            }),
            decode: reencode::<AgentHeaders>,
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
