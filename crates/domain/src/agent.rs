//! Agent nodes and work nodes (docs/13 "Agent node", docs/14 "One runtime,
//! three explicit graphs"; M6.1): the AgentGraph — which logical agents own
//! which work — and the WorkGraph — subtasks with dependencies, owners,
//! status, evidence and blockers — as durable projections of the task's
//! log. An agent node is a logical reasoning actor bound to a task, a
//! capsule and a model binding; it is not a process identity, and its
//! identity outlives the model it happens to run on (REQ-EV-0256). Nothing
//! here is a scheduler: admission (M6.3) decides what may run.

use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, RunId, TaskId};

/// What kind of agent a node is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentKind {
    /// The one agent responsible for the task's outcome (REQ-EV-0255).
    Primary,
    /// A delegated builder or investigator admitted by the primary.
    Subagent,
    /// A bounded read-only specialist sub-run (the Fast Context
    /// specialist, REQ-EV-0174).
    Specialist,
}

/// Where a node is in its life (docs/13 "Subagent" state machine, with the
/// scheduling modes of REQ-EV-0008/0049 as states of the same identity).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentStatus {
    /// Proposed by a parent; nothing reserved.
    Proposed,
    /// Admission in progress.
    AdmissionPending,
    /// Admitted: capacity, capability and isolation held; not yet running.
    Admitted,
    /// Running in the foreground of the user's attention.
    Running,
    /// Running detached; results return to the parent when it needs them.
    Background,
    /// Parked by the parent or the user: durable, resumable, not running.
    Parked,
    /// Waiting on input, approval, capacity or a provider.
    Waiting,
    /// Finished with a result envelope.
    Completed,
    /// Finished without one.
    Failed,
    /// Cancelled.
    Cancelled,
}

impl AgentStatus {
    /// Whether the node can transition out of this status.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// Whether `to` is a legal next status (docs/13 "Subagent"; a terminal
    /// child may be resumed as a new attempt by policy, REQ-EV-0050, which
    /// is a transition back to `Admitted`).
    #[must_use]
    pub const fn can_transition(self, to: Self) -> bool {
        use AgentStatus::*;
        matches!(
            (self, to),
            (Proposed, AdmissionPending)
                | (Proposed, Cancelled)
                | (AdmissionPending, Admitted)
                | (AdmissionPending, Failed)
                | (AdmissionPending, Cancelled)
                | (Admitted, Running)
                | (Admitted, Background)
                | (Admitted, Cancelled)
                | (Running, Background)
                | (Background, Running)
                | (Running, Parked)
                | (Background, Parked)
                | (Parked, Running)
                | (Parked, Background)
                | (Parked, Cancelled)
                | (Running, Waiting)
                | (Background, Waiting)
                | (Waiting, Running)
                | (Waiting, Background)
                | (Waiting, Parked)
                | (Waiting, Cancelled)
                | (Running, Completed)
                | (Running, Failed)
                | (Running, Cancelled)
                | (Background, Completed)
                | (Background, Failed)
                | (Background, Cancelled)
                | (Completed, Admitted)
                | (Failed, Admitted)
                | (Cancelled, Admitted)
        )
    }
}

/// The model binding a node runs on. It may change under policy (a
/// continuation on a stronger solver, a provider fallback) while the node's
/// identity, lineage and run stay (REQ-EV-0256).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentBinding {
    /// Gateway endpoint name.
    pub endpoint: String,
    /// Model id.
    pub model: String,
}

/// A logical agent (docs/13 "Agent node"), as the AgentGraph holds it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentNode {
    /// Identity, stable for the life of the task.
    pub agent_id: AgentId,
    /// The task it works.
    pub task_id: TaskId,
    /// The agent that admitted it; `None` for the primary.
    pub parent_agent_id: Option<AgentId>,
    /// The primary at the root of its lineage (itself for the primary).
    pub root_agent_id: AgentId,
    /// Delegation depth: the primary is 0 (REQ-EV-0051).
    pub depth: u32,
    /// Kind.
    pub kind: AgentKind,
    /// Status.
    pub status: AgentStatus,
    /// The run it executes in, once it has one.
    pub run_id: Option<RunId>,
    /// Object hash of its execution capsule (REQ-EV-0048), once admitted.
    pub capsule_ref: Option<String>,
    /// The binding it runs on.
    pub binding: AgentBinding,
    /// The idempotency key its spawn carried (REQ-EV-0007); the primary's
    /// is its task id.
    pub idempotency_key: String,
    /// The work nodes it owns.
    pub owns: Vec<WorkNodeId>,
    /// The task a subagent executes as (its own task, forked from the
    /// parent's; M6.3). `None` for the primary and for specialists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_task_id: Option<TaskId>,
}

/// What a parent proposes for a child (docs/14 "Decomposition"): Core
/// validates it rather than trusting model-generated parallelism.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtaskSpec {
    /// The child's objective, its whole goal.
    pub objective: String,
    /// Artifacts it is expected to produce (paths).
    #[serde(default)]
    pub expected_artifacts: Vec<String>,
    /// Work nodes it depends on.
    #[serde(default)]
    pub depends_on: Vec<WorkNodeId>,
    /// Paths it may read (empty = the whole worktree).
    #[serde(default)]
    pub read_scope: Vec<String>,
    /// Paths it may write (prefixes or globs); its lease is narrowed to them.
    #[serde(default)]
    pub write_scope: Vec<String>,
    /// Tools it needs by name; its projection is narrowed to them when set.
    #[serde(default)]
    pub required_tools: Vec<String>,
    /// Execution profile; `None` = the parent's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_profile: Option<String>,
    /// What proves it done, in words.
    #[serde(default)]
    pub verification: String,
    /// Turns it may spend.
    #[serde(default)]
    pub max_turns: u32,
    /// Tool calls it may make.
    #[serde(default)]
    pub max_tool_calls: u32,
    /// The work node it will own; `None` = one is created from the objective.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_node: Option<WorkNodeId>,
}

/// How a child is scheduled with respect to the parent's attention
/// (REQ-EV-0008).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpawnMode {
    /// The parent waits for the result in its turn.
    Foreground,
    /// The child runs detached; the parent collects the result later.
    #[default]
    Background,
}

/// The explicit envelope a child runs inside (REQ-EV-0048, docs/25
/// "Subagent continuation"): its context, tools, model policy, budgets and
/// capability ceiling. A child gets this and nothing of the parent's
/// transcript (REQ-EV-0078).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentExecutionCapsule {
    /// The child.
    pub agent_id: AgentId,
    /// Its parent.
    pub parent_agent_id: AgentId,
    /// The parent's task.
    pub parent_task_id: TaskId,
    /// The child's task.
    pub child_task_id: TaskId,
    /// Depth (REQ-EV-0051).
    pub depth: u32,
    /// What it is to do.
    pub spec: SubtaskSpec,
    /// Tools it may see, by name; empty = the profile's projection.
    pub tools: Vec<String>,
    /// The binding it runs on.
    pub binding: AgentBinding,
    /// Turns it may spend.
    pub max_turns: u32,
    /// Tool calls it may make.
    pub max_tool_calls: u32,
    /// The effect ceiling of its lease (`READ_ONLY` | `WRITE` | …), never
    /// above the parent's (REQ-EV-0046).
    pub effect_ceiling: String,
    /// The worktree root it works in.
    pub worktree: String,
    /// Its branch.
    pub branch: String,
    /// Modalities it may receive.
    pub allowed_modalities: Vec<String>,
    /// Private context refs it starts with (objects, never the parent's
    /// transcript).
    pub private_context_refs: Vec<String>,
    /// `FOREGROUND` | `BACKGROUND`.
    pub mode: SpawnMode,
}

/// The typed result a child hands back (docs/14 "Agent-to-agent
/// communication"): untrusted context until the parent selects it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentResult {
    /// The child.
    pub agent_id: AgentId,
    /// Its task.
    pub child_task_id: TaskId,
    /// `COMPLETED` | `FAILED` | `CANCELLED` | `WAITING`.
    pub status: String,
    /// Summary in the child's words.
    pub summary: String,
    /// Paths changed in its worktree.
    pub artifacts: Vec<String>,
    /// Evidence references.
    pub evidence_refs: Vec<String>,
    /// Unresolved risks.
    pub unresolved_risks: Vec<String>,
    /// Follow-ups the child proposes.
    pub proposed_follow_ups: Vec<String>,
    /// Its branch, for the parent's merge.
    pub branch: String,
    /// Its worktree root.
    pub worktree: String,
    /// The child's final candidate revision.
    pub candidate_revision: u64,
}

/// Whether two write scopes overlap: a path prefix or glob of one covers
/// a path of the other (docs/14 "Semantic conflict detection", the
/// explicit-overlap check of M6.3; M6.4 adds the symbol, hot-spot and
/// generated-file rules).
#[must_use]
pub fn write_scopes_overlap(a: &[String], b: &[String]) -> Vec<(String, String)> {
    fn norm(p: &str) -> String {
        p.trim()
            .trim_start_matches("./")
            .trim_end_matches('/')
            .to_owned()
    }
    fn covers(pattern: &str, path: &str) -> bool {
        let (pattern, path) = (norm(pattern), norm(path));
        if pattern.is_empty() || path.is_empty() {
            return pattern.is_empty() && path.is_empty();
        }
        if let Some(prefix) = pattern
            .strip_suffix("/**")
            .or_else(|| pattern.strip_suffix("/*"))
        {
            return path == prefix || path.starts_with(&format!("{prefix}/"));
        }
        if pattern == "**" || pattern == "*" {
            return true;
        }
        pattern == path
            || path.starts_with(&format!("{pattern}/"))
            || pattern.starts_with(&format!("{path}/"))
    }
    let mut out = Vec::new();
    for x in a {
        for y in b {
            if covers(x, y) || covers(y, x) {
                out.push((x.clone(), y.clone()));
            }
        }
    }
    out
}

/// Stable id of a work node within a task: the plan step's own id, as the
/// model named it, or one the runtime assigned.
pub type WorkNodeId = String;

/// Where a work node is (docs/14 "WorkGraph").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkStatus {
    /// Dependencies outstanding.
    Pending,
    /// Every dependency done; nobody working it yet.
    Ready,
    /// Being worked by its owner.
    Active,
    /// Blocked on something named in `blockers`.
    Blocked,
    /// Done, with evidence.
    Done,
    /// Given up.
    Failed,
    /// Dropped from the plan.
    Cancelled,
}

impl WorkStatus {
    /// Whether work on the node is over.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Cancelled)
    }
}

/// One node of the WorkGraph (docs/14: tasks/subtasks, dependencies,
/// artifacts and verification gates; REQ-EV-0052 / 0120: outside the
/// transcript, with dependencies, owner, status, evidence and blockers).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkNode {
    /// Id within the task.
    pub id: WorkNodeId,
    /// Task.
    pub task_id: TaskId,
    /// What it is.
    pub title: String,
    /// Nodes that must be done first.
    #[serde(default)]
    pub depends_on: Vec<WorkNodeId>,
    /// The agent working it, when one is.
    #[serde(default)]
    pub owner: Option<AgentId>,
    /// Status.
    pub status: WorkStatus,
    /// Artifacts it is expected to produce (paths).
    #[serde(default)]
    pub expected_artifacts: Vec<String>,
    /// What proves it done, in the agent's words.
    #[serde(default)]
    pub verification: String,
    /// Evidence references (object hashes, verification run ids, event
    /// offsets) recorded when it was marked done.
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    /// What blocks it, in words.
    #[serde(default)]
    pub blockers: Vec<String>,
    /// Attempts made on it (a status returning to `Active` after `Failed`
    /// or `Blocked` counts one).
    #[serde(default)]
    pub attempts: u32,
    /// Plan version that last changed it.
    #[serde(default)]
    pub plan_version: u32,
}

/// A change to one work node, as `plan.update` carries it and the log
/// records it: every field absent leaves the node as it is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkNodeChange {
    /// Id.
    pub id: WorkNodeId,
    /// Title, when set or changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Dependencies, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<WorkNodeId>>,
    /// Status, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<WorkStatus>,
    /// Expected artifacts, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_artifacts: Option<Vec<String>>,
    /// Verification, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification: Option<String>,
    /// Evidence references to add.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_refs: Vec<String>,
    /// Blockers, when set (an empty list clears them).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blockers: Option<Vec<String>>,
}

/// The WorkGraph of one task: the nodes and the rules that keep them
/// consistent. A pure value; the Core applies changes and records them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkGraph {
    /// Nodes, in creation order.
    pub nodes: Vec<WorkNode>,
}

/// Why a change to the WorkGraph was refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkGraphError {
    /// A dependency names a node that does not exist.
    UnknownDependency {
        /// Node.
        id: WorkNodeId,
        /// The missing dependency.
        depends_on: WorkNodeId,
    },
    /// The dependencies would form a cycle.
    DependencyCycle {
        /// A node on the cycle.
        id: WorkNodeId,
    },
    /// A node was marked done while a dependency is not.
    DependencyNotDone {
        /// Node.
        id: WorkNodeId,
        /// The dependency.
        depends_on: WorkNodeId,
    },
    /// A node was marked done with no evidence reference.
    EvidenceRequired {
        /// Node.
        id: WorkNodeId,
    },
    /// A new node has no title.
    TitleRequired {
        /// Node.
        id: WorkNodeId,
    },
    /// An id is empty or too long.
    InvalidId {
        /// Node.
        id: WorkNodeId,
    },
}

impl WorkGraph {
    /// Apply one plan version's changes: new nodes are created, existing
    /// ones changed, dependencies validated whole (no unknown target, no
    /// cycle), `DONE` needs every dependency done and an evidence reference,
    /// and the `PENDING`/`READY` of untouched nodes follows their
    /// dependencies. Refused whole on the first error; the graph is then
    /// unchanged.
    ///
    /// # Errors
    /// See [`WorkGraphError`].
    pub fn apply(
        &mut self,
        task_id: TaskId,
        plan_version: u32,
        changes: &[WorkNodeChange],
    ) -> Result<Vec<WorkNode>, WorkGraphError> {
        let mut next = self.clone();
        let mut touched: Vec<WorkNodeId> = Vec::new();
        for c in changes {
            if c.id.is_empty() || c.id.len() > 64 {
                return Err(WorkGraphError::InvalidId { id: c.id.clone() });
            }
            let pos = next.nodes.iter().position(|n| n.id == c.id);
            let node = match pos {
                Some(i) => &mut next.nodes[i],
                None => {
                    let Some(title) = c.title.as_ref().filter(|t| !t.trim().is_empty()) else {
                        return Err(WorkGraphError::TitleRequired { id: c.id.clone() });
                    };
                    next.nodes.push(WorkNode {
                        id: c.id.clone(),
                        task_id,
                        title: title.trim().to_owned(),
                        depends_on: vec![],
                        owner: None,
                        status: WorkStatus::Pending,
                        expected_artifacts: vec![],
                        verification: String::new(),
                        evidence_refs: vec![],
                        blockers: vec![],
                        attempts: 0,
                        plan_version,
                    });
                    next.nodes.last_mut().expect("pushed")
                }
            };
            if let Some(t) = c.title.as_ref().filter(|t| !t.trim().is_empty()) {
                node.title = t.trim().to_owned();
            }
            if let Some(d) = &c.depends_on {
                let mut d = d.clone();
                d.retain(|x| x != &c.id);
                d.dedup();
                node.depends_on = d;
            }
            if let Some(a) = &c.expected_artifacts {
                node.expected_artifacts = a.clone();
            }
            if let Some(v) = &c.verification {
                node.verification = v.clone();
            }
            for r in &c.evidence_refs {
                if !node.evidence_refs.contains(r) {
                    node.evidence_refs.push(r.clone());
                }
            }
            if let Some(b) = &c.blockers {
                node.blockers = b.clone();
            }
            if let Some(s) = c.status {
                if s == WorkStatus::Active
                    && matches!(node.status, WorkStatus::Failed | WorkStatus::Blocked)
                {
                    node.attempts += 1;
                }
                if s == WorkStatus::Active && node.attempts == 0 {
                    node.attempts = 1;
                }
                node.status = s;
            } else if node.blockers.iter().any(|b| !b.trim().is_empty())
                && !node.status.is_terminal()
            {
                node.status = WorkStatus::Blocked;
            }
            node.plan_version = plan_version;
            touched.push(c.id.clone());
        }
        // Dependencies exist and form no cycle.
        for n in &next.nodes {
            for d in &n.depends_on {
                if !next.nodes.iter().any(|m| &m.id == d) {
                    return Err(WorkGraphError::UnknownDependency {
                        id: n.id.clone(),
                        depends_on: d.clone(),
                    });
                }
            }
        }
        if let Some(id) = next.first_cycle() {
            return Err(WorkGraphError::DependencyCycle { id });
        }
        // Done needs its dependencies done and evidence.
        for id in &touched {
            let n = next.nodes.iter().find(|n| &n.id == id).expect("touched");
            if n.status == WorkStatus::Done {
                if n.evidence_refs.is_empty() {
                    return Err(WorkGraphError::EvidenceRequired { id: id.clone() });
                }
                for d in &n.depends_on {
                    let dep = next.nodes.iter().find(|m| &m.id == d).expect("checked");
                    if dep.status != WorkStatus::Done {
                        return Err(WorkGraphError::DependencyNotDone {
                            id: id.clone(),
                            depends_on: d.clone(),
                        });
                    }
                }
            }
        }
        // Pending/Ready follow the dependencies.
        let done: Vec<WorkNodeId> = next
            .nodes
            .iter()
            .filter(|n| n.status == WorkStatus::Done)
            .map(|n| n.id.clone())
            .collect();
        for n in &mut next.nodes {
            if matches!(n.status, WorkStatus::Pending | WorkStatus::Ready) {
                n.status = if n.depends_on.iter().all(|d| done.contains(d)) {
                    WorkStatus::Ready
                } else {
                    WorkStatus::Pending
                };
            }
        }
        let changed: Vec<WorkNode> = touched
            .iter()
            .filter_map(|id| next.nodes.iter().find(|n| &n.id == id).cloned())
            .collect();
        *self = next;
        Ok(changed)
    }

    /// A node on a dependency cycle, if there is one.
    #[must_use]
    pub fn first_cycle(&self) -> Option<WorkNodeId> {
        fn visit(
            g: &WorkGraph,
            id: &WorkNodeId,
            stack: &mut Vec<WorkNodeId>,
            done: &mut Vec<WorkNodeId>,
        ) -> Option<WorkNodeId> {
            if done.contains(id) {
                return None;
            }
            if stack.contains(id) {
                return Some(id.clone());
            }
            stack.push(id.clone());
            if let Some(n) = g.nodes.iter().find(|n| &n.id == id) {
                for d in &n.depends_on {
                    if let Some(c) = visit(g, d, stack, done) {
                        return Some(c);
                    }
                }
            }
            stack.pop();
            done.push(id.clone());
            None
        }
        let mut done = Vec::new();
        for n in &self.nodes {
            let mut stack = Vec::new();
            if let Some(c) = visit(self, &n.id, &mut stack, &mut done) {
                return Some(c);
            }
        }
        None
    }

    /// Nodes whose dependencies are all done and that nobody has finished.
    #[must_use]
    pub fn ready(&self) -> Vec<&WorkNode> {
        self.nodes
            .iter()
            .filter(|n| n.status == WorkStatus::Ready)
            .collect()
    }

    /// One line per node for the model's harness state.
    #[must_use]
    pub fn summary(&self) -> Vec<String> {
        self.nodes
            .iter()
            .map(|n| {
                let mut s = format!("{} [{:?}] {}", n.id, n.status, n.title);
                if !n.depends_on.is_empty() {
                    s.push_str(&format!(" (after {})", n.depends_on.join(", ")));
                }
                if let Some(o) = &n.owner {
                    s.push_str(&format!(" owner {o}"));
                }
                if !n.blockers.is_empty() {
                    s.push_str(&format!(" blocked: {}", n.blockers.join("; ")));
                }
                if n.attempts > 1 {
                    s.push_str(&format!(" attempts {}", n.attempts));
                }
                s
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(id: &str, title: Option<&str>, deps: Option<&[&str]>) -> WorkNodeChange {
        WorkNodeChange {
            id: id.into(),
            title: title.map(str::to_owned),
            depends_on: deps.map(|d| d.iter().map(|x| (*x).to_owned()).collect()),
            ..Default::default()
        }
    }

    #[test]
    fn nodes_follow_their_dependencies_and_done_needs_evidence() {
        let task = TaskId::new();
        let mut g = WorkGraph::default();
        let changed = g
            .apply(
                task,
                1,
                &[
                    change("read", Some("read the file"), None),
                    change("fix", Some("fix the guard"), Some(&["read"])),
                    change("test", Some("run the tests"), Some(&["fix"])),
                ],
            )
            .unwrap();
        assert_eq!(changed.len(), 3);
        assert_eq!(g.nodes[0].status, WorkStatus::Ready);
        assert_eq!(g.nodes[1].status, WorkStatus::Pending);
        assert_eq!(g.ready().len(), 1);
        // Done without evidence is refused, whole.
        let before = g.clone();
        let err = g
            .apply(
                task,
                2,
                &[WorkNodeChange {
                    id: "read".into(),
                    status: Some(WorkStatus::Done),
                    ..Default::default()
                }],
            )
            .unwrap_err();
        assert!(matches!(err, WorkGraphError::EvidenceRequired { .. }));
        assert_eq!(g, before);
        // Done with evidence unlocks the dependent.
        g.apply(
            task,
            2,
            &[WorkNodeChange {
                id: "read".into(),
                status: Some(WorkStatus::Done),
                evidence_refs: vec!["offset:12".into()],
                ..Default::default()
            }],
        )
        .unwrap();
        assert_eq!(g.nodes[0].status, WorkStatus::Done);
        assert_eq!(g.nodes[1].status, WorkStatus::Ready);
        assert_eq!(g.nodes[2].status, WorkStatus::Pending);
        // Done ahead of a dependency is refused.
        let err = g
            .apply(
                task,
                3,
                &[WorkNodeChange {
                    id: "test".into(),
                    status: Some(WorkStatus::Done),
                    evidence_refs: vec!["x".into()],
                    ..Default::default()
                }],
            )
            .unwrap_err();
        assert!(matches!(err, WorkGraphError::DependencyNotDone { .. }));
        // Blockers block; a return to active counts an attempt.
        g.apply(
            task,
            3,
            &[WorkNodeChange {
                id: "fix".into(),
                blockers: Some(vec!["needs the schema".into()]),
                ..Default::default()
            }],
        )
        .unwrap();
        assert_eq!(g.nodes[1].status, WorkStatus::Blocked);
        g.apply(
            task,
            4,
            &[WorkNodeChange {
                id: "fix".into(),
                status: Some(WorkStatus::Active),
                blockers: Some(vec![]),
                ..Default::default()
            }],
        )
        .unwrap();
        assert_eq!(g.nodes[1].status, WorkStatus::Active);
        assert_eq!(g.nodes[1].attempts, 1);
        assert!(g.summary()[1].contains("fix [Active] fix the guard (after read)"));
    }

    #[test]
    fn unknown_dependencies_and_cycles_are_refused() {
        let task = TaskId::new();
        let mut g = WorkGraph::default();
        let err = g
            .apply(task, 1, &[change("a", Some("a"), Some(&["nope"]))])
            .unwrap_err();
        assert!(matches!(err, WorkGraphError::UnknownDependency { .. }));
        assert!(g.nodes.is_empty());
        let err = g
            .apply(
                task,
                1,
                &[
                    change("a", Some("a"), Some(&["b"])),
                    change("b", Some("b"), Some(&["a"])),
                ],
            )
            .unwrap_err();
        assert!(matches!(err, WorkGraphError::DependencyCycle { .. }));
        assert!(g.nodes.is_empty());
        let err = g.apply(task, 1, &[change("c", None, None)]).unwrap_err();
        assert!(matches!(err, WorkGraphError::TitleRequired { .. }));
    }

    #[test]
    fn write_scopes_overlap_on_prefix_and_glob_but_not_on_disjoint_paths() {
        let a = vec!["src/api/".to_owned(), "docs/*".to_owned()];
        let b = vec!["src/api/users.rs".to_owned(), "tests/".to_owned()];
        let hits = write_scopes_overlap(&a, &b);
        assert_eq!(
            hits,
            vec![("src/api/".to_owned(), "src/api/users.rs".to_owned())]
        );
        assert!(write_scopes_overlap(&["src/a".to_owned()], &["src/b".to_owned()]).is_empty());
        assert!(
            !write_scopes_overlap(&["src/**".to_owned()], &["src/b/c.rs".to_owned()]).is_empty()
        );
        assert!(
            !write_scopes_overlap(&["Cargo.lock".to_owned()], &["Cargo.lock".to_owned()])
                .is_empty()
        );
        assert!(
            write_scopes_overlap(&["src/api".to_owned()], &["src/apix.rs".to_owned()]).is_empty()
        );
    }

    #[test]
    fn agent_status_transitions_are_the_documented_ones() {
        use AgentStatus::*;
        assert!(Proposed.can_transition(AdmissionPending));
        assert!(AdmissionPending.can_transition(Admitted));
        assert!(Admitted.can_transition(Running));
        assert!(Running.can_transition(Background));
        assert!(Background.can_transition(Running));
        assert!(Running.can_transition(Parked));
        assert!(Parked.can_transition(Running));
        assert!(Running.can_transition(Completed));
        assert!(
            Completed.can_transition(Admitted),
            "a terminal child may be resumed as a new attempt"
        );
        assert!(
            !Proposed.can_transition(Running),
            "nothing runs without admission"
        );
        assert!(!Completed.can_transition(Running));
        assert!(Completed.is_terminal());
    }
}
