//! Native computer control in the Core (PX-069, PX-070, PX-075, PX-076;
//! docs/66, DR-PX-2026-10-03-008).
//!
//! The Computer Runtime (`modbit-computer`) owns sessions, grants, handles,
//! the unknown-outcome latch and the actuator connection; this module is its
//! host in the Core:
//!
//! * **Configuration.** An actuator is configured by `MODBIT_COMPUTER_ACTUATOR`
//!   (+ `MODBIT_COMPUTER_ACTUATOR_ARGS`, a JSON array) or by
//!   `<data dir>/computer/actuator.json`. With none configured, or one that
//!   cannot be attached, the `computer.*` tools are absent from the
//!   projection and any call answers `ACTUATOR_UNAVAILABLE` (fail closed).
//! * **Events.** The runtime queues typed events; the pump appends them to the
//!   task's log (the audit holds counts and kinds, never contents).
//! * **Recovery.** At boot the Core folds the computer events of the whole log:
//!   a session a dead Core left open is closed `CORE_RESTART` (the actuator
//!   stopped with the Core's pipe; nothing is left held), an unknown-outcome
//!   latch stays until a new session observes or the person resolves it, and a
//!   user stop stays for its run.
//! * **Retention.** A stored frame is removed from the object store once the
//!   retention window passes (`MODBIT_COMPUTER_RETENTION_SECS`, default a
//!   day); the observation's record stays, the pixels do not.
//! * **Commands.** `GetComputerRuntime`, `ResolveComputerLatch`, `ComputerStop`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use modbit_computer::actuator::{ActuatorConfig, BoxFuture};
use modbit_computer::policy::{self, ComputerPolicy};
use modbit_computer::runtime::Latch;
use modbit_computer::{
    Answer, CallCtx, CallInfo, ComputerPort, ComputerRuntime, Failure, PendingEvent, Refusal,
    RuntimeConfig,
};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::TaskEvent;
use modbit_domain::{SessionId, TaskId, TenantId};
use modbit_event_store::{AppendRequest, EventStore, NewEvent};
use modbit_policy::config::{Permission, ResolvedConfig};
use modbit_protocol::v1 as wire;
use prost::Message;
use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;

use crate::server::{Core, accept, id16, reject, require_lease};

/// How long a stored frame is kept unless the environment says otherwise.
const DEFAULT_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

/// Approvals for native control are short-lived: the person looks at one
/// exact intent now (CUC-C01).
pub(crate) const APPROVAL_TTL_MS: i64 = 5 * 60 * 1000;

/// The computer event types the recovery folds.
const FOLDED: &[&str] = &[
    "ComputerSessionStarted",
    "ComputerSessionClosed",
    "ComputerLatched",
    "ComputerLatchResolved",
    "ComputerUserAborted",
];

/// The host of the Computer Runtime in one Core.
pub(crate) struct ComputerHost {
    pub runtime: ComputerRuntime,
    data_dir: PathBuf,
    retention: Duration,
    /// Serialises writes of the artifact index.
    index: Mutex<()>,
    /// What a computer-use child's stall rules have seen (PX-075).
    stall: Mutex<HashMap<TaskId, Stall>>,
    /// The tasks that are `computer-use` children.
    children: Mutex<std::collections::HashSet<TaskId>>,
}

/// Four failed attempts in a row, or three turns with no new observation or
/// action, stop a computer-use child (CUC-D06).
const MAX_FAILED_ATTEMPTS: u32 = 4;
const MAX_IDLE_TURNS: u32 = 3;

#[derive(Default)]
struct Stall {
    /// A computer call has been attempted: idle turns count from here.
    started: bool,
    fails: u32,
    idle: u32,
    progress: u64,
    seen: u64,
    last_code: String,
    last_message: String,
    last_title: String,
    last_tree: String,
    blockers: Vec<Value>,
    report: Option<Value>,
}

/// One stored frame the retention sweep knows of.
#[derive(serde::Serialize, serde::Deserialize)]
struct ArtifactRow {
    object: String,
    at_ms: i64,
}

fn actuator_config(data_dir: &Path) -> Option<ActuatorConfig> {
    let log_path = Some(data_dir.join("computer").join("actuator.log"));
    if let Ok(exe) = std::env::var("MODBIT_COMPUTER_ACTUATOR")
        && !exe.trim().is_empty()
    {
        let args: Vec<String> = std::env::var("MODBIT_COMPUTER_ACTUATOR_ARGS")
            .ok()
            .and_then(|a| serde_json::from_str(&a).ok())
            .unwrap_or_default();
        return Some(ActuatorConfig {
            executable: PathBuf::from(exe.trim()),
            args,
            env: vec![],
            log_path,
        });
    }
    let text = std::fs::read_to_string(data_dir.join("computer").join("actuator.json")).ok()?;
    #[derive(serde::Deserialize)]
    struct File {
        executable: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
    }
    let f: File = serde_json::from_str(&text).ok()?;
    Some(ActuatorConfig {
        executable: PathBuf::from(f.executable),
        args: f.args,
        env: f.env.into_iter().collect(),
        log_path,
    })
}

/// What administrative policy decided for computer control, from the
/// configuration a call is decided under (CUC-C04).
pub(crate) fn policy_of(config: &ResolvedConfig) -> ComputerPolicy {
    let denied = |c: &str| {
        config
            .permissions
            .get(c)
            .is_some_and(|p| p.value == Permission::Deny)
    };
    ComputerPolicy {
        observe_denied: denied("computer.observe"),
        act_denied: denied("computer.act"),
        screen_denied: denied("computer.screen"),
        apps_allow: config.computer_apps_allow.as_ref().map(|r| r.value.clone()),
    }
}

impl ComputerHost {
    pub(crate) fn new(data_dir: &Path) -> Self {
        let runtime = ComputerRuntime::new(RuntimeConfig::from_env());
        if let Some(c) = actuator_config(data_dir) {
            runtime.configure(c);
        }
        let retention = std::env::var("MODBIT_COMPUTER_RETENTION_SECS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|s| *s > 0)
            .map_or(DEFAULT_RETENTION, Duration::from_secs);
        Self {
            runtime,
            data_dir: data_dir.to_path_buf(),
            retention,
            index: Mutex::new(()),
            stall: Mutex::new(HashMap::new()),
            children: Mutex::new(std::collections::HashSet::new()),
        }
    }

    /// `task` is a `computer-use` child: its inputs stop at a blocker.
    pub(crate) fn mark_child(&self, task: TaskId) {
        self.children
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(task);
    }

    pub(crate) fn is_child(&self, task: TaskId) -> bool {
        self.children
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&task)
    }

    /// A `computer.*` call of the task ended: remember what the stall rules
    /// and the child's report need.
    pub(crate) fn note_tool(
        &self,
        task: TaskId,
        ok: bool,
        out: &Value,
        code: Option<&str>,
        message: Option<&str>,
    ) {
        let mut m = self.stall.lock().unwrap_or_else(|e| e.into_inner());
        let st = m.entry(task).or_default();
        st.started = true;
        if ok {
            st.fails = 0;
            st.progress += 1;
            if let Some(tree) = out["tree"].as_str() {
                st.last_tree = tree.chars().take(800).collect();
                st.last_title = out["window"]["title"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                st.blockers = out["blockers"].as_array().cloned().unwrap_or_default();
            }
        } else {
            st.fails += 1;
            st.last_code = code.unwrap_or("FAILED").to_owned();
            st.last_message = message.unwrap_or_default().chars().take(300).collect();
        }
    }

    /// A model turn of the task ended. When a stall rule trips, the report
    /// the child stops with: what it observed, what blocked it and the best
    /// next step.
    pub(crate) fn turn_boundary(&self, task: TaskId) -> Option<Value> {
        let mut m = self.stall.lock().unwrap_or_else(|e| e.into_inner());
        let st = m.get_mut(&task)?;
        if !st.started {
            return None;
        }
        if st.progress == st.seen {
            st.idle += 1;
        } else {
            st.idle = 0;
            st.seen = st.progress;
        }
        let why = if st.fails >= MAX_FAILED_ATTEMPTS {
            "FOUR_FAILED_ATTEMPTS"
        } else if st.idle >= MAX_IDLE_TURNS {
            "NO_NEW_OBSERVATION_OR_ACTION"
        } else {
            return None;
        };
        let blocked_by = if !st.blockers.is_empty() {
            format!(
                "the application shows something that is the person's to do: {}",
                st.blockers
                    .iter()
                    .filter_map(|b| b["kind"].as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else if st.fails > 0 {
            format!("{}: {}", st.last_code, st.last_message)
        } else {
            "no computer call made progress".to_owned()
        };
        let next = modbit_computer::Code::parse(&st.last_code)
            .map(|c| c.recovery().to_owned())
            .unwrap_or_else(|| {
                "the person looks at the application and tells the parent what to do; the child can be resumed with a new control session".to_owned()
            });
        let report = serde_json::json!({
            "stopped": true,
            "reason": why,
            "failed_attempts": st.fails,
            "idle_turns": st.idle,
            "observed": {"window": st.last_title, "tree_head": st.last_tree},
            "blocked_by": blocked_by,
            "blockers": st.blockers,
            "best_next_step": next,
        });
        st.report = Some(report.clone());
        Some(report)
    }

    /// The computer part of a child's result envelope (PX-075): the final
    /// state it observed, the actions it took by kind, the evidence it kept,
    /// and - when it stopped - the blocker report.
    pub(crate) fn envelope_report(
        &self,
        store: &EventStore,
        child: TaskId,
        summary: &str,
    ) -> Value {
        let mut actions: std::collections::BTreeMap<String, u32> = Default::default();
        let (mut screenshots, mut observations) = (0u32, 0u32);
        let mut evidence: Vec<String> = Vec::new();
        let mut applications: Vec<String> = Vec::new();
        let mut latched = false;
        if let Ok(events) = store.read_aggregate(child.as_bytes(), 0, usize::MAX) {
            for e in &events {
                if !e.envelope.event_type.starts_with("Computer") {
                    continue;
                }
                let Ok(p) = store.payload(&e.envelope) else {
                    continue;
                };
                match e.envelope.event_type.as_str() {
                    "ComputerSessionStarted" => {
                        if let Some(b) = p["bundle_id"].as_str()
                            && !applications.iter().any(|a| a == b)
                        {
                            applications.push(b.to_owned());
                        }
                    }
                    "ComputerActionPerformed" => {
                        if p["outcome"] != "FAILED" {
                            *actions
                                .entry(p["kind"].as_str().unwrap_or("?").to_owned())
                                .or_insert(0) += 1;
                        }
                    }
                    "ComputerObserved" => {
                        observations += 1;
                        if p["kind"] == "screenshot" {
                            screenshots += 1;
                        }
                        if let Some(r) = p["artifact_ref"].as_str().filter(|r| !r.is_empty()) {
                            evidence.push(format!("computer_frame:{r}"));
                        }
                    }
                    "ComputerLatched" => latched = true,
                    "ComputerLatchResolved" => latched = false,
                    _ => {}
                }
            }
        }
        let st = self.stall.lock().unwrap_or_else(|e| e.into_inner());
        let s = st.get(&child);
        serde_json::json!({
            "applications": applications,
            "final_state": {
                "summary": summary,
                "window": s.map(|s| s.last_title.clone()).unwrap_or_default(),
                "tree_head": s.map(|s| s.last_tree.clone()).unwrap_or_default(),
            },
            "actions_by_kind": actions,
            "screenshots": screenshots,
            "observations": observations,
            "evidence_refs": evidence,
            "latched": latched,
            "stopped": s.and_then(|s| s.report.clone()),
        })
    }

    /// Whether the tools are offered to a model: an actuator is attached (or
    /// restarting after having been).
    pub(crate) fn offered(&self) -> bool {
        self.runtime.offered()
    }

    fn index_path(&self) -> PathBuf {
        self.data_dir.join("computer").join("artifacts.jsonl")
    }

    fn note_artifact(&self, object: &str, at_ms: i64) {
        let _g = self.index.lock().unwrap_or_else(|e| e.into_inner());
        let path = self.index_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            use std::io::Write;
            let _ = writeln!(
                f,
                "{}",
                serde_json::to_string(&ArtifactRow {
                    object: object.to_owned(),
                    at_ms,
                })
                .unwrap_or_default()
            );
        }
    }

    /// Remove the frames older than the retention window; returns what went.
    fn expire(&self, store: &EventStore, now_ms: i64) -> Vec<(String, i64)> {
        let _g = self.index.lock().unwrap_or_else(|e| e.into_inner());
        let path = self.index_path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return vec![];
        };
        let window = i64::try_from(self.retention.as_millis()).unwrap_or(i64::MAX);
        let (mut keep, mut gone) = (Vec::new(), Vec::new());
        for line in text.lines() {
            let Ok(row) = serde_json::from_str::<ArtifactRow>(line) else {
                continue;
            };
            if now_ms - row.at_ms >= window {
                let _ = store.objects().remove(&row.object);
                gone.push((row.object, row.at_ms));
            } else {
                keep.push(line.to_owned());
            }
        }
        if !gone.is_empty() {
            let body = if keep.is_empty() {
                String::new()
            } else {
                format!("{}\n", keep.join("\n"))
            };
            let _ = std::fs::write(&path, body);
        }
        gone
    }

    /// Append what the runtime queued to the logs of the tasks it concerns.
    pub(crate) async fn flush(&self, store: &Arc<AsyncMutex<EventStore>>, tenant: TenantId) -> u64 {
        let pending = self.runtime.drain_events();
        if pending.is_empty() {
            return 0;
        }
        // One transaction per task, in the order the events were queued.
        let mut by_task: Vec<(TaskId, SessionId, Vec<PendingEvent>)> = Vec::new();
        for p in pending {
            match by_task.iter_mut().find(|(t, ..)| *t == p.task_id) {
                Some((_, _, v)) => v.push(p),
                None => by_task.push((p.task_id, p.session_id, vec![p])),
            }
        }
        let mut last = 0;
        let mut st = store.lock().await;
        for (task_id, session_id, events) in by_task {
            let now = modbit_domain::Timestamp::now().0;
            let mut new_events = Vec::with_capacity(events.len());
            for p in &events {
                if let TaskEvent::ComputerObserved { artifact_ref, .. } = &p.event
                    && !artifact_ref.is_empty()
                {
                    self.note_artifact(artifact_ref, now);
                }
                let mut ev = NewEvent::new(
                    p.event.event_type(),
                    serde_json::to_value(&p.event).unwrap_or_default(),
                    Actor::Core("computer-runtime".into()),
                );
                ev.occurred_at = Some(modbit_domain::Timestamp(now));
                new_events.push(ev);
            }
            let req = AppendRequest {
                tenant_id: tenant,
                session_id,
                task_id: Some(task_id),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Task,
                aggregate_id: *task_id.as_bytes(),
                expected_sequence: None,
                events: new_events,
            };
            match st.append(req) {
                Ok(ev) => last = last.max(ev.last().map_or(0, |e| e.offset)),
                Err(e) => eprintln!("modbit-core: computer events not recorded: {e}"),
            }
        }
        last
    }

    /// Close the task's control sessions at the end of a run (the grant ends
    /// with the turn), and record it.
    pub(crate) async fn end_run(&self, core: &Core, task_id: TaskId, run_id: &str, reason: &str) {
        let n = self.runtime.end_run(task_id, run_id, reason).await;
        if n > 0 {
            let last = self.flush(&core.store, core.tenant_id).await;
            if last > 0 {
                core.last_offset.send_replace(last);
            }
        }
    }

    /// Fold the computer events of the whole log once, at boot: the latches,
    /// the stops, and the sessions a dead Core left open (closed
    /// `CORE_RESTART`: the actuator stopped with the Core's pipe).
    pub(crate) async fn recover(&self, core: &Core) {
        struct Open {
            task: TaskId,
            session: SessionId,
            run: String,
            bundle: String,
            scope: String,
        }
        let events = {
            let st = core.store.lock().await;
            match st.read_all_of_types_to_end(FOLDED, 0) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("modbit-core: computer events unreadable at boot: {e}");
                    return;
                }
            }
        };
        let mut open: HashMap<String, Open> = HashMap::new();
        let mut latches: HashMap<TaskId, Latch> = HashMap::new();
        let mut aborted: HashMap<TaskId, String> = HashMap::new();
        {
            let st = core.store.lock().await;
            for e in &events {
                let Some(task) = e.envelope.task_id else {
                    continue;
                };
                let session = e.envelope.session_id;
                let Ok(payload) = st.payload(&e.envelope) else {
                    continue;
                };
                let Ok(ev) = serde_json::from_value::<TaskEvent>(payload) else {
                    continue;
                };
                match ev {
                    TaskEvent::ComputerSessionStarted {
                        control_id,
                        run_id,
                        bundle_id,
                        scope,
                        ..
                    } => {
                        open.insert(
                            control_id,
                            Open {
                                task,
                                session,
                                run: run_id,
                                bundle: bundle_id,
                                scope,
                            },
                        );
                    }
                    TaskEvent::ComputerSessionClosed { control_id, .. } => {
                        open.remove(&control_id);
                    }
                    TaskEvent::ComputerLatched {
                        control_id,
                        tool_call_id,
                        tool,
                        action,
                        reason,
                    } => {
                        latches.insert(
                            task,
                            Latch {
                                control_id,
                                tool_call_id,
                                tool,
                                action,
                                reason,
                                at_ms: modbit_domain::Timestamp::now().0,
                            },
                        );
                    }
                    TaskEvent::ComputerLatchResolved { .. } => {
                        latches.remove(&task);
                    }
                    TaskEvent::ComputerUserAborted { run_id, .. } => {
                        aborted.insert(task, run_id);
                    }
                    _ => {}
                }
            }
        }
        for (task, latch) in latches {
            self.runtime.hydrate(task, Some(latch), None);
        }
        for (task, run) in aborted {
            self.runtime.hydrate(task, None, Some(run));
        }
        // Whatever a dead Core left open is closed now, on the record. An
        // input that was dispatched and never answered (the Core died with it
        // in flight) is of unknown outcome: the task is latched, as it would
        // have been had the actuator died instead (CUC-D03).
        let mut n = 0;
        let mut st = core.store.lock().await;
        for (control_id, o) in open {
            let in_flight = st
                .open_tool_calls(&o.task)
                .unwrap_or_default()
                .into_iter()
                .find(|c| {
                    is_computer_tool(&c.tool_name)
                        && c.effect_class
                            == modbit_domain::toolcall::EffectClass::ExternalSideEffect
                        && !matches!(
                            c.tool_name.as_str(),
                            "computer.start" | "computer.start_screen"
                        )
                        && matches!(
                            c.state,
                            modbit_domain::toolcall::ToolCallState::Dispatched
                                | modbit_domain::toolcall::ToolCallState::UnknownOutcome
                        )
                });
            if let Some(c) = in_flight {
                let action = c.tool_name.trim_start_matches("computer.").to_owned();
                let reason =
                    "CORE_RESTART: the Core ended while the input was in flight".to_owned();
                self.runtime.hydrate(
                    o.task,
                    Some(Latch {
                        control_id: control_id.clone(),
                        tool_call_id: c.tool_call_id.to_string(),
                        tool: c.tool_name.clone(),
                        action: action.clone(),
                        reason: reason.clone(),
                        at_ms: modbit_domain::Timestamp::now().0,
                    }),
                    None,
                );
                let ev = TaskEvent::ComputerLatched {
                    control_id: control_id.clone(),
                    tool_call_id: c.tool_call_id.to_string(),
                    tool: c.tool_name.clone(),
                    action,
                    reason,
                };
                let mut new = NewEvent::new(
                    ev.event_type(),
                    serde_json::to_value(&ev).unwrap_or_default(),
                    Actor::Core("computer-runtime".into()),
                );
                new.occurred_at = Some(modbit_domain::Timestamp::now());
                let _ = st.append(AppendRequest {
                    tenant_id: core.tenant_id,
                    session_id: o.session,
                    task_id: Some(o.task),
                    run_id: None,
                    turn_id: None,
                    step_id: None,
                    aggregate_type: AggregateType::Task,
                    aggregate_id: *o.task.as_bytes(),
                    expected_sequence: None,
                    events: vec![new],
                });
            }
            let ev = TaskEvent::ComputerSessionClosed {
                control_id,
                reason: "CORE_RESTART".into(),
                bundle_id: o.bundle,
                scope: o.scope,
                duration_ms: 0,
                actions_by_kind: Default::default(),
                screenshots: 0,
                observations: 0,
                outcome: "STOPPED".into(),
                approval_ids: vec![],
            };
            let mut new = NewEvent::new(
                ev.event_type(),
                serde_json::to_value(&ev).unwrap_or_default(),
                Actor::Core("computer-runtime".into()),
            );
            new.occurred_at = Some(modbit_domain::Timestamp::now());
            let req = AppendRequest {
                tenant_id: core.tenant_id,
                session_id: o.session,
                task_id: Some(o.task),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Task,
                aggregate_id: *o.task.as_bytes(),
                expected_sequence: None,
                events: vec![new],
            };
            let _ = &o.run;
            if st.append(req).is_ok() {
                n += 1;
            }
        }
        if n > 0 {
            eprintln!(
                "modbit-core: closed {n} computer-control session(s) the last Core left open"
            );
        }
    }

    /// Attach the configured actuator and start the supervisor, the event
    /// pump and the retention sweep.
    pub(crate) async fn start(self: &Arc<Self>, core: &Arc<Core>) {
        // Attach before the Core says it is ready, so the first call finds
        // the actuator; a failure leaves the tools absent and the
        // supervisor retrying.
        if self.runtime.configured() {
            match self.runtime.attach().await {
                Ok(()) => {}
                Err(e) => eprintln!("modbit-core: no actuator attached: {e}"),
            }
        }
        let host = Arc::clone(self);
        let core2 = Arc::clone(core);
        tokio::spawn(async move {
            host.runtime.start_supervisor();
            let tick = Duration::from_millis(100);
            let mut last_sweep = std::time::Instant::now();
            loop {
                tokio::time::sleep(tick).await;
                let last = host.flush(&core2.store, core2.tenant_id).await;
                if last > 0 {
                    core2.last_offset.send_replace(last);
                }
                if last_sweep.elapsed()
                    >= host
                        .retention
                        .min(Duration::from_secs(60))
                        .max(Duration::from_millis(500))
                {
                    last_sweep = std::time::Instant::now();
                    let gone = {
                        let st = core2.store.lock().await;
                        host.expire(&st, modbit_domain::Timestamp::now().0)
                    };
                    if !gone.is_empty() {
                        host.record_expired(&core2, gone).await;
                    }
                }
            }
        });
    }

    async fn record_expired(&self, core: &Core, gone: Vec<(String, i64)>) {
        // The frames' tasks are found from the observation events that named
        // them, so the expiry lands on the same task's log.
        let mut st = core.store.lock().await;
        let events = match st.read_all_of_types_to_end(&["ComputerObserved"], 0) {
            Ok(e) => e,
            Err(_) => return,
        };
        for (object, taken) in gone {
            for e in &events {
                let Ok(payload) = st.payload(&e.envelope) else {
                    continue;
                };
                if payload["artifact_ref"].as_str() != Some(object.as_str()) {
                    continue;
                }
                let Some(task) = e.envelope.task_id else {
                    continue;
                };
                let ev = TaskEvent::ComputerArtifactExpired {
                    artifact_ref: object.clone(),
                    retention_secs: self.retention.as_secs(),
                    taken_at_ms: taken,
                };
                let mut new = NewEvent::new(
                    ev.event_type(),
                    serde_json::to_value(&ev).unwrap_or_default(),
                    Actor::Core("computer-runtime".into()),
                );
                new.occurred_at = Some(modbit_domain::Timestamp::now());
                let _ = st.append(AppendRequest {
                    tenant_id: core.tenant_id,
                    session_id: e.envelope.session_id,
                    task_id: Some(task),
                    run_id: None,
                    turn_id: None,
                    step_id: None,
                    aggregate_type: AggregateType::Task,
                    aggregate_id: *task.as_bytes(),
                    expected_sequence: None,
                    events: vec![new],
                });
                break;
            }
        }
    }
}

/// The port one tool call reaches the runtime through: the task, the run,
/// the mode and the policy it is decided under.
pub(crate) struct CorePort {
    pub host: Arc<ComputerHost>,
    pub task_id: TaskId,
    pub session_id: SessionId,
    pub run_id: String,
    pub mode: String,
    pub emergency_stopped: bool,
    pub policy: ComputerPolicy,
    pub redactor: modbit_secrets::Redactor,
}

impl CorePort {
    fn ctx(&self, info: &CallInfo) -> CallCtx {
        let redactor = self.redactor.clone();
        CallCtx {
            task_id: self.task_id,
            session_id: self.session_id,
            run_id: self.run_id.clone(),
            tool_call_id: info.tool_call_id.clone(),
            approval_id: info.approval_id.clone(),
            intent_hash: info.intent_hash.clone(),
            mode: self.mode.clone(),
            emergency_stopped: self.emergency_stopped,
            policy: self.policy.clone(),
            redact: Arc::new(move |s| redactor.error_text(s)),
            stop_on_blockers: self.host.is_child(self.task_id),
        }
    }
}

impl ComputerPort for CorePort {
    fn offered(&self) -> bool {
        self.host.offered()
    }

    fn prepare<'a>(
        &'a self,
        info: &'a CallInfo,
        tool: &'a str,
        op: &'a modbit_computer::ops::Op,
    ) -> BoxFuture<'a, Result<Option<Value>, Refusal>> {
        Box::pin(async move { self.host.runtime.prepare(&self.ctx(info), tool, op).await })
    }

    fn invoke<'a>(
        &'a self,
        info: &'a CallInfo,
        tool: &'a str,
        op: modbit_computer::ops::Op,
    ) -> BoxFuture<'a, Result<Answer, Failure>> {
        Box::pin(async move { self.host.runtime.invoke(&self.ctx(info), tool, op).await })
    }
}

// ---- commands ----

type Refused = (String, String);

fn refuse(code: &str, why: impl Into<String>) -> Refused {
    (code.to_owned(), why.into())
}

fn task_id_of(id: Option<&wire::Id>) -> Result<TaskId, Refused> {
    id.and_then(id16)
        .map(TaskId::from_bytes)
        .ok_or_else(|| refuse("BAD_PAYLOAD", "task_id required"))
}

/// The task's latest run, as the runtime labels it (`""` when it has none).
async fn current_run(core: &Core, task_id: TaskId) -> String {
    core.store
        .lock()
        .await
        .runs_for_task(&task_id)
        .ok()
        .and_then(|r| r.into_iter().max_by_key(|r| r.attempt))
        .map(|r| r.run_id.to_string())
        .unwrap_or_default()
}

/// The view of the runtime for a task.
pub(crate) fn runtime_view(core: &Core, task_id: TaskId, run: &str) -> wire::ComputerRuntimeView {
    let v = core.tools.computer.runtime.view(task_id, run);
    let mut out = wire::ComputerRuntimeView {
        configured: v.configured,
        attached: v.attached,
        detail: v.detail,
        user_aborted: v.user_aborted,
        non_drivable: policy::listing(),
        non_drivable_version: policy::NON_DRIVABLE_VERSION.to_owned(),
        ..Default::default()
    };
    if let Some(i) = v.info {
        out.actuator_name = i.name;
        out.actuator_version = i.version;
        out.actuator_pid = i.pid;
        out.platform = i.platform;
    }
    if let Some(s) = v.session {
        out.control_id = s.control_id;
        out.application = s.app.name;
        out.bundle_id = s.app.bundle_id;
        out.scope = s.scope.name().to_owned();
        out.controller = s.controller.to_owned();
        out.grant_expires_ms = s.grant_expires_ms;
        out.grant_calls_left = s.grant_calls_left;
        out.actions = s.actions;
        out.screenshots = s.screenshots;
        out.observations = s.observations;
    }
    if let Some(l) = v.latch {
        out.latch = format!("{} {}: {}", l.tool, l.action, l.reason);
        out.latch_tool_call_id = l.tool_call_id;
    }
    out
}

/// Every command of this module.
pub(crate) async fn handle(core: &Arc<Core>, env: &wire::CommandEnvelope) -> wire::CommandAck {
    let cid = env.command_id.clone();
    macro_rules! decode {
        ($ty:ty) => {
            match <$ty>::decode(env.payload.as_slice()) {
                Ok(p) => p,
                Err(_) => return reject(cid, "BAD_PAYLOAD", stringify!($ty)),
            }
        };
    }
    let task_of = |id: Option<&wire::Id>| task_id_of(id);
    match env.command_type.as_str() {
        "GetComputerRuntime" => {
            let p = decode!(wire::GetComputerRuntime);
            let task_id = match task_of(p.task_id.as_ref()) {
                Ok(t) => t,
                Err((c, m)) => return reject(cid, &c, m),
            };
            let run = current_run(core, task_id).await;
            accept(
                cid,
                false,
                runtime_view(core, task_id, &run).encode_to_vec(),
            )
        }
        "ResolveComputerLatch" => {
            let p = decode!(wire::ResolveComputerLatch);
            let task_id = match task_of(p.task_id.as_ref()) {
                Ok(t) => t,
                Err((c, m)) => return reject(cid, &c, m),
            };
            let task = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, crate::server::error_code(&e), e.to_string()),
            };
            if let Err(ack) = require_lease(core, &cid, env, &task.session_id).await {
                return ack;
            }
            let was = core
                .tools
                .computer
                .runtime
                .resolve_latch(task_id, task.session_id);
            let last = core.tools.computer.flush(&core.store, core.tenant_id).await;
            if last > 0 {
                core.last_offset.send_replace(last);
            }
            accept(
                cid,
                false,
                wire::ComputerLatchResolved { was_latched: was }.encode_to_vec(),
            )
        }
        "ComputerStop" => {
            let p = decode!(wire::ComputerStop);
            let task_id = match task_of(p.task_id.as_ref()) {
                Ok(t) => t,
                Err((c, m)) => return reject(cid, &c, m),
            };
            let task = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, crate::server::error_code(&e), e.to_string()),
            };
            if let Err(ack) = require_lease(core, &cid, env, &task.session_id).await {
                return ack;
            }
            let run = current_run(core, task_id).await;
            let reason = if p.reason.is_empty() {
                "on-screen Stop".to_owned()
            } else {
                p.reason.chars().take(200).collect()
            };
            let (closed, stopped) = core
                .tools
                .computer
                .runtime
                .stop_task(task_id, task.session_id, &run, &reason)
                .await;
            let last = core.tools.computer.flush(&core.store, core.tenant_id).await;
            if last > 0 {
                core.last_offset.send_replace(last);
            }
            accept(
                cid,
                false,
                wire::ComputerStopped {
                    sessions_closed: u32::try_from(closed).unwrap_or(u32::MAX),
                    actuator_stopped: stopped,
                }
                .encode_to_vec(),
            )
        }
        other => reject(cid, "UNKNOWN_COMMAND", other),
    }
}

/// Programs and API names that synthesize input or capture the screen outside
/// the Computer Runtime. A computer-use child's shell is for non-GUI work
/// (DR-PX-2026-10-03-008: driving the interface through the shell bypasses
/// the typed refusals, the latch and the approval class).
const GUI_DRIVERS: &[&str] = &[
    "osascript",
    "cliclick",
    "xdotool",
    "ydotool",
    "wmctrl",
    "nircmd",
    "autohotkey",
    "sendkeys",
    "sendinput",
    "cgevent",
    "pyautogui",
    "system events",
    "automator",
    "screencapture",
    "scrot",
    "mouse_event",
    "keybd_event",
];

/// Whether a command would drive or capture the GUI.
pub(crate) fn argv_drives_gui(argv: &[String]) -> bool {
    let lower = argv.join(" ").to_ascii_lowercase();
    GUI_DRIVERS.iter().any(|d| lower.contains(d))
}

/// The set of tool names `computer.*` (for the projection filter).
pub(crate) fn is_computer_tool(name: &str) -> bool {
    name.starts_with("computer.")
}
