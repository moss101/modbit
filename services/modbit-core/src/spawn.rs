//! Transactional subagent admission (M6.3; docs/14 "Decomposition",
//! "Transactional subagent admission", "Bounded recursive delegation";
//! REQ-EV-0267, 0007, 0051, 0048, 0078, 0046): the primary proposes a
//! `SubtaskSpec`; Core validates it rather than trusting the model's
//! parallelism, and admits the child as one transaction — idempotency,
//! depth, parent liveness at the expected generation, write-set overlap
//! with the workers already admitted, a capacity ticket, a worktree of its
//! own, a least-privilege lease, the AgentGraph node and the WorkGraph
//! ownership — or refuses at the first step that fails and returns
//! everything taken before it. The child then runs as its own task on the
//! same runtime loop, inside its capsule and nothing of the parent's
//! transcript.

use std::sync::Arc;

use modbit_core_runtime::capacity::ResourceVector;
use modbit_domain::agent::{
    AgentBinding, AgentExecutionCapsule, AgentKind, AgentNode, AgentStatus, SpawnMode, SubtaskSpec,
    WorkNodeChange, WorkStatus, write_scopes_overlap,
};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{Task, TaskEvent, TaskState};
use modbit_domain::toolcall::EffectClass;
use modbit_domain::{AgentId, RunId, TaskId};

use crate::runtime::{Lineage, StartConfig, append, typed};
use crate::server::Core;

/// The default delegation depth: one level. Nested delegation is disabled
/// unless `MODBIT_AGENT_MAX_DEPTH` says otherwise (REQ-EV-0051).
fn max_depth() -> u32 {
    std::env::var("MODBIT_AGENT_MAX_DEPTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
}

/// What a parent asks for.
pub(crate) struct SpawnRequest {
    /// The parent task, as the loop holds it.
    pub parent: Task,
    /// The parent's run.
    pub parent_run: RunId,
    /// The parent's lease generation: the child is admitted only while the
    /// parent is still the owner at this generation.
    pub parent_generation: u64,
    /// The binding the parent runs on; the child inherits it.
    pub binding: AgentBinding,
    /// The spec.
    pub spec: SubtaskSpec,
    /// Scheduling mode.
    pub mode: SpawnMode,
    /// The idempotency key the model gave (REQ-EV-0007).
    pub idempotency_key: String,
}

/// What admission produced.
#[derive(Clone, Debug)]
pub(crate) struct Spawned {
    pub agent_id: AgentId,
    pub child_task_id: TaskId,
    pub capsule_ref: String,
    pub ticket_id: String,
    pub worktree: String,
    pub branch: String,
    pub work_node: String,
    /// True when the key named a child that already exists.
    pub reattached: bool,
    /// Non-blocking conflict findings (M6.4) the parent is told.
    pub warnings: Vec<String>,
}

/// Why admission refused, with what it rolled back.
#[derive(Clone, Debug)]
pub(crate) struct SpawnRefused {
    pub code: String,
    pub detail: String,
    pub stage: &'static str,
    pub rolled_back: Vec<String>,
}

/// A failure injected for the fault proof (EPR-FI-style, QUAL-EV-0267):
/// `MODBIT_FAULT_SPAWN=<stage>` fails admission after that stage's resource
/// was taken, so the rollback is exercised for real.
fn injected_fault(stage: &str) -> bool {
    std::env::var("MODBIT_FAULT_SPAWN").is_ok_and(|v| v == stage)
}

/// Admit and start a child. See the module doc for the transaction.
pub(crate) async fn spawn(
    core: &Arc<Core>,
    req: SpawnRequest,
    actor: &Actor,
) -> Result<Spawned, SpawnRefused> {
    let parent = &req.parent;
    let lt = Lineage::run(
        core.tenant_id,
        parent.session_id,
        parent.task_id,
        req.parent_run,
    );
    let mut rolled_back: Vec<String> = Vec::new();
    let refuse =
        |code: &str, detail: String, stage: &'static str, rolled_back: Vec<String>| SpawnRefused {
            code: code.to_owned(),
            detail,
            stage,
            rolled_back,
        };
    let record_refusal = |core: &Core, r: &SpawnRefused| {
        let store = core.store.clone();
        let ev = typed(
            "SubagentAdmissionRefused",
            &TaskEvent::SubagentAdmissionRefused {
                idempotency_key: req.idempotency_key.clone(),
                code: r.code.clone(),
                detail: r.detail.clone(),
                stage: r.stage.into(),
                rolled_back: r.rolled_back.clone(),
            },
            actor.clone(),
        );
        (store, ev)
    };
    // 1. Idempotency: the key names a child that exists.
    let (parent_node, existing) = {
        let store = core.store.lock().await;
        let nodes = store.agent_nodes(&parent.task_id).unwrap_or_default();
        let existing = nodes
            .iter()
            .find(|n| n.idempotency_key == req.idempotency_key && n.kind == "SUBAGENT")
            .cloned();
        let primary = nodes.into_iter().find(|n| n.kind == "PRIMARY");
        (primary, existing)
    };
    if let Some(n) = existing {
        let admitted = {
            let store = core.store.lock().await;
            latest_admission(&store, parent, n.agent_id)
        };
        if let Some(a) = admitted {
            return Ok(Spawned {
                agent_id: n.agent_id,
                child_task_id: a.0,
                capsule_ref: a.1,
                ticket_id: a.2,
                worktree: a.3,
                branch: a.4,
                work_node: a.5,
                reattached: true,
                warnings: vec![],
            });
        }
    }
    let Some(parent_node) = parent_node else {
        let r = refuse(
            "PARENT_NOT_ACTIVE",
            "the parent task has no primary agent".into(),
            "PARENT",
            vec![],
        );
        let (store, ev) = record_refusal(core, &r);
        let mut store = store.lock().await;
        let _ = append(
            &mut store,
            core,
            lt,
            AggregateType::Task,
            *parent.task_id.as_bytes(),
            vec![ev],
        );
        return Err(r);
    };
    // 2. Depth (REQ-EV-0051).
    let depth = parent_node.depth + 1;
    if depth > max_depth() {
        let r = refuse(
            if max_depth() == 0 {
                "NESTING_DISABLED"
            } else {
                "DEPTH_EXCEEDED"
            },
            format!(
                "delegation depth {depth} exceeds the maximum {} (MODBIT_AGENT_MAX_DEPTH)",
                max_depth()
            ),
            "DEPTH",
            vec![],
        );
        let (store, ev) = record_refusal(core, &r);
        let mut store = store.lock().await;
        let _ = append(
            &mut store,
            core,
            lt,
            AggregateType::Task,
            *parent.task_id.as_bytes(),
            vec![ev],
        );
        return Err(r);
    }
    // 3. The parent is active at the expected generation.
    {
        let store = core.store.lock().await;
        let live = store.task(&parent.task_id).ok().flatten();
        let session = store.session(&parent.session_id).ok().flatten();
        let generation = session.map(|s| s.lease_generation).unwrap_or(0);
        let active = live.is_some_and(|t| matches!(t.state, TaskState::Running));
        if !active || generation != req.parent_generation {
            let r = refuse(
                "PARENT_NOT_ACTIVE",
                format!(
                    "parent running={active}, session lease generation {generation}, expected {}",
                    req.parent_generation
                ),
                "PARENT",
                vec![],
            );
            drop(store);
            let (store, ev) = record_refusal(core, &r);
            let mut store = store.lock().await;
            let _ = append(
                &mut store,
                core,
                lt,
                AggregateType::Task,
                *parent.task_id.as_bytes(),
                vec![ev],
            );
            return Err(r);
        }
    }
    // 4. Write-set conflicts with the workers already admitted (docs/14
    //    "Semantic conflict detection", M6.3 + M6.4): explicit path
    //    overlap, the same public symbol from the AST symbol graph,
    //    dependency hot spots, shared/migration/lockfiles, generated files
    //    and fixtures. A blocking conflict refuses; warnings go to the
    //    parent with the admission.
    let mut warnings: Vec<String> = Vec::new();
    {
        let store = core.store.lock().await;
        let live_children: Vec<(AgentId, Vec<String>)> = store
            .agent_nodes(&parent.task_id)
            .unwrap_or_default()
            .into_iter()
            .filter(|n| n.kind == "SUBAGENT")
            .filter(|n| !matches!(n.status.as_str(), "COMPLETED" | "FAILED" | "CANCELLED"))
            .filter_map(|n| latest_admission(&store, parent, n.agent_id).map(|a| (n.agent_id, a.6)))
            .collect();
        drop(store);
        let facts = repo_facts(core, parent).await;
        let admitted: Vec<(String, Vec<String>)> = live_children
            .iter()
            .map(|(id, scope)| (id.to_string(), scope.clone()))
            .collect();
        let conflicts = modbit_core_runtime::conflict::detect(
            &facts,
            &req.spec.write_scope,
            &admitted,
            &modbit_core_runtime::conflict::ConflictPolicy::default(),
        );
        let explicit = live_children
            .iter()
            .flat_map(|(other, scope)| {
                write_scopes_overlap(&req.spec.write_scope, scope)
                    .into_iter()
                    .map(move |(a, b)| format!("{a} ~ {b} ({other})"))
            })
            .collect::<Vec<_>>();
        let blocking: Vec<String> = conflicts
            .iter()
            .filter(|c| c.severity == modbit_core_runtime::conflict::Severity::Block)
            .map(|c| format!("{:?} with {}: {}", c.rule, c.with, c.detail))
            .collect();
        warnings = conflicts
            .iter()
            .filter(|c| c.severity == modbit_core_runtime::conflict::Severity::Warn)
            .map(|c| format!("{:?} with {}: {}", c.rule, c.with, c.detail))
            .collect();
        if !explicit.is_empty() || !blocking.is_empty() {
            let r = refuse(
                "WRITE_CONFLICT",
                format!(
                    "write scope conflicts: {}",
                    explicit
                        .into_iter()
                        .chain(blocking)
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
                "WRITE_SET",
                vec![],
            );
            let (store, ev) = record_refusal(core, &r);
            let mut store = store.lock().await;
            let _ = append(
                &mut store,
                core,
                lt,
                AggregateType::Task,
                *parent.task_id.as_bytes(),
                vec![ev],
            );
            return Err(r);
        }
        let store = core.store.lock().await;
        if req.spec.write_scope.is_empty() && !live_children.is_empty() {
            let r = refuse(
                "WRITE_CONFLICT",
                "a builder with an unbounded write scope cannot run beside admitted workers; name its write_scope".into(),
                "WRITE_SET",
                vec![],
            );
            drop(store);
            let (store, ev) = record_refusal(core, &r);
            let mut store = store.lock().await;
            let _ = append(
                &mut store,
                core,
                lt,
                AggregateType::Task,
                *parent.task_id.as_bytes(),
                vec![ev],
            );
            return Err(r);
        }
    }
    // 5. Capacity: the child's own run ticket (REQ-EV-0272).
    let agent_id = AgentId::new();
    let holder = format!("agent:{agent_id}");
    let ticket = {
        let mut store = core.store.lock().await;
        core.capacity.acquire(
            &mut store,
            core,
            parent,
            lt,
            actor,
            &holder,
            ResourceVector::one_run(),
            req.parent_generation,
        )
    };
    let ticket = match ticket {
        Ok(t) => t,
        Err(e) => {
            let r = refuse(
                e.code(),
                serde_json::to_string(&e).unwrap_or_default(),
                "CAPACITY",
                vec![],
            );
            let (store, ev) = record_refusal(core, &r);
            let mut store = store.lock().await;
            let _ = append(
                &mut store,
                core,
                lt,
                AggregateType::Task,
                *parent.task_id.as_bytes(),
                vec![ev],
            );
            return Err(r);
        }
    };
    // 6. The worktree: a checkpoint of the parent, then a fork carrying
    //    nothing of the parent's transcript (REQ-EV-0078), leased to the
    //    child's write scope and capped at its parent's ceiling
    //    (REQ-EV-0046 / 0048).
    if let Err(e) = crate::checkpoint::capture(core, parent, lt, actor, None, "before_spawn").await
    {
        rollback_ticket(core, parent, lt, actor, &ticket.ticket_id, &mut rolled_back).await;
        let r = refuse(
            "WORKTREE_FAILED",
            format!("checkpoint before spawn: {e}"),
            "WORKTREE",
            rolled_back,
        );
        let (store, ev) = record_refusal(core, &r);
        let mut store = store.lock().await;
        let _ = append(
            &mut store,
            core,
            lt,
            AggregateType::Task,
            *parent.task_id.as_bytes(),
            vec![ev],
        );
        return Err(r);
    }
    let child_task_id = TaskId::new();
    let fork = crate::branch::fork(
        core,
        crate::branch::ForkRequest {
            source: parent.clone(),
            checkpoint: None,
            goal_text: Some(req.spec.objective.clone()),
            carry: vec![],
            worktree_dir: None,
            new_task_id: child_task_id,
            subagent: Some(crate::branch::SubagentFork {
                write_scope: req.spec.write_scope.clone(),
                effect_ceiling_cap: Some(EffectClass::ReversibleWrite),
            }),
        },
        actor,
    )
    .await;
    let forked = match fork {
        Ok(Ok(f)) if !injected_fault("WORKTREE") => f,
        Ok(Ok(f)) => {
            // The injected fault: the worktree exists; it is removed and
            // the child task cancelled before the ticket is returned.
            rollback_fork(core, parent, actor, &f, &mut rolled_back).await;
            rollback_ticket(core, parent, lt, actor, &ticket.ticket_id, &mut rolled_back).await;
            let r = refuse(
                "WORKTREE_FAILED",
                "injected fault after the worktree".into(),
                "WORKTREE",
                rolled_back,
            );
            let (store, ev) = record_refusal(core, &r);
            let mut store = store.lock().await;
            let _ = append(
                &mut store,
                core,
                lt,
                AggregateType::Task,
                *parent.task_id.as_bytes(),
                vec![ev],
            );
            return Err(r);
        }
        Ok(Err(refused)) => {
            rollback_ticket(core, parent, lt, actor, &ticket.ticket_id, &mut rolled_back).await;
            let r = refuse(
                "WORKTREE_FAILED",
                format!("{}: {}", refused.code, refused.detail),
                "WORKTREE",
                rolled_back,
            );
            let (store, ev) = record_refusal(core, &r);
            let mut store = store.lock().await;
            let _ = append(
                &mut store,
                core,
                lt,
                AggregateType::Task,
                *parent.task_id.as_bytes(),
                vec![ev],
            );
            return Err(r);
        }
        Err(e) => {
            rollback_ticket(core, parent, lt, actor, &ticket.ticket_id, &mut rolled_back).await;
            let r = refuse("WORKTREE_FAILED", e.to_string(), "WORKTREE", rolled_back);
            let (store, ev) = record_refusal(core, &r);
            let mut store = store.lock().await;
            let _ = append(
                &mut store,
                core,
                lt,
                AggregateType::Task,
                *parent.task_id.as_bytes(),
                vec![ev],
            );
            return Err(r);
        }
    };
    // 7. The capsule, the node and the work ownership, persisted together.
    let max_turns = if req.spec.max_turns == 0 {
        20
    } else {
        req.spec.max_turns
    };
    let max_tool_calls = req.spec.max_tool_calls;
    let work_node = req
        .spec
        .work_node
        .clone()
        .unwrap_or_else(|| format!("agent-{}", req.idempotency_key));
    let capsule = AgentExecutionCapsule {
        agent_id,
        parent_agent_id: parent_node.agent_id,
        parent_task_id: parent.task_id,
        child_task_id,
        depth,
        spec: req.spec.clone(),
        tools: req.spec.required_tools.clone(),
        binding: req.binding.clone(),
        max_turns,
        max_tool_calls,
        effect_ceiling: "REVERSIBLE_WRITE".into(),
        worktree: forked.worktree.clone(),
        branch: forked.branch.clone(),
        allowed_modalities: vec!["text".into(), "image".into()],
        private_context_refs: vec![],
        mode: req.mode,
    };
    let (capsule_ref, work_changed, ready) = {
        let mut store = core.store.lock().await;
        let capsule_ref = store
            .objects()
            .put(&serde_json::to_vec(&capsule).unwrap_or_default())
            .unwrap_or_default();
        // The work node the child owns: named by the spec, or created from
        // the objective.
        let mut graph = modbit_domain::agent::WorkGraph {
            nodes: store.work_nodes(&parent.task_id).unwrap_or_default(),
        };
        let plan_version = graph
            .nodes
            .iter()
            .map(|n| n.plan_version)
            .max()
            .unwrap_or(0);
        let change = WorkNodeChange {
            id: work_node.clone(),
            title: Some(req.spec.objective.clone()),
            depends_on: Some(req.spec.depends_on.clone()),
            status: Some(WorkStatus::Active),
            expected_artifacts: Some(req.spec.expected_artifacts.clone()),
            verification: Some(req.spec.verification.clone()),
            evidence_refs: vec![],
            blockers: Some(vec![]),
        };
        let changed = graph.apply(parent.task_id, plan_version, &[change]);
        let (mut work_changed, ready) = match changed {
            Ok(c) => (
                c,
                graph
                    .ready()
                    .iter()
                    .map(|n| n.id.clone())
                    .collect::<Vec<_>>(),
            ),
            Err(_) => (vec![], vec![]),
        };
        for n in &mut work_changed {
            n.owner = Some(agent_id);
        }
        (capsule_ref, work_changed, ready)
    };
    let node = AgentNode {
        agent_id,
        task_id: parent.task_id,
        parent_agent_id: Some(parent_node.agent_id),
        root_agent_id: parent_node.root_agent_id,
        depth,
        kind: AgentKind::Subagent,
        status: AgentStatus::Admitted,
        run_id: None,
        capsule_ref: Some(capsule_ref.clone()),
        binding: req.binding.clone(),
        idempotency_key: req.idempotency_key.clone(),
        owns: vec![work_node.clone()],
        child_task_id: Some(child_task_id),
    };
    let mode_label = match req.mode {
        SpawnMode::Foreground => "FOREGROUND",
        SpawnMode::Background => "BACKGROUND",
    };
    {
        let mut store = core.store.lock().await;
        let mut events = vec![typed(
            "AgentNodeCreated",
            &TaskEvent::AgentNodeCreated { node: node.clone() },
            actor.clone(),
        )];
        if !work_changed.is_empty() {
            events.push(typed(
                "WorkNodesChanged",
                &TaskEvent::WorkNodesChanged {
                    plan_version: work_changed[0].plan_version,
                    changed: work_changed.clone(),
                    ready,
                },
                actor.clone(),
            ));
        }
        events.push(typed(
            "SubagentAdmitted",
            &TaskEvent::SubagentAdmitted {
                agent_id,
                child_task_id,
                capsule_ref: capsule_ref.clone(),
                ticket_id: ticket.ticket_id.clone(),
                worktree: forked.worktree.clone(),
                branch: forked.branch.clone(),
                write_scope: req.spec.write_scope.clone(),
                work_node: work_node.clone(),
                idempotency_key: req.idempotency_key.clone(),
                mode: mode_label.into(),
            },
            actor.clone(),
        ));
        if let Err(e) = append(
            &mut store,
            core,
            lt,
            AggregateType::Task,
            *parent.task_id.as_bytes(),
            events,
        ) {
            drop(store);
            rollback_fork(core, parent, actor, &forked, &mut rolled_back).await;
            rollback_ticket(core, parent, lt, actor, &ticket.ticket_id, &mut rolled_back).await;
            let r = refuse("STORE", e, "LEASE", rolled_back);
            let (store, ev) = record_refusal(core, &r);
            let mut store = store.lock().await;
            let _ = append(
                &mut store,
                core,
                lt,
                AggregateType::Task,
                *parent.task_id.as_bytes(),
                vec![ev],
            );
            return Err(r);
        }
    }
    // The child's own log names its capsule and its parent, so its harness
    // runs inside the capsule after any restart.
    {
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            Lineage::task(core.tenant_id, parent.session_id, child_task_id),
            AggregateType::Task,
            *child_task_id.as_bytes(),
            vec![typed(
                "SubagentCapsuleBound",
                &TaskEvent::SubagentCapsuleBound {
                    agent_id,
                    parent_task_id: parent.task_id,
                    capsule_ref: capsule_ref.clone(),
                },
                actor.clone(),
            )],
        );
    }
    // 8. Start the child's run on the admission ticket. A start that fails
    //    is the last rollback: node failed, worktree removed, ticket back.
    let child_task = {
        let store = core.store.lock().await;
        store.task(&child_task_id).ok().flatten()
    };
    let Some(child_task) = child_task else {
        rollback_fork(core, parent, actor, &forked, &mut rolled_back).await;
        rollback_ticket(core, parent, lt, actor, &ticket.ticket_id, &mut rolled_back).await;
        fail_node(
            core,
            parent,
            lt,
            actor,
            agent_id,
            "the child task was not readable",
        )
        .await;
        let r = refuse(
            "START_FAILED",
            "the child task was not readable".into(),
            "START",
            rolled_back,
        );
        let (store, ev) = record_refusal(core, &r);
        let mut store = store.lock().await;
        let _ = append(
            &mut store,
            core,
            lt,
            AggregateType::Task,
            *parent.task_id.as_bytes(),
            vec![ev],
        );
        return Err(r);
    };
    let cfg = StartConfig {
        endpoint: req.binding.endpoint.clone(),
        model: req.binding.model.clone(),
        budgets: modbit_core_runtime::Budgets {
            max_turns,
            max_tool_calls,
            max_consecutive_no_progress_turns: 3,
        },
        pinned: false,
        plan_id: String::new(),
        slot_id: String::new(),
        skills: vec![],
        lease_generation: req.parent_generation,
        ticket_id: ticket.ticket_id.clone(),
    };
    let child_actor = Actor::Agent(format!("subagent:{agent_id}"));
    match core
        .runtime
        .start(core, child_task, cfg, req.parent_generation, child_actor)
        .await
    {
        Ok((run_id, _)) => {
            let mut store = core.store.lock().await;
            let _ = append(
                &mut store,
                core,
                lt,
                AggregateType::Task,
                *parent.task_id.as_bytes(),
                vec![typed(
                    "AgentNodeTransitioned",
                    &TaskEvent::AgentNodeTransitioned {
                        agent_id,
                        from: AgentStatus::Admitted,
                        to: if req.mode == SpawnMode::Background {
                            AgentStatus::Background
                        } else {
                            AgentStatus::Running
                        },
                        run_id: Some(run_id),
                        reason: format!("child run started ({mode_label})"),
                    },
                    actor.clone(),
                )],
            );
            Ok(Spawned {
                agent_id,
                child_task_id,
                capsule_ref,
                ticket_id: ticket.ticket_id,
                worktree: forked.worktree,
                branch: forked.branch,
                work_node,
                reattached: false,
                warnings,
            })
        }
        Err((code, detail)) => {
            rollback_fork(core, parent, actor, &forked, &mut rolled_back).await;
            rollback_ticket(core, parent, lt, actor, &ticket.ticket_id, &mut rolled_back).await;
            fail_node(
                core,
                parent,
                lt,
                actor,
                agent_id,
                &format!("{code}: {detail}"),
            )
            .await;
            let r = refuse(&code, detail, "START", rolled_back);
            let (store, ev) = record_refusal(core, &r);
            let mut store = store.lock().await;
            let _ = append(
                &mut store,
                core,
                lt,
                AggregateType::Task,
                *parent.task_id.as_bytes(),
                vec![ev],
            );
            Err(r)
        }
    }
}

/// The latest admission of `agent_id` on the parent's log:
/// `(child_task_id, capsule_ref, ticket_id, worktree, branch, work_node, write_scope)`.
type Admission = (TaskId, String, String, String, String, String, Vec<String>);

pub(crate) fn latest_admission(
    store: &modbit_event_store::EventStore,
    parent: &Task,
    agent_id: AgentId,
) -> Option<Admission> {
    let events = store
        .read_session(&parent.session_id, 0, usize::MAX)
        .unwrap_or_default();
    events
        .iter()
        .rev()
        .filter(|e| {
            e.envelope.task_id == Some(parent.task_id)
                && e.envelope.event_type == "SubagentAdmitted"
        })
        .find_map(|e| {
            let p = store.payload(&e.envelope).ok()?;
            let id = p["agent_id"].as_str()?;
            if id != agent_id.to_string() {
                return None;
            }
            let child = TaskId::parse(p["child_task_id"].as_str()?).ok()?;
            Some((
                child,
                p["capsule_ref"].as_str().unwrap_or_default().to_owned(),
                p["ticket_id"].as_str().unwrap_or_default().to_owned(),
                p["worktree"].as_str().unwrap_or_default().to_owned(),
                p["branch"].as_str().unwrap_or_default().to_owned(),
                p["work_node"].as_str().unwrap_or_default().to_owned(),
                p["write_scope"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default(),
            ))
        })
}

async fn rollback_ticket(
    core: &Core,
    parent: &Task,
    lt: Lineage,
    actor: &Actor,
    ticket_id: &str,
    rolled_back: &mut Vec<String>,
) {
    let mut store = core.store.lock().await;
    core.capacity
        .release(&mut store, core, parent, lt, actor, ticket_id);
    rolled_back.push(format!("capacity ticket {ticket_id} released"));
}

/// Remove the child's worktree and cancel its task: nothing of a refused
/// admission stays behind (REQ-EV-0267).
async fn rollback_fork(
    core: &Core,
    parent: &Task,
    actor: &Actor,
    f: &crate::branch::Forked,
    rolled_back: &mut Vec<String>,
) {
    if let Some(root) = parent.workspace_root.as_deref()
        && let Ok(repo) = modbit_git::Repo::open(std::path::Path::new(root))
    {
        match repo.worktree_remove(std::path::Path::new(&f.worktree)) {
            Ok(()) => rolled_back.push(format!("worktree {} removed", f.worktree)),
            Err(e) => rolled_back.push(format!("worktree {} NOT removed: {e}", f.worktree)),
        }
    }
    let mut store = core.store.lock().await;
    let _ = append(
        &mut store,
        core,
        Lineage::task(core.tenant_id, parent.session_id, f.task_id),
        AggregateType::Task,
        *f.task_id.as_bytes(),
        vec![typed(
            "TaskCancelled",
            &TaskEvent::TaskCancelled,
            actor.clone(),
        )],
    );
    rolled_back.push(format!("child task {} cancelled", f.task_id));
}

async fn fail_node(
    core: &Core,
    parent: &Task,
    lt: Lineage,
    actor: &Actor,
    agent_id: AgentId,
    reason: &str,
) {
    let mut store = core.store.lock().await;
    let _ = append(
        &mut store,
        core,
        lt,
        AggregateType::Task,
        *parent.task_id.as_bytes(),
        vec![typed(
            "AgentNodeTransitioned",
            &TaskEvent::AgentNodeTransitioned {
                agent_id,
                from: AgentStatus::Admitted,
                to: AgentStatus::Failed,
                run_id: None,
                reason: reason.to_owned(),
            },
            actor.clone(),
        )],
    );
}

/// The child's typed result envelope, on the parent's log (M6.5; docs/14
/// "Agent-to-agent communication"): summary, evidence refs, artifacts,
/// unresolved risks, the branch to merge. Written by the child's loop as
/// it ends, whatever way it ended; the parent's `agent.wait` reads it.
/// The parent's node for the child moves with it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn record_result(
    store: &mut modbit_event_store::EventStore,
    core: &Core,
    child: &Task,
    capsule: &serde_json::Value,
    state: &modbit_core_runtime::harness::HarnessState,
    end: AgentStatus,
    end_reason: &str,
    actor: &Actor,
) {
    let (Some(agent_id), Some(parent_task_id)) = (
        capsule["agent_id"]
            .as_str()
            .and_then(|s| AgentId::parse(s).ok()),
        capsule["parent_task_id"]
            .as_str()
            .and_then(|s| TaskId::parse(s).ok()),
    ) else {
        return;
    };
    let status = match end {
        AgentStatus::Completed => "COMPLETED",
        AgentStatus::Failed => "FAILED",
        AgentStatus::Cancelled => "CANCELLED",
        _ => "WAITING",
    };
    // What the child changed in its worktree: every FileChanged on its
    // workspace aggregate, deduplicated by path, last op wins.
    let mut artifacts: Vec<String> = Vec::new();
    if let Some(root) = child.workspace_root.as_deref()
        && let Ok(events) =
            store.read_aggregate(&crate::tools::workspace_aggregate_id(root), 0, 100_000)
    {
        for e in &events {
            if let Ok(modbit_domain::workspace::WorkspaceEvent::FileChanged { path, .. }) = store
                .payload(&e.envelope)
                .and_then(|p| serde_json::from_value(p).map_err(Into::into))
                && !artifacts.contains(&path)
            {
                artifacts.push(path);
            }
        }
    }
    // Evidence: the child's verification runs and its latest gate.
    let mut evidence_refs: Vec<String> = Vec::new();
    for run in store.runs_for_task(&child.task_id).unwrap_or_default() {
        for vr in store.verification_runs(&run.run_id).unwrap_or_default() {
            evidence_refs.push(format!(
                "verification:{}:{}",
                vr.stage, vr.verification_run_id
            ));
            evidence_refs.extend(vr.report_refs.iter().map(|r| format!("object:{r}")));
        }
    }
    if let Some(a) = state.acceptance.as_ref()
        && let Some(g) = a["gate_ref"].as_str()
    {
        evidence_refs.push(format!("gate:{g}"));
    }
    let mut unresolved: Vec<String> = state.open_failures.clone();
    if status != "COMPLETED" {
        unresolved.push(format!("ended {status}: {end_reason}"));
    }
    if !state.self_review_clean && status == "COMPLETED" {
        unresolved.push("self-review left findings unresolved".into());
    }
    let (branch, worktree) = (
        capsule["branch"].as_str().unwrap_or_default().to_owned(),
        child.workspace_root.clone().unwrap_or_default(),
    );
    let result = modbit_domain::agent::SubagentResult {
        agent_id,
        child_task_id: child.task_id,
        status: status.into(),
        summary: state
            .completion_summary
            .clone()
            .unwrap_or_else(|| end_reason.to_owned()),
        artifacts: artifacts.clone(),
        evidence_refs: evidence_refs.clone(),
        unresolved_risks: unresolved.clone(),
        proposed_follow_ups: vec![],
        branch: branch.clone(),
        worktree,
        candidate_revision: state.candidate_revision.unwrap_or(0),
    };
    let result_ref = store
        .objects()
        .put(&serde_json::to_vec(&result).unwrap_or_default())
        .unwrap_or_default();
    let Ok(Some(parent)) = store.task(&parent_task_id) else {
        return;
    };
    let lt = Lineage::task(core.tenant_id, parent.session_id, parent.task_id);
    let node_status = store
        .agent_nodes(&parent.task_id)
        .unwrap_or_default()
        .into_iter()
        .find(|n| n.agent_id == agent_id)
        .map(|n| n.status);
    let mut events = vec![typed(
        "SubagentResultRecorded",
        &TaskEvent::SubagentResultRecorded {
            agent_id,
            child_task_id: child.task_id,
            status: status.into(),
            result_ref,
            summary: result.summary.clone(),
            artifacts,
            evidence_refs,
            unresolved_risks: unresolved,
            branch,
        },
        actor.clone(),
    )];
    if let Some(from) = node_status {
        let from: AgentStatus =
            serde_json::from_value(serde_json::Value::String(from)).unwrap_or(AgentStatus::Running);
        let to = match end {
            AgentStatus::Completed | AgentStatus::Failed | AgentStatus::Cancelled => end,
            _ => AgentStatus::Waiting,
        };
        if from != to && from.can_transition(to) {
            events.push(typed(
                "AgentNodeTransitioned",
                &TaskEvent::AgentNodeTransitioned {
                    agent_id,
                    from,
                    to,
                    run_id: None,
                    reason: end_reason.to_owned(),
                },
                actor.clone(),
            ));
        }
    }
    let _ = append(
        store,
        core,
        lt,
        AggregateType::Task,
        *parent.task_id.as_bytes(),
        events,
    );
}

/// What the parent's repository knows, from the Core's exact index, the
/// symbol index and the evidence graph (M3), for the conflict rules.
async fn repo_facts(core: &Core, parent: &Task) -> modbit_core_runtime::conflict::Facts {
    let mut facts = modbit_core_runtime::conflict::Facts::default();
    let Some(root) = parent.workspace_root.as_deref() else {
        return facts;
    };
    let canonical = std::path::Path::new(root)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(root));
    let Ok(index) = core.tools.index(&canonical).await else {
        return facts;
    };
    {
        let index = index.lock().await;
        facts.paths = index.texts().map(|(p, _, _)| p.to_owned()).collect();
    }
    if let Ok(sx) = core.tools.symbols(&canonical).await {
        let sx = sx.lock().await;
        for p in &facts.paths {
            let syms: Vec<modbit_core_runtime::conflict::SymbolFact> = sx
                .symbols_in(p)
                .iter()
                .map(|s| (s.name.clone(), s.kind.clone(), s.container.clone()))
                .collect();
            if !syms.is_empty() {
                facts.symbols.push((p.clone(), syms));
            }
        }
    }
    if let Ok(g) = core.tools.graph(&canonical).await {
        let g = g.lock().await;
        for p in &facts.paths {
            let v = g.query(&modbit_retrieval::GraphQuery {
                path: p.clone(),
                relation: "importers".into(),
                depth: 1,
                max: 1000,
            });
            if !v.importers.is_empty() {
                facts
                    .importers
                    .push((p.clone(), v.importers.into_iter().map(|(f, _)| f).collect()));
            }
        }
    }
    facts
}
