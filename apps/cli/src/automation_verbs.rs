//! `modbit-cli automation ...` (PX-086; docs/68): the headless verbs over the
//! same SurfaceProtocol commands the desktop calls. The CLI holds no
//! definition store, no scheduler and no policy: it reads a definition file,
//! sends the command and prints what the Core answered. A definition is data
//! the Core validates and hashes; enabling one is the person's explicit act,
//! naming the exact version and hash they read.
//!
//! ```text
//! automation list     [--json]
//! automation show     <id> [--json]
//! automation validate <file>
//! automation create   --workspace <dir> [--command-id <hex>] <file>
//! automation update   <id> <file>
//! automation load     --workspace <dir>
//! automation enable   <id> --version N --hash <sha256> (--as-shown | [--effects e]
//!                     [--capability c]... [--path p]... [--host h]...)
//! automation disable  <id> [note]
//! automation run      <id> [--trigger t] [--input k=v]... [--event-id id] [--test]
//!                     [--payload-file f --source forge|webhook --event name]
//! automation history  [<id>] [--task <hex>] [--limit N] [--json]
//! automation pause    (<id>|all) [note]
//! automation resume   (<id>|all)
//! automation kill     (<id>|all) [note]
//! automation ack      <dispatch-key>
//! automation fire     --source forge|webhook --event name --delivery <id> <payload-file>
//! ```

use modbit_protocol::client::Client;
use modbit_protocol::v1::{
    AckAutomationAttention, AutomationDetail, AutomationFireReport, AutomationList,
    AutomationRunList, AutomationRunStarted, AutomationRunView, AutomationValidation,
    AutomationView, CreateAutomation, DisableAutomation, EnableAutomation, FireAutomationEvent,
    GetAutomation, KillAutomation, KillReport, ListAutomationRuns, ListAutomations,
    LoadRepositoryAutomations, PauseAutomation, RepositoryAutomations, RunAutomation,
    UpdateAutomation, ValidateAutomation,
};
use prost::Message;
use serde_json::json;

use crate::{USAGE, envelope, fresh_id, parse_id};

#[derive(Default)]
struct Args {
    workspace: Option<String>,
    command_id: Option<String>,
    version: Option<u32>,
    hash: Option<String>,
    effects: Option<String>,
    capabilities: Vec<String>,
    paths: Vec<String>,
    hosts: Vec<String>,
    as_shown: bool,
    trigger: Option<String>,
    inputs: Vec<String>,
    event_id: Option<String>,
    test: bool,
    payload_file: Option<String>,
    source: Option<String>,
    event: Option<String>,
    delivery: Option<String>,
    task: Option<String>,
    limit: Option<u32>,
    json: bool,
    positional: Vec<String>,
}

fn parse(words: &[&str]) -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = words.iter().copied();
    let value = |it: &mut std::iter::Copied<std::slice::Iter<'_, &str>>, flag: &str| {
        it.next()
            .map(str::to_owned)
            .ok_or_else(|| format!("{flag} takes a value"))
    };
    while let Some(w) = it.next() {
        match w {
            "--workspace" => a.workspace = Some(value(&mut it, w)?),
            "--command-id" => a.command_id = Some(value(&mut it, w)?),
            "--version" => {
                a.version = Some(
                    value(&mut it, w)?
                        .parse()
                        .map_err(|_| "--version takes a number")?,
                );
            }
            "--hash" => a.hash = Some(value(&mut it, w)?),
            "--effects" => a.effects = Some(value(&mut it, w)?),
            "--capability" => a.capabilities.push(value(&mut it, w)?),
            "--path" => a.paths.push(value(&mut it, w)?),
            "--host" => a.hosts.push(value(&mut it, w)?),
            "--as-shown" => a.as_shown = true,
            "--trigger" => a.trigger = Some(value(&mut it, w)?),
            "--input" => a.inputs.push(value(&mut it, w)?),
            "--event-id" => a.event_id = Some(value(&mut it, w)?),
            "--test" => a.test = true,
            "--payload-file" => a.payload_file = Some(value(&mut it, w)?),
            "--source" => a.source = Some(value(&mut it, w)?),
            "--event" => a.event = Some(value(&mut it, w)?),
            "--delivery" => a.delivery = Some(value(&mut it, w)?),
            "--task" => a.task = Some(value(&mut it, w)?),
            "--limit" => {
                a.limit = Some(
                    value(&mut it, w)?
                        .parse()
                        .map_err(|_| "--limit takes a number")?,
                );
            }
            "--json" => a.json = true,
            other if other.starts_with("--") => {
                return Err(format!("unknown flag {other}\n{USAGE}"));
            }
            other => a.positional.push(other.to_owned()),
        }
    }
    Ok(a)
}

fn read(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("reading `{path}`: {e}"))
}

async fn send<R: Message + Default>(
    client: &mut Client,
    command_type: &str,
    msg: impl Message,
    command_id: Option<&str>,
) -> Result<R, String> {
    let mut env = envelope(command_type, msg.encode_to_vec());
    if let Some(id) = command_id {
        env.command_id = Some(parse_id(id)?);
    }
    let ack = client.command(env).await.map_err(|e| e.to_string())?;
    Client::result(&ack).map_err(|e| e.to_string())
}

fn view_line(v: &AutomationView) -> String {
    let next = v
        .triggers
        .iter()
        .filter(|t| t.next_due_ms > 0)
        .map(|t| t.next_due_ms)
        .min()
        .map_or("-".to_owned(), |n| n.to_string());
    format!(
        "automation {} {} state={} version={} source={} principal={} effects={} next_due_ms={} last={} failures={}",
        v.automation_id,
        v.name,
        v.state,
        v.current_version,
        v.source_kind,
        v.principal,
        v.effects,
        next,
        if v.last_run_status.is_empty() {
            "-"
        } else {
            &v.last_run_status
        },
        v.consecutive_failures
    )
}

fn view_json(v: &AutomationView) -> serde_json::Value {
    json!({
        "automation_id": v.automation_id, "name": v.name, "state": v.state,
        "version": v.current_version, "definition_hash": v.definition_hash,
        "source_kind": v.source_kind, "source_path": v.source_path,
        "principal": v.principal, "effects": v.effects, "capabilities": v.capabilities,
        "paths": v.paths, "hosts": v.hosts, "paused": v.paused,
        "disabled_reason": v.disabled_reason, "content_changed": v.content_changed,
        "triggers": v.triggers.iter().map(|t| json!({
            "id": t.id, "kind": t.kind, "summary": t.summary, "next_due_ms": t.next_due_ms
        })).collect::<Vec<_>>(),
        "enabled": v.enabled.as_ref().map(|e| json!({
            "version": e.version, "definition_hash": e.definition_hash,
            "approver": e.approver, "approved_ms": e.approved_ms
        })),
    })
}

fn run_line(r: &AutomationRunView) -> String {
    format!(
        "run {} {} v{} {} {} status={} reason={} task={} fired_ms={} cost_minor={}{}{}",
        &r.dispatch_key[..r.dispatch_key.len().min(12)],
        r.name,
        r.version,
        r.trigger_kind,
        r.event_id,
        r.status,
        if r.reason.is_empty() { "-" } else { &r.reason },
        if r.task_id.is_empty() {
            "-"
        } else {
            &r.task_id
        },
        r.fired_ms,
        r.cost_minor,
        if r.catch_up { " catch-up" } else { "" },
        if r.test { " test" } else { "" },
    )
}

fn print_validation(v: &AutomationValidation) {
    if v.ok {
        println!(
            "valid {} hash={} effects={} needs_listed_approval={}",
            v.name, v.definition_hash, v.effects, v.needs_listed_approval
        );
        for t in &v.triggers {
            println!(
                "  trigger {} {} {}{}",
                t.id,
                t.kind,
                t.summary,
                if t.next_due_ms > 0 {
                    format!(" next_due_ms={}", t.next_due_ms)
                } else {
                    String::new()
                }
            );
        }
    } else {
        println!("invalid");
        for i in &v.issues {
            println!("  {} {}: {}", i.path, i.code, i.message);
        }
    }
}

/// `automation ...`.
pub async fn run(client: &mut Client, words: &[&str]) -> Result<(), String> {
    let (verb, rest) = words.split_first().ok_or(USAGE)?;
    let a = parse(rest)?;
    let id = || a.positional.first().cloned().ok_or(USAGE.to_owned());
    match *verb {
        "list" => {
            let l: AutomationList =
                send(client, "ListAutomations", ListAutomations {}, None).await?;
            if a.json {
                println!(
                    "{}",
                    json!({
                        "global_paused": l.global_paused, "now_ms": l.now_ms, "clock": l.clock,
                        "automations": l.automations.iter().map(view_json).collect::<Vec<_>>(),
                        "attention": l.attention.iter().map(|x| json!({
                            "kind": x.kind, "automation_id": x.automation_id, "name": x.name,
                            "dispatch_key": x.dispatch_key, "reason": x.reason, "action": x.action
                        })).collect::<Vec<_>>(),
                    })
                );
                return Ok(());
            }
            println!(
                "{} automation(s) global_paused={} clock={} now_ms={}",
                l.automations.len(),
                l.global_paused,
                l.clock,
                l.now_ms
            );
            for v in &l.automations {
                println!("{}", view_line(v));
            }
            for x in &l.attention {
                println!(
                    "attention {} {} {} -> {}",
                    x.kind, x.name, x.reason, x.action
                );
            }
        }
        "show" => {
            let d: AutomationDetail = send(
                client,
                "GetAutomation",
                GetAutomation {
                    automation_id: id()?,
                },
                None,
            )
            .await?;
            let v = d.view.unwrap_or_default();
            if a.json {
                println!("{}", view_json(&v));
                return Ok(());
            }
            println!("{}", view_line(&v));
            println!("hash {}", v.definition_hash);
            println!("workspace {}", v.workspace_root);
            if v.needs_listed_approval {
                println!(
                    "approval must list: effects={} capabilities={:?} paths={:?} hosts={:?}",
                    v.effects, v.capabilities, v.paths, v.hosts
                );
            }
            for t in &v.triggers {
                println!(
                    "  trigger {} {} {} next_due_ms={}",
                    t.id, t.kind, t.summary, t.next_due_ms
                );
            }
            if let Some(e) = &v.enabled {
                println!(
                    "enabled version={} hash={} approver={} approved_ms={}",
                    e.version, e.definition_hash, e.approver, e.approved_ms
                );
            }
            if !v.disabled_reason.is_empty() {
                println!("disabled {} {}", v.disabled_reason, v.disabled_detail);
            }
            println!("{}", v.definition_json);
        }
        "validate" => {
            let v: AutomationValidation = send(
                client,
                "ValidateAutomation",
                ValidateAutomation {
                    definition_json: read(&id()?)?,
                },
                None,
            )
            .await?;
            print_validation(&v);
            if !v.ok {
                return Err("the definition is not valid".into());
            }
        }
        "create" => {
            let v: AutomationView = send(
                client,
                "CreateAutomation",
                CreateAutomation {
                    definition_json: read(&id()?)?,
                    workspace_root: a.workspace.clone().ok_or(USAGE)?,
                },
                a.command_id.as_deref(),
            )
            .await?;
            println!("{}", view_line(&v));
            println!("hash {}", v.definition_hash);
        }
        "update" => {
            let (aid, file) = (
                a.positional.first().ok_or(USAGE)?.clone(),
                a.positional.get(1).ok_or(USAGE)?.clone(),
            );
            let v: AutomationView = send(
                client,
                "UpdateAutomation",
                UpdateAutomation {
                    automation_id: aid,
                    definition_json: read(&file)?,
                },
                a.command_id.as_deref(),
            )
            .await?;
            println!("{}", view_line(&v));
            println!("hash {}", v.definition_hash);
        }
        "load" => {
            let r: RepositoryAutomations = send(
                client,
                "LoadRepositoryAutomations",
                LoadRepositoryAutomations {
                    workspace_root: a.workspace.clone().ok_or(USAGE)?,
                },
                None,
            )
            .await?;
            for v in &r.loaded {
                println!("{} file={}", view_line(v), v.source_path);
                println!("  hash {}", v.definition_hash);
            }
            for p in &r.problems {
                println!("problem {} {}: {}", p.path, p.code, p.message);
            }
        }
        "enable" => {
            let aid = id()?;
            let version = a
                .version
                .ok_or("--version is required: name the version you read")?;
            let hash = a
                .hash
                .clone()
                .ok_or("--hash is required: name the hash you read")?;
            let (mut effects, mut capabilities, mut paths, mut hosts) = (
                a.effects.clone().unwrap_or_default(),
                a.capabilities.clone(),
                a.paths.clone(),
                a.hosts.clone(),
            );
            if a.as_shown {
                // The lists the Core shows for that version; the hash still
                // binds the exact content.
                let d: AutomationDetail = send(
                    client,
                    "GetAutomation",
                    GetAutomation {
                        automation_id: aid.clone(),
                    },
                    None,
                )
                .await?;
                let v = d.view.unwrap_or_default();
                effects = v.effects;
                capabilities = v.capabilities;
                paths = v.paths;
                hosts = v.hosts;
            } else if effects.is_empty() {
                effects = "read_only".into();
            }
            let v: AutomationView = send(
                client,
                "EnableAutomation",
                EnableAutomation {
                    automation_id: aid,
                    version,
                    definition_hash: hash,
                    effects,
                    capabilities,
                    paths,
                    hosts,
                },
                a.command_id.as_deref(),
            )
            .await?;
            println!("{}", view_line(&v));
        }
        "disable" => {
            let v: AutomationView = send(
                client,
                "DisableAutomation",
                DisableAutomation {
                    automation_id: id()?,
                    note: a.positional.get(1..).unwrap_or_default().join(" "),
                },
                None,
            )
            .await?;
            println!("{}", view_line(&v));
        }
        "run" => {
            let mut inputs = serde_json::Map::new();
            for kv in &a.inputs {
                let (k, v) = kv
                    .split_once('=')
                    .ok_or_else(|| format!("--input takes name=value, not `{kv}`"))?;
                inputs.insert(
                    k.to_owned(),
                    serde_json::from_str(v).unwrap_or_else(|_| v.into()),
                );
            }
            let r: AutomationRunStarted = send(
                client,
                "RunAutomation",
                RunAutomation {
                    automation_id: id()?,
                    trigger_id: a.trigger.clone().unwrap_or_default(),
                    inputs_json: if inputs.is_empty() {
                        String::new()
                    } else {
                        serde_json::Value::Object(inputs).to_string()
                    },
                    test: a.test,
                    payload_json: match &a.payload_file {
                        Some(f) => read(f)?,
                        None => String::new(),
                    },
                    source: a.source.clone().unwrap_or_default(),
                    event: a.event.clone().unwrap_or_default(),
                    event_id: a.event_id.clone().unwrap_or_default(),
                },
                a.command_id.as_deref(),
            )
            .await?;
            println!(
                "run key={} status={} reason={} task={}{}",
                r.dispatch_key,
                r.status,
                if r.reason.is_empty() { "-" } else { &r.reason },
                if r.task_id.is_empty() {
                    "-"
                } else {
                    &r.task_id
                },
                if r.would_skip_by_filter {
                    " the trigger's filters would have skipped this payload"
                } else {
                    ""
                }
            );
            if !r.detail.is_empty() {
                println!("  {}", r.detail);
            }
        }
        "history" => {
            let l: AutomationRunList = send(
                client,
                "ListAutomationRuns",
                ListAutomationRuns {
                    automation_id: a.positional.first().cloned().unwrap_or_default(),
                    task_id: a.task.clone().unwrap_or_default(),
                    limit: a.limit.unwrap_or(0),
                },
                None,
            )
            .await?;
            if a.json {
                println!(
                    "{}",
                    serde_json::Value::Array(
                        l.runs
                            .iter()
                            .map(|r| json!({
                                "dispatch_key": r.dispatch_key, "automation_id": r.automation_id,
                                "name": r.name, "version": r.version, "trigger_id": r.trigger_id,
                                "trigger_kind": r.trigger_kind, "event_id": r.event_id,
                                "status": r.status, "reason": r.reason, "detail": r.detail,
                                "task_id": r.task_id, "session_id": r.session_id,
                                "principal": r.principal, "fired_ms": r.fired_ms,
                                "finished_ms": r.finished_ms, "cost_minor": r.cost_minor,
                                "test": r.test, "catch_up": r.catch_up, "missed": r.missed,
                                "findings": r.findings, "outputs": r.outputs_json,
                            }))
                            .collect()
                    )
                );
                return Ok(());
            }
            for r in &l.runs {
                println!("{}", run_line(r));
            }
        }
        "pause" | "resume" => {
            let paused = *verb == "pause";
            let aid = target(&a)?;
            let _: AutomationList = send_pause(client, &aid, paused, &a).await?;
            println!(
                "{} {}",
                if paused { "paused" } else { "resumed" },
                if aid.is_empty() {
                    "all automations"
                } else {
                    &aid
                }
            );
        }
        "kill" => {
            let aid = target(&a)?;
            let r: KillReport = send(
                client,
                "KillAutomation",
                KillAutomation {
                    automation_id: aid,
                    note: a.positional.get(1..).unwrap_or_default().join(" "),
                },
                a.command_id.as_deref(),
            )
            .await?;
            println!(
                "killed cancelled_runs={} dropped_queued={} paused={}",
                r.cancelled_runs, r.dropped_queued, r.paused
            );
        }
        "ack" => {
            let ack = client
                .command(envelope(
                    "AckAutomationAttention",
                    AckAutomationAttention {
                        dispatch_key: id()?,
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            println!("acknowledged status={}", ack.status);
        }
        "fire" => {
            let r: AutomationFireReport = send(
                client,
                "FireAutomationEvent",
                FireAutomationEvent {
                    source: a.source.clone().ok_or(USAGE)?,
                    event: a.event.clone().ok_or(USAGE)?,
                    delivery_id: a
                        .delivery
                        .clone()
                        .unwrap_or_else(|| modbit_protocol::local::encode_hex(&fresh_id().value)),
                    payload_json: read(&id()?)?,
                },
                None,
            )
            .await?;
            println!("matched={}", r.matched);
            for f in &r.fired {
                println!(
                    "  {} trigger={} key={} status={} reason={}",
                    f.automation_id, f.trigger_id, f.dispatch_key, f.status, f.reason
                );
            }
        }
        _ => return Err(USAGE.to_owned()),
    }
    Ok(())
}

/// The definition a switch names, or `all` for every one (the empty id).
fn target(a: &Args) -> Result<String, String> {
    match a.positional.first().map(String::as_str) {
        Some("all") => Ok(String::new()),
        Some(id) => Ok(id.to_owned()),
        None => Err(format!("name an automation id, or `all`\n{USAGE}")),
    }
}

async fn send_pause(
    client: &mut Client,
    automation_id: &str,
    paused: bool,
    a: &Args,
) -> Result<AutomationList, String> {
    let mut env = envelope(
        "PauseAutomation",
        PauseAutomation {
            automation_id: automation_id.to_owned(),
            paused,
            note: a.positional.get(1..).unwrap_or_default().join(" "),
        }
        .encode_to_vec(),
    );
    if let Some(id) = a.command_id.as_deref() {
        env.command_id = Some(parse_id(id)?);
    }
    let ack = client.command(env).await.map_err(|e| e.to_string())?;
    // A definition's answer is its view; the global switch answers the list.
    if automation_id.is_empty() {
        Client::result(&ack).map_err(|e| e.to_string())
    } else {
        Ok(AutomationList::default())
    }
}
