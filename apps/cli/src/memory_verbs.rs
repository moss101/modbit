//! `modbit-cli memory ...` (PX-113; docs/19 "Engineering Memory"): the
//! headless verbs over the same SurfaceProtocol commands the desktop calls —
//! `ListMemory`, `ProposeMemory`, `PromoteMemory`, `EditMemory`,
//! `ForgetMemory`. The CLI holds no memory logic: it decodes arguments, sends
//! the command under the session lease and prints what the Core answered.
//! Every mutation is an event on the Core's log; a retry of the same command
//! id replays.
//!
//! ```text
//! memory list    --task <id> [--status s]... [--scope kind]... [--text t] [--json]
//! memory show    --task <id> <memory-id>
//! memory propose --session <id> --task <id> [--scope s] --type t --topic x
//!                [--confidence f] [--ttl-ms n] [--sensitive] <content>
//! memory promote --session <id> --task <id> <memory-id>
//! memory edit    --session <id> --task <id> <memory-id> [--topic t] [--content c]
//!                [--confidence f] [--ttl-ms n | --clear-ttl] [--sensitive | --not-sensitive]
//! memory forget  --session <id> --task <id> [--supersede] <memory-id>
//! ```
//!
//! An id may be written as a prefix of at least eight characters when it is
//! unambiguous in the task's scopes.

use modbit_protocol::client::Client;
use modbit_protocol::v1::{
    EditMemory, ForgetMemory, ListMemory, MemoryEdited, MemoryForgotten, MemoryItemView,
    MemoryList, MemoryPromoted, MemoryProposed, PromoteMemory, ProposeMemory,
};
use prost::Message;

use crate::{USAGE, envelope, envelope_fenced, join_lease, parse_id};

/// What the flags and words of one memory verb said.
#[derive(Default)]
struct Args {
    session: Option<String>,
    task: Option<String>,
    scopes: Vec<String>,
    statuses: Vec<String>,
    text: Option<String>,
    record_type: Option<String>,
    topic: Option<String>,
    content: Option<String>,
    source: Option<String>,
    confidence: Option<f32>,
    ttl_ms: Option<i64>,
    clear_ttl: bool,
    sensitive: Option<bool>,
    supersede: bool,
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
            "--session" => a.session = Some(value(&mut it, w)?),
            "--task" => a.task = Some(value(&mut it, w)?),
            "--scope" => a.scopes.push(value(&mut it, w)?),
            "--status" => a.statuses.push(value(&mut it, w)?),
            "--text" => a.text = Some(value(&mut it, w)?),
            "--type" => a.record_type = Some(value(&mut it, w)?),
            "--topic" => a.topic = Some(value(&mut it, w)?),
            "--content" => a.content = Some(value(&mut it, w)?),
            "--source" => a.source = Some(value(&mut it, w)?),
            "--confidence" => {
                a.confidence = Some(
                    value(&mut it, w)?
                        .parse()
                        .map_err(|_| "--confidence takes a number from 0 to 1")?,
                );
            }
            "--ttl-ms" => {
                a.ttl_ms = Some(
                    value(&mut it, w)?
                        .parse()
                        .map_err(|_| "--ttl-ms takes a number of milliseconds")?,
                );
            }
            "--clear-ttl" => a.clear_ttl = true,
            "--sensitive" => a.sensitive = Some(true),
            "--not-sensitive" => a.sensitive = Some(false),
            "--supersede" => a.supersede = true,
            "--json" => a.json = true,
            other if other.starts_with("--") => return Err(format!("unknown flag `{other}`")),
            other => a.positional.push(other.to_owned()),
        }
    }
    Ok(a)
}

fn short(id: &str) -> &str {
    &id[..id.len().min(12)]
}

fn clip(text: &str, max: usize) -> String {
    let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max {
        return one_line;
    }
    let cut: String = one_line.chars().take(max).collect();
    format!("{cut}...")
}

fn item_json(i: &MemoryItemView) -> serde_json::Value {
    serde_json::json!({
        "id": i.id, "scope": i.scope, "record_type": i.record_type, "topic": i.topic,
        "content": i.content, "source": i.source, "author": i.author,
        "confidence": i.confidence, "created_at_ms": i.created_at_ms,
        "expires_at_ms": i.expires_at_ms, "sensitivity": i.sensitivity,
        "status": i.status, "supersedes": i.supersedes, "conflicts": i.conflicts,
        "last_validation_revision": i.last_validation_revision, "validated": i.validated,
    })
}

fn print_item(i: &MemoryItemView, full: bool) {
    println!(
        "{} {} scope={} type={} topic={:?} source={} author={} confidence={:.2} validated={}{}{}",
        short(&i.id),
        i.status,
        i.scope,
        i.record_type,
        i.topic,
        i.source,
        i.author,
        i.confidence,
        i.validated,
        if i.expires_at_ms > 0 {
            format!(" expires={}", i.expires_at_ms)
        } else {
            String::new()
        },
        if i.sensitivity == "sensitive" {
            " SENSITIVE"
        } else {
            ""
        },
    );
    if full {
        println!("  id: {}", i.id);
        println!("  content: {}", i.content);
        if !i.supersedes.is_empty() {
            println!("  supersedes: {}", i.supersedes.join(","));
        }
        if !i.last_validation_revision.is_empty() {
            println!("  validated at revision: {}", i.last_validation_revision);
        }
    } else {
        println!("  {}", clip(&i.content, 110));
    }
}

fn need_task(a: &Args) -> Result<modbit_protocol::v1::Id, String> {
    parse_id(a.task.as_deref().ok_or(USAGE)?)
}

pub(crate) async fn run(client: &mut Client, words: &[&str]) -> Result<(), String> {
    let Some((verb, rest)) = words.split_first() else {
        return Err(USAGE.into());
    };
    let a = parse(rest)?;
    let task = need_task(&a)?;
    // A mutation presents the session lease; a read needs none.
    let lease = if matches!(*verb, "propose" | "promote" | "edit" | "forget") {
        let sid = parse_id(a.session.as_deref().ok_or(USAGE)?)?;
        Some(join_lease(client, &sid).await?)
    } else {
        None
    };
    match *verb {
        "list" | "show" => {
            let show = *verb == "show";
            let id = if show {
                a.positional.first().cloned().ok_or(USAGE)?
            } else {
                String::new()
            };
            let ack = client
                .command(envelope(
                    "ListMemory",
                    ListMemory {
                        task_id: Some(task),
                        statuses: a.statuses.clone(),
                        scope_kinds: a.scopes.clone(),
                        text: a.text.clone().unwrap_or_default(),
                        memory_id: id,
                        include_history: show,
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let l: MemoryList = Client::result(&ack).map_err(|e| e.to_string())?;
            if a.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "items": l.items.iter().map(item_json).collect::<Vec<_>>(),
                        "scopes": l.scopes,
                        "conflicts": l.conflicts.iter().map(|c| c.item_ids.clone()).collect::<Vec<_>>(),
                        "history": l.history.iter().map(|h| serde_json::json!({
                            "offset": h.offset, "event_type": h.event_type, "actor": h.actor,
                            "occurred_at_ms": h.occurred_at_ms, "session_id": h.session_id, "detail": h.detail,
                        })).collect::<Vec<_>>(),
                    })
                );
                return Ok(());
            }
            if l.items.is_empty() {
                println!("no memory items in this task's scopes");
            }
            for i in &l.items {
                print_item(i, show);
            }
            for c in &l.conflicts {
                println!(
                    "conflict: {}",
                    c.item_ids
                        .iter()
                        .map(|i| short(i))
                        .collect::<Vec<_>>()
                        .join(" vs ")
                );
            }
            for h in &l.history {
                println!(
                    "event {} {} by {}{}",
                    h.offset,
                    h.event_type,
                    h.actor,
                    if h.detail.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", h.detail)
                    }
                );
            }
        }
        "propose" => {
            let content = a
                .content
                .clone()
                .or_else(|| (!a.positional.is_empty()).then(|| a.positional.join(" ")))
                .ok_or(USAGE)?;
            let ack = client
                .command(envelope_fenced(
                    "ProposeMemory",
                    ProposeMemory {
                        task_id: Some(task),
                        scope: a.scopes.first().cloned().unwrap_or_default(),
                        record_type: a.record_type.clone().ok_or(USAGE)?,
                        topic: a.topic.clone().ok_or(USAGE)?,
                        content,
                        confidence: a.confidence.unwrap_or(0.0),
                        ttl_ms: a.ttl_ms.unwrap_or(0),
                        sensitive: a.sensitive.unwrap_or(false),
                        source: a.source.clone().unwrap_or_default(),
                    }
                    .encode_to_vec(),
                    lease,
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: MemoryProposed = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "{} {} scope={}{}",
                if r.existed { "exists" } else { "proposed" },
                r.memory_id,
                r.scope,
                if r.existed {
                    format!(" status={}", r.status)
                } else {
                    String::new()
                }
            );
        }
        "promote" => {
            let id = a.positional.first().cloned().ok_or(USAGE)?;
            let ack = client
                .command(envelope_fenced(
                    "PromoteMemory",
                    PromoteMemory {
                        task_id: Some(task),
                        memory_id: id,
                    }
                    .encode_to_vec(),
                    lease,
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: MemoryPromoted = Client::result(&ack).map_err(|e| e.to_string())?;
            if r.outcome == "curated" {
                println!(
                    "curated {}{}",
                    r.memory_id,
                    if r.superseded.is_empty() {
                        String::new()
                    } else {
                        format!(" superseded={}", r.superseded.join(","))
                    }
                );
            } else {
                println!(
                    "{} {} {} {}",
                    r.outcome, r.memory_id, r.refusal_code, r.refusal_detail
                );
                crate::EXIT_CODE.store(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        "edit" => {
            let id = a.positional.first().cloned().ok_or(USAGE)?;
            let ack = client
                .command(envelope_fenced(
                    "EditMemory",
                    EditMemory {
                        task_id: Some(task),
                        memory_id: id,
                        topic: a.topic.clone(),
                        content: a.content.clone(),
                        confidence: a.confidence,
                        ttl_ms: a.ttl_ms,
                        clear_ttl: a.clear_ttl,
                        sensitive: a.sensitive,
                    }
                    .encode_to_vec(),
                    lease,
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: MemoryEdited = Client::result(&ack).map_err(|e| e.to_string())?;
            if r.outcome == "edited" {
                println!(
                    "edited {} -> {} status={}{}",
                    short(&r.memory_id),
                    r.new_memory_id,
                    r.status,
                    if r.retired.is_empty() {
                        String::new()
                    } else {
                        format!(" retired={}", r.retired.join(","))
                    }
                );
            } else {
                println!(
                    "{} {} {} {}",
                    r.outcome, r.memory_id, r.refusal_code, r.refusal_detail
                );
                crate::EXIT_CODE.store(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        "forget" => {
            let id = a.positional.first().cloned().ok_or(USAGE)?;
            let ack = client
                .command(envelope_fenced(
                    "ForgetMemory",
                    ForgetMemory {
                        task_id: Some(task),
                        memory_id: id,
                        supersede: a.supersede,
                    }
                    .encode_to_vec(),
                    lease,
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: MemoryForgotten = Client::result(&ack).map_err(|e| e.to_string())?;
            if r.changed {
                println!("forgotten {} status={}", r.memory_id, r.status);
            } else {
                println!(
                    "unchanged {} (no such item in this task's scopes)",
                    r.memory_id
                );
                crate::EXIT_CODE.store(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}
