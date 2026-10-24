//! `modbit-cli worktree ...` (PX-066; docs/65 AFW-J05..J08): the headless
//! verbs over the same SurfaceProtocol commands the desktop calls —
//! `ListWorktrees`, `ApplyWorktree`, `UndoApply`. The CLI holds no apply,
//! Git, conflict or undo logic: it decodes arguments, sends the command under
//! the session lease and prints what the Core answered, including the typed
//! conflict view and every refusal code. The approval is the ordinary
//! `approval list` / `approval resolve` (the intent printed here is the one
//! the person approves); calling `apply` or `undo` again applies it.
//!
//! ```text
//! worktree list  [--task <id>] [--json]
//! worktree apply --session <id> --task <id> [--option MERGE_MANUALLY|STASH|OVERWRITE|FULL_OVERWRITE|UNDO_AND_APPLY|CANCEL]
//!                [--confirm <path>]... [--remember] [--revision N] [--plan-digest <d>] [--json]
//! worktree undo  --session <id> --task <id> [--apply-id <id>] [--json]
//! ```
//!
//! Exit status: 0 applied / undone / nothing to apply / cancelled; 2 approval
//! pending; 3 a conflict that needs a decision (apply CONFLICT, UNDO_CONFLICT);
//! 1 any refusal (STALE, REFUSED, DENIED, a rejected command).

use modbit_protocol::client::Client;
use modbit_protocol::local::encode_hex;
use modbit_protocol::v1::{
    ApplyConflictView, ApplyWorktree, ListWorktrees, UndoApply, WorktreeApplyAck, WorktreeList,
};
use prost::Message;
use std::sync::atomic::Ordering::SeqCst;

use crate::{USAGE, envelope, envelope_fenced, join_lease, parse_id};

#[derive(Default)]
struct Args {
    session: Option<String>,
    task: Option<String>,
    option: Option<String>,
    apply_id: Option<String>,
    plan_digest: Option<String>,
    revision: Option<u64>,
    confirm: Vec<String>,
    remember: bool,
    json: bool,
}

fn parse(words: &[&str]) -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = words.iter().copied();
    while let Some(w) = it.next() {
        let mut value = |flag: &str| {
            it.next()
                .map(str::to_owned)
                .ok_or_else(|| format!("{flag} takes a value"))
        };
        match w {
            "--session" => a.session = Some(value(w)?),
            "--task" => a.task = Some(value(w)?),
            "--option" => a.option = Some(value(w)?.to_uppercase().replace('-', "_")),
            "--apply-id" => a.apply_id = Some(value(w)?),
            "--plan-digest" => a.plan_digest = Some(value(w)?),
            "--revision" => {
                a.revision = Some(
                    value(w)?
                        .parse()
                        .map_err(|_| "--revision takes a number".to_owned())?,
                )
            }
            "--confirm" => a.confirm.push(value(w)?),
            "--remember" => a.remember = true,
            "--json" => a.json = true,
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    Ok(a)
}

fn ack_json(a: &WorktreeApplyAck) -> serde_json::Value {
    serde_json::json!({
        "status": a.status, "code": a.code, "detail": a.detail,
        "apply_id": a.apply_id,
        "approval_id": a.approval_id.replace('-', ""),
        "intent_hash": a.intent_hash, "plan_digest": a.plan_digest,
        "candidate_revision": a.candidate_revision,
        "candidate_tree": a.candidate_tree,
        "checkout_head_before": a.checkout_head_before,
        "option": a.option, "stash_ref": a.stash_ref, "replayed": a.replayed,
        "applied_paths": a.applied_paths, "unresolved_paths": a.unresolved_paths,
        "divergent_paths": a.divergent_paths,
        "effect_receipt_ids": a.effect_receipt_ids,
        "conflict": a.conflict.as_ref().map(conflict_json),
    })
}

fn conflict_json(c: &ApplyConflictView) -> serde_json::Value {
    serde_json::json!({
        "plan_digest": c.plan_digest, "candidate_tree": c.candidate_tree,
        "checkout_head": c.checkout_head, "candidate_revision": c.candidate_revision,
        "default_option": c.default_option, "remembered_option": c.remembered_option,
        "conflicting_paths": c.conflicting_paths, "protected_paths": c.protected_paths,
        "dirty_paths": c.dirty_paths,
        "paths": c.paths.iter().map(|p| serde_json::json!({
            "path": p.path, "change": p.change, "state": p.state,
            "conflict": p.conflict, "protected": p.protected,
        })).collect::<Vec<_>>(),
        "options": c.options.iter().map(|o| serde_json::json!({
            "option": o.option, "destructive": o.destructive,
            "needs_confirmation": o.needs_confirmation,
            "confirm_paths": o.confirm_paths, "available": o.available,
            "why_unavailable": o.why_unavailable,
        })).collect::<Vec<_>>(),
    })
}

fn dash(s: &str) -> &str {
    if s.is_empty() { "-" } else { s }
}

fn print_ack(a: &WorktreeApplyAck) {
    let what = if a.status == "UNDONE" || a.status == "UNDO_CONFLICT" {
        "undo"
    } else {
        "apply"
    };
    println!(
        "{what} status={} apply={} candidate-revision={} candidate-tree={} checkout-head-before={} intent={} plan={}",
        a.status,
        dash(&a.apply_id),
        a.candidate_revision,
        dash(&a.candidate_tree),
        dash(&a.checkout_head_before),
        dash(&a.intent_hash),
        dash(&a.plan_digest),
    );
    if !a.approval_id.is_empty() {
        let id = a.approval_id.replace('-', "");
        println!(
            "approval {id} (modbit-cli approval resolve --approval {id} --intent {} approve, then run this command again)",
            a.intent_hash
        );
    }
    if !a.code.is_empty() || !a.detail.is_empty() {
        println!("code {} detail {}", dash(&a.code), a.detail);
    }
    if !a.option.is_empty() {
        println!("option {}", a.option);
    }
    if !a.stash_ref.is_empty() {
        println!("stash {}", a.stash_ref);
    }
    for p in &a.applied_paths {
        println!("applied {p}");
    }
    for p in &a.unresolved_paths {
        println!("unresolved {p} (conflict markers left in the file)");
    }
    for p in &a.divergent_paths {
        println!("divergent {p} (edited since the apply; nothing was changed)");
    }
    if !a.effect_receipt_ids.is_empty() {
        println!("receipts {}", a.effect_receipt_ids.join(","));
    }
    if let Some(c) = &a.conflict {
        println!("conflicting paths: {}", c.conflicting_paths.join(", "));
        for p in &c.paths {
            let kind = if p.conflict.is_empty() {
                String::new()
            } else {
                format!(" ({})", p.conflict)
            };
            println!("  {} {} {}{kind}", p.state, p.change, p.path);
        }
        for o in &c.options {
            let mut line = format!("  option {}", o.option);
            if o.option == c.default_option {
                line.push_str(" [default]");
            }
            if o.destructive {
                line.push_str(" [destructive]");
            }
            if o.needs_confirmation {
                line.push_str(&format!(
                    " [type each with --confirm: {}]",
                    o.confirm_paths.join(" ")
                ));
            }
            if !o.available {
                line.push_str(&format!(" [unavailable: {}]", o.why_unavailable));
            }
            println!("{line}");
        }
    }
}

fn exit_for(status: &str) -> u8 {
    match status {
        "APPLIED" | "UNDONE" | "NOTHING_TO_APPLY" | "CANCELLED" => 0,
        "APPROVAL_PENDING" => 2,
        "CONFLICT" | "UNDO_CONFLICT" => 3,
        _ => 1,
    }
}

pub(crate) async fn run(client: &mut Client, words: &[&str]) -> Result<(), String> {
    let Some((verb, rest)) = words.split_first() else {
        return Err(USAGE.into());
    };
    let a = parse(rest)?;
    if *verb == "list" {
        let task_id = match &a.task {
            Some(t) => Some(parse_id(t)?),
            None => None,
        };
        let ack = client
            .command(envelope(
                "ListWorktrees",
                ListWorktrees {
                    session_id: None,
                    task_id,
                }
                .encode_to_vec(),
            ))
            .await
            .map_err(|e| e.to_string())?;
        let l: WorktreeList = Client::result(&ack).map_err(|e| e.to_string())?;
        let task_hex = |w: &modbit_protocol::v1::WorktreeView| {
            w.task_id
                .as_ref()
                .map(|t| encode_hex(&t.value))
                .unwrap_or_default()
        };
        if a.json {
            let rows: Vec<_> = l
                .worktrees
                .iter()
                .map(|w| {
                    serde_json::json!({
                        "worktree_id": w.worktree_id, "task_id": task_hex(w),
                        "path": w.path, "origin_root": w.origin_root, "branch": w.branch,
                        "base_revision": w.base_revision, "state": w.state,
                        "disposition": w.disposition, "dirty": w.dirty,
                        "unapplied": w.unapplied, "changed_files": w.changed_files,
                    })
                })
                .collect();
            println!("{}", serde_json::json!({ "worktrees": rows }));
        } else if l.worktrees.is_empty() {
            println!("no worktrees");
        } else {
            for w in &l.worktrees {
                println!(
                    "{} task={} state={} disposition={} unapplied={} changed={} path={}",
                    w.worktree_id,
                    task_hex(w),
                    w.state,
                    dash(&w.disposition),
                    w.unapplied,
                    w.changed_files,
                    w.path
                );
            }
        }
        return Ok(());
    }
    let sid = parse_id(a.session.as_deref().ok_or(USAGE)?)?;
    let task = parse_id(a.task.as_deref().ok_or(USAGE)?)?;
    let lease = join_lease(client, &sid).await?;
    let (kind, payload) = match *verb {
        "apply" => (
            "ApplyWorktree",
            ApplyWorktree {
                task_id: Some(task),
                expected_candidate_revision: a.revision.unwrap_or(0),
                option: a.option.clone().unwrap_or_default(),
                confirm_paths: a.confirm.clone(),
                remember: a.remember,
                expected_plan_digest: a.plan_digest.clone().unwrap_or_default(),
            }
            .encode_to_vec(),
        ),
        "undo" => (
            "UndoApply",
            UndoApply {
                task_id: Some(task),
                apply_id: a.apply_id.clone().unwrap_or_default(),
            }
            .encode_to_vec(),
        ),
        _ => return Err(USAGE.into()),
    };
    let ack = client
        .command(envelope_fenced(kind, payload, Some(lease)))
        .await
        .map_err(|e| e.to_string())?;
    let r: WorktreeApplyAck = Client::result(&ack).map_err(|e| e.to_string())?;
    if a.json {
        println!("{}", ack_json(&r));
    } else {
        print_ack(&r);
    }
    crate::EXIT_CODE.store(exit_for(&r.status), SeqCst);
    Ok(())
}
