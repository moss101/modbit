#![recursion_limit = "256"]
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

    let skill_list = SkillList {
        skills: vec![SkillView {
            name: "release-notes".into(),
            version: "1.2.0".into(),
            description: "write release notes from merged changes".into(),
            scope: "USER".into(),
            content_hash: "e".repeat(64),
            trust: "TRUSTED_BY_OWNER".into(),
            trust_detail: String::new(),
            enabled: true,
            invocation: "BOTH".into(),
            paths: vec!["docs/**".into()],
            paths_active: true,
            index_tokens: 14,
            indexed: true,
            index_form: "FULL".into(),
            selected: false,
            source: "/profile/skills/release-notes".into(),
            provenance_source: String::new(),
            provenance_author: "me".into(),
            provenance_license: "MIT".into(),
            required_tools: vec!["fs.read".into()],
            lifecycle: "ENABLED".into(),
        }],
        rejected: vec![SkillRefusalView {
            source: "/profile/skills/broken".into(),
            code: "MALFORMED_MANIFEST".into(),
            reason: "no front matter".into(),
        }],
        index_budget_tokens: 2000,
        index_used_tokens: 14,
        index_omitted: 0,
        system_root: "/etc/modbit/skills".into(),
        // REQ-PX-052: the slash menu's union (field block 260-279).
        slash: vec![
            SlashEntry {
                kind: "SKILL".into(),
                id: "core-guide".into(),
                display_name: "core-guide".into(),
                description: "how the product works".into(),
                scope: "SYSTEM".into(),
                trust: "SYSTEM".into(),
                trust_detail: String::new(),
                enabled: true,
                invocation: "BOTH".into(),
                built_in: true,
                content_hash: "ab".repeat(32),
                provenance_source: "system".into(),
                source: "/etc/modbit/skills/core-guide".into(),
            },
            SlashEntry {
                kind: "COMMAND".into(),
                id: "kit/tidy".into(),
                display_name: "kit/tidy".into(),
                description: "tidy up a file".into(),
                scope: "EXTENSION".into(),
                trust: "VERIFIED:acme".into(),
                trust_detail: String::new(),
                enabled: true,
                invocation: "USER_ONLY".into(),
                built_in: false,
                content_hash: "cd".repeat(32),
                provenance_source: "extension:kit@1.2.0".into(),
                source: "/ext/kit".into(),
            },
        ],
        slash_divider_at: 1,
    };
    let budgets = SetTaskBudgets {
        task_id: id(0x31),
        max_cost_minor: 5000,
        max_wall_ms: 9_007_199_254_740_993, // > 2^53
        max_children: 2,
        forbid_spawn: false,
    };
    // PX-050 / PX-057 / PX-059: the in-run control messages. 64-bit counters
    // and offsets past 2^53 so both languages agree on them.
    let queue = QueuedInputList {
        task_id: id(0x31),
        items: vec![
            QueuedInputView {
                input_id: "a".into(),
                position: 1,
                mode: "FOLLOW_UP".into(),
                text: "message A".into(),
                state: "QUEUED".into(),
                model: String::new(),
                edited: true,
                sent_now: false,
                provenance: String::new(),
                untrusted: false,
                queued_offset: 9_007_199_254_740_993,
                changed_offset: 9_007_199_254_740_995,
            },
            QueuedInputView {
                input_id: "forge-comment-7".into(),
                position: 0,
                mode: "STEER".into(),
                text: "please rename".into(),
                state: "DISPATCHED".into(),
                model: "gpt-5-mini".into(),
                edited: false,
                sent_now: true,
                provenance: "forge_review_comment".into(),
                untrusted: true,
                queued_offset: 12,
                changed_offset: 14,
            },
        ],
        queued: 1,
        run_alive: true,
        offset: 9_007_199_254_740_999,
    };
    let rules = AllowRuleList {
        rules: vec![AllowRuleView {
            rule_id: "r-1".into(),
            pattern: vec!["cargo".into(), "test".into()],
            scope: "REPO".into(),
            scope_key: "/repo".into(),
            created_by: "user:b1".into(),
            created_at_ms: 1_700_000_000_123,
            expires_at_ms: 9_007_199_254_740_993,
            covers_always_ask: false,
            state: "ACTIVE".into(),
            revoked_by: String::new(),
            origin_task: "t-1".into(),
            offset: 77,
        }],
    };
    let run_mode = RunModeView {
        task_id: id(0x31),
        mode: "ALLOWLIST".into(),
        acknowledged: true,
        always_ask: vec!["NETWORK".into(), "SECRET".into()],
        modes: vec!["ASK".into(), "ALLOWLIST".into()],
        warning: "a mode that approves more".into(),
        session_only: false,
        offset: 9_007_199_254_740_993,
        rules_in_force: 2,
    };
    let accounting = ContextAccountingView {
        available: true,
        total_tokens: 9_007_199_254_740_993,
        total_source: "PROVIDER_REPORTED".into(),
        estimated_total: 9_007_199_254_740_000,
        provider_reported_input: 9_007_199_254_740_993,
        estimator_error_bp: 12,
        declared_error_bp: 2500,
        rounding_rule: "LARGEST_REMAINDER".into(),
        window_tokens: 200_000,
        window_source: "REGISTRY".into(),
        used_bp: 4504,
        categories: vec![
            ContextCategoryView {
                category: "SYSTEM".into(),
                tokens: 6_000_000_000_000_001,
                estimated_tokens: 6_000_000_000_000_000,
                share_bp: 6661,
                sources: vec!["system segment".into()],
            },
            ContextCategoryView {
                category: "CONVERSATION".into(),
                tokens: 3_007_199_254_740_992,
                estimated_tokens: 3_007_199_254_740_000,
                share_bp: 3338,
                sources: vec![],
            },
        ],
        endpoint: "openai".into(),
        model: "gpt-5-mini".into(),
        turn_id: "01a09072-1262-70f2-b103-63d3e8f0feda".into(),
        turn_ordinal: 4,
        offset: 9_007_199_254_741_001,
        instruction_layers_in_force: 2,
        memory_items_injected: 1,
        compaction_epoch: 1,
        compaction_summaries: 1,
        budgets: Some(BudgetAccountingView {
            max_cost_minor: 5000,
            spent_minor: 1234,
            held_by_children_minor: 100,
            max_wall_ms: 9_007_199_254_740_993,
            wall_ms_used: 42,
            max_children: 3,
            live_children: 1,
            forbid_spawn: false,
        }),
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
        preference: None,
        preference_routing: None,
    };
    // PX-051 / PX-053: a task's posture. Carries a 64-bit offset past 2^53 and
    // enums by name, so both languages agree on them.
    let posture = TaskPostureView {
        task_id: id(0x31),
        mode: TaskMode::Plan as i32,
        mode_offset: 9_007_199_254_740_993,
        mode_in_force: TaskMode::Agent as i32,
        posture: Some(ModePostureView {
            effect_ceiling: "READONLY".into(),
            allowed_capabilities: vec!["fs.read".into(), "git.read".into()],
            subagents: false,
            reproduction_first: false,
            writes: false,
        }),
        preference: Some(ExecutionPreferenceView {
            objective: ObjectiveProfile::Cost as i32,
            effort: "high".into(),
            service_tier: "flex".into(),
            pin_endpoint: "openai".into(),
            pin_model: "gpt-5-mini".into(),
            offset: 41,
            applied_offset: 0,
            effort_applied: String::new(),
            service_tier_applied: String::new(),
        }),
        routing: Some(RoutingOutcomeView {
            outcome: "DIRECT".into(),
            reason_code: "NO_ACTIVE_REGISTRY".into(),
            detail: "no signed registry is active".into(),
            floor_mode: String::new(),
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
                    name: "before the refactor".into(),
                    turn_id: id(0x14),
                    turn_ordinal: 2,
                    retention: vec!["NAMED".into(), "FORK_PARENT".into()],
                    cost: Some(CaptureCost {
                        capture_ms: 12,
                        hashed_files: 1,
                        cache_hits: 40,
                        blobs_written: 1,
                        bytes_written: 9_007_199_254_740_993, // > 2^53: 64-bit in TypeScript
                    }),
                }],
                restores: vec![RestoreView {
                    checkpoint_id: "01a09072-1262-70f2-b103-63d3e8f0feda".into(),
                    epoch: 1,
                    offset: 140,
                    files_written: 1,
                    files_reverted: 1,
                    preconditions_checked: 2,
                    pre_restore_checkpoint_id: "01a09072-1262-70f2-b103-63d3e8f0fedb".into(),
                    redo: false,
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
    let memory_injection = MemoryInjectionView {
        pack_id: "c".repeat(64),
        token_budget: 800,
        token_used: 211,
        omitted_count: 3,
        entries: vec![MemoryInjectedEntry {
            memory_id: "a".repeat(64),
            scope: "agent_profile:primary".into(),
            record_type: "convention".into(),
            topic: "indentation".into(),
            source: "user_stated".into(),
            author: "user:b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1".into(),
            confidence: 0.9,
            validated: true,
            token_cost: 42,
            reasons: vec!["relevant: topic".into(), "scope:agent_profile".into()],
            conflicts_with: vec![],
            clipped: false,
            created_at_ms: 1_757_289_600_000,
            expires_at_ms: 0,
            last_validation_revision: String::new(),
        }],
        excluded: vec![
            MemoryExclusionView {
                memory_id: "b".repeat(64),
                reason: format!("shadowed_by:{}", "a".repeat(64)),
            },
            MemoryExclusionView {
                memory_id: "d".repeat(64),
                reason: "sensitive".into(),
            },
        ],
        rejected_ids: vec![],
        compiler_version: "memory-pack-v1".into(),
    };
    let impact = ImpactResult {
        changed: vec!["src/util.rs".into()],
        dependents: vec![ImpactedFile {
            path: "src/render.rs".into(),
            rank: 1.02,
            distance: 1,
            confidence: "resolved".into(),
            edge_path: vec![ImpactEdgeStep {
                from: "src/render.rs".into(),
                to: "src/util.rs".into(),
                kind: "call".into(),
                confidence: "resolved".into(),
                symbol: "pad".into(),
                line: 5,
            }],
            reasons: vec!["call".into()],
            tests: vec!["tests/render_test.rs".into()],
        }],
        tests: vec![ImpactedTestView {
            path: "tests/render_test.rs".into(),
            reasons: vec!["covers_dependent".into()],
            distance: 2,
            covers: vec!["src/render.rs".into()],
        }],
        revision: 9_007_199_254_740_993,
        partial: true,
        partial_reason: "more than 1 dependents; the list is cut at 1".into(),
        symbols: vec!["pad".into()],
        limitation: "heuristic".into(),
        ambiguous_edges: 2,
        unresolved_edges: 311,
    };
    let index_status = IndexStatusView {
        workspace_root: "/repo".into(),
        store_dir: "/profile/indexes/0123456789abcdef".into(),
        workspace_revision: 4,
        builds: 0,
        loads: 5,
        refreshes: 2,
        last_refresh_ms: 7,
        last_refresh_files: 1,
        components: vec![
            IndexComponentView {
                name: "symbols".into(),
                state: "loaded".into(),
                files: 256,
                persisted_bytes: 0,
                load_ms: 9,
                build_ms: 0,
                reason: String::new(),
            },
            IndexComponentView {
                name: "lexical".into(),
                state: "rebuilt".into(),
                files: 256,
                persisted_bytes: 0,
                load_ms: 0,
                build_ms: 410,
                reason: "lexical: checksum mismatch in 0.idx".into(),
            },
        ],
        rebuild_reasons: vec!["lexical: checksum mismatch in 0.idx".into()],
        persisted_bytes: 9_007_199_254_740_993,
        persisted_generation: 3,
        first_ready_ms: 120,
        searches_indexed: 11,
        searches_scanned: 4,
        recomputed_files: 1,
        recomputed_sample: vec!["src/util.rs".into()],
    };
    let runtime = BrowserRuntimeView {
        browser_session_id: id(0x61),
        latch: "browser.act click: BROWSER_TIMEOUT".into(),
        latch_tool_call_id: "01a08f33-0d97-7d40-b299-f8a7bc5cda60".into(),
        page_kind: "login".into(),
        change_seq: 9_007_199_254_740_993,
        notices: 12,
        compiled_seq: 11,
        known_entities: 7,
        history: 3,
        last_fingerprint: "e".repeat(64),
        delivered_fingerprint: "f".repeat(64),
    };
    let noticed = BrowserHostNoticed {
        accepted: true,
        change_seq: 12,
        notices: 4,
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
            // PX-066 (worktrees.proto): the person's apply-back choice.
            name: "apply_worktree",
            type_name: "modbit.v1.ApplyWorktree",
            bytes: ApplyWorktree {
                task_id: id(0x31),
                expected_candidate_revision: 18_446_744_073_709_551_615,
                option: "OVERWRITE".into(),
                confirm_paths: vec!["a.txt".into(), "src/b ü.rs".into()],
                remember: false,
                expected_plan_digest: "ab".repeat(32),
            }
            .encode_to_vec(),
            expected: json!({
                "taskId": idhex(0x31), "expectedCandidateRevision": "18446744073709551615",
                "option": "OVERWRITE", "confirmPaths": ["a.txt", "src/b ü.rs"],
                "remember": false, "expectedPlanDigest": "ab".repeat(32)
            }),
            decode: reencode::<ApplyWorktree>,
        },
        Sample {
            // PX-065 (worktrees.proto): one row of the worktree list.
            name: "worktree_view",
            type_name: "modbit.v1.WorktreeView",
            bytes: WorktreeView {
                worktree_id: "01a1183e-0000-7000-8000-000000000001".into(),
                task_id: id(0x32),
                kind: "TASK".into(),
                path: "/profile/worktrees/x".into(),
                origin_root: "/work/repo".into(),
                branch: "modbit/task-x".into(),
                base_revision: "ab".repeat(20),
                state: "ACTIVE".into(),
                disposition: "APPLIED".into(),
                dirty: true,
                unapplied: false,
                changed_files: 4,
                bytes: 9_007_199_254_740_993,
                created_at_ms: 1_757_289_600_123,
                last_activity_ms: 1_757_289_700_000,
                task_running: false,
                orphan: false,
                protected: true,
                removable: false,
                removable_reason: "younger than the protection window".into(),
                setup_status: "PASSED".into(),
                setup_detail: String::new(),
                neutralized: vec!["HOOK".into(), "FILTER".into()],
            }
            .encode_to_vec(),
            expected: json!({
                "worktreeId": "01a1183e-0000-7000-8000-000000000001", "taskId": idhex(0x32),
                "kind": "TASK", "path": "/profile/worktrees/x", "originRoot": "/work/repo",
                "branch": "modbit/task-x", "baseRevision": "ab".repeat(20), "state": "ACTIVE",
                "disposition": "APPLIED", "dirty": true, "unapplied": false, "changedFiles": 4,
                "bytes": "9007199254740993", "createdAtMs": "1757289600123",
                "lastActivityMs": "1757289700000", "taskRunning": false, "orphan": false,
                "protected": true, "removable": false,
                "removableReason": "younger than the protection window",
                "setupStatus": "PASSED", "setupDetail": "", "neutralized": ["HOOK", "FILTER"]
            }),
            decode: reencode::<WorktreeView>,
        },
        Sample {
            // PX-082 (automations.proto): one definition as the list shows it.
            name: "automation_view",
            type_name: "modbit.v1.AutomationView",
            bytes: AutomationView {
                automation_id: "ab".repeat(16),
                name: "drift".into(),
                description: "Report the drift of the default branch.".into(),
                current_version: 3,
                definition_hash: "cd".repeat(32),
                state: "ENABLED".into(),
                disabled_reason: String::new(),
                disabled_detail: String::new(),
                paused: false,
                enabled: Some(AutomationEnableView {
                    version: 3,
                    definition_hash: "cd".repeat(32),
                    effects: "reversible_write".into(),
                    capabilities: vec!["fs.write".into()],
                    paths: vec!["reports/**".into()],
                    hosts: vec![],
                    approver: "user:x".into(),
                    approved_ms: 1_757_289_600_123,
                    repo_revision: String::new(),
                    source_sha256: String::new(),
                }),
                source_kind: "repository".into(),
                source_path: ".modbit/automations/drift.json".into(),
                workspace_root: "/work/repo".into(),
                principal: "service:ci-bot".into(),
                triggers: vec![AutomationTriggerView {
                    id: "nightly".into(),
                    kind: "schedule".into(),
                    summary: "cron 0 3 * * * (UTC)".into(),
                    next_due_ms: 1_757_300_000_000,
                }],
                effects: "reversible_write".into(),
                capabilities: vec!["fs.write".into()],
                paths: vec!["reports/**".into()],
                hosts: vec![],
                consecutive_failures: 2,
                last_run_ms: 1_757_289_700_000,
                last_run_status: "failed".into(),
                definition_json: "{}".into(),
                needs_listed_approval: true,
                content_changed: false,
                queued: 1,
                active: 1,
                limit_deadline_minutes: 30,
                limit_max_cost_minor: 9_007_199_254_740_993,
                limit_approval_wait_minutes: 1440,
            }
            .encode_to_vec(),
            expected: json!({
                "automationId": "ab".repeat(16), "name": "drift",
                "description": "Report the drift of the default branch.", "currentVersion": 3,
                "definitionHash": "cd".repeat(32), "state": "ENABLED", "disabledReason": "",
                "disabledDetail": "", "paused": false,
                "enabled": {
                    "version": 3, "definitionHash": "cd".repeat(32), "effects": "reversible_write",
                    "capabilities": ["fs.write"], "paths": ["reports/**"], "hosts": [],
                    "approver": "user:x", "approvedMs": "1757289600123", "repoRevision": "",
                    "sourceSha256": ""
                },
                "sourceKind": "repository", "sourcePath": ".modbit/automations/drift.json",
                "workspaceRoot": "/work/repo", "principal": "service:ci-bot",
                "triggers": [{"id": "nightly", "kind": "schedule",
                    "summary": "cron 0 3 * * * (UTC)", "nextDueMs": "1757300000000"}],
                "effects": "reversible_write", "capabilities": ["fs.write"],
                "paths": ["reports/**"], "hosts": [], "consecutiveFailures": 2,
                "lastRunMs": "1757289700000", "lastRunStatus": "failed", "definitionJson": "{}",
                "needsListedApproval": true, "contentChanged": false, "queued": 1, "active": 1,
                "limitDeadlineMinutes": "30", "limitMaxCostMinor": "9007199254740993",
                "limitApprovalWaitMinutes": "1440"
            }),
            decode: reencode::<AutomationView>,
        },
        Sample {
            // PX-083 (automations.proto): one row of the run history.
            name: "automation_run_view",
            type_name: "modbit.v1.AutomationRunView",
            bytes: AutomationRunView {
                dispatch_key: "ef".repeat(32),
                automation_id: "ab".repeat(16),
                name: "drift".into(),
                version: 3,
                trigger_id: "nightly".into(),
                trigger_kind: "schedule".into(),
                event_id: "nightly@1757300000000".into(),
                status: "cancelled".into(),
                reason: "APPROVAL_EXPIRED".into(),
                detail: "no one answered".into(),
                task_id: idhex(0x41),
                session_id: idhex(0x42),
                principal: "user:x".into(),
                fired_ms: 1_757_300_000_000,
                dispatched_ms: 1_757_300_000_100,
                finished_ms: 1_757_386_400_000,
                cost_minor: 12,
                test: false,
                catch_up: true,
                missed: 4,
                findings: 1,
                outputs_json: "{\"task_state\":\"Cancelled\"}".into(),
                acknowledged: false,
                slot_ms: 1_757_300_000_000,
            }
            .encode_to_vec(),
            expected: json!({
                "dispatchKey": "ef".repeat(32), "automationId": "ab".repeat(16), "name": "drift",
                "version": 3, "triggerId": "nightly", "triggerKind": "schedule",
                "eventId": "nightly@1757300000000", "status": "cancelled",
                "reason": "APPROVAL_EXPIRED", "detail": "no one answered", "taskId": idhex(0x41),
                "sessionId": idhex(0x42), "principal": "user:x", "firedMs": "1757300000000",
                "dispatchedMs": "1757300000100", "finishedMs": "1757386400000", "costMinor": "12",
                "test": false, "catchUp": true, "missed": "4", "findings": 1,
                "outputsJson": "{\"task_state\":\"Cancelled\"}", "acknowledged": false,
                "slotMs": "1757300000000"
            }),
            decode: reencode::<AutomationRunView>,
        },
        Sample {
            // PX-082: the live validation the editor shows.
            name: "automation_validation",
            type_name: "modbit.v1.AutomationValidation",
            bytes: AutomationValidation {
                ok: false,
                issues: vec![AutomationIssue {
                    path: "/triggers/0/cron".into(),
                    code: "BELOW_FLOOR".into(),
                    message: "the floor is one run per 5 minutes".into(),
                }],
                name: "drift".into(),
                definition_hash: String::new(),
                canonical_json: String::new(),
                effects: "read_only".into(),
                capabilities: vec![],
                paths: vec![],
                hosts: vec![],
                needs_listed_approval: false,
                triggers: vec![],
            }
            .encode_to_vec(),
            expected: json!({
                "ok": false,
                "issues": [{"path": "/triggers/0/cron", "code": "BELOW_FLOOR",
                    "message": "the floor is one run per 5 minutes"}],
                "name": "drift", "definitionHash": "", "canonicalJson": "",
                "effects": "read_only", "capabilities": [], "paths": [], "hosts": [],
                "needsListedApproval": false, "triggers": []
            }),
            decode: reencode::<AutomationValidation>,
        },
        Sample {
            // PX-083 / PX-084: the fields only the Core's automation host sets.
            name: "create_task_automation_host",
            type_name: "modbit.v1.CreateTask",
            bytes: CreateTask {
                session_id: id(0x11),
                goal_text: "Report the drift.".into(),
                workspace_id: None,
                execution_profile: "plan".into(),
                origin: "automation".into(),
                workspace_root: "/work/repo".into(),
                automation_id: "ab".repeat(16),
                automation_version: 3,
                automation_event_id: "nightly@1757300000000".into(),
                automation_dispatch_key: "ef".repeat(32),
                automation_principal: "service:ci-bot".into(),
                automation_trigger: "nightly".into(),
                lease_operations: vec!["fs.read".into(), "git.read".into()],
                lease_resources: vec!["fs.write:/work/repo/reports/**".into()],
                lease_effect_ceiling: "READ_ONLY".into(),
                trigger_payload: "{\"action\":\"opened\"}".into(),
                trigger_payload_label: "forge_pr".into(),
                payload_findings: 2,
                automation_test: true,
                automation_definition_hash: "cd".repeat(32),
                automation_trigger_kind: "event".into(),
                ..Default::default()
            }
            .encode_to_vec(),
            expected: json!({
                "sessionId": idhex(0x11), "goalText": "Report the drift.",
                "workspaceId": null, "preference": null,
                "executionProfile": "plan", "origin": "automation",
                "workspaceRoot": "/work/repo", "issueUrl": "", "issueJson": "",
                "mode": "TASK_MODE_UNSPECIFIED", "isolation": "TASK_ISOLATION_UNSPECIFIED",
                "automationId": "ab".repeat(16), "automationVersion": 3,
                "automationEventId": "nightly@1757300000000",
                "automationDispatchKey": "ef".repeat(32), "automationPrincipal": "service:ci-bot",
                "automationTrigger": "nightly", "leaseOperations": ["fs.read", "git.read"],
                "leaseResources": ["fs.write:/work/repo/reports/**"],
                "leaseEffectCeiling": "READ_ONLY", "triggerPayload": "{\"action\":\"opened\"}",
                "triggerPayloadLabel": "forge_pr", "payloadFindings": 2, "automationTest": true,
                "automationDefinitionHash": "cd".repeat(32), "automationTriggerKind": "event"
            }),
            decode: reencode::<CreateTask>,
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
                },
                "preference": null, "preferenceRouting": null
            }),
            decode: reencode::<RoutingPlanView>,
        },
        Sample {
            name: "task_posture_view",
            type_name: "modbit.v1.TaskPostureView",
            bytes: posture.encode_to_vec(),
            expected: json!({
                "taskId": idhex(0x31), "mode": "TASK_MODE_PLAN",
                "modeOffset": "9007199254740993", "modeInForce": "TASK_MODE_AGENT",
                "posture": {
                    "effectCeiling": "READONLY",
                    "allowedCapabilities": ["fs.read", "git.read"],
                    "subagents": false, "reproductionFirst": false, "writes": false
                },
                "preference": {
                    "objective": "OBJECTIVE_PROFILE_COST", "effort": "high",
                    "serviceTier": "flex", "pinEndpoint": "openai", "pinModel": "gpt-5-mini",
                    "offset": "41", "appliedOffset": "0", "effortApplied": "",
                    "serviceTierApplied": ""
                },
                "routing": {
                    "outcome": "DIRECT", "reasonCode": "NO_ACTIVE_REGISTRY",
                    "detail": "no signed registry is active", "floorMode": ""
                }
            }),
            decode: reencode::<TaskPostureView>,
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
                            "createdAtMs": "1700000000000", "committedAtMs": "1700000000500",
                            "name": "before the refactor", "turnId": idhex(0x14), "turnOrdinal": 2,
                            "retention": ["NAMED", "FORK_PARENT"],
                            "cost": {
                                "captureMs": "12", "hashedFiles": 1, "cacheHits": 40,
                                "blobsWritten": 1, "bytesWritten": "9007199254740993"
                            }
                        }],
                        "restores": [{
                            "checkpointId": "01a09072-1262-70f2-b103-63d3e8f0feda", "epoch": 1,
                            "offset": "140", "filesWritten": 1, "filesReverted": 1,
                            "preconditionsChecked": 2,
                            "preRestoreCheckpointId": "01a09072-1262-70f2-b103-63d3e8f0fedb",
                            "redo": false
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
            name: "skill_list",
            type_name: "modbit.v1.SkillList",
            bytes: skill_list.encode_to_vec(),
            expected: json!({
                "skills": [{
                    "name": "release-notes", "version": "1.2.0",
                    "description": "write release notes from merged changes",
                    "scope": "USER", "contentHash": "e".repeat(64),
                    "trust": "TRUSTED_BY_OWNER", "trustDetail": "", "enabled": true,
                    "invocation": "BOTH", "paths": ["docs/**"], "pathsActive": true,
                    "indexTokens": 14, "indexed": true, "indexForm": "FULL",
                    "selected": false, "source": "/profile/skills/release-notes",
                    "provenanceSource": "", "provenanceAuthor": "me", "provenanceLicense": "MIT",
                    "requiredTools": ["fs.read"], "lifecycle": "ENABLED"
                }],
                "rejected": [{"source": "/profile/skills/broken", "code": "MALFORMED_MANIFEST", "reason": "no front matter"}],
                "indexBudgetTokens": 2000, "indexUsedTokens": 14, "indexOmitted": 0,
                "systemRoot": "/etc/modbit/skills",
                "slash": [
                    {
                        "kind": "SKILL", "id": "core-guide", "displayName": "core-guide",
                        "description": "how the product works", "scope": "SYSTEM", "trust": "SYSTEM",
                        "trustDetail": "", "enabled": true, "invocation": "BOTH", "builtIn": true,
                        "contentHash": "ab".repeat(32), "provenanceSource": "system",
                        "source": "/etc/modbit/skills/core-guide"
                    },
                    {
                        "kind": "COMMAND", "id": "kit/tidy", "displayName": "kit/tidy",
                        "description": "tidy up a file", "scope": "EXTENSION", "trust": "VERIFIED:acme",
                        "trustDetail": "", "enabled": true, "invocation": "USER_ONLY", "builtIn": false,
                        "contentHash": "cd".repeat(32), "provenanceSource": "extension:kit@1.2.0",
                        "source": "/ext/kit"
                    }
                ],
                "slashDividerAt": 1
            }),
            decode: reencode::<SkillList>,
        },
        Sample {
            name: "queued_input_list",
            type_name: "modbit.v1.QueuedInputList",
            bytes: queue.encode_to_vec(),
            expected: json!({
                "taskId": idhex(0x31), "queued": 1, "runAlive": true,
                "offset": "9007199254740999",
                "items": [
                    {
                        "inputId": "a", "position": 1, "mode": "FOLLOW_UP", "text": "message A",
                        "state": "QUEUED", "model": "", "edited": true, "sentNow": false,
                        "provenance": "", "untrusted": false,
                        "queuedOffset": "9007199254740993", "changedOffset": "9007199254740995"
                    },
                    {
                        "inputId": "forge-comment-7", "position": 0, "mode": "STEER",
                        "text": "please rename", "state": "DISPATCHED", "model": "gpt-5-mini",
                        "edited": false, "sentNow": true, "provenance": "forge_review_comment",
                        "untrusted": true, "queuedOffset": "12", "changedOffset": "14"
                    }
                ]
            }),
            decode: reencode::<QueuedInputList>,
        },
        Sample {
            name: "allow_rule_list",
            type_name: "modbit.v1.AllowRuleList",
            bytes: rules.encode_to_vec(),
            expected: json!({
                "rules": [{
                    "ruleId": "r-1", "pattern": ["cargo", "test"], "scope": "REPO",
                    "scopeKey": "/repo", "createdBy": "user:b1",
                    "createdAtMs": "1700000000123", "expiresAtMs": "9007199254740993",
                    "coversAlwaysAsk": false, "state": "ACTIVE", "revokedBy": "",
                    "originTask": "t-1", "offset": "77"
                }]
            }),
            decode: reencode::<AllowRuleList>,
        },
        Sample {
            name: "run_mode_view",
            type_name: "modbit.v1.RunModeView",
            bytes: run_mode.encode_to_vec(),
            expected: json!({
                "taskId": idhex(0x31), "mode": "ALLOWLIST", "acknowledged": true,
                "alwaysAsk": ["NETWORK", "SECRET"], "modes": ["ASK", "ALLOWLIST"],
                "warning": "a mode that approves more", "sessionOnly": false,
                "offset": "9007199254740993", "rulesInForce": 2
            }),
            decode: reencode::<RunModeView>,
        },
        Sample {
            name: "context_accounting_view",
            type_name: "modbit.v1.ContextAccountingView",
            bytes: accounting.encode_to_vec(),
            expected: json!({
                "available": true, "totalTokens": "9007199254740993",
                "totalSource": "PROVIDER_REPORTED", "estimatedTotal": "9007199254740000",
                "providerReportedInput": "9007199254740993", "estimatorErrorBp": 12,
                "declaredErrorBp": 2500, "roundingRule": "LARGEST_REMAINDER",
                "windowTokens": "200000", "windowSource": "REGISTRY", "usedBp": 4504,
                "categories": [
                    {
                        "category": "SYSTEM", "tokens": "6000000000000001",
                        "estimatedTokens": "6000000000000000", "shareBp": 6661,
                        "sources": ["system segment"]
                    },
                    {
                        "category": "CONVERSATION", "tokens": "3007199254740992",
                        "estimatedTokens": "3007199254740000", "shareBp": 3338, "sources": []
                    }
                ],
                "endpoint": "openai", "model": "gpt-5-mini",
                "turnId": "01a09072-1262-70f2-b103-63d3e8f0feda", "turnOrdinal": 4,
                "offset": "9007199254741001", "instructionLayersInForce": 2,
                "memoryItemsInjected": 1, "compactionEpoch": 1, "compactionSummaries": 1,
                "budgets": {
                    "maxCostMinor": "5000", "spentMinor": "1234", "heldByChildrenMinor": "100",
                    "maxWallMs": "9007199254740993", "wallMsUsed": "42", "maxChildren": 3,
                    "liveChildren": 1, "forbidSpawn": false
                }
            }),
            decode: reencode::<ContextAccountingView>,
        },
        Sample {
            name: "set_task_budgets",
            type_name: "modbit.v1.SetTaskBudgets",
            bytes: budgets.encode_to_vec(),
            expected: json!({
                "taskId": idhex(0x31), "maxCostMinor": "5000",
                "maxWallMs": "9007199254740993", "maxChildren": 2, "forbidSpawn": false
            }),
            decode: reencode::<SetTaskBudgets>,
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
        // PX-113: what the prompt envelope injected, as the Inspector shows it.
        Sample {
            name: "memory_injection_view",
            type_name: "modbit.v1.MemoryInjectionView",
            bytes: memory_injection.encode_to_vec(),
            expected: json!({
                "packId": "c".repeat(64), "tokenBudget": 800, "tokenUsed": 211, "omittedCount": 3,
                "entries": [{
                    "memoryId": "a".repeat(64), "scope": "agent_profile:primary",
                    "recordType": "convention", "topic": "indentation",
                    "source": "user_stated", "author": "user:b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1",
                    "confidence": 0.9f32 as f64, "validated": true, "tokenCost": 42,
                    "reasons": ["relevant: topic", "scope:agent_profile"],
                    "conflictsWith": [], "clipped": false,
                    "createdAtMs": "1757289600000", "expiresAtMs": "0",
                    "lastValidationRevision": ""
                }],
                "excluded": [
                    {"memoryId": "b".repeat(64), "reason": format!("shadowed_by:{}", "a".repeat(64))},
                    {"memoryId": "d".repeat(64), "reason": "sensitive"}
                ],
                "rejectedIds": [], "compilerVersion": "memory-pack-v1"
            }),
            decode: reencode::<MemoryInjectionView>,
        },
        // PX-110: what a change could break, with the edge path and confidence.
        Sample {
            name: "impact_result",
            type_name: "modbit.v1.ImpactResult",
            bytes: impact.encode_to_vec(),
            expected: json!({
                "changed": ["src/util.rs"],
                "dependents": [{
                    "path": "src/render.rs", "rank": 1.02, "distance": 1, "confidence": "resolved",
                    "edgePath": [{
                        "from": "src/render.rs", "to": "src/util.rs", "kind": "call",
                        "confidence": "resolved", "symbol": "pad", "line": 5
                    }],
                    "reasons": ["call"], "tests": ["tests/render_test.rs"]
                }],
                "tests": [{
                    "path": "tests/render_test.rs", "reasons": ["covers_dependent"],
                    "distance": 2, "covers": ["src/render.rs"]
                }],
                "revision": "9007199254740993", "partial": true,
                "partialReason": "more than 1 dependents; the list is cut at 1",
                "symbols": ["pad"], "limitation": "heuristic",
                "ambiguousEdges": 2, "unresolvedEdges": 311
            }),
            decode: reencode::<ImpactResult>,
        },
        // PX-111: how a workspace's indexes came to be.
        Sample {
            name: "index_status_view",
            type_name: "modbit.v1.IndexStatusView",
            bytes: index_status.encode_to_vec(),
            expected: json!({
                "workspaceRoot": "/repo", "storeDir": "/profile/indexes/0123456789abcdef",
                "workspaceRevision": "4", "builds": "0", "loads": "5", "refreshes": "2",
                "lastRefreshMs": "7", "lastRefreshFiles": "1",
                "components": [
                    {"name": "symbols", "state": "loaded", "files": "256", "persistedBytes": "0",
                     "loadMs": "9", "buildMs": "0", "reason": ""},
                    {"name": "lexical", "state": "rebuilt", "files": "256", "persistedBytes": "0",
                     "loadMs": "0", "buildMs": "410", "reason": "lexical: checksum mismatch in 0.idx"}
                ],
                "rebuildReasons": ["lexical: checksum mismatch in 0.idx"],
                "persistedBytes": "9007199254740993", "persistedGeneration": "3",
                "firstReadyMs": "120", "searchesIndexed": "11", "searchesScanned": "4",
                "recomputedFiles": "1", "recomputedSample": ["src/util.rs"]
            }),
            decode: reencode::<IndexStatusView>,
        },
        // PX-042: a search over one session's conversations.
        Sample {
            name: "search_conversations",
            type_name: "modbit.v1.SearchConversations",
            bytes: SearchConversations {
                session_id: id(0x31),
                query: "\"file must say\" valid".into(),
                limit: 20,
                max_snippets: 3,
                include_archived: true,
                task_id: id(0x32),
            }
            .encode_to_vec(),
            expected: json!({
                "sessionId": idhex(0x31), "query": "\"file must say\" valid", "limit": 20,
                "maxSnippets": 3, "includeArchived": true, "taskId": idhex(0x32)
            }),
            decode: reencode::<SearchConversations>,
        },
        Sample {
            name: "conversation_search_results",
            type_name: "modbit.v1.ConversationSearchResults",
            bytes: ConversationSearchResults {
                session_id: id(0x31),
                query: "file must say \"file must\"".into(),
                hits: vec![ConversationHit {
                    task_id: id(0x32),
                    title: "Reject negative quantities.".into(),
                    status_class: AgentStatusClass::ReadyForReviewUnseen as i32,
                    status_label: "Ready for review".into(),
                    archived: false,
                    title_matched: false,
                    matched_rows: 2,
                    score: 6,
                    last_offset: 9_007_199_254_740_993,
                    snippets: vec![ConversationSnippet {
                        row_id: "msg:0123".into(),
                        kind: TranscriptRowKind::AssistantMessage as i32,
                        source: SnippetSource::Assistant as i32,
                        offset: 411,
                        turn_id: "turn-3".into(),
                        text: "The check failed; the file must say validated.".into(),
                        cut_before: false,
                        cut_after: true,
                        matches: vec![MatchRange { start: 24, end: 37 }],
                        score: 1,
                    }],
                }],
                total_hits: 1,
                has_more: false,
                tasks_considered: 4,
                tasks_rebuilt: 2,
                rows_indexed: 13,
                index_bytes: 9_007_199_254_740_993,
                index_budget_bytes: 33_554_432,
                tasks_truncated: 1,
                index_digest: "ab".repeat(32),
                as_of_offset: 777,
            }
            .encode_to_vec(),
            expected: json!({
                "sessionId": idhex(0x31), "query": "file must say \"file must\"",
                "hits": [{
                    "taskId": idhex(0x32), "title": "Reject negative quantities.",
                    "statusClass": "AGENT_STATUS_CLASS_READY_FOR_REVIEW_UNSEEN",
                    "statusLabel": "Ready for review", "archived": false, "titleMatched": false,
                    "matchedRows": 2, "score": 6, "lastOffset": "9007199254740993",
                    "snippets": [{
                        "rowId": "msg:0123", "kind": "TRANSCRIPT_ROW_KIND_ASSISTANT_MESSAGE",
                        "source": "SNIPPET_SOURCE_ASSISTANT", "offset": "411", "turnId": "turn-3",
                        "text": "The check failed; the file must say validated.",
                        "cutBefore": false, "cutAfter": true,
                        "matches": [{"start": 24, "end": 37}], "score": 1
                    }]
                }],
                "totalHits": 1, "hasMore": false, "tasksConsidered": 4, "tasksRebuilt": 2,
                "rowsIndexed": 13, "indexBytes": "9007199254740993", "indexBudgetBytes": "33554432",
                "tasksTruncated": 1, "indexDigest": "ab".repeat(32), "asOfOffset": "777"
            }),
            decode: reencode::<ConversationSearchResults>,
        },
        // PX-043: stopping a task's background terminal.
        Sample {
            name: "kill_terminal",
            type_name: "modbit.v1.KillTerminal",
            bytes: KillTerminal {
                task_id: id(0x41),
                session_id: "0123456789abcdef".into(),
                reason: "the dev server is wedged".into(),
            }
            .encode_to_vec(),
            expected: json!({
                "taskId": idhex(0x41), "sessionId": "0123456789abcdef",
                "reason": "the dev server is wedged"
            }),
            decode: reencode::<KillTerminal>,
        },
        Sample {
            name: "terminal_killed",
            type_name: "modbit.v1.TerminalKilled",
            bytes: TerminalKilled {
                task_id: id(0x41),
                session_id: "0123456789abcdef".into(),
                outcome: "KILLED".into(),
                exit_code: None,
                signal: Some(9),
                output_ref: "cd".repeat(32),
                total_bytes: 9_007_199_254_740_993,
                ended_by: "user:b1b1b1b1".into(),
                reason: "the dev server is wedged".into(),
                decision: "allow: local_trusted:reversiblewrite".into(),
                offset: 412,
            }
            .encode_to_vec(),
            expected: json!({
                "taskId": idhex(0x41), "sessionId": "0123456789abcdef", "outcome": "KILLED",
                "exitCode": null, "signal": 9, "outputRef": "cd".repeat(32),
                "totalBytes": "9007199254740993", "endedBy": "user:b1b1b1b1",
                "reason": "the dev server is wedged",
                "decision": "allow: local_trusted:reversiblewrite", "offset": "412"
            }),
            decode: reencode::<TerminalKilled>,
        },
        // PX-121, PX-122: the browser runtime's state a client reads.
        Sample {
            name: "browser_runtime_view",
            type_name: "modbit.v1.BrowserRuntimeView",
            bytes: runtime.encode_to_vec(),
            expected: json!({
                "browserSessionId": idhex(0x61),
                "latch": "browser.act click: BROWSER_TIMEOUT",
                "latchToolCallId": "01a08f33-0d97-7d40-b299-f8a7bc5cda60",
                "pageKind": "login", "changeSeq": "9007199254740993", "notices": "12",
                "compiledSeq": "11", "knownEntities": 7, "history": 3,
                "lastFingerprint": "e".repeat(64), "deliveredFingerprint": "f".repeat(64)
            }),
            decode: reencode::<BrowserRuntimeView>,
        },
        // PX-122: the acknowledgement of a host's change notice.
        Sample {
            name: "browser_host_noticed",
            type_name: "modbit.v1.BrowserHostNoticed",
            bytes: noticed.encode_to_vec(),
            expected: json!({"accepted": true, "changeSeq": "12", "notices": "4"}),
            decode: reencode::<BrowserHostNoticed>,
        },
        // PX-127: a pull-request comment as Review shows it: untrusted text,
        // what became of it, whether the agent has answered.
        Sample {
            name: "review_comment_thread_view",
            type_name: "modbit.v1.ReviewCommentThreadView",
            bytes: ReviewCommentThreadView {
                comment_id: 9_007_199_254_740_993,
                kind: "review".into(),
                author: "reviewer".into(),
                url: "https://github.test/o/r/pull/3#discussion_r11".into(),
                path: "src/app.ts".into(),
                line: 2,
                body: "@modbit please guard negatives".into(),
                trust: "UNTRUSTED_EXTERNAL_CONTENT".into(),
                disposition: "STEERED".into(),
                reason: String::new(),
                input_id: "forge-comment-11".into(),
                provenance: "forge_review_comment".into(),
                ingested_offset: 4096,
                answered: true,
                reported_back: false,
                created_at: "2026-10-07T20:20:26Z".into(),
            }
            .encode_to_vec(),
            expected: json!({
                "commentId": "9007199254740993", "kind": "review", "author": "reviewer",
                "url": "https://github.test/o/r/pull/3#discussion_r11",
                "path": "src/app.ts", "line": "2",
                "body": "@modbit please guard negatives",
                "trust": "UNTRUSTED_EXTERNAL_CONTENT", "disposition": "STEERED",
                "reason": "", "inputId": "forge-comment-11",
                "provenance": "forge_review_comment", "ingestedOffset": "4096",
                "answered": true, "reportedBack": false,
                "createdAt": "2026-10-07T20:20:26Z"
            }),
            decode: reencode::<ReviewCommentThreadView>,
        },
        // PX-056 (composer.proto): the model picker's variants and the typed block code.
        Sample {
            name: "model_variant_list",
            type_name: "modbit.v1.ModelVariantList",
            bytes: ModelVariantList {
                models: vec![ModelVariantsEntry {
                    endpoint: "openai".into(),
                    model: "gpt-5-mini".into(),
                    provider: "openai".into(),
                    reasoning: true,
                    vision: true,
                    context_tokens: 400_000,
                    default_effort: "medium".into(),
                    default_service_tier: String::new(),
                    variants: vec![
                        ModelVariantView {
                            effort: "medium".into(),
                            service_tier: String::new(),
                            label: "Medium effort".into(),
                            is_default: true,
                            raises_cost: false,
                        },
                        ModelVariantView {
                            effort: "high".into(),
                            service_tier: "priority".into(),
                            label: "High effort".into(),
                            is_default: false,
                            raises_cost: true,
                        },
                    ],
                    credential_available: true,
                    blocked_by_policy: "block=openai/gpt-5-mini".into(),
                    pin_refusal_code: "POLICY_BLOCKED".into(),
                    pin_allowed: false,
                }],
                objectives: vec!["COST".into(), "BALANCE".into(), "INTELLIGENCE".into()],
                default_objective: "BALANCE".into(),
            }
            .encode_to_vec(),
            expected: json!({
                "models": [{
                    "endpoint": "openai", "model": "gpt-5-mini", "provider": "openai",
                    "reasoning": true, "vision": true, "contextTokens": 400000,
                    "defaultEffort": "medium", "defaultServiceTier": "",
                    "variants": [
                        { "effort": "medium", "serviceTier": "", "label": "Medium effort", "isDefault": true, "raisesCost": false },
                        { "effort": "high", "serviceTier": "priority", "label": "High effort", "isDefault": false, "raisesCost": true }
                    ],
                    "credentialAvailable": true, "blockedByPolicy": "block=openai/gpt-5-mini",
                    "pinRefusalCode": "POLICY_BLOCKED", "pinAllowed": false
                }],
                "objectives": ["COST", "BALANCE", "INTELLIGENCE"],
                "defaultObjective": "BALANCE"
            }),
            decode: reencode::<ModelVariantList>,
        },
    ]
}
