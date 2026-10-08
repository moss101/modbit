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

use modbit_core_runtime::budget::{self, ChildHold, Held};
use modbit_core_runtime::capacity::ResourceVector;
use modbit_domain::agent::{
    AgentBinding, AgentExecutionCapsule, AgentKind, AgentNode, AgentStatus, SpawnMode, SubtaskSpec,
    WorkNodeChange, WorkStatus, write_scopes_overlap,
};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::state::StateMachine;
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
    /// The parent's budgets (REQ-PX-116): the caps the child's is clamped
    /// to, and how many children it may have alive.
    pub parent_budgets: modbit_core_runtime::Budgets,
    /// What the parent has spent itself so far.
    pub parent_own: Held,
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
    /// The mode in force and why (REQ-EV-0180).
    pub mode: SpawnMode,
    pub scheduling: &'static str,
    /// The profile compiled in and what it narrowed (REQ-EV-0115).
    pub profile: String,
    pub narrowed_tools: Vec<String>,
    /// Non-blocking conflict findings (M6.4) the parent is told.
    pub warnings: Vec<String>,
    /// What was reserved against the parent, in words (REQ-PX-116).
    pub budget: String,
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

/// A process kill injected between the admission append (which reserves the
/// child's budget) and the child's start (`MODBIT_FAULT_SPAWN_KILL=
/// AFTER_ADMIT`): the real crash the recovery proof needs, not a simulated
/// one.
fn injected_kill(stage: &str) {
    if std::env::var("MODBIT_FAULT_SPAWN_KILL").is_ok_and(|v| v == stage) {
        eprintln!("modbit-core: injected kill at {stage} of a spawn");
        std::process::abort();
    }
}

/// Admit and start a child. See the module doc for the transaction.
pub(crate) async fn spawn(
    core: &Arc<Core>,
    req: SpawnRequest,
    actor: &Actor,
) -> Result<Spawned, SpawnRefused> {
    let mut req = req;
    let parent = req.parent.clone();
    let parent = &parent;
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
    // 0. The profile, when one is named (REQ-EV-0115 / 0182 / 0241): a
    // declarative file compiled into the request — its tools narrowed to
    // what the surface serves under the child's ceiling (never widened),
    // its model when the gateway serves it, its scope and budgets when the
    // spawn left them unset, its body as the child's domain context.
    let mut profile_name = String::new();
    let mut narrowed_tools: Vec<String> = Vec::new();
    let mut profile_context = String::new();
    if let Some(name) = req.spec.profile.clone().filter(|n| !n.trim().is_empty()) {
        let roots = crate::agent_profiles::roots(core, parent);
        let loaded = crate::agent_profiles::load(&roots, name.trim());
        let profile = match loaded {
            Ok(Some(p)) => p,
            Ok(None) => {
                let r = refuse(
                    "PROFILE_UNKNOWN",
                    format!(
                        "no agent profile `{name}` under {}",
                        roots
                            .iter()
                            .map(|r| r.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    "PROFILE",
                    vec![],
                );
                record_and_return(core, parent, lt, &req.idempotency_key, actor, r).await;
                return Err(refuse(
                    "PROFILE_UNKNOWN",
                    format!("no agent profile `{name}`"),
                    "PROFILE",
                    vec![],
                ));
            }
            Err(e) => {
                let r = refuse("PROFILE_INVALID", e.to_string(), "PROFILE", vec![]);
                record_and_return(core, parent, lt, &req.idempotency_key, actor, r).await;
                return Err(refuse("PROFILE_INVALID", e.to_string(), "PROFILE", vec![]));
            }
        };
        let compiled = crate::agent_profiles::compile(
            core,
            parent,
            &profile,
            modbit_domain::toolcall::EffectClass::ReversibleWrite,
        );
        if req.spec.required_tools.is_empty() {
            req.spec.required_tools = compiled.tools.clone();
        } else {
            // The spawn's own list is intersected with the profile's.
            req.spec
                .required_tools
                .retain(|t| compiled.tools.contains(t) || t.starts_with("fs.read"));
        }
        narrowed_tools = compiled.narrowed;
        if req.spec.write_scope.is_empty() {
            req.spec.write_scope = profile.write_scope.clone();
        }
        if req.spec.max_turns == 0 {
            req.spec.max_turns = profile.max_turns;
        }
        if req.spec.max_tool_calls == 0 {
            req.spec.max_tool_calls = profile.max_tool_calls;
        }
        if !profile.model.trim().is_empty() {
            if core
                .gateway
                .capability(&req.binding.endpoint, profile.model.trim())
                .is_some()
            {
                req.binding.model = profile.model.trim().to_owned();
            } else {
                narrowed_tools.push(format!(
                    "model {}: not served by endpoint `{}`; the parent's binding stands",
                    profile.model.trim(),
                    req.binding.endpoint
                ));
            }
        }
        profile_name = profile.name.clone();
        profile_context = profile.context.clone();
    }
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
            // FIX-16: a key names one child with one spec. The same key with
            // a different spec is not a retry — it is a different request
            // that must not silently become the old child. The comparison is
            // against the capsule the admission stored (the spec as admitted,
            // after any profile was compiled in); a capsule that cannot be
            // read cannot be compared and fails closed.
            let stored_spec = {
                let store = core.store.lock().await;
                store
                    .objects()
                    .get(&a.1)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<AgentExecutionCapsule>(&b).ok())
                    .map(|c| c.spec)
            };
            let conflict = match &stored_spec {
                Some(old) if *old == req.spec => None,
                Some(old) => Some(format!(
                    "idempotency_key `{}` already names child {} admitted with a different spec (differs in: {}); a retry must repeat the spec exactly — use a new idempotency_key for a different child",
                    req.idempotency_key,
                    n.agent_id,
                    spec_differences(old, &req.spec).join(", ")
                )),
                None => Some(format!(
                    "idempotency_key `{}` already names child {}, whose admitted spec cannot be read to compare; use a new idempotency_key",
                    req.idempotency_key, n.agent_id
                )),
            };
            if let Some(detail) = conflict {
                let r = refuse("IDEMPOTENCY_CONFLICT", detail, "IDEMPOTENCY", vec![]);
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
            // M6.7: a child the dead Core left suspended continues on its
            // own log when its parent asks for it again — the same identity,
            // lineage, capsule and offsets, a fresh run ticket (docs/25
            // "Subagent continuation").
            let ticket_id = a.2.clone();
            if let Err((code, detail)) =
                resume_child(core, parent, &n, &a.0, &a.1, req.parent_generation, actor).await
            {
                let r = refuse(&code, detail, "START", vec![]);
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
            return Ok(Spawned {
                agent_id: n.agent_id,
                child_task_id: a.0,
                capsule_ref: a.1,
                ticket_id,
                worktree: a.3,
                branch: a.4,
                work_node: a.5,
                reattached: true,
                mode: req.mode,
                scheduling: "REATTACHED",
                profile: String::new(),
                narrowed_tools: vec![],
                warnings: vec![],
                budget: String::new(),
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
    // 3b. Delegation policy and limits (REQ-PX-116). `agent.spawn` is a
    //     permissioned operation like any other: a configuration layer that
    //     denies it (or asks, which a harness spawn has no way to answer)
    //     forbids delegation; the task's own `max_children` of 0 does too;
    //     a parent never has more children alive than `max_children`; and
    //     the child's budget is a slice of the parent's remainder — clamped
    //     to it here, reserved against it in the append that makes the child
    //     exist (step 7), so the parent cannot exceed its own cap through
    //     its children.
    // The early look: a spawn that cannot be given a slice is refused before
    // anything is taken. The slice itself is cut again at step 7.
    let _early_grant = {
        let config = core.tools.configurations.for_task(
            parent.task_id,
            &core.data_dir,
            parent.workspace_root.as_deref(),
        );
        let forbidden: Option<(&'static str, String)> = match config
            .permissions
            .get(SPAWN_CAPABILITY)
        {
            Some(p) if p.value == modbit_policy::config::Permission::Deny => Some((
                "SPAWN_FORBIDDEN",
                format!(
                    "the configuration in force denies `{SPAWN_CAPABILITY}` (decided by the {:?} layer); a person changes the policy, the agent does not",
                    p.provenance.decided_by
                ),
            )),
            Some(p) if p.value == modbit_policy::config::Permission::Ask => Some((
                "SPAWN_FORBIDDEN",
                format!(
                    "the configuration in force asks before `{SPAWN_CAPABILITY}` (the {:?} layer) and a harness spawn has no approval to ask; delegation is off until the policy allows it",
                    p.provenance.decided_by
                ),
            )),
            _ if req.parent_budgets.max_children == 0 => Some((
                "SPAWN_FORBIDDEN",
                "this task forbids delegation (max_children 0): work alone".into(),
            )),
            _ => None,
        };
        if let Some((code, detail)) = forbidden {
            let r = refuse(code, detail, "POLICY", vec![]);
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
        let holds = child_holds(core, parent).await;
        let live = holds.iter().filter(|(_, _, h)| h.live).count();
        let max_children = req.parent_budgets.max_children as usize;
        if live >= max_children {
            let r = refuse(
                "MAX_CHILDREN_EXCEEDED",
                format!(
                    "{live} children are alive and this task allows {max_children} at once (max_children); wait for one to finish with agent.wait, or do the work yourself"
                ),
                "LIMIT",
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
        match clamp_for(&req, &holds) {
            Ok(x) => x,
            Err(detail) => {
                let r = refuse("PARENT_BUDGET_INSUFFICIENT", detail, "BUDGET", vec![]);
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
    };
    // 4. Write-set conflicts with the workers already admitted (docs/14
    //    "Semantic conflict detection", M6.3 + M6.4): explicit path
    //    overlap, the same public symbol from the AST symbol graph,
    //    dependency hot spots, shared/migration/lockfiles, generated files
    //    and fixtures. A blocking conflict refuses; warnings go to the
    //    parent with the admission.
    let warnings: Vec<String> = {
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
        let warnings: Vec<String> = conflicts
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
        warnings
    };
    // 4b. The work node the child will own, validated against the parent's
    //     WorkGraph before anything is taken (FIX-16): an unknown dependency,
    //     a cycle or a bad id refuses the spawn, typed, with nothing to give
    //     back. (It used to be swallowed and the child admitted with no work
    //     node recorded, the model none the wiser.)
    let work_node = req
        .spec
        .work_node
        .clone()
        .unwrap_or_else(|| format!("agent-{}", req.idempotency_key));
    {
        let store = core.store.lock().await;
        if let Err(e) = apply_work_change(&store, parent, &req.spec, &work_node) {
            drop(store);
            let r = refuse(
                "WORK_GRAPH_INVALID",
                format!("work node `{work_node}`: {}", work_graph_error_text(&e)),
                "WORK_GRAPH",
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
                read_scope: req.spec.read_scope.clone(),
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
    //    The budget is what the clamp leaves of what was asked, taken again
    //    now: the children already running spend while the worktree is made,
    //    and the slice is cut from what the parent has left at this moment,
    //    not at step 3b.
    let (grant, clamp_notes) = {
        let holds = child_holds(core, parent).await;
        match clamp_for(&req, &holds) {
            Ok(x) => x,
            Err(detail) => {
                rollback_fork(core, parent, actor, &forked, &mut rolled_back).await;
                rollback_ticket(core, parent, lt, actor, &ticket.ticket_id, &mut rolled_back).await;
                let r = refuse("PARENT_BUDGET_INSUFFICIENT", detail, "BUDGET", rolled_back);
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
    };
    let max_turns = u32::try_from(grant.turns).unwrap_or(u32::MAX);
    let max_tool_calls = u32::try_from(grant.tool_calls).unwrap_or(u32::MAX);
    let private_context_refs = context_refs(core, parent, &req.spec).await;
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
        max_cost_minor: grant.cost_minor.unwrap_or(0),
        max_wall_ms: grant.wall_ms.unwrap_or(0),
        effect_ceiling: "REVERSIBLE_WRITE".into(),
        worktree: forked.worktree.clone(),
        branch: forked.branch.clone(),
        allowed_modalities: vec!["text".into(), "image".into()],
        private_context_refs,
        mode: req.mode,
        profile: profile_name.clone(),
        narrowed_tools: narrowed_tools.clone(),
        profile_context: profile_context.clone(),
    };
    // The same change again, now that the worktree and ticket are held: the
    // validated graph can only have moved if a sibling's node did, and a
    // failure here is compensated like every other late one (worktree,
    // child task and ticket returned) and told to the model.
    let applied = {
        let store = core.store.lock().await;
        if injected_fault("WORK_GRAPH") {
            Err("injected fault at the work graph".to_owned())
        } else {
            apply_work_change(&store, parent, &req.spec, &work_node)
                .map_err(|e| work_graph_error_text(&e))
        }
    };
    let (work_changed, ready, blocking) = match applied {
        Ok((changed, ready, blocking)) => {
            let mut work_changed = changed;
            for n in &mut work_changed {
                n.owner = Some(agent_id);
            }
            (work_changed, ready, blocking)
        }
        Err(detail) => {
            rollback_fork(core, parent, actor, &forked, &mut rolled_back).await;
            rollback_ticket(core, parent, lt, actor, &ticket.ticket_id, &mut rolled_back).await;
            let r = refuse(
                "WORK_GRAPH_INVALID",
                format!("work node `{work_node}`: {detail}"),
                "WORK_GRAPH",
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
    };
    let (mode, scheduling) = match (req.mode, blocking) {
        (SpawnMode::Background, true) => (SpawnMode::Foreground, "BLOCKING"),
        (SpawnMode::Background, false) => (SpawnMode::Background, "SEPARABLE"),
        (SpawnMode::Foreground, _) => (SpawnMode::Foreground, "REQUESTED"),
    };
    let capsule = AgentExecutionCapsule { mode, ..capsule };
    let capsule_ref = {
        let store = core.store.lock().await;
        store
            .objects()
            .put(&serde_json::to_vec(&capsule).unwrap_or_default())
            .unwrap_or_default()
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
    let mode_label = match mode {
        SpawnMode::Foreground => "FOREGROUND",
        SpawnMode::Background => "BACKGROUND",
    };
    {
        let mut store = core.store.lock().await;
        // The reservation is checked against the log and made in the very
        // append that creates the child, under the store lock: two
        // admissions of one parent cannot both claim the same remainder,
        // and a Core killed before this append leaves no reservation and no
        // child, after it both (REQ-PX-116).
        let recheck: std::result::Result<(), String> = {
            let holds: Vec<ChildHold> = child_holds_in(&store, core, parent)
                .into_iter()
                // What a running child has spent past its reservation since
                // the slice was cut is that child's own overrun, charged to
                // the parent at its next round boundary; it is not another
                // admission and must not refuse this one. Reservations are
                // what two admissions compete for.
                .map(|(_, _, mut h)| {
                    if h.live {
                        h.spent = modbit_core_runtime::budget::Held::default();
                    }
                    h
                })
                .collect();
            let rem = budget::remaining_for_children(
                &req.parent_budgets.caps(),
                &req.parent_own,
                &budget::committed(&holds),
            );
            if grant.turns > rem.turns
                || grant.tool_calls > rem.tool_calls
                || grant
                    .cost_minor
                    .zip(rem.cost_minor)
                    .is_some_and(|(g, r)| g > r)
            {
                Err(format!(
                    "another admission took the remainder first (turns {}, tool calls {}, cost {:?})",
                    rem.turns, rem.tool_calls, rem.cost_minor
                ))
            } else if injected_fault("BUDGET") {
                Err("injected fault at the budget reservation".to_owned())
            } else {
                Ok(())
            }
        };
        if let Err(detail) = recheck {
            drop(store);
            rollback_fork(core, parent, actor, &forked, &mut rolled_back).await;
            rollback_ticket(core, parent, lt, actor, &ticket.ticket_id, &mut rolled_back).await;
            let r = refuse("PARENT_BUDGET_INSUFFICIENT", detail, "BUDGET", rolled_back);
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
                scheduling: scheduling.into(),
                profile: profile_name.clone(),
                narrowed_tools: narrowed_tools.clone(),
                reserved_turns: max_turns,
                reserved_tool_calls: max_tool_calls,
                reserved_cost_minor: grant.cost_minor.unwrap_or(0),
                reserved_wall_ms: grant.wall_ms.unwrap_or(0),
                clamped: clamp_notes.clone(),
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
    injected_kill("AFTER_ADMIT");
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
            max_cost_minor: grant.cost_minor,
            max_wall_ms: grant.wall_ms,
            // A child does not delegate (REQ-EV-0051).
            max_children: 0,
        },
        pinned: false,
        plan_id: String::new(),
        slot_id: String::new(),
        skills: vec![],
        lease_generation: req.parent_generation,
        ticket_id: ticket.ticket_id.clone(),
    };
    let child_actor = Actor::Agent(format!("subagent:{agent_id}"));
    // The child's loop is the same loop that is calling this (a parent
    // spawning from inside its turn): the start goes through the
    // type-erased `start_boxed`, so the loop's future type does not
    // contain itself.
    match core
        .runtime
        .start_boxed(core, child_task, cfg, req.parent_generation, child_actor)
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
                        to: if mode == SpawnMode::Background {
                            AgentStatus::Background
                        } else {
                            AgentStatus::Running
                        },
                        run_id: Some(run_id),
                        reason: format!("child run started ({mode_label}, {scheduling})"),
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
                mode,
                scheduling,
                profile: profile_name,
                narrowed_tools,
                warnings,
                budget: format!(
                    "turns {max_turns}, tool_calls {max_tool_calls}, cost_minor {}, wall_ms {}{}",
                    grant
                        .cost_minor
                        .map_or("uncapped".to_owned(), |c| c.to_string()),
                    grant
                        .wall_ms
                        .map_or("uncapped".to_owned(), |c| c.to_string()),
                    if clamp_notes.is_empty() {
                        String::new()
                    } else {
                        format!(" (clamped to your remainder: {})", clamp_notes.join("; "))
                    }
                ),
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

/// The change a spawn makes to its parent's WorkGraph: the node the child
/// owns, active, with the spec's dependencies, artifacts and verification.
fn work_change(spec: &SubtaskSpec, work_node: &str) -> WorkNodeChange {
    WorkNodeChange {
        id: work_node.to_owned(),
        title: Some(spec.objective.clone()),
        depends_on: Some(spec.depends_on.clone()),
        status: Some(WorkStatus::Active),
        expected_artifacts: Some(spec.expected_artifacts.clone()),
        verification: Some(spec.verification.clone()),
        evidence_refs: vec![],
        blockers: Some(vec![]),
    }
}

/// What applying a spawn's work-node change to a copy of the parent's
/// WorkGraph produces: the nodes that changed, the nodes ready afterwards,
/// and whether the parent's own pending work depends on the child's node.
type AppliedWork = (Vec<modbit_domain::agent::WorkNode>, Vec<String>, bool);

/// Apply the spawn's work-node change to a copy of the parent's WorkGraph.
/// Nothing is recorded; an error is the graph's own refusal.
fn apply_work_change(
    store: &modbit_event_store::EventStore,
    parent: &Task,
    spec: &SubtaskSpec,
    work_node: &str,
) -> Result<AppliedWork, modbit_domain::agent::WorkGraphError> {
    let mut graph = modbit_domain::agent::WorkGraph {
        nodes: store.work_nodes(&parent.task_id).unwrap_or_default(),
    };
    let plan_version = graph
        .nodes
        .iter()
        .map(|n| n.plan_version)
        .max()
        .unwrap_or(0);
    let changed = graph.apply(
        parent.task_id,
        plan_version,
        &[work_change(spec, work_node)],
    )?;
    let ready = graph.ready().iter().map(|n| n.id.clone()).collect();
    // REQ-EV-0180: background only when the parent can go on without this
    // result — nothing of the parent's own pending work depends on the
    // child's node. Otherwise the child is scheduled in the foreground
    // whatever the spawn asked, and the record says why.
    let blocking = graph.blocks_parent(work_node);
    Ok((changed, ready, blocking))
}

fn work_graph_error_text(e: &modbit_domain::agent::WorkGraphError) -> String {
    serde_json::to_string(e).unwrap_or_else(|_| format!("{e:?}"))
}

/// The spec fields two specs disagree on, by name.
fn spec_differences(a: &SubtaskSpec, b: &SubtaskSpec) -> Vec<&'static str> {
    let mut d = Vec::new();
    macro_rules! field {
        ($f:ident) => {
            if a.$f != b.$f {
                d.push(stringify!($f));
            }
        };
    }
    field!(objective);
    field!(expected_artifacts);
    field!(depends_on);
    field!(read_scope);
    field!(write_scope);
    field!(required_tools);
    field!(execution_profile);
    field!(verification);
    field!(max_turns);
    field!(max_tool_calls);
    field!(max_cost_minor);
    field!(max_wall_ms);
    field!(work_node);
    field!(profile);
    d
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

/// The clamp of a spawn's budget request against what its parent has left,
/// given what every child holds now: the grant and what the clamp changed,
/// in words; or why nothing can be given.
fn clamp_for(
    req: &SpawnRequest,
    holds: &[(AgentId, TaskId, ChildHold)],
) -> std::result::Result<(budget::Grant, Vec<String>), String> {
    let only: Vec<ChildHold> = holds.iter().map(|(_, _, h)| *h).collect();
    let live = only.iter().filter(|h| h.live).count();
    let max_children = req.parent_budgets.max_children as usize;
    let rem = budget::remaining_for_children(
        &req.parent_budgets.caps(),
        &req.parent_own,
        &budget::committed(&only),
    );
    let asked = budget::Request {
        turns: u64::from(req.spec.max_turns),
        tool_calls: u64::from(req.spec.max_tool_calls),
        cost_minor: req.spec.max_cost_minor,
        wall_ms: req.spec.max_wall_ms,
    };
    let g = budget::clamp(
        &asked,
        u64::from(modbit_core_runtime::Budgets::default().max_tool_calls),
        &rem,
        u32::try_from(max_children.saturating_sub(live)).unwrap_or(0),
    )
    .map_err(|e| {
        format!(
            "{e}; a child's budget is a slice of its parent's remainder (turns {}, tool calls {}, cost {}, wall clock {}). Finish the work yourself or wait for a child to settle",
            rem.turns,
            rem.tool_calls,
            rem.cost_minor.map_or("uncapped".to_owned(), |c| c.to_string()),
            rem.wall_ms.map_or("uncapped".to_owned(), |c| format!("{c} ms")),
        )
    })?;
    let mut notes = Vec::new();
    if asked.turns != 0 && g.turns < asked.turns {
        notes.push(format!("max_turns {} -> {}", asked.turns, g.turns));
    }
    if asked.tool_calls != 0 && g.tool_calls < asked.tool_calls {
        notes.push(format!(
            "max_tool_calls {} -> {}",
            asked.tool_calls, g.tool_calls
        ));
    }
    if let (true, Some(c)) = (asked.cost_minor != 0, g.cost_minor)
        && c < asked.cost_minor
    {
        notes.push(format!("max_cost_minor {} -> {c}", asked.cost_minor));
    }
    if let (true, Some(w)) = (asked.wall_ms != 0, g.wall_ms)
        && w < asked.wall_ms
    {
        notes.push(format!("max_wall_ms {} -> {w}", asked.wall_ms));
    }
    Ok((g, notes))
}

/// The capability a configuration layer denies to forbid delegation
/// (REQ-PX-116): `{"permissions": {"agent.spawn": "DENY"}}` in the user,
/// project, admin or device layer. A lower layer can only tighten it.
pub(crate) const SPAWN_CAPABILITY: &str = "agent.spawn";

/// Whether the configuration in force lets `task` delegate at all — what the
/// tool projection offers (REQ-PX-116): a layer that denies or asks before
/// `agent.spawn` takes the tool away rather than leaving a refusal to find.
pub(crate) fn spawn_allowed(core: &Core, task: &Task) -> bool {
    let config = core.tools.configurations.for_task(
        task.task_id,
        &core.data_dir,
        task.workspace_root.as_deref(),
    );
    !config.permissions.get(SPAWN_CAPABILITY).is_some_and(|p| {
        matches!(
            p.value,
            modbit_policy::config::Permission::Deny | modbit_policy::config::Permission::Ask
        )
    })
}

/// A node counts as alive while it is being run.
fn node_is_live(status: &str) -> bool {
    matches!(status, "ADMITTED" | "RUNNING" | "BACKGROUND")
}

/// What every child of `parent` holds against it, read off the log: the
/// capsule's reservation, and what the child has spent (counted the way the
/// child counts its own, `usage::SpendScan`). `(agent, child task, hold)`.
pub(crate) fn child_holds_in(
    store: &modbit_event_store::EventStore,
    core: &Core,
    parent: &Task,
) -> Vec<(AgentId, TaskId, ChildHold)> {
    let nodes: Vec<_> = store
        .agent_nodes(&parent.task_id)
        .unwrap_or_default()
        .into_iter()
        .filter(|n| n.kind == "SUBAGENT")
        .collect();
    if nodes.is_empty() {
        return vec![];
    }
    let registry = core.gateway.registry();
    let mut scans: std::collections::HashMap<TaskId, crate::usage::SpendScan<'_>> = nodes
        .iter()
        .filter_map(|n| {
            n.child_task_id
                .or_else(|| latest_admission(store, parent, n.agent_id).map(|a| a.0))
        })
        .map(|t| (t, crate::usage::SpendScan::new(registry.as_ref())))
        .collect();
    for e in store
        .read_session(&parent.session_id, 0, usize::MAX)
        .unwrap_or_default()
    {
        if let Some(scan) = e.envelope.task_id.and_then(|t| scans.get_mut(&t)) {
            let payload = store.payload(&e.envelope).unwrap_or_default();
            scan.observe(&e.envelope, &payload);
        }
    }
    nodes
        .into_iter()
        .filter_map(|n| {
            let child = n
                .child_task_id
                .or_else(|| latest_admission(store, parent, n.agent_id).map(|a| a.0))?;
            let capsule = n
                .capsule_ref
                .as_deref()
                .and_then(|r| store.objects().get(r).ok())
                .and_then(|b| serde_json::from_slice::<AgentExecutionCapsule>(&b).ok());
            let spent = scans
                .remove(&child)
                .map(|s| s.finish().held())
                .unwrap_or_default();
            let reserved = capsule.map_or_else(Held::default, |c| Held {
                turns: u64::from(c.max_turns),
                tool_calls: u64::from(c.max_tool_calls),
                cost_minor: c.max_cost_minor,
                wall_ms: c.max_wall_ms,
            });
            Some((
                n.agent_id,
                child,
                ChildHold {
                    reserved,
                    spent,
                    live: node_is_live(&n.status),
                },
            ))
        })
        .collect()
}

/// [`child_holds_in`] under the store's lock.
pub(crate) async fn child_holds(core: &Core, parent: &Task) -> Vec<(AgentId, TaskId, ChildHold)> {
    let store = core.store.lock().await;
    child_holds_in(&store, core, parent)
}

/// The parent's round boundary (REQ-PX-116): what its children hold against
/// its budget now, from the log — a restarted Core reads the same.
pub(crate) async fn refresh_children_held(
    core: &Core,
    task: &Task,
    state: &mut modbit_core_runtime::harness::HarnessState,
) {
    let holds = child_holds(core, task).await;
    let only: Vec<ChildHold> = holds.iter().map(|(_, _, h)| *h).collect();
    state.children_held = budget::committed(&only);
    state.live_children = u32::try_from(only.iter().filter(|h| h.live).count()).unwrap_or(u32::MAX);
}

/// The budget a child gets when it runs again (follow-up, resume after a
/// park or a restart): its capsule's own caps, and no more than its parent
/// can now cover — the reservation made afresh, since the child's unused
/// part went back to the parent when it stopped. A parent that cannot cover
/// more leaves the child at what it already spent, so its first round
/// boundary ends it `BUDGET_EXHAUSTED` with its partial evidence.
pub(crate) async fn regrant(
    core: &Core,
    parent: &Task,
    child: AgentId,
    capsule: Option<&AgentExecutionCapsule>,
) -> modbit_core_runtime::Budgets {
    let defaults = modbit_core_runtime::Budgets::default();
    let capsule_caps = budget::Caps {
        turns: capsule.map_or(20, |c| {
            if c.max_turns == 0 {
                20
            } else {
                u64::from(c.max_turns)
            }
        }),
        tool_calls: capsule.map_or(u64::from(defaults.max_tool_calls), |c| {
            if c.max_tool_calls == 0 {
                u64::from(defaults.max_tool_calls)
            } else {
                u64::from(c.max_tool_calls)
            }
        }),
        cost_minor: capsule.and_then(|c| (c.max_cost_minor > 0).then_some(c.max_cost_minor)),
        wall_ms: capsule.and_then(|c| (c.max_wall_ms > 0).then_some(c.max_wall_ms)),
    };
    // The parent's own caps and spend: from its log, the way its loop reads them.
    let (parent_caps, own, others, used) = {
        let store = core.store.lock().await;
        let events = store
            .read_session(&parent.session_id, 0, usize::MAX)
            .unwrap_or_default();
        let registry = core.gateway.registry();
        let mut caps = modbit_core_runtime::Budgets::default();
        let mut scan = crate::usage::SpendScan::new(registry.as_ref());
        for e in events
            .iter()
            .filter(|e| e.envelope.task_id == Some(parent.task_id))
        {
            let payload = store.payload(&e.envelope).unwrap_or_default();
            scan.observe(&e.envelope, &payload);
            if e.envelope.event_type == "TaskBudgetsSet" {
                caps.max_cost_minor = payload["max_cost_minor"].as_u64().filter(|c| *c > 0);
                caps.max_wall_ms = payload["max_wall_ms"].as_u64().filter(|c| *c > 0);
            }
        }
        let holds = child_holds_in(&store, core, parent);
        let mine = holds
            .iter()
            .find(|(a, _, _)| *a == child)
            .map(|(_, _, h)| *h);
        let others: Vec<ChildHold> = holds
            .iter()
            .filter(|(a, _, _)| *a != child)
            .map(|(_, _, h)| *h)
            .collect();
        (
            caps,
            scan.finish().held(),
            budget::committed(&others),
            mine.map(|h| h.spent).unwrap_or_default(),
        )
    };
    // The parent's turn and tool-call caps are the ones its loop was
    // started under; its cost and wall-clock caps are on its log.
    let mut parent_caps = parent_caps;
    if let Some(b) = core.runtime.budgets_of(&parent.task_id).await {
        parent_caps.max_turns = b.max_turns;
        parent_caps.max_tool_calls = b.max_tool_calls;
    }
    let rem = budget::remaining_for_children(&parent_caps.caps(), &own, &others);
    let g = budget::regrant(&capsule_caps, &used, &rem);
    modbit_core_runtime::Budgets {
        max_turns: u32::try_from(g.turns).unwrap_or(u32::MAX),
        max_tool_calls: u32::try_from(g.tool_calls).unwrap_or(u32::MAX),
        max_consecutive_no_progress_turns: 3,
        max_cost_minor: g.cost_minor,
        max_wall_ms: g.wall_ms,
        max_children: 0,
    }
}

/// The references a child starts with (REQ-PX-116): what its parent has
/// already read that falls inside the child's read and write scope — a path
/// and the content hash it was read at, never the parent's transcript.
/// Nothing outside the scope is handed over: a reference to a file the child
/// cannot read would be a way around its lease.
async fn context_refs(core: &Core, parent: &Task, spec: &SubtaskSpec) -> Vec<String> {
    const MAX_REFS: usize = 32;
    let scope: Vec<String> = spec
        .read_scope
        .iter()
        .chain(spec.write_scope.iter())
        .map(|p| {
            p.trim()
                .trim_start_matches("./")
                .trim_end_matches("/**")
                .trim_end_matches('/')
                .to_owned()
        })
        .filter(|p| !p.is_empty())
        .collect();
    let in_scope = |path: &str| {
        scope.is_empty()
            || scope
                .iter()
                .any(|s| path == s || path.starts_with(&format!("{s}/")))
    };
    let ledger = core.tools.ledger(&core.store, parent.task_id).await;
    let ledger = ledger.lock().await;
    let mut refs: Vec<String> = Vec::new();
    let reads = ledger.reads.iter().filter_map(|r| {
        r.content_hash
            .as_ref()
            .map(|h| (r.path.as_str(), h.as_str()))
    });
    let entries = ledger.entries.iter().filter_map(|e| {
        e.content_hash
            .as_ref()
            .map(|h| (e.path.as_str(), h.as_str()))
    });
    for (path, hash) in reads.chain(entries) {
        if in_scope(path) {
            let r = format!("workspace:{path}@{hash}");
            if !refs.contains(&r) {
                refs.push(r);
            }
        }
    }
    // The newest are the ones the parent is working from.
    let skip = refs.len().saturating_sub(MAX_REFS);
    refs.split_off(skip)
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

/// A spawned child that is not over: its node is in no terminal status.
#[derive(Clone, Debug)]
pub(crate) struct UnsettledChild {
    pub agent_id: AgentId,
    pub key: String,
    pub status: String,
    pub child_task_id: Option<TaskId>,
}

/// The children `parent` spawned whose agent nodes are not terminal
/// (`COMPLETED`, `FAILED`, `CANCELLED`): still admitted, running, parked or
/// waiting. A parent is not done while it has any (FIX-16).
pub(crate) async fn unsettled_children(core: &Core, parent: &TaskId) -> Vec<UnsettledChild> {
    let store = core.store.lock().await;
    let task = store.task(parent).ok().flatten();
    store
        .agent_nodes(parent)
        .unwrap_or_default()
        .into_iter()
        .filter(|n| n.kind == "SUBAGENT")
        .filter(|n| !matches!(n.status.as_str(), "COMPLETED" | "FAILED" | "CANCELLED"))
        .map(|n| UnsettledChild {
            child_task_id: n.child_task_id.or_else(|| {
                task.as_ref()
                    .and_then(|t| latest_admission(&store, t, n.agent_id))
                    .map(|a| a.0)
            }),
            agent_id: n.agent_id,
            key: n.idempotency_key,
            status: n.status,
        })
        .collect()
}

/// Cancel every child `parent` still has alive, and theirs in turn (FIX-16:
/// a cancellation domain, not a flag on one task). A child with a live loop
/// is told to stop at its next safe boundary — its own loop end records the
/// cancelled result and node, as for `agent.cancel`. A child with no loop (a
/// parked or restart-suspended one) is cancelled durably here, with its node,
/// since nothing else would ever end it. Returns the children touched.
/// Idempotent: a child already over is skipped.
pub(crate) async fn cancel_children(
    core: &Arc<Core>,
    parent: &Task,
    reason: &str,
    actor: &Actor,
) -> Vec<AgentId> {
    let mut touched = Vec::new();
    let mut queue = vec![parent.clone()];
    while let Some(p) = queue.pop() {
        for child in unsettled_children(core, &p.task_id).await {
            let Some(child_task_id) = child.child_task_id else {
                continue;
            };
            touched.push(child.agent_id);
            if core.runtime.cancel(&child_task_id).await {
                // Its loop ends `Cancelled` and cascades to its own children.
                continue;
            }
            let task = {
                let mut store = core.store.lock().await;
                let Ok(Some(task)) = store.task(&child_task_id) else {
                    continue;
                };
                if !task.state.is_terminal()
                    && let Err(e) =
                        crate::runtime::cancel_without_loop(&mut store, core, &task, actor)
                {
                    eprintln!("modbit-core: cancelling child task {child_task_id}: {e}");
                }
                task
            };
            crate::sandboxes::release_if_ended(core, child_task_id, actor).await;
            // The node: from where it stands now to CANCELLED.
            {
                let mut store = core.store.lock().await;
                let current = store
                    .agent_nodes(&p.task_id)
                    .unwrap_or_default()
                    .into_iter()
                    .find(|n| n.agent_id == child.agent_id)
                    .map(|n| n.status);
                if let Some(from) = current
                    .and_then(|s| serde_json::from_value(serde_json::Value::String(s)).ok())
                    .filter(|f: &AgentStatus| f.can_transition(AgentStatus::Cancelled))
                {
                    let _ = append(
                        &mut store,
                        core,
                        Lineage::task(core.tenant_id, p.session_id, p.task_id),
                        AggregateType::Task,
                        *p.task_id.as_bytes(),
                        vec![typed(
                            "AgentNodeTransitioned",
                            &TaskEvent::AgentNodeTransitioned {
                                agent_id: child.agent_id,
                                from,
                                to: AgentStatus::Cancelled,
                                run_id: None,
                                reason: reason.to_owned(),
                            },
                            actor.clone(),
                        )],
                    );
                }
            }
            queue.push(task);
        }
    }
    touched
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
        AgentStatus::Parked => "PARKED",
        // A child that ran out of its slice ends with the typed stop and its
        // partial evidence; its node stays WAITING and resumable (REQ-PX-116).
        _ if end_reason.starts_with("budget exhausted") => "BUDGET_EXHAUSTED",
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
    // REQ-EV-0050: a new attempt links the envelope it follows, so the
    // lineage of results is on the log with the lineage of agents.
    let attempt = store
        .runs_for_task(&child.task_id)
        .map(|r| r.len() as u32)
        .unwrap_or(1)
        .max(1);
    if let Ok(events) = store.read_session(&child.session_id, 0, usize::MAX)
        && let Some(prior) = events.iter().rev().find_map(|e| {
            if e.envelope.task_id != Some(parent_task_id)
                || e.envelope.event_type != "SubagentResultRecorded"
            {
                return None;
            }
            let p = store.payload(&e.envelope).ok()?;
            (p["agent_id"].as_str() == Some(&agent_id.to_string()))
                .then(|| p["result_ref"].as_str().unwrap_or_default().to_owned())
        })
        && !prior.is_empty()
    {
        evidence_refs.push(format!("prior_result:{prior}"));
    }
    evidence_refs.push(format!("attempt:{attempt}"));
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
            AgentStatus::Completed
            | AgentStatus::Failed
            | AgentStatus::Cancelled
            | AgentStatus::Parked => end,
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

/// M6.7 (docs/25 "Subagent continuation"): a child whose run a restart
/// left suspended resumes on its own log — the same task, node, capsule and
/// offsets, the capsule's budgets, a fresh run ticket — and its node on the
/// parent's graph moves back to `RUNNING` / `BACKGROUND`. A child that is
/// not suspended is left alone. Errors carry the start refusal.
pub(crate) async fn resume_child(
    core: &Arc<Core>,
    parent: &Task,
    node: &modbit_event_store::projections::AgentNodeRow,
    child_task_id: &TaskId,
    capsule_ref: &str,
    lease_generation: u64,
    actor: &Actor,
) -> std::result::Result<Option<RunId>, (String, String)> {
    let (child, capsule) = {
        let store = core.store.lock().await;
        let child = store.task(child_task_id).ok().flatten();
        let capsule = store
            .objects()
            .get(capsule_ref)
            .ok()
            .and_then(|b| serde_json::from_slice::<AgentExecutionCapsule>(&b).ok());
        (child, capsule)
    };
    let Some(child) = child else {
        return Ok(None);
    };
    // REQ-PX-116: a child a Core died on between its admission (the append
    // that reserved its budget) and its start is admitted and never ran: its
    // task is still Queued with no run. It is started now — both the
    // reservation and the child — rather than left holding its reservation
    // for ever.
    let never_started = child.state == TaskState::Queued
        && node.status == "ADMITTED"
        && core
            .store
            .lock()
            .await
            .runs_for_task(&child.task_id)
            .unwrap_or_default()
            .is_empty();
    if (!matches!(child.state, TaskState::Waiting(_)) && !never_started)
        || core.runtime.is_running(&child.task_id).await
    {
        return Ok(None);
    }
    let suspended = {
        let store = core.store.lock().await;
        store
            .runs_for_task(&child.task_id)
            .unwrap_or_default()
            .iter()
            .any(|r| r.state == modbit_domain::run::RunState::Suspended)
    };
    if !suspended && !never_started {
        return Ok(None);
    }
    let mode = capsule.as_ref().map_or(SpawnMode::Background, |c| c.mode);
    // The reservation is made afresh: what the child did not use went back
    // to its parent when it stopped (REQ-PX-116).
    let budgets = regrant(core, parent, node.agent_id, capsule.as_ref()).await;
    let cfg = StartConfig {
        endpoint: node.endpoint.clone(),
        model: node.model.clone(),
        budgets,
        pinned: false,
        plan_id: String::new(),
        slot_id: String::new(),
        skills: vec![],
        lease_generation,
        ticket_id: String::new(),
    };
    let child_actor = Actor::Agent(format!("subagent:{}", node.agent_id));
    let (run_id, resumed) = core
        .runtime
        .start_boxed(core, child, cfg, lease_generation, child_actor)
        .await?;
    let from: AgentStatus = serde_json::from_value(serde_json::Value::String(node.status.clone()))
        .unwrap_or(AgentStatus::Waiting);
    let to = if mode == SpawnMode::Background {
        AgentStatus::Background
    } else {
        AgentStatus::Running
    };
    if from != to && from.can_transition(to) {
        let mut store = core.store.lock().await;
        let _ = append(
            &mut store,
            core,
            Lineage::task(core.tenant_id, parent.session_id, parent.task_id)
                .fenced(lease_generation),
            AggregateType::Task,
            *parent.task_id.as_bytes(),
            vec![typed(
                "AgentNodeTransitioned",
                &TaskEvent::AgentNodeTransitioned {
                    agent_id: node.agent_id,
                    from,
                    to,
                    run_id: Some(run_id),
                    reason: if resumed {
                        "child run resumed after a restart (M6.7)".into()
                    } else {
                        "child run started on reattach".into()
                    },
                },
                actor.clone(),
            )],
        );
    }
    Ok(Some(run_id))
}

/// M6.7: every child of `parent` a restart left suspended, resumed (each
/// on a fresh ticket; one that finds no capacity stays `WAITING` and is
/// reported by `agent.wait`).
pub(crate) async fn resume_suspended_children(
    core: &Arc<Core>,
    parent: &Task,
    lease_generation: u64,
    actor: &Actor,
) {
    let nodes = {
        let store = core.store.lock().await;
        store.agent_nodes(&parent.task_id).unwrap_or_default()
    };
    for n in nodes
        .into_iter()
        .filter(|n| n.kind == "SUBAGENT" && matches!(n.status.as_str(), "WAITING" | "ADMITTED"))
    {
        let admitted = {
            let store = core.store.lock().await;
            latest_admission(&store, parent, n.agent_id)
        };
        if let Some(a) = admitted {
            let _ = resume_child(core, parent, &n, &a.0, &a.1, lease_generation, actor).await;
        }
    }
}

/// REQ-EV-0050 / 0179 / 0009: a typed follow-up from the parent. A live
/// child is steered (`TaskInputQueued` on its log, STEER interrupts its
/// stream, FOLLOW_UP waits for the turn); a parked or suspended child gets
/// the input and resumes; a child that ended — COMPLETED (its task with
/// the reviewer), FAILED or CANCELLED — continues as a new attempt on the
/// same task, node and lineage: the node returns to `ADMITTED` and a fresh
/// run starts with the follow-up as its first input, its result envelope
/// linking the prior one. Returns what happened, or a refusal.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn follow_up_child(
    core: &Arc<Core>,
    parent: &Task,
    node: &modbit_event_store::projections::AgentNodeRow,
    child_task_id: &TaskId,
    capsule_ref: &str,
    lease_generation: u64,
    message: &str,
    live_mode: modbit_domain::task::InputMode,
    actor: &Actor,
) -> std::result::Result<String, (String, String)> {
    use modbit_domain::task::InputMode;
    let child = {
        let store = core.store.lock().await;
        store.task(child_task_id).ok().flatten()
    };
    let Some(child) = child else {
        return Err(("UNKNOWN_TASK".into(), child_task_id.to_string()));
    };
    let input = |mode: InputMode, note: &str| {
        typed(
            "TaskInputQueued",
            &TaskEvent::TaskInputQueued {
                provenance: String::new(),
                untrusted: false,
                input_id: format!(
                    "parent-{}-{}",
                    node.agent_id,
                    modbit_domain::Timestamp::now().0
                ),
                mode,
                text: format!("From your parent ({note}): {message}"),
            },
            actor.clone(),
        )
    };
    let child_lt = Lineage::task(core.tenant_id, child.session_id, child.task_id);
    match node.status.as_str() {
        "RUNNING" | "BACKGROUND" | "ADMITTED" => {
            let mut store = core.store.lock().await;
            append(
                &mut store,
                core,
                child_lt,
                AggregateType::Task,
                *child.task_id.as_bytes(),
                vec![input(
                    live_mode,
                    if live_mode == InputMode::Steer {
                        "steer"
                    } else {
                        "follow-up"
                    },
                )],
            )
            .map_err(|e| ("STORE".to_owned(), e))?;
            Ok(format!(
                "status: SUCCESS\ndelivery: {}\nnote: the child is live; a STEER interrupts its current model call, a FOLLOW_UP lands after its turn",
                if live_mode == InputMode::Steer {
                    "STEERED"
                } else {
                    "QUEUED"
                }
            ))
        }
        "PARKED" | "WAITING" => {
            {
                let mut store = core.store.lock().await;
                append(
                    &mut store,
                    core,
                    child_lt,
                    AggregateType::Task,
                    *child.task_id.as_bytes(),
                    vec![input(InputMode::FollowUp, "follow-up")],
                )
                .map_err(|e| ("STORE".to_owned(), e))?;
            }
            match resume_child(core, parent, node, child_task_id, capsule_ref, lease_generation, actor)
                .await?
            {
                Some(run_id) => Ok(format!(
                    "status: SUCCESS\ndelivery: RESUMED\nrun_id: {run_id}\nnote: the child resumed with its whole state and your follow-up as its next input"
                )),
                None => Ok("status: SUCCESS\ndelivery: QUEUED\nnote: the child is not resumable right now; the input waits on its log".into()),
            }
        }
        "COMPLETED" | "FAILED" | "CANCELLED" => {
            let from: AgentStatus =
                serde_json::from_value(serde_json::Value::String(node.status.clone()))
                    .unwrap_or(AgentStatus::Completed);
            {
                let mut store = core.store.lock().await;
                let mut events = Vec::new();
                if child.state == TaskState::ReadyForReview {
                    events.push(typed(
                        "TaskReturnedToWork",
                        &TaskEvent::TaskReturnedToWork,
                        actor.clone(),
                    ));
                }
                events.push(typed(
                    "TaskWaiting",
                    &TaskEvent::TaskWaiting {
                        reason: modbit_domain::task::WaitReason::UserInput,
                    },
                    actor.clone(),
                ));
                events.push(input(InputMode::FollowUp, "follow-up, a new attempt"));
                append(
                    &mut store,
                    core,
                    child_lt,
                    AggregateType::Task,
                    *child.task_id.as_bytes(),
                    events,
                )
                .map_err(|e| ("STORE".to_owned(), e))?;
                // The node comes back to ADMITTED: the same identity and
                // lineage, a new attempt (REQ-EV-0050).
                if from.can_transition(AgentStatus::Admitted) {
                    append(
                        &mut store,
                        core,
                        Lineage::task(core.tenant_id, parent.session_id, parent.task_id)
                            .fenced(lease_generation),
                        AggregateType::Task,
                        *parent.task_id.as_bytes(),
                        vec![typed(
                            "AgentNodeTransitioned",
                            &TaskEvent::AgentNodeTransitioned {
                                agent_id: node.agent_id,
                                from,
                                to: AgentStatus::Admitted,
                                run_id: None,
                                reason: "follow-up from the parent: a new attempt on the same lineage (REQ-EV-0050)".into(),
                            },
                            actor.clone(),
                        )],
                    )
                    .map_err(|e| ("STORE".to_owned(), e))?;
                }
            }
            let child = {
                let store = core.store.lock().await;
                store.task(child_task_id).ok().flatten()
            }
            .ok_or_else(|| ("UNKNOWN_TASK".to_owned(), child_task_id.to_string()))?;
            let capsule = {
                let store = core.store.lock().await;
                store
                    .objects()
                    .get(capsule_ref)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<AgentExecutionCapsule>(&b).ok())
            };
            let mode = capsule.as_ref().map_or(SpawnMode::Background, |c| c.mode);
            let budgets = regrant(core, parent, node.agent_id, capsule.as_ref()).await;
            let cfg = StartConfig {
                endpoint: node.endpoint.clone(),
                model: node.model.clone(),
                budgets,
                pinned: false,
                plan_id: String::new(),
                slot_id: String::new(),
                skills: vec![],
                lease_generation,
                ticket_id: String::new(),
            };
            let child_actor = Actor::Agent(format!("subagent:{}", node.agent_id));
            let (run_id, _) = core
                .runtime
                .start_boxed(core, child, cfg, lease_generation, child_actor)
                .await?;
            let to = if mode == SpawnMode::Background {
                AgentStatus::Background
            } else {
                AgentStatus::Running
            };
            let mut store = core.store.lock().await;
            let _ = append(
                &mut store,
                core,
                Lineage::task(core.tenant_id, parent.session_id, parent.task_id)
                    .fenced(lease_generation),
                AggregateType::Task,
                *parent.task_id.as_bytes(),
                vec![typed(
                    "AgentNodeTransitioned",
                    &TaskEvent::AgentNodeTransitioned {
                        agent_id: node.agent_id,
                        from: AgentStatus::Admitted,
                        to,
                        run_id: Some(run_id),
                        reason: "new attempt started with the parent's follow-up".into(),
                    },
                    actor.clone(),
                )],
            );
            Ok(format!(
                "status: SUCCESS\ndelivery: NEW_ATTEMPT\nrun_id: {run_id}\nnote: the child continues on its own task and lineage as a new attempt; its next envelope links the prior one; collect with agent.wait"
            ))
        }
        other => Err((
            "NOT_STEERABLE".into(),
            format!("the child is {other}; steer a live, parked, suspended or ended child"),
        )),
    }
}

/// Record an admission refusal on the parent's log (the early stages,
/// before anything is taken).
async fn record_and_return(
    core: &Arc<Core>,
    parent: &Task,
    lt: Lineage,
    idempotency_key: &str,
    actor: &Actor,
    r: SpawnRefused,
) {
    let ev = typed(
        "SubagentAdmissionRefused",
        &TaskEvent::SubagentAdmissionRefused {
            idempotency_key: idempotency_key.to_owned(),
            code: r.code.clone(),
            detail: r.detail.clone(),
            stage: r.stage.to_owned(),
            rolled_back: r.rolled_back.clone(),
        },
        actor.clone(),
    );
    let mut store = core.store.lock().await;
    let _ = append(
        &mut store,
        core,
        lt,
        AggregateType::Task,
        *parent.task_id.as_bytes(),
        vec![ev],
    );
}
