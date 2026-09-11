//! Session branching (REQ-EV-0077 fork with carryover, REQ-EV-0122 the
//! `BranchCarryoverCapsule`, REQ-EV-0123 rewind preview / hash-checked
//! revert / session tree; docs/19 "session/branch generation", docs/13).
//!
//! A fork is a new task in the same session with its own git worktree — a
//! branch at the checkpoint's HEAD with the checkpoint's dirty state
//! materialized, object by object, hash-checked — and therefore its own
//! revision lineage. What it carries from the source is recorded once, as
//! a content-addressed capsule the fork's log names: the plan, the answered
//! questions, the retrieval records whose bytes the fork still has, the
//! source's latest context. Pending approvals and unfinished tool calls are
//! bound to the source's intents and are never carried; the capsule lists
//! them as dropped. Every fork moves the session's branch generation
//! (`SessionBranched`), which invalidates compactions computed under the
//! old one (M4.2).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use modbit_checkpoint::{
    BranchCarryoverCapsule, CARRYOVER_SCHEMA_VERSION, CarriedContext, CarriedDecision,
    CarriedEvidence, CarriedPlan, Carry, DroppedApproval, RewindEntry, chain_to, materialize,
    plan_rewind, validate,
};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::session::SessionEvent;
use modbit_domain::task::{Task, TaskEvent, TaskOrigin, TaskState};
use modbit_domain::{CheckpointId, TaskId, Timestamp};
use modbit_event_store::{AppendRequest, EventStore};
use modbit_protocol::v1 as wire;
use modbit_workspace::{ChangeOp, ChangeOpKind, WritePrecondition, content_hash};

use crate::checkpoint::{Objects, RestoreRefused, chain_refusal, current, manifests};
use crate::runtime::{Lineage, append, typed};
use crate::server::Core;
use crate::tools::{append_file_events, file_changed_events, read_workspace_file};

/// What a fork asks for.
pub(crate) struct ForkRequest {
    /// The source task.
    pub source: Task,
    /// The checkpoint to fork from; `None` = the source's current one.
    pub checkpoint: Option<CheckpointId>,
    /// The fork's goal; `None` = the source's.
    pub goal_text: Option<String>,
    /// What to carry; empty = everything.
    pub carry: Vec<Carry>,
    /// Where the worktree goes; `None` = `<profile>/worktrees/<task id>`.
    pub worktree_dir: Option<PathBuf>,
    /// The new task's id (the command id, for idempotency).
    pub new_task_id: TaskId,
}

/// What a fork produced.
pub(crate) struct Forked {
    pub task_id: TaskId,
    pub checkpoint_id: CheckpointId,
    pub epoch: u32,
    pub capsule_ref: String,
    pub worktree: String,
    pub branch: String,
    pub branch_generation: u64,
    pub carried: Vec<Carry>,
    pub decisions_carried: u32,
    pub evidence_carried: u32,
    pub approvals_dropped: u32,
    pub calls_dropped: u32,
    pub files_materialized: u32,
    pub offset: u64,
}

/// The source's decisions, plan, evidence and context, read off its log.
struct SourceFacts {
    plan: Option<CarriedPlan>,
    decisions: Vec<CarriedDecision>,
    retrievals: Vec<(String, String, String)>,
    context_pack_ref: Option<String>,
    latest_run: Option<String>,
}

fn source_facts(store: &EventStore, task: &Task) -> SourceFacts {
    let mut plan = None;
    let mut asked: HashMap<String, (String, String)> = HashMap::new();
    let mut decisions = Vec::new();
    let mut retrievals = Vec::new();
    let mut context_pack_ref = None;
    let events = store
        .read_session(&task.session_id, 0, usize::MAX)
        .unwrap_or_default();
    for e in events
        .iter()
        .filter(|e| e.envelope.task_id == Some(task.task_id))
    {
        let p = store.payload(&e.envelope).unwrap_or_default();
        match e.envelope.event_type.as_str() {
            "PlanRecorded" => {
                plan = Some(CarriedPlan {
                    plan_ref: p["plan_ref"].as_str().unwrap_or_default().to_owned(),
                    expected_files: strings(&p["expected_files"]),
                    version: p["version"].as_u64().unwrap_or(1) as u32,
                });
            }
            "PlanRevised" => {
                let mut files = plan
                    .as_ref()
                    .map(|p| p.expected_files.clone())
                    .unwrap_or_default();
                for a in strings(&p["added"]) {
                    if !files.contains(&a) {
                        files.push(a);
                    }
                }
                let removed = strings(&p["removed"]);
                files.retain(|f| !removed.contains(f));
                plan = Some(CarriedPlan {
                    plan_ref: p["plan_ref"].as_str().unwrap_or_default().to_owned(),
                    expected_files: files,
                    version: p["version"].as_u64().unwrap_or(1) as u32,
                });
            }
            "UserQuestionAsked" => {
                asked.insert(
                    p["question_id"].as_str().unwrap_or_default().to_owned(),
                    (
                        p["question"].as_str().unwrap_or_default().to_owned(),
                        p["reason"].as_str().unwrap_or_default().to_owned(),
                    ),
                );
            }
            "UserQuestionAnswered" => {
                let qid = p["question_id"].as_str().unwrap_or_default().to_owned();
                if let Some((question, reason)) = asked.get(&qid) {
                    decisions.push(CarriedDecision {
                        question_id: qid,
                        question: question.clone(),
                        reason: reason.clone(),
                        option_id: p["option_id"].as_str().map(str::to_owned),
                        text: p["text"].as_str().map(str::to_owned),
                    });
                }
            }
            "RetrievalRecorded" => {
                retrievals.push((
                    p["path"].as_str().unwrap_or_default().to_owned(),
                    p["content_hash"].as_str().unwrap_or_default().to_owned(),
                    p["tool_call_id"].as_str().unwrap_or_default().to_owned(),
                ));
            }
            "ContextPackCompiled" => {
                context_pack_ref = p["context_pack_id"].as_str().map(str::to_owned);
            }
            _ => {}
        }
    }
    let latest_run = store
        .runs_for_task(&task.task_id)
        .unwrap_or_default()
        .into_iter()
        .next()
        .map(|r| r.run_id.to_string());
    SourceFacts {
        plan,
        decisions,
        retrievals,
        context_pack_ref,
        latest_run,
    }
}

fn strings(v: &serde_json::Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn sanitized_root(p: &Path) -> String {
    p.to_string_lossy().trim_start_matches(r"\\?\").to_owned()
}

/// Fork `req.source` at a checkpoint into a new task with its own worktree.
pub(crate) async fn fork(
    core: &Core,
    req: ForkRequest,
    actor: &Actor,
) -> anyhow::Result<Result<Forked, RestoreRefused>> {
    let source = &req.source;
    let root = source
        .workspace_root
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("the source task has no workspace root"))?;
    let carry: Vec<Carry> = if req.carry.is_empty() {
        Carry::ALL.to_vec()
    } else {
        req.carry.clone()
    };
    // The checkpoint chain, validated object by object before anything is
    // created (docs/19: nothing is restored from an object that does not
    // hash to its name).
    let (state, bytes, facts, approvals_dropped, calls_dropped, compaction, session_generation) = {
        let store = core.store.lock().await;
        let all = manifests(&store, source);
        let target = match req
            .checkpoint
            .or_else(|| current(&store, source).map(|c| c.checkpoint_id))
        {
            Some(t) => t,
            None => {
                return Ok(Err(RestoreRefused {
                    code: "NO_CHECKPOINT",
                    detail: "the source task has no committed checkpoint to fork from".into(),
                }));
            }
        };
        let chain = match chain_to(&all, target) {
            Ok(c) => c,
            Err(e) => return Ok(Err(chain_refusal(e))),
        };
        let state = match materialize(&chain) {
            Ok(s) => s,
            Err(e) => return Ok(Err(chain_refusal(e))),
        };
        let bytes = match validate(&state, &Objects(store.objects().clone())) {
            Ok(b) => b,
            Err(e) => return Ok(Err(chain_refusal(e))),
        };
        let facts = source_facts(&store, source);
        let approvals_dropped: Vec<DroppedApproval> = store
            .approvals_for_task(&source.task_id)
            .unwrap_or_default()
            .into_iter()
            .filter(|a| a.state == modbit_domain::approval::ApprovalState::Requested)
            .map(|a| DroppedApproval {
                approval_id: a.approval_id.to_string(),
                tool_call_id: a.tool_call_id.to_string(),
                reason: "pending on the source: bound to the source's intent, never carried".into(),
            })
            .collect();
        let calls_dropped: Vec<String> = store
            .open_tool_calls(&source.task_id)
            .unwrap_or_default()
            .iter()
            .map(|c| c.tool_call_id.to_string())
            .collect();
        let compaction = store
            .compaction_epochs(&source.task_id)
            .unwrap_or_default()
            .into_iter()
            .filter(|r| r.status == "COMMITTED")
            .max_by_key(|r| r.epoch)
            .map(|r| (r.epoch, r.result_object_hash.clone()));
        let session_generation = store
            .session(&source.session_id)?
            .map(|s| s.branch_generation)
            .unwrap_or(0);
        (
            state,
            bytes,
            facts,
            approvals_dropped,
            calls_dropped,
            compaction,
            session_generation,
        )
    };
    // The worktree: a branch at the checkpoint's HEAD, checked out beside
    // the profile, never inside the source repository.
    let repo = modbit_git::Repo::open(Path::new(root))?;
    let base = state
        .git_head
        .clone()
        .or_else(|| repo.head().ok())
        .unwrap_or_else(|| "HEAD".into());
    let short = &req.new_task_id.to_string()[..8];
    let branch = format!("modbit/fork-{short}");
    let worktree_dir = req.worktree_dir.clone().unwrap_or_else(|| {
        core.data_dir
            .join("worktrees")
            .join(req.new_task_id.to_string())
    });
    if let Some(parent) = worktree_dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    repo.create_branch(&branch, &base)?;
    let wt = repo.worktree_add(&worktree_dir, &branch)?;
    let worktree = sanitized_root(
        &wt.dir()
            .canonicalize()
            .unwrap_or_else(|_| wt.dir().to_path_buf()),
    );
    // Materialize the checkpoint's dirty state into the worktree through the
    // workspace service, so the fork's first revision lineage is on the log.
    let (ws, canonical) = core.tools.workspace(&worktree).await?;
    let mut files_materialized = 0u32;
    let fork_revision;
    {
        let mut ws = ws.lock().await;
        let mut ops: Vec<ChangeOp> = Vec::new();
        let mut pre_bytes: HashMap<String, Option<Vec<u8>>> = HashMap::new();
        for (path, hash) in &state.files {
            if hash == modbit_checkpoint::DELETED {
                if read_workspace_file(&ws, path).is_some() {
                    pre_bytes.insert(path.clone(), read_workspace_file(&ws, path));
                    ops.push(ChangeOp {
                        path: path.clone(),
                        kind: ChangeOpKind::Delete,
                        pre: WritePrecondition::default(),
                    });
                    files_materialized += 1;
                }
                continue;
            }
            let Some(content) = bytes.get(path) else {
                continue;
            };
            let existing = read_workspace_file(&ws, path);
            if existing
                .as_deref()
                .is_some_and(|c| content_hash(c) == *hash)
            {
                continue;
            }
            pre_bytes.insert(path.clone(), existing.clone());
            ops.push(ChangeOp {
                path: path.clone(),
                kind: if existing.is_some() {
                    ChangeOpKind::ReplaceExact(content.clone())
                } else {
                    ChangeOpKind::Create(content.clone())
                },
                pre: WritePrecondition::default(),
            });
            files_materialized += 1;
        }
        let pre_revision = ws.revision().number;
        if !ops.is_empty() {
            let changes = ws
                .apply_transaction(&ops)
                .map_err(|e| anyhow::anyhow!("fork materialization: {e}"))?;
            let value = serde_json::json!({ "changes": changes });
            let objects = core.store.lock().await.objects().clone();
            let events = file_changed_events(
                &objects,
                &ws,
                req.new_task_id,
                modbit_domain::ToolCallId::new(),
                &crate::tools::workspace_changes(&value),
                &pre_bytes,
                pre_revision,
                "fork:",
            );
            let mut st = core.store.lock().await;
            append_file_events(
                &mut st,
                core.tenant_id,
                source.session_id,
                req.new_task_id,
                &canonical.to_string_lossy(),
                events,
            )?;
        }
        fork_revision = ws.revision().number;
    }
    // Evidence the fork can honestly claim: retrieval records whose bytes
    // its worktree has at the path.
    let evidence: Vec<(CarriedEvidence, String)> = if carry.contains(&Carry::Evidence) {
        let ws = ws.lock().await;
        facts
            .retrievals
            .iter()
            .filter(|(path, hash, _)| {
                read_workspace_file(&ws, path)
                    .as_deref()
                    .is_some_and(|c| content_hash(c) == *hash)
            })
            .map(|(path, hash, call)| {
                (
                    CarriedEvidence {
                        path: path.clone(),
                        content_hash: hash.clone(),
                    },
                    call.clone(),
                )
            })
            .collect()
    } else {
        vec![]
    };
    let branch_generation = session_generation + 1;
    let capsule = BranchCarryoverCapsule {
        schema_version: CARRYOVER_SCHEMA_VERSION,
        source_task_id: source.task_id,
        source_run_id: facts.latest_run.clone(),
        source_checkpoint_id: state.checkpoint_id,
        source_epoch: state.epoch,
        source_event_offset: state.runtime.event_offset,
        fork_task_id: req.new_task_id,
        branch_generation,
        carried: carry.clone(),
        plan: if carry.contains(&Carry::Plan) {
            facts.plan.clone()
        } else {
            None
        },
        decisions: if carry.contains(&Carry::Decisions) {
            facts.decisions.clone()
        } else {
            vec![]
        },
        evidence: evidence.iter().map(|(e, _)| e.clone()).collect(),
        context: if carry.contains(&Carry::Context) {
            Some(CarriedContext {
                context_pack_ref: facts.context_pack_ref.clone(),
                compaction_epoch: compaction.as_ref().map(|c| c.0).unwrap_or(0),
                compaction_manifest_ref: compaction.and_then(|c| c.1),
            })
        } else {
            None
        },
        approvals_dropped,
        calls_dropped,
        worktree_files: state.files.clone(),
        git_head: state.git_head.clone(),
        created_at: Timestamp::now(),
    };
    let mut store = core.store.lock().await;
    let capsule_ref = store.objects().put(&serde_json::to_vec(&capsule)?)?;
    // The session branches first: every pending compaction under the old
    // generation is stale from here on (M4.2).
    let session_lt = Lineage::session(core.tenant_id, source.session_id);
    append(
        &mut store,
        core,
        session_lt,
        AggregateType::Session,
        *source.session_id.as_bytes(),
        vec![typed(
            "SessionBranched",
            &SessionEvent::SessionBranched {
                branch_generation,
                kind: "fork".into(),
                reason: format!(
                    "task {} forked from task {} at checkpoint {} (epoch {})",
                    req.new_task_id, source.task_id, state.checkpoint_id, state.epoch
                ),
            },
            actor.clone(),
        )],
    )
    .map_err(|e| anyhow::anyhow!("SessionBranched: {e}"))?;
    // The fork's own log: created, queued, forked, then what it carries as
    // the same durable records the harness rebuilds from.
    let mut events = vec![
        typed(
            "TaskCreated",
            &TaskEvent::TaskCreated {
                session_id: source.session_id,
                goal_text: req
                    .goal_text
                    .clone()
                    .unwrap_or_else(|| source.goal_text.clone()),
                workspace_id: modbit_domain::WorkspaceId::new(),
                workspace_root: Some(worktree.clone()),
                base_revision: state.git_head.clone(),
                execution_profile: source.execution_profile.clone(),
                policy_profile_id: None,
                origin: TaskOrigin::Fork,
            },
            actor.clone(),
        ),
        typed("TaskQueued", &TaskEvent::TaskQueued, actor.clone()),
        typed(
            "TaskForked",
            &TaskEvent::TaskForked {
                from_task_id: source.task_id.to_string(),
                from_checkpoint_id: state.checkpoint_id.to_string(),
                from_epoch: state.epoch,
                from_run_id: facts.latest_run.clone(),
                from_event_offset: state.runtime.event_offset,
                branch_generation,
                capsule_ref: capsule_ref.clone(),
                carried: carry.iter().map(|c| c.label().to_owned()).collect(),
                decisions_carried: capsule.decisions.len() as u32,
                evidence_carried: capsule.evidence.len() as u32,
                approvals_dropped: capsule.approvals_dropped.len() as u32,
                worktree: worktree.clone(),
                branch: branch.clone(),
            },
            actor.clone(),
        ),
    ];
    if let Some(p) = &capsule.plan {
        events.push(typed(
            "PlanRecorded",
            &TaskEvent::PlanRecorded {
                plan_ref: p.plan_ref.clone(),
                expected_files: p.expected_files.clone(),
                version: p.version,
            },
            actor.clone(),
        ));
    }
    for (e, call) in &evidence {
        events.push(typed(
            "RetrievalRecorded",
            &TaskEvent::RetrievalRecorded {
                path: e.path.clone(),
                content_hash: e.content_hash.clone(),
                workspace_revision: fork_revision,
                tool_call_id: call.clone(),
                tool_name: "fork:carried".into(),
            },
            actor.clone(),
        ));
    }
    let stored = store.append(AppendRequest {
        tenant_id: core.tenant_id,
        session_id: source.session_id,
        task_id: Some(req.new_task_id),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::Task,
        aggregate_id: *req.new_task_id.as_bytes(),
        expected_sequence: Some(0),
        events,
    })?;
    let mut offset = stored.last().map(|e| e.offset).unwrap_or(0);
    // Capability Kernel (docs/23): the fork's own default lease on its own root.
    let (resources, operations, effect_ceiling) = modbit_policy::default_lease_for_profile(
        &source.execution_profile,
        Some(worktree.as_str()),
    );
    let lease_id = modbit_domain::CapabilityLeaseId::new();
    let granted = store.append(AppendRequest {
        tenant_id: core.tenant_id,
        session_id: source.session_id,
        task_id: Some(req.new_task_id),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::CapabilityLease,
        aggregate_id: *lease_id.as_bytes(),
        expected_sequence: Some(0),
        events: vec![typed(
            "CapabilityLeaseGranted",
            &modbit_domain::lease::CapabilityLeaseEvent::CapabilityLeaseGranted {
                tenant_id: core.tenant_id,
                task_id: req.new_task_id,
                agent_id: None,
                resources,
                operations,
                effect_ceiling,
                execution_profile: source.execution_profile.clone(),
                generation: 1,
                expires_at: None,
            },
            actor.clone(),
        )],
    })?;
    if let Some(last) = granted.last() {
        offset = last.offset;
    }
    core.last_offset.send_replace(offset);
    Ok(Ok(Forked {
        task_id: req.new_task_id,
        checkpoint_id: state.checkpoint_id,
        epoch: state.epoch,
        capsule_ref,
        worktree,
        branch,
        branch_generation,
        carried: carry,
        decisions_carried: capsule.decisions.len() as u32,
        evidence_carried: capsule.evidence.len() as u32,
        approvals_dropped: capsule.approvals_dropped.len() as u32,
        calls_dropped: capsule.calls_dropped.len() as u32,
        files_materialized,
        offset,
    }))
}

/// A rewind preview.
pub(crate) struct Preview {
    pub checkpoint_id: CheckpointId,
    pub epoch: u32,
    pub event_offset: u64,
    pub entries: Vec<RewindEntry>,
    pub workspace_revision: u64,
}

/// What restoring `target` would do to the task's worktree, without doing
/// it: no event, no write, no revision change (REQ-EV-0123).
pub(crate) async fn preview_rewind(
    core: &Core,
    task: &Task,
    target: Option<CheckpointId>,
) -> anyhow::Result<Result<Preview, RestoreRefused>> {
    let root = task
        .workspace_root
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("task has no workspace root"))?;
    let (ws, canonical) = core.tools.workspace(root).await?;
    let state = {
        let store = core.store.lock().await;
        let all = manifests(&store, task);
        let target = match target.or_else(|| current(&store, task).map(|c| c.checkpoint_id)) {
            Some(t) => t,
            None => {
                return Ok(Err(RestoreRefused {
                    code: "NO_CHECKPOINT",
                    detail: "the task has no committed checkpoint".into(),
                }));
            }
        };
        let chain = match chain_to(&all, target) {
            Ok(c) => c,
            Err(e) => return Ok(Err(chain_refusal(e))),
        };
        let state = match materialize(&chain) {
            Ok(s) => s,
            Err(e) => return Ok(Err(chain_refusal(e))),
        };
        // The objects are read and hash-checked here too: a preview that
        // promised a restore the objects cannot deliver would mislead.
        if let Err(e) = validate(&state, &Objects(store.objects().clone())) {
            return Ok(Err(chain_refusal(e)));
        }
        state
    };
    let repo = modbit_git::Repo::open(&canonical)?;
    let ws = ws.lock().await;
    let now = worktree_now(&repo, &ws)?;
    let entries = plan_rewind(&state, &now.current, &now.tracked);
    Ok(Ok(Preview {
        checkpoint_id: state.checkpoint_id,
        epoch: state.epoch,
        event_offset: state.runtime.event_offset,
        entries,
        workspace_revision: ws.revision().number,
    }))
}

/// The worktree's dirty and untracked paths with their content hashes now
/// (`None` = a tracked path deleted from the worktree), and which of them
/// are tracked at HEAD.
pub(crate) struct WorktreeNow {
    /// Path → content hash now (`None` = deleted).
    pub current: BTreeMap<String, Option<String>>,
    /// Paths tracked at HEAD.
    pub tracked: BTreeSet<String>,
}

pub(crate) fn worktree_now(
    repo: &modbit_git::Repo,
    ws: &modbit_workspace::WorkspaceService,
) -> anyhow::Result<WorktreeNow> {
    let mut current = BTreeMap::new();
    let mut tracked = BTreeSet::new();
    for e in repo.status()? {
        let now = read_workspace_file(ws, &e.path).map(|b| content_hash(&b));
        if e.code != "??" {
            tracked.insert(e.path.clone());
        }
        current.insert(e.path, now);
    }
    Ok(WorktreeNow { current, tracked })
}

/// The session's run DAG, read off the log (REQ-EV-0123: explicit and
/// auditable).
pub(crate) fn session_tree(
    store: &EventStore,
    session_id: modbit_domain::SessionId,
) -> anyhow::Result<wire::SessionTreeView> {
    let session = store
        .session(&session_id)?
        .ok_or_else(|| anyhow::anyhow!("unknown session"))?;
    let events = store.read_session(&session_id, 0, usize::MAX)?;
    let mut branches = Vec::new();
    let mut forks: HashMap<TaskId, (String, String, u32, u64, String, u64)> = HashMap::new();
    let mut restores: HashMap<TaskId, Vec<wire::RestoreView>> = HashMap::new();
    let mut order: Vec<TaskId> = Vec::new();
    for e in &events {
        let p = store.payload(&e.envelope).unwrap_or_default();
        match e.envelope.event_type.as_str() {
            "SessionBranched" => branches.push(wire::BranchEventView {
                branch_generation: p["branch_generation"].as_u64().unwrap_or(0),
                kind: p["kind"].as_str().unwrap_or_default().to_owned(),
                reason: p["reason"].as_str().unwrap_or_default().to_owned(),
                offset: e.offset,
            }),
            "TaskCreated" => {
                if let Some(t) = e.envelope.task_id {
                    order.push(t);
                }
            }
            "TaskForked" => {
                if let Some(t) = e.envelope.task_id {
                    forks.insert(
                        t,
                        (
                            p["from_task_id"].as_str().unwrap_or_default().to_owned(),
                            p["from_checkpoint_id"]
                                .as_str()
                                .unwrap_or_default()
                                .to_owned(),
                            p["from_epoch"].as_u64().unwrap_or(0) as u32,
                            p["from_event_offset"].as_u64().unwrap_or(0),
                            p["capsule_ref"].as_str().unwrap_or_default().to_owned(),
                            p["branch_generation"].as_u64().unwrap_or(0),
                        ),
                    );
                }
            }
            "CheckpointRestored" => {
                if let Some(t) = e.envelope.task_id {
                    restores.entry(t).or_default().push(wire::RestoreView {
                        checkpoint_id: p["checkpoint_id"].as_str().unwrap_or_default().to_owned(),
                        epoch: p["epoch"].as_u64().unwrap_or(0) as u32,
                        offset: e.offset,
                        files_written: p["files_written"].as_u64().unwrap_or(0) as u32,
                        files_reverted: p["files_reverted"].as_u64().unwrap_or(0) as u32,
                        preconditions_checked: p["preconditions_checked"].as_u64().unwrap_or(0)
                            as u32,
                    });
                }
            }
            _ => {}
        }
    }
    let mut tasks = Vec::new();
    for t in order {
        let Some(task) = store.task(&t)? else {
            continue;
        };
        let fork = forks.get(&t);
        let runs = store
            .runs_for_task(&t)
            .unwrap_or_default()
            .into_iter()
            .map(|r| wire::TaskRunView {
                run_id: Some(wire::Id {
                    value: r.run_id.as_bytes().to_vec(),
                }),
                state: format!("{:?}", r.state),
            })
            .collect();
        let checkpoints = crate::checkpoint::list(store, &task).checkpoints;
        tasks.push(wire::SessionTreeNode {
            task_id: Some(wire::Id {
                value: t.as_bytes().to_vec(),
            }),
            goal_text: task.goal_text.clone(),
            state: match task.state {
                TaskState::Waiting(r) => format!("Waiting({r:?})"),
                other => format!("{other:?}"),
            },
            origin: format!("{:?}", task.origin).to_lowercase(),
            workspace_root: task.workspace_root.clone().unwrap_or_default(),
            forked_from_task: fork
                .and_then(|f| TaskId::parse(&f.0).ok())
                .map(|id| wire::Id {
                    value: id.as_bytes().to_vec(),
                }),
            forked_from_checkpoint: fork.map(|f| f.1.clone()).unwrap_or_default(),
            forked_from_epoch: fork.map(|f| f.2).unwrap_or(0),
            forked_from_offset: fork.map(|f| f.3).unwrap_or(0),
            capsule_ref: fork.map(|f| f.4.clone()).unwrap_or_default(),
            branch_generation: fork.map(|f| f.5).unwrap_or(0),
            runs,
            checkpoints,
            restores: restores.remove(&t).unwrap_or_default(),
        });
    }
    Ok(wire::SessionTreeView {
        session_id: Some(wire::Id {
            value: session_id.as_bytes().to_vec(),
        }),
        branch_generation: session.branch_generation,
        tasks,
        branches,
    })
}
