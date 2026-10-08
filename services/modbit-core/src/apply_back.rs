//! Apply-back of a task worktree's result to the user's checkout (PX-066;
//! docs/65 AFW-J08, AFW-G05).
//!
//! `ApplyWorktree` is the person's command; the effect is the governed tool
//! `git.worktree.apply`, so the Capability Kernel asks for an approval bound
//! to the exact intent — the digest of the plan: the base, the candidate
//! tree, the checkout's head and every changed path with the checkout's
//! hash — and the pipeline journals the dispatch and receipts the result.
//!
//! The command classifies first and never overwrites on its own: a plan with
//! conflicts, and no explicit option, comes back as a typed
//! [`wire::ApplyConflictView`] listing every path and the options (Cancel is
//! the default; an overwrite needs the files typed). A choice is remembered
//! per task only if it is non-destructive.
//!
//! The effect keeps the crash story simple. Before any write, the pre-apply
//! checkpoint — the exact bytes of every path the apply touches — is in the
//! object store and `WorktreeApplyStarted` is on the log. A Core killed at
//! any point after that finds the apply started and not finished at startup
//! and puts the checkpoint back: the checkout is the pre-apply state, never a
//! mixture. `UndoApply` is the same restore, all or nothing: a path edited
//! since the apply is a reported conflict and nothing changes.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use modbit_domain::event::Actor;
use modbit_domain::state::StateMachine;
use modbit_domain::{SessionId, TaskId, TenantId};
use modbit_event_store::EventStore;
use modbit_git::Repo;
use modbit_git::apply::{
    ApplyPlan, Blobs, Choice, EntryState, Manifest, Request, RestoreMode, View, execute, plan,
    prepare, restore,
};
use modbit_protocol::v1 as wire;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::server::Core;
use crate::worktrees::{self, Record};

type Refusal = (String, String);

fn refuse(code: &str, msg: impl Into<String>) -> Refusal {
    (code.to_owned(), msg.into())
}

/// The object store as the pre-apply checkpoint's blob store.
pub(crate) struct StoreBlobs(pub modbit_event_store::ObjectStore);

impl Blobs for StoreBlobs {
    fn put(&self, bytes: &[u8]) -> Result<String, String> {
        self.0.put(bytes).map_err(|e| e.to_string())
    }

    fn get(&self, id: &str) -> Result<Vec<u8>, String> {
        self.0.get(id).map_err(|e| e.to_string())
    }
}

/// Applies in this process, per worktree: two at once would race on the
/// checkout.
fn applying() -> &'static std::sync::Mutex<HashSet<String>> {
    static A: std::sync::OnceLock<std::sync::Mutex<HashSet<String>>> = std::sync::OnceLock::new();
    A.get_or_init(Default::default)
}

struct ApplyingGuard(String);

impl ApplyingGuard {
    fn take(id: &str) -> Result<Self, Refusal> {
        let mut g = applying().lock().unwrap_or_else(|e| e.into_inner());
        if !g.insert(id.to_owned()) {
            return Err(refuse(
                "APPLY_IN_PROGRESS",
                "another apply or undo of this worktree is running",
            ));
        }
        Ok(Self(id.to_owned()))
    }
}

impl Drop for ApplyingGuard {
    fn drop(&mut self) {
        applying()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

/// Kill the Core at a named point, as a `SIGKILL` there would
/// (`MODBIT_FAULT_WORKTREE=<point>`; unset in production).
pub(crate) fn fault_point(point: &str) {
    if std::env::var("MODBIT_FAULT_WORKTREE").is_ok_and(|v| v == point) {
        eprintln!("modbit-core: fault injection: aborting at {point}");
        std::process::abort();
    }
}

fn protected_fn(origin: &Path) -> Result<modbit_workspace::PathPolicy, Refusal> {
    modbit_workspace::PathPolicy::new(origin, &[]).map_err(|e| refuse("PATH_POLICY", e.to_string()))
}

/// Everything the plan is made from, read now (blocking).
struct Planned {
    checkout: Repo,
    plan: ApplyPlan,
}

fn make_plan(rec: &Record, view: View) -> Result<Planned, Refusal> {
    let worktree = Repo::open(Path::new(&rec.path))
        .map_err(|e| refuse("WORKTREE_UNREADABLE", format!("{}: {e}", rec.path)))?;
    let checkout = Repo::open(Path::new(&rec.origin_root)).map_err(|e| {
        refuse(
            "CHECKOUT_UNREADABLE",
            format!("the checkout `{}`: {e}", rec.origin_root),
        )
    })?;
    let tree = worktree
        .dirty_tree()
        .map_err(|e| refuse("WORKTREE_UNREADABLE", e.to_string()))?;
    let policy = protected_fn(Path::new(&rec.origin_root))?;
    let base = (!rec.base_revision.is_empty()).then_some(rec.base_revision.as_str());
    let p = plan(&checkout, base, &tree, view, &|rel| {
        policy.protected_match(rel).is_some()
    })
    .map_err(|e| refuse("PLAN_FAILED", e.to_string()))?;
    if std::env::var("MODBIT_DEBUG_PLAN").is_ok() {
        eprintln!(
            "PLAN view={view:?} digest={} entries={:?}",
            p.digest,
            p.entries
                .iter()
                .map(|e| (&e.path, e.state.label(), &e.checkout_sha256))
                .collect::<Vec<_>>()
        );
    }
    Ok(Planned { checkout, plan: p })
}

fn view_for(choice: Choice) -> View {
    if choice == Choice::Stash {
        View::Head
    } else {
        View::Disk
    }
}

fn chosen(s: &str) -> Option<Choice> {
    match s {
        "UNDO_AND_APPLY" => Some(Choice::Cancel),
        other => Choice::parse(other),
    }
}

/// The paths a choice replaces or writes against protection, which the
/// person must type to confirm it (docs/65 AFW-J08).
fn must_confirm(plan: &ApplyPlan, choice: Choice) -> Vec<String> {
    let conflicting: Vec<String> = plan.conflicts().iter().map(|e| e.path.clone()).collect();
    let protected: Vec<String> = plan.protected().iter().map(|e| e.path.clone()).collect();
    let touched: HashSet<&str> = plan.entries.iter().map(|e| e.path.as_str()).collect();
    let mut v: Vec<String> = match choice {
        Choice::Cancel => vec![],
        Choice::MergeManually | Choice::Stash => protected,
        Choice::Overwrite => conflicting.into_iter().chain(protected).collect(),
        Choice::FullOverwrite => conflicting
            .into_iter()
            .chain(protected)
            .chain(
                plan.dirty
                    .iter()
                    .filter(|p| !touched.contains(p.as_str()))
                    .cloned(),
            )
            .collect(),
    };
    v.sort();
    v.dedup();
    v
}

// ---- the conflict view ----

fn path_views(plan: &ApplyPlan) -> Vec<wire::ApplyPathView> {
    plan.entries
        .iter()
        .map(|e| wire::ApplyPathView {
            path: e.path.clone(),
            change: format!("{:?}", e.change).to_uppercase(),
            state: e.state.label().to_owned(),
            conflict: match e.state {
                EntryState::Conflict(k) => serde_json::to_value(k)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default(),
                _ => String::new(),
            },
            protected: e.protected,
            markers_available: e.markers_available,
        })
        .collect()
}

/// The typed conflict result for `plan` (Cancel is the default).
fn conflict_view(
    rec: &Record,
    plan: &ApplyPlan,
    head_plan: Option<&ApplyPlan>,
    revision: u64,
) -> wire::ApplyConflictView {
    let conflicting: Vec<String> = plan.conflicts().iter().map(|e| e.path.clone()).collect();
    let protected: Vec<String> = plan.protected().iter().map(|e| e.path.clone()).collect();
    let no_markers: Vec<&str> = plan
        .conflicts()
        .iter()
        .filter(|e| !e.markers_available)
        .map(|e| e.path.as_str())
        .collect();
    let stash_ok = head_plan.is_some_and(|h| h.conflicts().is_empty());
    let options = vec![
        wire::ApplyOptionView {
            option: "CANCEL".into(),
            destructive: false,
            needs_confirmation: false,
            confirm_paths: vec![],
            available: true,
            why_unavailable: String::new(),
        },
        wire::ApplyOptionView {
            option: "MERGE_MANUALLY".into(),
            destructive: false,
            needs_confirmation: !protected.is_empty(),
            confirm_paths: protected.clone(),
            available: no_markers.is_empty(),
            why_unavailable: if no_markers.is_empty() {
                String::new()
            } else {
                format!(
                    "not text a merge can mark (binary, or deleted on one side): {}",
                    no_markers.join(", ")
                )
            },
        },
        wire::ApplyOptionView {
            option: "STASH".into(),
            destructive: false,
            needs_confirmation: !protected.is_empty(),
            confirm_paths: protected.clone(),
            available: stash_ok,
            why_unavailable: if stash_ok {
                String::new()
            } else {
                "the task's change still conflicts with the committed checkout".into()
            },
        },
        wire::ApplyOptionView {
            option: "OVERWRITE".into(),
            destructive: true,
            needs_confirmation: true,
            confirm_paths: must_confirm(plan, Choice::Overwrite),
            available: !conflicting.is_empty(),
            why_unavailable: if conflicting.is_empty() {
                "nothing conflicts".into()
            } else {
                String::new()
            },
        },
        wire::ApplyOptionView {
            option: "FULL_OVERWRITE".into(),
            destructive: true,
            needs_confirmation: true,
            confirm_paths: must_confirm(plan, Choice::FullOverwrite),
            available: true,
            why_unavailable: String::new(),
        },
        wire::ApplyOptionView {
            option: "UNDO_AND_APPLY".into(),
            destructive: false,
            needs_confirmation: false,
            confirm_paths: vec![],
            available: rec.applied_in_force().is_some(),
            why_unavailable: if rec.applied_in_force().is_some() {
                String::new()
            } else {
                "no earlier apply of this worktree is in force".into()
            },
        },
    ];
    wire::ApplyConflictView {
        plan_digest: plan.digest.clone(),
        candidate_tree: plan.candidate_tree.clone(),
        checkout_head: plan.checkout_head.clone().unwrap_or_default(),
        candidate_revision: revision,
        paths: path_views(plan),
        conflicting_paths: conflicting,
        protected_paths: protected,
        dirty_paths: plan.dirty.clone(),
        options,
        default_option: "CANCEL".into(),
        remembered_option: rec
            .remembered
            .clone()
            .filter(|o| Choice::parse(o).is_some_and(|c| !c.destructive()))
            .unwrap_or_default(),
    }
}

// ---- the command ----

fn digest_of(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0]);
    }
    hex::encode(h.finalize())
}

/// What a governed call produced.
enum Called {
    Pending {
        approval_id: String,
        intent_hash: String,
    },
    Denied(String),
    Done(Box<modbit_tools::ToolCallResult>),
}

/// Invoke `tool` for `task` through the pipeline (approval, journal, receipt),
/// re-entering the call an earlier attempt left open.
#[allow(clippy::too_many_arguments)]
async fn call_tool(
    core: &Arc<Core>,
    task: &modbit_domain::task::Task,
    tool: &str,
    args: &Value,
    seed: &str,
    actor: &Actor,
    lease_generation: Option<u64>,
) -> Result<Called, Refusal> {
    let arguments_json = args.to_string();
    let call_for = |attempt: u32| -> modbit_domain::ToolCallId {
        let h = Sha256::digest(format!("{tool}:{}:{seed}:{attempt}", task.task_id).as_bytes());
        let mut b = [0u8; 16];
        b.copy_from_slice(&h[..16]);
        modbit_domain::ToolCallId::from_bytes(b)
    };
    let (tool_call_id, existing, lease, approval, stopped) = {
        let store = core.store.lock().await;
        // The first attempt not over without a result: a denied or an
        // unknown-outcome attempt is terminal for its call; the person may
        // decide again on the next one.
        let mut chosen = call_for(0);
        for attempt in 0..32u32 {
            let id = call_for(attempt);
            chosen = id;
            match store
                .tool_call(&id)
                .map_err(|e| refuse("STORE", e.to_string()))?
            {
                Some(c) if c.state.is_terminal() && c.result_ref.is_none() => continue,
                _ => break,
            }
        }
        (
            chosen,
            store
                .tool_call(&chosen)
                .map_err(|e| refuse("STORE", e.to_string()))?,
            store
                .leases_for_task(&task.task_id)
                .map_err(|e| refuse("STORE", e.to_string()))?
                .into_iter()
                .next(),
            store
                .approval_for_call(&chosen)
                .map_err(|e| refuse("STORE", e.to_string()))?,
            store
                .session(&task.session_id)
                .ok()
                .flatten()
                .and_then(|s| s.emergency_stopped_at)
                .is_some(),
        )
    };
    let done = core
        .tools
        .invoke(
            &core.store,
            crate::tools::InvokeRequest {
                tenant_id: core.tenant_id,
                session_id: task.session_id,
                task_id: task.task_id,
                workspace_root: task.workspace_root.clone(),
                execution_profile: &task.execution_profile,
                tool_call_id,
                tool_name: tool,
                arguments_json: &arguments_json,
                output_budget_bytes: 65_536,
                actor: actor.clone(),
                lease,
                approval,
                emergency_stopped: stopped,
                existing,
                run_id: None,
                turn_id: None,
                call_id: None,
                lease_generation,
                projection: None,
                cancel: None,
                compensates: None,
            },
        )
        .await
        .map_err(|e| refuse("TOOL_HOST", e.to_string()))?;
    let offset = core.store.lock().await.last_offset().unwrap_or(0);
    core.last_offset.send_replace(offset);
    let hash = modbit_tools::arguments_hash(&arguments_json).unwrap_or_default();
    let approval_id = done.approval_id.map(|a| a.to_string()).unwrap_or_default();
    Ok(match done.result.status {
        modbit_tools::ToolStatus::ApprovalPending => Called::Pending {
            approval_id,
            intent_hash: hash,
        },
        modbit_tools::ToolStatus::PolicyDenied => Called::Denied(format!(
            "{}: {}",
            done.result.error_code.clone().unwrap_or_default(),
            done.result.error_message.clone().unwrap_or_default()
        )),
        _ => Called::Done(Box::new(done.result)),
    })
}

fn ack_base(plan: &ApplyPlan, revision: u64) -> wire::WorktreeApplyAck {
    wire::WorktreeApplyAck {
        plan_digest: plan.digest.clone(),
        candidate_revision: revision,
        candidate_tree: plan.candidate_tree.clone(),
        checkout_head_before: plan.checkout_head.clone().unwrap_or_default(),
        ..Default::default()
    }
}

/// `ApplyWorktree`.
pub(crate) async fn apply_command(
    core: &Arc<Core>,
    p: &wire::ApplyWorktree,
    actor: &Actor,
    lease_generation: Option<u64>,
) -> Result<wire::WorktreeApplyAck, Refusal> {
    let Some(tid) = p.task_id.as_ref().and_then(crate::server::id16) else {
        return Err(refuse("BAD_PAYLOAD", "task_id required"));
    };
    let task_id = TaskId::from_bytes(tid);
    let (task, rec) = {
        let store = core.store.lock().await;
        let task = store
            .task(&task_id)
            .ok()
            .flatten()
            .ok_or_else(|| refuse("UNKNOWN_TASK", task_id.to_string()))?;
        let rec = worktrees::record_for_task(&store, &task).ok_or_else(|| {
            refuse(
                "NO_WORKTREE",
                "the task has no worktree of its own to apply",
            )
        })?;
        (task, rec)
    };
    if core.runtime.is_running(&task_id).await {
        return Err(refuse(
            "TASK_RUNNING",
            "the task is still running: apply a result once its run has stopped, or the candidate moves under the approval",
        ));
    }
    // Undo-and-apply: an earlier apply in force is undone first (its own
    // approval), then this call goes on as a plain apply.
    if p.option == "UNDO_AND_APPLY"
        && let Some(prior) = rec.applied_in_force()
    {
        let undo = wire::UndoApply {
            task_id: p.task_id.clone(),
            apply_id: prior.apply_id.clone(),
        };
        return undo_command(core, &undo, actor, lease_generation).await;
    }
    let remembered = rec
        .remembered
        .as_deref()
        .and_then(Choice::parse)
        .filter(|c| !c.destructive() && *c != Choice::Cancel);
    let requested = match p.option.as_str() {
        "" | "UNDO_AND_APPLY" => remembered,
        other => Some(
            chosen(other)
                .ok_or_else(|| refuse("BAD_PAYLOAD", format!("unknown option `{other}`")))?,
        ),
    };
    let choice = requested.unwrap_or(Choice::Cancel);
    let rec2 = rec.clone();
    let (planned, head_plan) = tokio::task::spawn_blocking(move || {
        let planned = make_plan(&rec2, view_for(choice))?;
        let head = if choice == Choice::Stash {
            None
        } else {
            make_plan(&rec2, View::Head).ok().map(|h| h.plan)
        };
        Ok::<_, Refusal>((planned, head))
    })
    .await
    .map_err(|e| refuse("PLAN_FAILED", e.to_string()))??;
    let plan = &planned.plan;
    let revision = {
        let (ws, _) = core
            .tools
            .workspace(&rec.path)
            .await
            .map_err(|e| refuse("WORKSPACE", e.to_string()))?;
        ws.lock().await.revision().number
    };
    let mut ack = ack_base(plan, revision);
    if p.expected_candidate_revision != 0 && p.expected_candidate_revision != revision {
        ack.status = "STALE".into();
        ack.code = "STALE_REVISION".into();
        ack.detail = format!(
            "the review was of candidate revision {}, the worktree is at {revision}: review again",
            p.expected_candidate_revision
        );
        return Ok(ack);
    }
    if !p.expected_plan_digest.is_empty() && p.expected_plan_digest != plan.digest {
        ack.status = "STALE".into();
        ack.code = "STALE_PLAN".into();
        ack.detail =
            "the checkout or the worktree changed since the plan the person reviewed".into();
        ack.conflict = Some(conflict_view(&rec, plan, head_plan.as_ref(), revision));
        return Ok(ack);
    }
    let writes_needed = plan
        .entries
        .iter()
        .any(|e| e.state != EntryState::AlreadyApplied);
    if !writes_needed {
        // The checkout already holds everything the task changed: the result
        // is applied, and the worktree can say so.
        let mut store = core.store.lock().await;
        let already = rec.disposition.as_ref().is_some_and(|d| {
            d.kind == "APPLIED" && d.tree.as_deref() == Some(plan.candidate_tree.as_str())
        });
        if !already {
            let offset = worktrees::append_events(
                &mut store,
                core.tenant_id,
                task.session_id,
                task.task_id,
                worktrees::aggregate_id(&rec.worktree_id),
                vec![worktrees::ev(
                    worktrees::DISPOSED,
                    json!({
                        "worktree_id": rec.worktree_id,
                        "disposition": "APPLIED",
                        "tree": plan.candidate_tree,
                        "by": format!("{actor:?}"),
                        "reason": "the checkout already holds every change of the task",
                    }),
                    actor,
                )],
            )
            .map_err(|e| refuse("STORE", e))?;
            core.last_offset.send_replace(offset);
        }
        ack.status = "NOTHING_TO_APPLY".into();
        ack.detail = "the checkout already holds every change the task made".into();
        return Ok(ack);
    }
    let needs_choice = !plan.conflicts().is_empty() || !plan.protected().is_empty();
    if needs_choice && requested.is_none() {
        ack.status = "CONFLICT".into();
        ack.code = if plan.conflicts().is_empty() {
            "PROTECTED_PATHS".into()
        } else {
            "CONFLICTS".into()
        };
        ack.detail = "applying needs a choice: nothing was changed (Cancel is the default)".into();
        ack.conflict = Some(conflict_view(&rec, plan, head_plan.as_ref(), revision));
        return Ok(ack);
    }
    if (choice == Choice::Cancel && needs_choice) || p.option == "CANCEL" {
        ack.status = "CANCELLED".into();
        ack.detail = "cancelled: nothing was changed".into();
        return Ok(ack);
    }
    // Typed confirmation: an overwrite replaces bytes only Undo can bring back,
    // and a protected path is written only on the person's word. The Core
    // checks it whatever the client did; nothing is asked of the approver
    // until it holds.
    if needs_choice {
        let named: HashSet<&str> = p.confirm_paths.iter().map(String::as_str).collect();
        let missing: Vec<String> = must_confirm(plan, choice)
            .into_iter()
            .filter(|f| !named.contains(f.as_str()))
            .collect();
        if !missing.is_empty() {
            ack.status = "REFUSED".into();
            ack.code = if choice.destructive() {
                "OVERWRITE_NOT_CONFIRMED"
            } else {
                "PROTECTED_PATHS"
            }
            .into();
            ack.detail = format!(
                "{} needs the files typed to confirm it: {}",
                choice.label(),
                missing.join(", ")
            );
            ack.conflict = Some(conflict_view(&rec, plan, head_plan.as_ref(), revision));
            return Ok(ack);
        }
    }
    // The choice, remembered only when it cannot destroy anything.
    if p.remember {
        if choice.destructive() {
            ack.detail = "a destructive choice is never remembered; ".into();
        } else if choice != Choice::Cancel && rec.remembered.as_deref() != Some(choice.label()) {
            let mut store = core.store.lock().await;
            let offset = worktrees::append_events(
                &mut store,
                core.tenant_id,
                task.session_id,
                task.task_id,
                worktrees::aggregate_id(&rec.worktree_id),
                vec![worktrees::ev(
                    worktrees::CHOICE,
                    json!({"worktree_id": rec.worktree_id, "option": choice.label()}),
                    actor,
                )],
            )
            .map_err(|e| refuse("STORE", e))?;
            core.last_offset.send_replace(offset);
        }
    }
    // The exact intent: this plan, this choice, these confirmations.
    let mut confirm = p.confirm_paths.clone();
    confirm.sort();
    confirm.dedup();
    let option_label = requested.map_or("", Choice::label);
    // One apply per attempt: the id (and with it the governed call and its
    // approval) is the plan, the choice and how many applies this worktree
    // has had, so a second apply of the same plan after an undo is a new
    // effect with a new approval, while the retry after an approval is the
    // same call.
    let apply_id = format!(
        "ap-{}",
        &digest_of(&[
            &plan.digest,
            option_label,
            &confirm.join("\n"),
            &rec.worktree_id,
            &rec.applies.len().to_string(),
        ])[..16]
    );
    let mut args = json!({
        "worktree_id": rec.worktree_id,
        "plan_digest": plan.digest,
        "apply_id": apply_id,
        "candidate_revision": revision,
    });
    if requested.is_some() {
        args["option"] = json!(choice.label());
    }
    if !confirm.is_empty() {
        args["confirm_paths"] = json!(confirm);
    }
    ack.apply_id = apply_id.clone();
    ack.option = option_label.to_owned();
    let called = call_tool(
        core,
        &task,
        "git.apply.worktree",
        &args,
        &apply_id,
        actor,
        lease_generation,
    )
    .await?;
    match called {
        Called::Pending {
            approval_id,
            intent_hash,
        } => {
            ack.status = "APPROVAL_PENDING".into();
            ack.approval_id = approval_id;
            ack.intent_hash = intent_hash;
            ack.detail +=
                "decide the approval, then call ApplyWorktree again with the same arguments";
            Ok(ack)
        }
        Called::Denied(why) => {
            ack.status = "DENIED".into();
            ack.detail = why;
            Ok(ack)
        }
        Called::Done(r) => {
            ack.effect_receipt_ids = r.effect_receipt_ids.clone();
            if r.status == modbit_tools::ToolStatus::Success {
                let o = &r.structured_output;
                ack.status = "APPLIED".into();
                ack.applied_paths = strs(&o["applied_paths"]);
                ack.unresolved_paths = strs(&o["unresolved_paths"]);
                ack.stash_ref = o["stash_ref"].as_str().unwrap_or_default().to_owned();
                ack.replayed = o["replayed"].as_bool().unwrap_or(false);
                ack.detail = o["note"].as_str().unwrap_or_default().to_owned();
            } else {
                let code = r.error_code.clone().unwrap_or_default();
                ack.status = if code == "STALE_PLAN" {
                    "STALE"
                } else {
                    "REFUSED"
                }
                .into();
                ack.detail = r.error_message.clone().unwrap_or_default();
                ack.code = code;
            }
            Ok(ack)
        }
    }
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// `UndoApply`.
pub(crate) async fn undo_command(
    core: &Arc<Core>,
    p: &wire::UndoApply,
    actor: &Actor,
    lease_generation: Option<u64>,
) -> Result<wire::WorktreeApplyAck, Refusal> {
    let Some(tid) = p.task_id.as_ref().and_then(crate::server::id16) else {
        return Err(refuse("BAD_PAYLOAD", "task_id required"));
    };
    let task_id = TaskId::from_bytes(tid);
    let (task, rec) = {
        let store = core.store.lock().await;
        let task = store
            .task(&task_id)
            .ok()
            .flatten()
            .ok_or_else(|| refuse("UNKNOWN_TASK", task_id.to_string()))?;
        let rec = worktrees::record_for_task(&store, &task)
            .ok_or_else(|| refuse("NO_WORKTREE", "the task has no worktree of its own"))?;
        (task, rec)
    };
    let entry = if p.apply_id.is_empty() {
        rec.applied_in_force()
    } else {
        rec.applies
            .iter()
            .rev()
            .find(|a| a.apply_id == p.apply_id && a.state == "APPLIED")
    }
    .cloned()
    .ok_or_else(|| {
        refuse(
            "NOTHING_TO_UNDO",
            "no apply of this worktree is in force to undo",
        )
    })?;
    let args = json!({"worktree_id": rec.worktree_id, "apply_id": entry.apply_id});
    let mut ack = wire::WorktreeApplyAck {
        apply_id: entry.apply_id.clone(),
        option: entry.option.clone(),
        stash_ref: entry.stash_ref.clone().unwrap_or_default(),
        ..Default::default()
    };
    match call_tool(
        core,
        &task,
        "git.apply.undo",
        &args,
        &entry.apply_id,
        actor,
        lease_generation,
    )
    .await?
    {
        Called::Pending {
            approval_id,
            intent_hash,
        } => {
            ack.status = "APPROVAL_PENDING".into();
            ack.approval_id = approval_id;
            ack.intent_hash = intent_hash;
            ack.detail = "decide the approval, then call UndoApply again".into();
        }
        Called::Denied(why) => {
            ack.status = "DENIED".into();
            ack.detail = why;
        }
        Called::Done(r) => {
            ack.effect_receipt_ids = r.effect_receipt_ids.clone();
            if r.status == modbit_tools::ToolStatus::Success {
                let o = &r.structured_output;
                ack.status = o["status"].as_str().unwrap_or("UNDONE").to_owned();
                ack.divergent_paths = strs(&o["divergent_paths"]);
                ack.applied_paths = strs(&o["restored_paths"]);
                ack.detail = o["note"].as_str().unwrap_or_default().to_owned();
            } else {
                ack.status = "REFUSED".into();
                ack.code = r.error_code.clone().unwrap_or_default();
                ack.detail = r.error_message.clone().unwrap_or_default();
            }
        }
    }
    Ok(ack)
}

// ---- the effect (run by the governed tools) ----

fn find_record(store: &EventStore, session: &SessionId, worktree_id: &str) -> Option<Record> {
    worktrees::registry_of_session(store, session)
        .into_iter()
        .find(|r| r.worktree_id == worktree_id && !r.removed)
}

/// `git.worktree.apply`: re-plan, compare with the approved digest, write the
/// pre-apply checkpoint and the write-ahead record, then the files.
pub(crate) async fn exec_apply(
    store: &Arc<Mutex<EventStore>>,
    tenant: TenantId,
    session: SessionId,
    task: TaskId,
    actor: &Actor,
    args: &Value,
) -> Result<Value, Refusal> {
    let worktree_id = args["worktree_id"].as_str().unwrap_or_default().to_owned();
    let plan_digest = args["plan_digest"].as_str().unwrap_or_default().to_owned();
    let apply_id = args["apply_id"].as_str().unwrap_or_default().to_owned();
    let confirm: Vec<String> = strs(&args["confirm_paths"]);
    let choice = match args["option"].as_str() {
        None => Choice::Overwrite,
        Some(o) => {
            Choice::parse(o).ok_or_else(|| refuse("BAD_PAYLOAD", format!("option `{o}`")))?
        }
    };
    let rec = {
        let st = store.lock().await;
        find_record(&st, &session, &worktree_id)
            .ok_or_else(|| refuse("NO_WORKTREE", format!("no worktree `{worktree_id}`")))?
    };
    if rec.task_id != Some(task) {
        return Err(refuse(
            "NOT_YOUR_WORKTREE",
            "a task applies only the worktree made for it",
        ));
    }
    // Replayed after a crash between the record and the answer.
    if let Some(a) = rec
        .applies
        .iter()
        .rev()
        .find(|a| a.apply_id == apply_id && a.state == "APPLIED")
    {
        return Ok(json!({
            "status": "APPLIED", "apply_id": a.apply_id, "replayed": true,
            "plan_digest": a.plan_digest, "candidate_tree": a.applied_tree,
            "applied_paths": [], "unresolved_paths": a.unresolved,
            "note": "this apply was already done",
        }));
    }
    let _guard = ApplyingGuard::take(&worktree_id)?;
    if std::env::var("MODBIT_DEBUG_PLAN").is_ok() {
        eprintln!("EXEC choice={choice:?} args={args}");
    }
    let rec2 = rec.clone();
    let blobs = StoreBlobs(store.lock().await.objects().clone());
    let (planned, prepared) = tokio::task::spawn_blocking(move || {
        let planned = make_plan(&rec2, view_for(choice))?;
        if planned.plan.digest != plan_digest {
            return Err(refuse(
                "STALE_PLAN",
                "the checkout or the worktree changed since the plan this call was approved for; \
                 nothing was written, review again",
            ));
        }
        let prepared = prepare(
            &planned.checkout,
            &planned.plan,
            &Request {
                choice,
                confirm_paths: confirm,
                apply_id,
            },
            &blobs,
        )
        .map_err(|r| {
            refuse(
                r.code,
                if r.paths.is_empty() {
                    r.detail
                } else {
                    format!("{}: {}", r.detail, r.paths.join(", "))
                },
            )
        })?;
        Ok((planned, prepared))
    })
    .await
    .map_err(|e| refuse("APPLY_FAILED", e.to_string()))??;
    let manifest = prepared.manifest.clone();
    let manifest_ref = store
        .lock()
        .await
        .objects()
        .put(&serde_json::to_vec(&manifest).map_err(|e| refuse("STORE", e.to_string()))?)
        .map_err(|e| refuse("STORE", e.to_string()))?;
    // Write-ahead: the checkpoint and the intent are durable before a byte of
    // the checkout changes.
    {
        let mut st = store.lock().await;
        worktrees::append_events(
            &mut st,
            tenant,
            session,
            task,
            worktrees::aggregate_id(&worktree_id),
            vec![worktrees::ev(
                worktrees::APPLY_STARTED,
                json!({
                    "worktree_id": worktree_id,
                    "apply_id": manifest.apply_id,
                    "manifest_ref": manifest_ref,
                    "plan_digest": manifest.plan_digest,
                    "option": manifest.choice.label(),
                    "candidate_tree": manifest.candidate_tree,
                    "checkout_head": manifest.checkout_head,
                    "checkout_root": rec.origin_root,
                    "stash_ref": manifest.stash_ref,
                    "paths": manifest.entries.iter().map(|e| e.path.clone()).collect::<Vec<_>>(),
                }),
                actor,
            )],
        )
        .map_err(|e| refuse("STORE", e))?;
    }
    fault_point("WORKTREE_APPLY_AFTER_STARTED");
    let checkout_dir = planned.checkout.dir().to_path_buf();
    let prepared2 = prepared.clone();
    tokio::task::spawn_blocking(move || {
        execute(&checkout_dir, &prepared2, &mut |i, _| {
            if i == 1 {
                fault_point("WORKTREE_APPLY_MID");
            }
        })
    })
    .await
    .map_err(|e| refuse("APPLY_FAILED", e.to_string()))?
    .map_err(|e| {
        refuse(
            "APPLY_FAILED",
            format!("{e}; the pre-apply checkpoint is on the log and recovery restores it"),
        )
    })?;
    fault_point("WORKTREE_APPLY_AFTER_WRITES");
    let applied_paths: Vec<String> = manifest.entries.iter().map(|e| e.path.clone()).collect();
    {
        let mut st = store.lock().await;
        worktrees::append_events(
            &mut st,
            tenant,
            session,
            task,
            worktrees::aggregate_id(&worktree_id),
            vec![
                worktrees::ev(
                    worktrees::APPLIED,
                    json!({
                        "worktree_id": worktree_id,
                        "apply_id": manifest.apply_id,
                        "applied_paths": applied_paths,
                        "unresolved_paths": prepared.unresolved,
                        "candidate_tree": manifest.candidate_tree,
                        "checkout_head_before": manifest.checkout_head,
                    }),
                    actor,
                ),
                worktrees::ev(
                    worktrees::DISPOSED,
                    json!({
                        "worktree_id": worktree_id,
                        "disposition": "APPLIED",
                        "tree": manifest.candidate_tree,
                        "apply_id": manifest.apply_id,
                        "by": format!("{actor:?}"),
                        "reason": format!("applied to {}", rec.origin_root),
                    }),
                    actor,
                ),
            ],
        )
        .map_err(|e| refuse("STORE", e))?;
    }
    let neutral =
        worktrees::record_neutralized(store, tenant, session, task, &rec.origin_root, actor).await;
    Ok(json!({
        "status": "APPLIED",
        "apply_id": manifest.apply_id,
        "plan_digest": manifest.plan_digest,
        "candidate_tree": manifest.candidate_tree,
        "checkout_head_before": manifest.checkout_head,
        "option": manifest.choice.label(),
        "applied_paths": applied_paths,
        "unresolved_paths": prepared.unresolved,
        "stash_ref": manifest.stash_ref,
        "manifest_ref": manifest_ref,
        "neutralized": neutral,
        "note": if prepared.unresolved.is_empty() { String::new() } else {
            format!("conflict markers were left in: {}", prepared.unresolved.join(", "))
        },
    }))
}

/// `git.worktree.undo`: all-or-nothing restore of the pre-apply checkpoint.
pub(crate) async fn exec_undo(
    store: &Arc<Mutex<EventStore>>,
    tenant: TenantId,
    session: SessionId,
    task: TaskId,
    actor: &Actor,
    args: &Value,
) -> Result<Value, Refusal> {
    let worktree_id = args["worktree_id"].as_str().unwrap_or_default().to_owned();
    let apply_id = args["apply_id"].as_str().unwrap_or_default().to_owned();
    let rec = {
        let st = store.lock().await;
        find_record(&st, &session, &worktree_id)
            .ok_or_else(|| refuse("NO_WORKTREE", format!("no worktree `{worktree_id}`")))?
    };
    if rec.task_id != Some(task) {
        return Err(refuse(
            "NOT_YOUR_WORKTREE",
            "a task undoes only the apply of the worktree made for it",
        ));
    }
    let entry = rec
        .applies
        .iter()
        .rev()
        .find(|a| a.apply_id == apply_id)
        .cloned()
        .ok_or_else(|| refuse("NOTHING_TO_UNDO", format!("no apply `{apply_id}`")))?;
    if entry.state == "UNDONE" {
        return Ok(json!({
            "status": "UNDONE", "apply_id": apply_id, "replayed": true,
            "restored_paths": [], "divergent_paths": [],
            "note": "this apply was already undone",
        }));
    }
    if entry.state != "APPLIED" {
        return Err(refuse(
            "NOTHING_TO_UNDO",
            format!("apply `{apply_id}` is {}, not in force", entry.state),
        ));
    }
    let _guard = ApplyingGuard::take(&worktree_id)?;
    let objects = store.lock().await.objects().clone();
    let manifest: Manifest = serde_json::from_slice(
        &objects
            .get(&entry.manifest_ref)
            .map_err(|e| refuse("STORE", format!("the pre-apply checkpoint: {e}")))?,
    )
    .map_err(|e| refuse("STORE", format!("the pre-apply checkpoint: {e}")))?;
    let root = entry.checkout_root.clone();
    let m2 = manifest.clone();
    let report = tokio::task::spawn_blocking(move || {
        restore(
            Path::new(&root),
            &m2,
            &StoreBlobs(objects),
            RestoreMode::AllOrNothing,
        )
    })
    .await
    .map_err(|e| refuse("UNDO_FAILED", e.to_string()))?
    .map_err(|e| refuse("UNDO_FAILED", e.to_string()))?;
    if !report.divergent.is_empty() {
        return Ok(json!({
            "status": "UNDO_CONFLICT",
            "apply_id": apply_id,
            "restored_paths": [],
            "divergent_paths": report.divergent,
            "note": "these files changed after the apply, so nothing was restored; \
                     the checkout is exactly as it was before this call",
        }));
    }
    {
        let mut st = store.lock().await;
        worktrees::append_events(
            &mut st,
            tenant,
            session,
            task,
            worktrees::aggregate_id(&worktree_id),
            vec![worktrees::ev(
                worktrees::APPLY_UNDONE,
                json!({
                    "worktree_id": worktree_id,
                    "apply_id": apply_id,
                    "restored_paths": report.restored,
                    "already": report.already,
                    "by": format!("{actor:?}"),
                }),
                actor,
            )],
        )
        .map_err(|e| refuse("STORE", e))?;
    }
    Ok(json!({
        "status": "UNDONE",
        "apply_id": apply_id,
        "restored_paths": report.restored,
        "divergent_paths": [],
        "note": "",
    }))
}

// ---- recovery ----

/// An apply a dead Core left started and not finished is rolled back to its
/// pre-apply checkpoint — the checkout is the pre-apply state, never a
/// mixture. Returns how many were rolled back.
pub(crate) async fn recover(core: &Arc<Core>) -> usize {
    let records = {
        let store = core.store.lock().await;
        worktrees::registry(&store)
    };
    let mut rolled = 0;
    for rec in records {
        for entry in rec.applies.iter().filter(|a| a.state == "STARTED") {
            let (Some(task), Some(session)) = (rec.task_id, rec.session_id) else {
                continue;
            };
            let objects = core.store.lock().await.objects().clone();
            let manifest: Result<Manifest, String> = objects
                .get(&entry.manifest_ref)
                .map_err(|e| e.to_string())
                .and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()));
            let report = match manifest {
                Ok(m) => {
                    let root = entry.checkout_root.clone();
                    tokio::task::spawn_blocking(move || {
                        restore(
                            Path::new(&root),
                            &m,
                            &StoreBlobs(objects),
                            RestoreMode::BestEffort,
                        )
                    })
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.map_err(|e| e.to_string()))
                }
                Err(e) => Err(e),
            };
            let (payload, ok) = match &report {
                Ok(r) => (
                    json!({
                        "worktree_id": rec.worktree_id,
                        "apply_id": entry.apply_id,
                        "restored_paths": r.restored,
                        "already": r.already,
                        "divergent_paths": r.divergent,
                        "why": "the Core stopped before the apply finished; the pre-apply checkpoint was put back",
                    }),
                    true,
                ),
                Err(e) => (
                    json!({
                        "worktree_id": rec.worktree_id,
                        "apply_id": entry.apply_id,
                        "error": e,
                        "why": "the pre-apply checkpoint could not be put back",
                    }),
                    false,
                ),
            };
            if !ok {
                eprintln!(
                    "modbit-core: apply {} of worktree {} could not be rolled back: {}",
                    entry.apply_id, rec.worktree_id, payload["error"]
                );
                continue;
            }
            let mut store = core.store.lock().await;
            let _ = worktrees::append_events(
                &mut store,
                core.tenant_id,
                session,
                task,
                worktrees::aggregate_id(&rec.worktree_id),
                vec![worktrees::ev(
                    worktrees::APPLY_ROLLED_BACK,
                    payload,
                    &Actor::Core("worktree-recovery".into()),
                )],
            );
            rolled += 1;
        }
    }
    if rolled > 0 {
        let offset = core.store.lock().await.last_offset().unwrap_or(0);
        core.last_offset.send_replace(offset);
    }
    rolled
}
