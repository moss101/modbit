//! `modbit-cli queue | run-mode | allow | context accounting ...` (PX-050,
//! PX-057, PX-059; docs/65 AFW-E, AFW-F, AFW-H): the headless verbs over the
//! same SurfaceProtocol commands the desktop calls. The CLI holds no queue,
//! no policy and no accounting: it decodes arguments, sends the command under
//! the session lease and prints what the Core answered.
//!
//! ```text
//! queue list      --task <id> [--all]
//! queue edit      --session <id> --task <id> --input <id> [--mode M] [--model m] [text]
//! queue remove    --session <id> --task <id> --input <id> [reason]
//! queue move      --session <id> --task <id> --input <id> [--before <id>]
//! queue send-now  --session <id> --task <id> --input <id> [--interrupt-id <id>]
//! queue stop      --session <id> --task <id> [--interrupt-id <id>] [reason]
//! run-mode get    --task <id>
//! run-mode set    --session <id> --task <id> [--ack] <ASK|ALLOWLIST|ALLOWLIST_SANDBOX|RUN_EVERYTHING>
//! allow add       --session <id> --task <id> [--scope TASK|REPO|USER] [--expires-in-ms N]
//!                 [--covers-always-ask] -- <argv-prefix>...
//! allow list      [--task <id>] [--all]
//! allow revoke    --session <id> --task <id> --rule <id> [reason]
//! context accounting <task-id>
//! ```

use modbit_protocol::client::Client;
use modbit_protocol::v1::{
    AddAllowRule, AllowRuleList, AllowRuleRevoked, AllowRuleView, ContextAccountingView,
    EditQueuedInput, GetContextAccounting, GetRunMode, InterruptAccepted, InterruptTask,
    ListAllowRules, ListQueuedInputs, QueuedInputChanged, QueuedInputList, RemoveQueuedInput,
    ReorderQueuedInput, RevokeAllowRule, RunModeView, SendQueuedInputNow, SetRunMode,
};
use prost::Message;

use crate::{USAGE, envelope, envelope_fenced, join_lease, parse_id};

/// What the flags and words of one verb said.
#[derive(Default)]
struct Args {
    session: Option<String>,
    task: Option<String>,
    input: Option<String>,
    before: Option<String>,
    rule: Option<String>,
    mode: Option<String>,
    model: Option<String>,
    scope: Option<String>,
    interrupt_id: Option<String>,
    expires_in_ms: Option<i64>,
    ack: bool,
    all: bool,
    covers_always_ask: bool,
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
            "--input" => a.input = Some(value(&mut it, w)?),
            "--before" => a.before = Some(value(&mut it, w)?),
            "--rule" => a.rule = Some(value(&mut it, w)?),
            "--mode" => a.mode = Some(value(&mut it, w)?),
            "--model" => a.model = Some(value(&mut it, w)?),
            "--scope" => a.scope = Some(value(&mut it, w)?),
            "--interrupt-id" => a.interrupt_id = Some(value(&mut it, w)?),
            "--expires-in-ms" => {
                a.expires_in_ms = Some(
                    value(&mut it, w)?
                        .parse()
                        .map_err(|_| "--expires-in-ms takes a number of milliseconds")?,
                );
            }
            "--ack" => a.ack = true,
            "--all" => a.all = true,
            "--covers-always-ask" => a.covers_always_ask = true,
            // Everything after `--` is the argv prefix of a rule, verbatim.
            "--" => {
                a.positional.extend(it.by_ref().map(str::to_owned));
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag {other}\n{USAGE}"));
            }
            other => a.positional.push(other.to_owned()),
        }
    }
    Ok(a)
}

fn task_of(a: &Args) -> Result<modbit_protocol::v1::Id, String> {
    parse_id(a.task.as_deref().ok_or(USAGE)?)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// `queue ...`.
pub async fn queue(client: &mut Client, words: &[&str]) -> Result<(), String> {
    let (verb, rest) = words.split_first().ok_or(USAGE)?;
    let a = parse(rest)?;
    let task_id = task_of(&a)?;
    match *verb {
        "list" => {
            let ack = client
                .command(envelope(
                    "ListQueuedInputs",
                    ListQueuedInputs {
                        task_id: Some(task_id),
                        include_settled: a.all,
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let l: QueuedInputList = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "{} queued run_alive={} offset={}",
                l.queued, l.run_alive, l.offset
            );
            for i in &l.items {
                println!(
                    "  {:>2} {} {} [{}]{}{} {}",
                    i.position,
                    i.input_id,
                    i.mode,
                    i.state,
                    if i.edited { " edited" } else { "" },
                    if i.sent_now { " sent-now" } else { "" },
                    i.text.lines().next().unwrap_or_default()
                );
            }
        }
        "edit" | "remove" | "move" => {
            let sid = parse_id(a.session.as_deref().ok_or(USAGE)?)?;
            let input_id = a.input.clone().ok_or(USAGE)?;
            let lease = join_lease(client, &sid).await?;
            let (ty, payload) = match *verb {
                "edit" => (
                    "EditQueuedInput",
                    EditQueuedInput {
                        task_id: Some(task_id),
                        input_id,
                        text: a.positional.join(" "),
                        mode: a.mode.clone().unwrap_or_default().to_ascii_uppercase(),
                        model: a.model.clone().unwrap_or_default(),
                    }
                    .encode_to_vec(),
                ),
                "remove" => (
                    "RemoveQueuedInput",
                    RemoveQueuedInput {
                        task_id: Some(task_id),
                        input_id,
                        reason: a.positional.join(" "),
                    }
                    .encode_to_vec(),
                ),
                _ => (
                    "ReorderQueuedInput",
                    ReorderQueuedInput {
                        task_id: Some(task_id),
                        input_id,
                        before_input_id: a.before.clone().unwrap_or_default(),
                    }
                    .encode_to_vec(),
                ),
            };
            let ack = client
                .command(envelope_fenced(ty, payload, Some(lease)))
                .await
                .map_err(|e| e.to_string())?;
            let r: QueuedInputChanged = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "input {} state={} position={} offset={}",
                r.input_id, r.state, r.position, r.offset
            );
        }
        "send-now" | "stop" => {
            let sid = parse_id(a.session.as_deref().ok_or(USAGE)?)?;
            let lease = join_lease(client, &sid).await?;
            let (ty, payload) = if *verb == "send-now" {
                (
                    "SendQueuedInputNow",
                    SendQueuedInputNow {
                        task_id: Some(task_id),
                        input_id: a.input.clone().ok_or(USAGE)?,
                        interrupt_id: a.interrupt_id.clone().unwrap_or_default(),
                        reason: a.positional.join(" "),
                    }
                    .encode_to_vec(),
                )
            } else {
                (
                    "InterruptTask",
                    InterruptTask {
                        task_id: Some(task_id),
                        interrupt_id: a.interrupt_id.clone().unwrap_or_default(),
                        reason: a.positional.join(" "),
                    }
                    .encode_to_vec(),
                )
            };
            let ack = client
                .command(envelope_fenced(ty, payload, Some(lease)))
                .await
                .map_err(|e| e.to_string())?;
            let r: InterruptAccepted = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "interrupt {} kind={} run_alive={} offset={}",
                r.interrupt_id, r.kind, r.run_alive, r.offset
            );
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

fn print_mode(v: &RunModeView) {
    println!(
        "run-mode {} acknowledged={} session_only={} rules_in_force={}",
        v.mode, v.acknowledged, v.session_only, v.rules_in_force
    );
    println!("  always asks: {}", v.always_ask.join(", "));
}

/// `run-mode ...`.
pub async fn run_mode(client: &mut Client, words: &[&str]) -> Result<(), String> {
    let (verb, rest) = words.split_first().ok_or(USAGE)?;
    let a = parse(rest)?;
    let task_id = task_of(&a)?;
    match *verb {
        "get" => {
            let ack = client
                .command(envelope(
                    "GetRunMode",
                    GetRunMode {
                        task_id: Some(task_id),
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            print_mode(&Client::result(&ack).map_err(|e| e.to_string())?);
        }
        "set" => {
            let sid = parse_id(a.session.as_deref().ok_or(USAGE)?)?;
            let lease = join_lease(client, &sid).await?;
            let ack = client
                .command(envelope_fenced(
                    "SetRunMode",
                    SetRunMode {
                        task_id: Some(task_id),
                        mode: a
                            .positional
                            .first()
                            .ok_or(USAGE)?
                            .to_ascii_uppercase()
                            .replace('-', "_"),
                        acknowledge_risk: a.ack,
                    }
                    .encode_to_vec(),
                    Some(lease),
                ))
                .await
                .map_err(|e| e.to_string())?;
            print_mode(&Client::result(&ack).map_err(|e| e.to_string())?);
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

fn print_rule(r: &AllowRuleView) {
    println!(
        "  {} [{}] {} {}{}{}",
        r.rule_id,
        r.state,
        r.scope,
        r.pattern.join(" "),
        if r.expires_at_ms > 0 {
            format!(" expires={}", r.expires_at_ms)
        } else {
            String::new()
        },
        if r.covers_always_ask {
            " covers-always-ask"
        } else {
            ""
        }
    );
}

/// `allow ...`.
pub async fn allow(client: &mut Client, words: &[&str]) -> Result<(), String> {
    let (verb, rest) = words.split_first().ok_or(USAGE)?;
    let a = parse(rest)?;
    match *verb {
        "add" => {
            let sid = parse_id(a.session.as_deref().ok_or(USAGE)?)?;
            let task_id = task_of(&a)?;
            let lease = join_lease(client, &sid).await?;
            let ack = client
                .command(envelope_fenced(
                    "AddAllowRule",
                    AddAllowRule {
                        task_id: Some(task_id),
                        rule_id: String::new(),
                        pattern: a.positional.clone(),
                        scope: a
                            .scope
                            .clone()
                            .unwrap_or_else(|| "REPO".into())
                            .to_ascii_uppercase(),
                        expires_at_ms: a.expires_in_ms.map_or(0, |ms| now_ms() + ms),
                        covers_always_ask: a.covers_always_ask,
                    }
                    .encode_to_vec(),
                    Some(lease),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: AllowRuleView = Client::result(&ack).map_err(|e| e.to_string())?;
            print_rule(&r);
        }
        "list" => {
            let ack = client
                .command(envelope(
                    "ListAllowRules",
                    ListAllowRules {
                        task_id: a.task.as_deref().map(parse_id).transpose()?,
                        include_inactive: a.all,
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let l: AllowRuleList = Client::result(&ack).map_err(|e| e.to_string())?;
            println!("{} rule(s)", l.rules.len());
            for r in &l.rules {
                print_rule(r);
            }
        }
        "revoke" => {
            let sid = parse_id(a.session.as_deref().ok_or(USAGE)?)?;
            let task_id = task_of(&a)?;
            let lease = join_lease(client, &sid).await?;
            let ack = client
                .command(envelope_fenced(
                    "RevokeAllowRule",
                    RevokeAllowRule {
                        task_id: Some(task_id),
                        rule_id: a.rule.clone().ok_or(USAGE)?,
                        reason: a.positional.join(" "),
                    }
                    .encode_to_vec(),
                    Some(lease),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: AllowRuleRevoked = Client::result(&ack).map_err(|e| e.to_string())?;
            println!("revoked {} offset={}", r.rule_id, r.offset);
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

/// `context accounting <task-id>`.
pub async fn accounting(client: &mut Client, task: &str) -> Result<(), String> {
    let ack = client
        .command(envelope(
            "GetContextAccounting",
            GetContextAccounting {
                task_id: Some(parse_id(task)?),
            }
            .encode_to_vec(),
        ))
        .await
        .map_err(|e| e.to_string())?;
    let v: ContextAccountingView = Client::result(&ack).map_err(|e| e.to_string())?;
    if !v.available {
        println!("no request has been compiled for this task yet");
        return Ok(());
    }
    println!(
        "{} tokens ({}) of {} [{}], {}.{:02}% used; estimator {} (error {} bp, declared {} bp); {} on {}/{}",
        v.total_tokens,
        v.total_source,
        v.window_tokens,
        v.window_source,
        v.used_bp / 100,
        v.used_bp % 100,
        v.estimated_total,
        v.estimator_error_bp,
        v.declared_error_bp,
        v.rounding_rule,
        v.endpoint,
        v.model
    );
    for c in &v.categories {
        println!(
            "  {:<12} {:>9} (estimated {:>9}, {:>3}.{:02}%) {}",
            c.category,
            c.tokens,
            c.estimated_tokens,
            c.share_bp / 100,
            c.share_bp % 100,
            c.sources.join(", ")
        );
    }
    if let Some(b) = &v.budgets {
        println!(
            "  budgets: cost {}/{} (children hold {}), wall {}/{} ms, children {}/{}{}",
            b.spent_minor,
            b.max_cost_minor,
            b.held_by_children_minor,
            b.wall_ms_used,
            b.max_wall_ms,
            b.live_children,
            b.max_children,
            if b.forbid_spawn {
                " (spawn forbidden)"
            } else {
                ""
            }
        );
    }
    Ok(())
}
