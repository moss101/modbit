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
    AcquireSessionLease, ApprovalList, ApprovalResolvedAck, CapabilityLeaseList, ClientKind,
    CommandEnvelope, CreateSession, CreateTask, EffectReceiptList, EmergencyStop, EmergencyStopped,
    GetCapabilityLeases, GetEffectReceipts, GetSessionSnapshot, Id, InvokeTool, ListApprovals,
    ListModels, ListTools, ModelList, ModelProbed, ProbeModel, ResolveApproval, SessionCreated,
    SessionLeaseAcquired, SessionSnapshot, TaskCreated, ToolInvoked, ToolList,
};
use prost::Message;

const USAGE: &str = "usage: modbit-cli --data-dir <dir> (session create | session show --session <id> | task create --session <id> [--workspace <dir>] <goal> | events tail --session <id> [--after N] [--count N] | tool list | tool invoke --session <id> --task <id> [--call <id>] <tool> <arguments-json> | approval list --session <id> | approval resolve --session <id> --approval <id> (approve|deny) [reason] | stop --session <id> [reason] | receipts [--task <id>] | lease list --task <id> | model list | model probe --endpoint <name> --model <id> [--tools] <prompt>)";

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

fn spawn_core(data_dir: &str) -> Result<(std::process::Child, ReadyLine), String> {
    let exe = std::env::var("MODBIT_CORE_BIN").unwrap_or_else(|_| "modbit-core".into());
    let mut child = Command::new(&exe)
        .arg("--data-dir")
        .arg(data_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
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
    let (mut child, ready) = spawn_core(&data_dir)?;
    let result = run_command(&ready, rest).await;
    let _ = child.kill();
    let _ = child.wait();
    result
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
                        println!("{} seq={} {}", e.offset, ev.sequence, ev.event_type);
                    }
                    Ok(Ok(None)) => break,
                    Ok(Err(e)) => return Err(e.to_string()),
                    Err(_) => break, // idle: nothing more right now
                }
            }
        }
        ["tool", "list"] => {
            let ack = client
                .command(envelope("ListTools", ListTools {}.encode_to_vec()))
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
        } else if *w == "--tools" {
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
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("modbit-cli: {e}");
            ExitCode::from(1)
        }
    }
}
