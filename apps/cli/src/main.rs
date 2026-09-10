//! `modbit-cli` — headless thin SurfaceProtocol client (docs/29). It owns no
//! orchestration, context, memory, Git, recovery, policy or tool code and holds
//! no provider credentials; it spawns or attaches to a `modbit-core` and speaks
//! the authenticated local SurfaceProtocol.
//!
//! Usage:
//!   modbit-cli --data-dir <dir> session create
//!   modbit-cli --data-dir <dir> task create --session <hex-id> [--workspace <dir>] "<goal>"
//!   modbit-cli --data-dir <dir> session show --session <hex-id>
//!   modbit-cli --data-dir <dir> events tail --session <hex-id> [--after <offset>] [--count <n>]
//!   modbit-cli --data-dir <dir> tool list
//!   modbit-cli --data-dir <dir> tool invoke --session <hex-id> --task <hex-id> [--call <hex-id>] <tool> <arguments-json>
//!   modbit-cli --data-dir <dir> approval list --session <hex-id>
//!   modbit-cli --data-dir <dir> approval resolve --session <hex-id> --approval <hex-id> (approve|deny) [reason]
//!   modbit-cli --data-dir <dir> stop --session <hex-id> [reason]
//!   modbit-cli --data-dir <dir> receipts [--task <hex-id>]
//!   modbit-cli --data-dir <dir> lease list --task <hex-id>
//!   modbit-cli --data-dir <dir> task run --session <hex-id> --task <hex-id> [--endpoint <name>] [--model <id>] [--max-turns N] [--wait]
//!   modbit-cli --data-dir <dir> task cancel --session <hex-id> --task <hex-id>
//!   modbit-cli --data-dir <dir> task status --task <hex-id>
//!   modbit-cli --data-dir <dir> review show --task <hex-id>
//!   modbit-cli --data-dir <dir> review decide --session <hex-id> --task <hex-id> (accept|return) [--reject path#index ...] [note]
//!   modbit-cli --data-dir <dir> model list
//!   modbit-cli --data-dir <dir> model probe --endpoint <name> --model <id> [--tools] <prompt>
//!
//! M1.3 mode: each invocation spawns a Core for the data directory, runs one
//! command, and exits; the Core exits with the CLI. Attaching to a long-running
//! Core belongs to PX-000 (M2).

use std::io::{BufRead, BufReader};
use std::process::{Command, ExitCode, Stdio};

use modbit_protocol::client::Client;
use modbit_protocol::local::{ReadyLine, decode_hex, encode_hex};
use modbit_protocol::v1::{
    AcquireSessionLease, ApprovalList, ApprovalResolvedAck, AttachmentIngested, CancelTask,
    CapabilityLeaseList, ClientKind, CommandEnvelope, ContextInspectorView, CreateSession,
    CreateTask, DecideReview, EffectReceiptList, EmergencyStop, EmergencyStopped,
    GetCapabilityLeases, GetContextInspector, GetEffectReceipts, GetReviewBundle,
    GetSessionSnapshot, GetTaskEconomics, GetTaskStatus, HunkRef, Id, IngestAttachment, InvokeTool,
    LanguageList, ListApprovals, ListLanguages, ListModels, ListQuestions, ListTools, ModelList,
    ModelProbed, ProbeModel, QuestionList, QuestionResponded, ResolveApproval, RespondToQuestion,
    ReviewBundle, ReviewDecided, SessionCreated, SessionLeaseAcquired, SessionSnapshot,
    SetTaskSelection, StartTask, TaskCancelRequested, TaskCreated, TaskEconomicsView,
    TaskRunStarted, TaskSelectionRecorded, TaskStatus, ToolInvoked, ToolList, UndoPlanView,
    UndoToolCall,
};
use prost::Message;

/// Process exit code (docs: apps/cli/README.md). Set by task-state commands.
static EXIT_CODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Exit code for a task state: 0 done (ReadyForReview/Completed), 2 needs
/// input (Waiting: question or approval), 3 cancelled/failed, 4 still running.
fn exit_for_state(state: &str) -> u8 {
    match state {
        "ReadyForReview" | "Completed" => 0,
        "Waiting" => 2,
        "Cancelled" | "Failed" => 3,
        _ => 4,
    }
}

const USAGE: &str = "usage: modbit-cli --data-dir <dir> (session create | session show --session <id> | task create --session <id> [--workspace <dir>] <goal> | events tail --session <id> [--after N] [--count N] [--json] | task attach --session <id> --task <id> <file> | question list --task <id> | question answer --session <id> --task <id> --question <id> [--option <id>] [--endpoint <name>] [--model <id>] [--wait] [text] | tool list [--task <id>] | tool invoke --session <id> --task <id> [--call <id>] <tool> <arguments-json> | approval list --session <id> | approval resolve --session <id> --approval <id> (approve|deny) [reason] | stop --session <id> [reason] | receipts [--task <id>] | lease list --task <id> | task run --session <id> --task <id> [--endpoint <name>] [--model <id>] [--max-turns N] [--wait] | task cancel --session <id> --task <id> | task status --task <id> | review show --task <id> | review decide --session <id> --task <id> (accept|return) [--reject path#index ...] [note] | change undo --session <id> --task <id> --call <id> [--apply] | context show <task-id> | task economics --task <id> | task select --session <id> --task <id> [--path p]... [--lines a:b] [--symbol s] [--hunk path#index]... [--source review|editor|cli] | language list | model list | model probe --endpoint <name> --model <id> [--tools] <prompt>)";

fn parse_id(hex: &str) -> Result<Id, String> {
    let bytes = decode_hex(hex)
        .filter(|b| b.len() == 16)
        .ok_or_else(|| format!("`{hex}` is not a 32-hex-char id"))?;
    Ok(Id { value: bytes })
}

fn fresh_id() -> Id {
    Id {
        value: (0..16).map(|_| rand::random::<u8>()).collect(),
    }
}

fn envelope(command_type: &str, payload: Vec<u8>) -> CommandEnvelope {
    envelope_fenced(command_type, payload, None)
}

/// Mutating commands present the session lease generation (docs/13 fencing).
fn envelope_fenced(
    command_type: &str,
    payload: Vec<u8>,
    generation: Option<u64>,
) -> CommandEnvelope {
    CommandEnvelope {
        command_id: Some(fresh_id()),
        tenant_id: None,
        user_id: None,
        session_id: None,
        aggregate_id: None,
        expected_generation: generation,
        command_type: command_type.into(),
        schema_version: 1,
        payload,
        issued_at: None,
    }
}

/// Windows inherits every inheritable handle into a child, not just the stdio
/// we set: the detached Core would keep our caller's pipes open until it
/// idle-exits (or forever while it serves), so a caller reading our output
/// would hang. Clear the inherit flag on our std handles before spawning.
#[cfg(windows)]
#[allow(unsafe_code)]
fn stop_inheriting_std_handles() {
    use windows_sys::Win32::Foundation::{
        HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation,
    };
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };
    for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        // SAFETY: GetStdHandle/SetHandleInformation take and return plain
        // handles owned by this process; clearing the inherit flag has no
        // effect on the handle's validity.
        unsafe {
            let h = GetStdHandle(which);
            if !h.is_null() && h != INVALID_HANDLE_VALUE {
                SetHandleInformation(h, HANDLE_FLAG_INHERIT, 0);
            }
        }
    }
}

#[cfg(not(windows))]
fn stop_inheriting_std_handles() {}

fn spawn_core(data_dir: &str) -> Result<(std::process::Child, ReadyLine), String> {
    let exe = std::env::var("MODBIT_CORE_BIN").unwrap_or_else(|_| "modbit-core".into());
    stop_inheriting_std_handles();
    // The Core outlives this invocation so that later ones attach to it; it
    // exits on its own after 30s without a client. Its stderr goes to the
    // profile's core.log (inheriting ours would tie it to our caller's pipes).
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::path::Path::new(data_dir).join("core.log"))
        .map_err(|e| format!("opening core.log: {e}"))?;
    let mut child = Command::new(&exe)
        .arg("--data-dir")
        .arg(data_dir)
        .args(["--idle-exit-secs", "30"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(log))
        .spawn()
        .map_err(|e| format!("spawning `{exe}`: {e} (set MODBIT_CORE_BIN)"))?;
    let stdout = child.stdout.take().expect("piped stdout");
    let mut lines = BufReader::new(stdout).lines();
    let ready = loop {
        match lines.next() {
            Some(Ok(line)) => {
                if let Some(r) = ReadyLine::parse(&line) {
                    break r;
                }
            }
            _ => return Err("modbit-core exited before it was ready".into()),
        }
    };
    std::thread::spawn(move || for _ in lines {});
    Ok((child, ready))
}

async fn run(args: Vec<String>) -> Result<(), String> {
    let mut data_dir = None;
    let mut rest = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--data-dir" {
            data_dir = it.next();
        } else {
            rest.push(a);
        }
    }
    let data_dir = data_dir.ok_or(USAGE)?;
    let (child, ready) = attach_or_spawn(&data_dir).await?;
    let result = run_command(&ready, rest).await;
    // A spawned Core is left running for the next invocation (idle exit).
    drop(child);
    result
}

/// Attach to the profile's running Core through its owner-only `core.ready`
/// file, or spawn one. A spawn that loses the profile lock to a Core that is
/// just starting retries the attach (REQ-PX-000: concurrent headless clients).
async fn attach_or_spawn(
    data_dir: &str,
) -> Result<(Option<std::process::Child>, ReadyLine), String> {
    if let Some(r) = attach(data_dir).await {
        return Ok((None, r));
    }
    let mut last = String::new();
    for _ in 0..25 {
        match spawn_core(data_dir) {
            Ok((child, ready)) => return Ok((Some(child), ready)),
            Err(e) => {
                last = e;
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                if let Some(r) = attach(data_dir).await {
                    return Ok((None, r));
                }
            }
        }
    }
    Err(last)
}

async fn attach(data_dir: &str) -> Option<ReadyLine> {
    let text = std::fs::read_to_string(std::path::Path::new(data_dir).join("core.ready")).ok()?;
    let ready = ReadyLine::parse(text.trim())?;
    let secret = decode_hex(&ready.boot_secret_hex)?;
    // Prove the Core is alive and ours before using it.
    match Client::connect(
        &ready.endpoint,
        &secret,
        ClientKind::Cli,
        env!("CARGO_PKG_VERSION"),
    )
    .await
    {
        Ok(_) => Some(ready),
        Err(e) => {
            eprintln!("modbit-cli: core.ready present but attach failed ({e}); spawning");
            None
        }
    }
}

async fn run_command(ready: &ReadyLine, rest: Vec<String>) -> Result<(), String> {
    let secret = decode_hex(&ready.boot_secret_hex).ok_or("bad secret in ready line")?;
    let mut client = Client::connect(
        &ready.endpoint,
        &secret,
        ClientKind::Cli,
        env!("CARGO_PKG_VERSION"),
    )
    .await
    .map_err(|e| e.to_string())?;
    let words: Vec<&str> = rest.iter().map(String::as_str).collect();
    let opt = |flag: &str| {
        words
            .iter()
            .position(|w| *w == flag)
            .and_then(|i| words.get(i + 1).copied())
    };
    match words.as_slice() {
        ["session", "create", ..] => {
            let ack = client
                .command(envelope(
                    "CreateSession",
                    CreateSession { space_id: None }.encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: SessionCreated = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "session {}",
                encode_hex(&r.session_id.unwrap_or_default().value)
            );
        }
        ["session", "show", ..] => {
            let sid = parse_id(opt("--session").ok_or(USAGE)?)?;
            let ack = client
                .command(envelope(
                    "GetSessionSnapshot",
                    GetSessionSnapshot {
                        session_id: Some(sid),
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let s: SessionSnapshot = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "session state={} generation={} last_offset={}",
                s.state, s.generation, s.last_offset
            );
            for t in s.tasks {
                println!(
                    "task {} state={} generation={} goal={:?}",
                    encode_hex(&t.task_id.unwrap_or_default().value),
                    t.state,
                    t.generation,
                    t.goal_text
                );
            }
        }
        ["task", "create", ..] => {
            let sid = parse_id(opt("--session").ok_or(USAGE)?)?;
            let workspace_root = opt("--workspace").unwrap_or_default().to_owned();
            let mut goal_words = Vec::new();
            let mut skip = false;
            for w in words.iter().skip(2) {
                if skip {
                    skip = false;
                } else if *w == "--session" || *w == "--workspace" {
                    skip = true;
                } else {
                    goal_words.push(*w);
                }
            }
            let goal = goal_words.join(" ");
            if goal.is_empty() {
                return Err(USAGE.into());
            }
            let lease = acquire_lease(&mut client, &sid).await?;
            let ack = client
                .command(envelope_fenced(
                    "CreateTask",
                    CreateTask {
                        session_id: Some(sid),
                        goal_text: goal,
                        workspace_id: None,
                        execution_profile: String::new(),
                        origin: "cli".into(),
                        workspace_root,
                    }
                    .encode_to_vec(),
                    Some(lease),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: TaskCreated = Client::result(&ack).map_err(|e| e.to_string())?;
            println!("task {}", encode_hex(&r.task_id.unwrap_or_default().value));
        }
        ["events", "tail", ..] => {
            let sid = parse_id(opt("--session").ok_or(USAGE)?)?;
            let after: u64 = opt("--after")
                .map(|v| v.parse().map_err(|_| USAGE))
                .transpose()?
                .unwrap_or(0);
            let count: usize = opt("--count")
                .map(|v| v.parse().map_err(|_| USAGE))
                .transpose()?
                .unwrap_or(100);
            client
                .subscribe(sid, after)
                .await
                .map_err(|e| e.to_string())?;
            for _ in 0..count {
                match tokio::time::timeout(
                    std::time::Duration::from_millis(500),
                    client.next_event(),
                )
                .await
                {
                    Ok(Ok(Some(e))) => {
                        let ev = e.event.unwrap_or_default();
                        if words.contains(&"--json") {
                            // Inline payloads are unwrapped; object payloads keep their ref.
                            let mut payload =
                                serde_json::from_slice::<serde_json::Value>(&ev.payload)
                                    .unwrap_or_else(|_| {
                                        serde_json::Value::String(encode_hex(&ev.payload))
                                    });
                            if payload["kind"] == "inline"
                                && let Some(inner) = payload.get_mut("payload")
                            {
                                payload = inner.take();
                            }
                            println!(
                                "{}",
                                serde_json::json!({
                                    "offset": e.offset,
                                    "sequence": ev.sequence,
                                    "aggregate_type": ev.aggregate_type,
                                    "event_type": ev.event_type,
                                    "task_id": ev.task_id.as_ref().map(|t| encode_hex(&t.value)),
                                    "payload": payload,
                                })
                            );
                        } else {
                            println!("{} seq={} {}", e.offset, ev.sequence, ev.event_type);
                        }
                    }
                    Ok(Ok(None)) => break,
                    Ok(Err(e)) => return Err(e.to_string()),
                    Err(_) => break, // idle: nothing more right now
                }
            }
        }
        ["tool", "list"] => {
            let ack = client
                .command(envelope(
                    "ListTools",
                    ListTools {
                        task_id: opt("--task").map(parse_id).transpose()?,
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let list: ToolList = Client::result(&ack).map_err(|e| e.to_string())?;
            for t in list.tools {
                println!(
                    "tool {} v{} effect={} {}",
                    t.name, t.version, t.effect_class, t.description
                );
            }
        }
        ["tool", "invoke", ..] => {
            let sid = parse_id(opt("--session").ok_or(USAGE)?)?;
            let task_id = parse_id(opt("--task").ok_or(USAGE)?)?;
            let positional = positionals(&words, 2);
            let [tool_name, arguments_json] = positional.as_slice() else {
                return Err(USAGE.into());
            };
            let tool_call_id = match opt("--call") {
                Some(h) => parse_id(h)?,
                None => fresh_id(),
            };
            let lease = acquire_lease(&mut client, &sid).await?;
            let ack = client
                .command(envelope_fenced(
                    "InvokeTool",
                    InvokeTool {
                        task_id: Some(task_id),
                        tool_name: (*tool_name).into(),
                        arguments_json: (*arguments_json).into(),
                        tool_call_id: Some(tool_call_id),
                        output_budget_bytes: 64 * 1024,
                    }
                    .encode_to_vec(),
                    Some(lease),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: ToolInvoked = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "tool_call {} status={} error_code={:?} revision_after={} approval={} receipts={}",
                encode_hex(&r.tool_call_id.unwrap_or_default().value),
                r.status,
                r.error_code,
                r.workspace_revision_after,
                if r.approval_id.is_empty() {
                    "-"
                } else {
                    &r.approval_id
                },
                r.effect_receipt_ids.join(",")
            );
            if !r.structured_output_json.is_empty() {
                println!("{}", r.structured_output_json);
            }
            if !r.error_message.is_empty() {
                println!("error: {}", r.error_message);
            }
        }
        ["approval", "list", ..] => {
            let sid = parse_id(opt("--session").ok_or(USAGE)?)?;
            let ack = client
                .command(envelope(
                    "ListApprovals",
                    ListApprovals {
                        session_id: Some(sid),
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let list: ApprovalList = Client::result(&ack).map_err(|e| e.to_string())?;
            for a in list.approvals {
                println!(
                    "approval {} status={} tool={} effect={} task={} call={} intent={}",
                    encode_hex(&a.approval_id.unwrap_or_default().value),
                    a.status,
                    a.tool_name,
                    a.effect_class,
                    encode_hex(&a.task_id.unwrap_or_default().value),
                    encode_hex(&a.tool_call_id.unwrap_or_default().value),
                    a.intent_hash
                );
            }
        }
        ["approval", "resolve", ..] => {
            let sid = parse_id(opt("--session").ok_or(USAGE)?)?;
            let approval_id = parse_id(opt("--approval").ok_or(USAGE)?)?;
            let positional = positionals(&words, 2);
            let (verb, reason) = match positional.as_slice() {
                [v, rest @ ..] => (*v, rest.join(" ")),
                _ => return Err(USAGE.into()),
            };
            let approve = match verb {
                "approve" => true,
                "deny" => false,
                _ => return Err(USAGE.into()),
            };
            let lease = acquire_lease(&mut client, &sid).await?;
            let ack = client
                .command(envelope_fenced(
                    "ResolveApproval",
                    ResolveApproval {
                        approval_id: Some(approval_id),
                        approve,
                        reason,
                    }
                    .encode_to_vec(),
                    Some(lease),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: ApprovalResolvedAck = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "approval {} status={}",
                encode_hex(&r.approval_id.unwrap_or_default().value),
                r.status
            );
        }
        ["stop", ..] => {
            let sid = parse_id(opt("--session").ok_or(USAGE)?)?;
            let reason = positionals(&words, 1).join(" ");
            let lease = acquire_lease(&mut client, &sid).await?;
            let ack = client
                .command(envelope_fenced(
                    "EmergencyStop",
                    EmergencyStop {
                        session_id: Some(sid),
                        reason,
                    }
                    .encode_to_vec(),
                    Some(lease),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: EmergencyStopped = Client::result(&ack).map_err(|e| e.to_string())?;
            println!("emergency_stop leases_revoked={}", r.leases_revoked);
        }
        ["receipts", ..] => {
            let task_id = opt("--task").map(parse_id).transpose()?;
            let ack = client
                .command(envelope(
                    "GetEffectReceipts",
                    GetEffectReceipts { task_id }.encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: EffectReceiptList = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "receipt_chain valid={} count={}{}",
                r.chain_valid,
                r.receipts.len(),
                if r.detail.is_empty() {
                    String::new()
                } else {
                    format!(" detail={}", r.detail)
                }
            );
            for x in r.receipts {
                println!(
                    "receipt {} status={} call={} decision={} approval={} hash={} prev={}",
                    encode_hex(&x.effect_id.unwrap_or_default().value),
                    x.status,
                    encode_hex(&x.tool_call_id.unwrap_or_default().value),
                    x.policy_decision,
                    x.approval_id
                        .map(|a| encode_hex(&a.value))
                        .unwrap_or_else(|| "-".into()),
                    x.receipt_hash,
                    if x.previous_receipt_hash.is_empty() {
                        "-"
                    } else {
                        &x.previous_receipt_hash
                    }
                );
            }
        }
        ["lease", "list", ..] => {
            let task_id = parse_id(opt("--task").ok_or(USAGE)?)?;
            let ack = client
                .command(envelope(
                    "GetCapabilityLeases",
                    GetCapabilityLeases {
                        task_id: Some(task_id),
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: CapabilityLeaseList = Client::result(&ack).map_err(|e| e.to_string())?;
            for l in r.leases {
                println!(
                    "lease {} status={} profile={} ceiling={} ops={}{}",
                    encode_hex(&l.lease_id.unwrap_or_default().value),
                    l.status,
                    l.execution_profile,
                    l.effect_ceiling,
                    l.operations.join(","),
                    if l.revoke_reason.is_empty() {
                        String::new()
                    } else {
                        format!(" revoked={:?}", l.revoke_reason)
                    }
                );
            }
        }
        ["task", "run", ..] => {
            let sid = parse_id(opt("--session").ok_or(USAGE)?)?;
            let task_id = parse_id(opt("--task").ok_or(USAGE)?)?;
            let max_turns: u32 = opt("--max-turns")
                .map(|v| v.parse().map_err(|_| USAGE))
                .transpose()?
                .unwrap_or(0);
            let lease = acquire_lease(&mut client, &sid).await?;
            start_task(
                &mut client,
                &task_id,
                lease,
                opt("--endpoint").unwrap_or_default(),
                opt("--model").unwrap_or_default(),
                max_turns,
            )
            .await?;
            if words.contains(&"--wait") {
                wait_until_idle(&mut client, &task_id).await?;
            }
        }
        ["task", "attach", ..] => {
            let sid = parse_id(opt("--session").ok_or(USAGE)?)?;
            let task_id = parse_id(opt("--task").ok_or(USAGE)?)?;
            let file = positionals(&words, 2)
                .first()
                .copied()
                .ok_or(USAGE)?
                .to_owned();
            let data = std::fs::read(&file).map_err(|e| format!("reading {file}: {e}"))?;
            let filename = std::path::Path::new(&file)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let lease = acquire_lease(&mut client, &sid).await?;
            let ack = client
                .command(envelope_fenced(
                    "IngestAttachment",
                    IngestAttachment {
                        task_id: Some(task_id),
                        filename,
                        channel: "cli".into(),
                        data,
                    }
                    .encode_to_vec(),
                    Some(lease),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let a: AttachmentIngested = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "attachment {} kind={} mime={} content_ref={} offset={} replayed={}",
                a.attachment_id, a.kind, a.mime, a.content_ref, a.offset, a.replayed
            );
        }
        ["question", "list", ..] => {
            let task_id = parse_id(opt("--task").ok_or(USAGE)?)?;
            let ack = client
                .command(envelope(
                    "ListQuestions",
                    ListQuestions {
                        task_id: Some(task_id),
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let l: QuestionList = Client::result(&ack).map_err(|e| e.to_string())?;
            for q in &l.questions {
                println!(
                    "question {} answered={} reason={} flags={} option={} text={:?} {:?}",
                    q.question_id,
                    q.answered,
                    q.reason,
                    q.flags.join(","),
                    q.option_id,
                    q.text,
                    q.question
                );
                for o in &q.options {
                    println!("  option {} {:?}", o.id, o.label);
                }
            }
        }
        ["question", "answer", ..] => {
            let sid = parse_id(opt("--session").ok_or(USAGE)?)?;
            let task_id = parse_id(opt("--task").ok_or(USAGE)?)?;
            let question_id = opt("--question").ok_or(USAGE)?.to_owned();
            let text = positionals(&words, 2).join(" ");
            let lease = acquire_lease(&mut client, &sid).await?;
            let ack = client
                .command(envelope_fenced(
                    "RespondToQuestion",
                    RespondToQuestion {
                        task_id: Some(task_id.clone()),
                        question_id: question_id.clone(),
                        option_id: opt("--option").unwrap_or_default().to_owned(),
                        text,
                    }
                    .encode_to_vec(),
                    Some(lease),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: QuestionResponded = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "answered {} already_answered={}",
                r.question_id, r.already_answered
            );
            start_task(
                &mut client,
                &task_id,
                lease,
                opt("--endpoint").unwrap_or_default(),
                opt("--model").unwrap_or_default(),
                0,
            )
            .await?;
            if words.contains(&"--wait") {
                wait_until_idle(&mut client, &task_id).await?;
            }
        }
        ["task", "cancel", ..] => {
            let sid = parse_id(opt("--session").ok_or(USAGE)?)?;
            let task_id = parse_id(opt("--task").ok_or(USAGE)?)?;
            let lease = acquire_lease(&mut client, &sid).await?;
            let ack = client
                .command(envelope_fenced(
                    "CancelTask",
                    CancelTask {
                        task_id: Some(task_id),
                    }
                    .encode_to_vec(),
                    Some(lease),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: TaskCancelRequested = Client::result(&ack).map_err(|e| e.to_string())?;
            println!("cancel was_running={}", r.was_running);
        }
        ["task", "status", ..] => {
            let task_id = parse_id(opt("--task").ok_or(USAGE)?)?;
            let ack = client
                .command(envelope(
                    "GetTaskStatus",
                    GetTaskStatus {
                        task_id: Some(task_id),
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let st: TaskStatus = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "task state={} wait_reason={} run_state={} loop_alive={}",
                st.state, st.wait_reason, st.run_state, st.loop_alive
            );
            EXIT_CODE.store(
                exit_for_state(&st.state),
                std::sync::atomic::Ordering::SeqCst,
            );
        }
        ["change", "undo", ..] => {
            let session_id = parse_id(opt("--session").ok_or(USAGE)?)?;
            let task_id = parse_id(opt("--task").ok_or(USAGE)?)?;
            let call = parse_id(opt("--call").ok_or(USAGE)?)?;
            let apply = words.contains(&"--apply");
            let payload = UndoToolCall {
                task_id: Some(task_id),
                tool_call_id: Some(call),
                apply,
            }
            .encode_to_vec();
            let ack = if apply {
                let lease = acquire_lease(&mut client, &session_id).await?;
                client
                    .command(envelope_fenced("UndoToolCall", payload, Some(lease)))
                    .await
            } else {
                client.command(envelope("UndoToolCall", payload)).await
            }
            .map_err(|e| e.to_string())?;
            let v: UndoPlanView = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "undo applied={} steps={} refusals={} workspace_revision={}",
                v.applied,
                v.steps.len(),
                v.refusals.len(),
                v.workspace_revision
            );
            for st in &v.steps {
                println!(
                    "step {} {} expected={} restore={} result={}",
                    st.action, st.path, st.expected_content_hash, st.restore_ref, st.resulting_hash
                );
            }
            for r in &v.refusals {
                println!("refusal {} {} {}", r.code, r.path, r.detail);
            }
        }
        ["review", "show", ..] => {
            let task_id = parse_id(opt("--task").ok_or(USAGE)?)?;
            let ack = client
                .command(envelope(
                    "GetReviewBundle",
                    GetReviewBundle {
                        task_id: Some(task_id),
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let b: ReviewBundle = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "review state={} workspace_revision={} base_commit={} files={} receipts={}",
                b.task_state,
                b.workspace_revision,
                b.base_commit,
                b.files.len(),
                b.receipts
            );
            for f in &b.files {
                println!(
                    "file {} status={} hunks={} revision={}",
                    f.path,
                    f.status,
                    f.hunks.len(),
                    f.file_revision
                );
                for h in &f.hunks {
                    println!("  hunk {}#{} {}", f.path, h.index, h.header);
                    for l in &h.lines {
                        println!("    {l}");
                    }
                }
            }
            for v in &b.verification_runs {
                println!(
                    "verification {} stage={} status={} checks={}",
                    v.verification_run_id,
                    v.stage,
                    v.status,
                    v.check_ids.len()
                );
            }
            for a in &b.attributions {
                println!("attribution {a}");
            }
            for q in &b.quarantined {
                println!("quarantined {q}");
            }
            for i in &b.invariant_findings {
                println!("invariant {i}");
            }
            for e in &b.evidence_links {
                println!("evidence {e}");
            }
        }
        ["review", "decide", ..] => {
            let sid = parse_id(opt("--session").ok_or(USAGE)?)?;
            let task_id = parse_id(opt("--task").ok_or(USAGE)?)?;
            let mut rejected = Vec::new();
            let mut positional = Vec::new();
            let mut i = 2;
            while i < words.len() {
                match words[i] {
                    "--session" | "--task" => i += 2,
                    "--reject" => {
                        let r = words.get(i + 1).ok_or(USAGE)?;
                        let (path, idx) = r.rsplit_once('#').ok_or(USAGE)?;
                        rejected.push(HunkRef {
                            path: path.into(),
                            index: idx.parse().map_err(|_| USAGE)?,
                        });
                        i += 2;
                    }
                    w => {
                        positional.push(w);
                        i += 1;
                    }
                }
            }
            let (verb, note) = match positional.as_slice() {
                [v, rest @ ..] => (*v, rest.join(" ")),
                _ => return Err(USAGE.into()),
            };
            let decision = match verb {
                "accept" => "ACCEPT",
                "return" => "RETURN",
                _ => return Err(USAGE.into()),
            };
            let lease = acquire_lease(&mut client, &sid).await?;
            let ack = client
                .command(envelope_fenced(
                    "DecideReview",
                    DecideReview {
                        task_id: Some(task_id),
                        decision: decision.into(),
                        rejected,
                        note,
                        expected_workspace_revision: 0,
                    }
                    .encode_to_vec(),
                    Some(lease),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: ReviewDecided = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "review decided state={} commit={} reverted={} workspace_revision={}",
                r.task_state,
                if r.commit.is_empty() { "-" } else { &r.commit },
                r.reverted.join(","),
                r.workspace_revision
            );
        }
        ["context", "show", task] => {
            let ack = client
                .command(envelope(
                    "GetContextInspector",
                    GetContextInspector {
                        task_id: Some(parse_id(task)?),
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let v: ContextInspectorView = Client::result(&ack).map_err(|e| e.to_string())?;
            // The compaction and prompt-cache line stands on its own: a task
            // can have compacted its transcript without a pack of its own.
            if v.compaction_epochs > 0 || v.prefix_cache_misses > 0 {
                println!(
                    "compaction: epoch={} epochs={} compacted_entries={} manifest={} cache_hits={} cache_misses={}",
                    v.compaction_epoch,
                    v.compaction_epochs,
                    v.compacted_entries,
                    if v.manifest_ref.is_empty() {
                        "none".to_owned()
                    } else {
                        v.manifest_ref[..12.min(v.manifest_ref.len())].to_owned()
                    },
                    v.prefix_cache_hits,
                    v.prefix_cache_misses
                );
            }
            if v.pack_id.is_empty() {
                println!("context pack: none compiled for this task yet");
            } else {
                println!(
                    "context pack {} revision={} tokens={}/{} complete={} omitted={} ({} tokens) estimator={}",
                    &v.pack_id[..12.min(v.pack_id.len())],
                    v.workspace_revision,
                    v.token_used,
                    v.token_budget,
                    v.complete,
                    v.omitted_count,
                    v.omitted_tokens,
                    v.token_estimator
                );
                println!(
                    "prompt envelope: pack={} injected={} refused={} injected_tokens={}",
                    &v.context_pack_id[..12.min(v.context_pack_id.len())],
                    v.injected_refs.len(),
                    v.rejected_refs.len(),
                    v.injected_tokens
                );
                for e in &v.entries {
                    println!(
                        "entry {} {}{} reason={:?} sources={} freshness={} tokens={} revision={} injected={} used={}{}",
                        &e.entry_id[..8.min(e.entry_id.len())],
                        e.source_ref,
                        if e.line_end > 0 {
                            format!(" L{}-{}", e.line_start, e.line_end)
                        } else {
                            String::new()
                        },
                        e.reason,
                        e.sources.join(","),
                        e.freshness,
                        e.token_cost,
                        e.workspace_revision,
                        e.injected,
                        e.used,
                        if e.stub { " [stub]" } else { "" }
                    );
                }
                for p in &v.omitted_paths {
                    println!("omitted {p}");
                }
            }
        }
        ["task", "select", rest @ ..] => {
            // task select --task <id> [--path p]... [--lines a:b] [--symbol s]
            //             [--hunk path#index]... [--source review|editor|cli]
            let (mut task, mut session): (Option<&str>, Option<&str>) = (None, None);
            let (mut paths, mut hunks) = (Vec::new(), Vec::new());
            let (mut symbol, mut source) = (String::new(), "cli".to_owned());
            let (mut line_start, mut line_end) = (0_u32, 0_u32);
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                match *a {
                    "--task" => task = it.next().copied(),
                    "--session" => session = it.next().copied(),
                    "--path" => paths.push((*it.next().unwrap_or(&"")).to_owned()),
                    "--hunk" => hunks.push((*it.next().unwrap_or(&"")).to_owned()),
                    "--symbol" => symbol = (*it.next().unwrap_or(&"")).to_owned(),
                    "--source" => source = (*it.next().unwrap_or(&"cli")).to_owned(),
                    "--lines" => {
                        let v = *it.next().unwrap_or(&"");
                        let (a, b) = v.split_once(':').ok_or("--lines takes start:end")?;
                        line_start = a.parse().map_err(|_| "bad --lines start")?;
                        line_end = b.parse().map_err(|_| "bad --lines end")?;
                    }
                    other => return Err(format!("unknown flag `{other}`")),
                }
            }
            let task_id = parse_id(task.ok_or("--task required")?)?;
            let sid = parse_id(session.ok_or("--session required")?)?;
            let generation = acquire_lease(&mut client, &sid).await?;
            let ack = client
                .command(envelope_fenced(
                    "SetTaskSelection",
                    SetTaskSelection {
                        task_id: Some(task_id),
                        paths: paths.clone(),
                        symbol: symbol.clone(),
                        line_start,
                        line_end,
                        review_hunks: hunks.clone(),
                        source: source.clone(),
                    }
                    .encode_to_vec(),
                    Some(generation),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: TaskSelectionRecorded = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "selection recorded offset={} paths={} symbol={} hunks={} source={}",
                r.offset,
                if paths.is_empty() {
                    "-".to_owned()
                } else {
                    paths.join(",")
                },
                if symbol.is_empty() { "-" } else { &symbol },
                if hunks.is_empty() {
                    "-".to_owned()
                } else {
                    hunks.join(",")
                },
                source
            );
        }
        ["task", "economics", "--task", tid] => {
            let ack = client
                .command(envelope(
                    "GetTaskEconomics",
                    GetTaskEconomics {
                        task_id: Some(parse_id(tid)?),
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let v: TaskEconomicsView = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "task {} state={} verified={} checks_passed={}/{}",
                &v.task_id[..8.min(v.task_id.len())],
                v.state,
                v.verified,
                v.checks_passed,
                v.checks_passed + v.checks_failed
            );
            println!(
                "model {} calls={} input_tokens={} cached_input={} output_tokens={} cost_usd={}",
                if v.model.is_empty() {
                    "(none)"
                } else {
                    v.model.as_str()
                },
                v.model_calls,
                v.input_tokens,
                v.cached_input_tokens,
                v.output_tokens,
                if v.pricing_known == 1 {
                    format!("{:.6} (catalog list prices, no cache discount)", v.cost_usd)
                } else {
                    "unknown (the model is not in a registered catalog)".to_owned()
                }
            );
            println!(
                "time wall_ms={} model_ms={} tool_ms={} tool_calls={}",
                v.wall_ms, v.model_ms, v.tool_ms, v.tool_calls
            );
            println!(
                "context injected_tokens={} prefix_cache={}/{} epochs={} compacted_entries={}",
                v.context_tokens_injected,
                v.prefix_cache_hits,
                v.prefix_cache_hits + v.prefix_cache_misses,
                v.compaction_epochs,
                v.compacted_entries
            );
        }
        ["language", "list"] => {
            let ack = client
                .command(envelope("ListLanguages", ListLanguages {}.encode_to_vec()))
                .await
                .map_err(|e| e.to_string())?;
            let r: LanguageList = Client::result(&ack).map_err(|e| e.to_string())?;
            for l in r.languages {
                println!(
                    "language {} tier={} label={:?} fixture={} proven={} provisional={} not_claimed={}",
                    l.language,
                    l.tier,
                    l.label,
                    if l.fixture.is_empty() {
                        "-"
                    } else {
                        &l.fixture
                    },
                    l.proven.join(","),
                    l.provisional.join(","),
                    l.not_claimed.join(",")
                );
            }
        }
        ["model", "list"] => {
            let ack = client
                .command(envelope("ListModels", ListModels {}.encode_to_vec()))
                .await
                .map_err(|e| e.to_string())?;
            let r: ModelList = Client::result(&ack).map_err(|e| e.to_string())?;
            for m in r.models {
                println!(
                    "model {}/{} provider={} context={} tools={} vision={} reasoning={} credential={}",
                    m.endpoint,
                    m.model,
                    m.provider,
                    m.context_tokens,
                    m.tools,
                    m.vision,
                    m.reasoning,
                    m.credential_available
                );
            }
            for h in r.health {
                println!(
                    "health {} requests={} successes={} failures={} interruptions={} rate_limited={} first_token_ms={}",
                    h.endpoint,
                    h.requests,
                    h.successes,
                    h.failures,
                    h.interruptions,
                    h.rate_limited,
                    h.last_first_token_ms
                );
            }
        }
        ["model", "probe", ..] => {
            let endpoint = opt("--endpoint").ok_or(USAGE)?.to_owned();
            let model = opt("--model").ok_or(USAGE)?.to_owned();
            let with_tools = words.contains(&"--tools");
            let prompt = positionals(&words, 2)
                .into_iter()
                .filter(|w| *w != "--tools")
                .collect::<Vec<_>>()
                .join(" ");
            let ack = client
                .command(envelope(
                    "ProbeModel",
                    ProbeModel {
                        endpoint,
                        model,
                        prompt,
                        with_tools,
                        timeout_ms: 60_000,
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: ModelProbed = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "probe status={} stop={} error={:?} tokens={}/{} cached={} tool_call={} route={}",
                r.status,
                r.stop_reason,
                r.error_code,
                r.input_tokens,
                r.output_tokens,
                r.cached_input_tokens,
                if r.tool_call_name.is_empty() {
                    "-".to_owned()
                } else {
                    format!("{}{}", r.tool_call_name, r.tool_call_arguments_json)
                },
                r.route_json
            );
            if !r.text.is_empty() {
                println!("{}", r.text);
            }
            if !r.error_message.is_empty() {
                println!("error: {}", r.error_message);
            }
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

/// Positional words after `skip` leading words, dropping `--flag value` pairs.
fn positionals<'a>(words: &[&'a str], skip: usize) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut skip_next = false;
    for w in words.iter().skip(skip) {
        if skip_next {
            skip_next = false;
        } else if *w == "--tools" || *w == "--wait" {
            // bare flag
        } else if w.starts_with("--") {
            skip_next = true;
        } else {
            out.push(*w);
        }
    }
    out
}

/// The CLI becomes the session's single mutation owner for this invocation.
async fn start_task(
    client: &mut Client,
    task_id: &Id,
    lease: u64,
    endpoint: &str,
    model: &str,
    max_turns: u32,
) -> Result<(), String> {
    let ack = client
        .command(envelope_fenced(
            "StartTask",
            StartTask {
                task_id: Some(task_id.clone()),
                endpoint: endpoint.to_owned(),
                model: model.to_owned(),
                max_turns,
                max_tool_calls: 0,
                max_no_progress_turns: 0,
            }
            .encode_to_vec(),
            Some(lease),
        ))
        .await
        .map_err(|e| e.to_string())?;
    let r: TaskRunStarted = Client::result(&ack).map_err(|e| e.to_string())?;
    println!(
        "run {} resumed={} endpoint={} model={}",
        encode_hex(&r.run_id.unwrap_or_default().value),
        r.resumed,
        r.endpoint,
        r.model
    );
    Ok(())
}

/// Poll until the agent loop is idle, print the state and set the exit code
/// (0 done, 2 needs input, 3 cancelled/failed): headless use never hangs on
/// a question or an approval.
async fn wait_until_idle(client: &mut Client, task_id: &Id) -> Result<(), String> {
    loop {
        let ack = client
            .command(envelope(
                "GetTaskStatus",
                GetTaskStatus {
                    task_id: Some(task_id.clone()),
                }
                .encode_to_vec(),
            ))
            .await
            .map_err(|e| e.to_string())?;
        let st: TaskStatus = Client::result(&ack).map_err(|e| e.to_string())?;
        if !st.loop_alive {
            println!(
                "task state={} wait_reason={} run_state={}",
                st.state, st.wait_reason, st.run_state
            );
            EXIT_CODE.store(
                exit_for_state(&st.state),
                std::sync::atomic::Ordering::SeqCst,
            );
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    Ok(())
}

async fn acquire_lease(client: &mut Client, sid: &Id) -> Result<u64, String> {
    let lease = client
        .command(envelope(
            "AcquireSessionLease",
            AcquireSessionLease {
                session_id: Some(sid.clone()),
                owner: format!("modbit-cli {}", env!("CARGO_PKG_VERSION")),
            }
            .encode_to_vec(),
        ))
        .await
        .map_err(|e| e.to_string())?;
    let lease: SessionLeaseAcquired = Client::result(&lease).map_err(|e| e.to_string())?;
    Ok(lease.lease_generation)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    match rt.block_on(run(args)) {
        Ok(()) => ExitCode::from(EXIT_CODE.load(std::sync::atomic::Ordering::SeqCst)),
        Err(e) => {
            eprintln!("modbit-cli: {e}");
            ExitCode::from(1)
        }
    }
}
