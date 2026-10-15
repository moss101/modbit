//! The Computer Runtime (PX-069, PX-070, PX-075, PX-076).
//!
//! One runtime per Core. It owns the attached actuator, the control sessions
//! (at most one controller at a time, one session per task), the observation
//! grants, the element and frame handles, the per-task unknown-outcome latch,
//! the user's sticky stop, the watchdog and the audit events - and nothing
//! about models, tools or the event store, which call it. A call arrives as a
//! tool name and JSON arguments with a [`CallCtx`] naming who is asking; it
//! leaves as an [`Answer`] or a typed [`Failure`].
//!
//! The order of the checks is the contract (see `precheck` in `act.rs`):
//! the actuator must be attached; a user stop is final for the run; an
//! unknown-outcome latch refuses every input; a session must exist and its
//! grant must be live; the controller must be the agent (not the person);
//! policy must admit the application and scope; the application must be
//! drivable; the handle must be current. Each refusal is typed and names its
//! escalation, and a refusal before an input is sent actuates nothing.

mod act;
mod observe;
mod supervise;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use modbit_domain::task::TaskEvent;
use modbit_domain::{SessionId, TaskId};
use modbit_protocol::v1 as wire;
use serde_json::{Value, json};

use crate::actuator::{Actuator, ActuatorConfig, ActuatorInfo, CallError, Reply};
use crate::frame::{Encoded, FrameMemo};
use crate::model::{AppIdentity, Node, Rect, Scope, Window};
use crate::ops::Op;
use crate::policy::ComputerPolicy;
use crate::taxonomy::{Code, Refusal};

/// Observation calls a grant allows.
pub const DEFAULT_GRANT_CALL_CAP: u32 = 100;

/// Timing and limits. The defaults are the contract; the environment may
/// shorten them for tests (`MODBIT_COMPUTER_*`).
#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    /// An observation grant lasts at most this long (CUC-C02: 15 minutes).
    pub grant_ttl: Duration,
    /// Observation calls per grant.
    pub grant_call_cap: u32,
    /// A session with no call for this long is closed by the watchdog.
    pub idle_ttl: Duration,
    /// How long an observation may take.
    pub observe_deadline: Duration,
    /// How long an input may take before its outcome is unknown.
    pub action_deadline: Duration,
    /// After the person touches the machine, agent input waits this long.
    pub human_cooldown: Duration,
    /// How often the watchdog and the heartbeat tick.
    pub tick: Duration,
    /// The lease the actuator holds without hearing from the Core.
    pub lease_ttl: Duration,
    /// This process's id (never drivable).
    pub own_pid: u32,
    /// The longest a handshake may take.
    pub handshake_timeout: Duration,
    /// How long a call waits for an actuator that is restarting.
    pub restart_wait: Duration,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            grant_ttl: Duration::from_secs(15 * 60),
            grant_call_cap: DEFAULT_GRANT_CALL_CAP,
            idle_ttl: Duration::from_secs(600),
            observe_deadline: Duration::from_secs(20),
            action_deadline: Duration::from_secs(15),
            human_cooldown: Duration::from_secs(2),
            tick: Duration::from_millis(200),
            lease_ttl: Duration::from_secs(10),
            own_pid: std::process::id(),
            handshake_timeout: Duration::from_secs(5),
            restart_wait: Duration::from_secs(4),
        }
    }
}

impl RuntimeConfig {
    /// The defaults with the `MODBIT_COMPUTER_*` overrides applied
    /// (`GRANT_MS`, `GRANT_CALLS`, `IDLE_MS`, `OBSERVE_MS`, `ACTION_MS`,
    /// `HUMAN_COOLDOWN_MS`, `TICK_MS`).
    #[must_use]
    pub fn from_env() -> Self {
        let mut c = Self::default();
        let ms = |k: &str| {
            std::env::var(format!("MODBIT_COMPUTER_{k}"))
                .ok()
                .and_then(|v| v.trim().parse::<u64>().ok())
                .filter(|v| *v > 0)
                .map(Duration::from_millis)
        };
        if let Some(d) = ms("GRANT_MS") {
            c.grant_ttl = d.min(Duration::from_secs(15 * 60));
        }
        if let Some(d) = ms("IDLE_MS") {
            c.idle_ttl = d;
        }
        if let Some(d) = ms("OBSERVE_MS") {
            c.observe_deadline = d;
        }
        if let Some(d) = ms("ACTION_MS") {
            c.action_deadline = d;
        }
        if let Some(d) = ms("HUMAN_COOLDOWN_MS") {
            c.human_cooldown = d;
        }
        if let Some(d) = ms("TICK_MS") {
            c.tick = d;
        }
        if let Some(n) = std::env::var("MODBIT_COMPUTER_GRANT_CALLS")
            .ok()
            .and_then(|v| v.trim().parse::<u32>().ok())
            .filter(|n| *n > 0)
        {
            c.grant_call_cap = n.min(DEFAULT_GRANT_CALL_CAP);
        }
        c
    }
}

/// Who is asking, and what the Core knew about the call when it decided it.
#[derive(Clone)]
pub struct CallCtx {
    /// The task.
    pub task_id: TaskId,
    /// Its session.
    pub session_id: SessionId,
    /// The run (the turn a grant ends with); empty outside a run.
    pub run_id: String,
    /// The call.
    pub tool_call_id: String,
    /// The approval that authorised it (an input or a start), when it has one.
    pub approval_id: String,
    /// The intent hash the approval bound.
    pub intent_hash: String,
    /// The task's mode in force; a switch ends a grant.
    pub mode: String,
    /// The session is under an emergency stop.
    pub emergency_stopped: bool,
    /// What administrative policy decided for this call.
    pub policy: ComputerPolicy,
    /// Removes credential shapes from text the application showed.
    pub redact: Arc<dyn Fn(&str) -> String + Send + Sync>,
    /// The caller is a `computer-use` child (PX-075): while the window shows
    /// a login, a passkey, a captcha, a permission prompt or a destructive
    /// confirmation, its inputs are refused - it reports the blocker and does
    /// not improvise (CUC-D06).
    pub stop_on_blockers: bool,
}

impl CallCtx {
    /// A context with no policy restriction and an identity redactor (tests).
    #[must_use]
    pub fn bare(task_id: TaskId, session_id: SessionId, run_id: &str) -> Self {
        Self {
            task_id,
            session_id,
            run_id: run_id.to_owned(),
            tool_call_id: String::new(),
            approval_id: String::new(),
            intent_hash: String::new(),
            mode: "AGENT".into(),
            emergency_stopped: false,
            policy: ComputerPolicy::default(),
            redact: Arc::new(|s| s.to_owned()),
            stop_on_blockers: false,
        }
    }
}

/// A frame ready for the media pipeline.
#[derive(Clone, Debug)]
pub struct FrameArtifact {
    /// The encoded, masked frame.
    pub encoded: Arc<Encoded>,
    /// The digest of the masked pixels.
    pub digest: String,
    /// The object digest the encoded bytes are stored under (sha256 of the bytes).
    pub object_digest: String,
    /// The same pixels were encoded before.
    pub reused: bool,
    /// What the frame is of (untrusted provenance text).
    pub source: String,
}

/// What a call produced.
#[derive(Clone, Debug)]
pub struct Answer {
    /// The structured answer.
    pub output: Value,
    /// A frame to put through the media pipeline.
    pub frame: Option<FrameArtifact>,
}

/// Why a call did not produce an answer.
#[derive(Clone, Debug)]
pub enum Failure {
    /// Refused or failed before any effect, or with a known effect.
    Refused(Refusal),
    /// An input that may have happened (CUC-D03): the task is latched.
    Unknown {
        /// What was observed.
        reason: String,
        /// The structured answer (the latch, the recovery).
        output: Value,
    },
}

impl From<Refusal> for Failure {
    fn from(r: Refusal) -> Self {
        Failure::Refused(r)
    }
}

/// An event the runtime wants on the task's log.
#[derive(Clone, Debug)]
pub struct PendingEvent {
    /// The task.
    pub task_id: TaskId,
    /// Its session.
    pub session_id: SessionId,
    /// The run, when known.
    pub run_id: String,
    /// The event.
    pub event: TaskEvent,
}

#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub id: String,
    pub nodes: Vec<Node>,
    pub window: Window,
    pub digest: String,
}

#[derive(Clone, Debug)]
pub(crate) struct FrameHandle {
    pub token: String,
    pub window_id: String,
    pub window_version: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Controller {
    Agent,
    /// The person touched the machine; the agent waits until `until_ms`.
    Parked {
        until_ms: u64,
    },
    Stopped,
}

#[derive(Debug, Default)]
pub(crate) struct Counts {
    pub actions_by_kind: BTreeMap<String, u32>,
    pub screenshots: u32,
    pub observations: u32,
    pub approvals: Vec<String>,
    pub failed: u32,
}

pub(crate) struct Session {
    pub control_id: String,
    pub task_id: TaskId,
    pub session_id: SessionId,
    pub run_id: String,
    pub mode: String,
    pub app: AppIdentity,
    pub scope: Scope,
    pub window_id: String,
    pub started: Instant,
    pub expires_ms: i64,
    pub calls_left: u32,
    pub last_activity: Instant,
    pub snapshot: Option<Snapshot>,
    pub frame: Option<FrameHandle>,
    pub last_window: Window,
    pub controller: Controller,
    pub counts: Counts,
    pub gate: Arc<tokio::sync::Mutex<()>>,
    pub memo: FrameMemo,
    /// Opened over a latch: the first observation reconciles it.
    pub reconciling: bool,
    /// Which actuator connection this session lives on.
    pub epoch: u64,
    /// The blockers the last tree showed (kinds), empty when none.
    pub blockers: Vec<String>,
    /// An input is in flight: if the connection ends under it, the call owns
    /// the outcome (it latches and closes the session itself).
    pub in_flight: bool,
}

/// The unknown-outcome latch of a task (CUC-D03).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Latch {
    /// The session the input was sent in.
    pub control_id: String,
    /// The call whose outcome is unknown.
    pub tool_call_id: String,
    /// The tool.
    pub tool: String,
    /// The action kind.
    pub action: String,
    /// What was observed.
    pub reason: String,
    /// When (ms since the epoch).
    pub at_ms: i64,
}

#[derive(Default)]
pub(crate) struct State {
    pub sessions: HashMap<String, Session>,
    /// The session that holds the machine (the single-controller lock).
    pub controller: Option<String>,
    pub latches: HashMap<TaskId, Latch>,
    /// Tasks the person stopped, with the run the stop applies to.
    pub aborted: HashMap<TaskId, String>,
    /// Agent input waits until this time after the person touched the machine.
    pub human_until_ms: u64,
    /// Why a task's last session closed, so the next call is told the truth
    /// ("the grant ended") instead of "no session".
    pub closed: HashMap<TaskId, (String, String)>,
}

pub(crate) struct Attached {
    pub actuator: Arc<dyn Actuator>,
    pub epoch: u64,
}

pub(crate) struct Shared {
    pub cfg: RuntimeConfig,
    pub state: Mutex<State>,
    pub actuator: Mutex<Option<Attached>>,
    pub config: Mutex<Option<ActuatorConfig>>,
    /// Why no actuator is attached.
    pub detail: Mutex<String>,
    /// The configuration cannot work (the executable is missing, the
    /// handshake was refused): not offered, not retried.
    pub fatal: AtomicBool,
    pub pending: Mutex<Vec<PendingEvent>>,
    pub epoch: AtomicU64,
    pub supervisor: AtomicBool,
    pub next_attempt: Mutex<Instant>,
    pub backoff: Mutex<Duration>,
    pub last_ping: Mutex<Instant>,
}

/// The Computer Runtime.
#[derive(Clone)]
pub struct ComputerRuntime {
    pub(crate) shared: Arc<Shared>,
}

/// The current time in ms since the epoch.
#[must_use]
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}{:012x}", rand::random::<u64>() & 0xffff_ffff_ffff)
}

/// What a client reads of the runtime for one task.
#[derive(Clone, Debug, Default)]
pub struct RuntimeView {
    /// An actuator is configured.
    pub configured: bool,
    /// One is attached.
    pub attached: bool,
    /// What it said of itself.
    pub info: Option<ActuatorInfo>,
    /// Why none is attached.
    pub detail: String,
    /// The task's session.
    pub session: Option<SessionView>,
    /// The task's latch.
    pub latch: Option<Latch>,
    /// The person's stop applies to the task's current run.
    pub user_aborted: bool,
}

/// One control session as a client sees it.
#[derive(Clone, Debug)]
pub struct SessionView {
    /// Id.
    pub control_id: String,
    /// Application.
    pub app: AppIdentity,
    /// Scope.
    pub scope: Scope,
    /// `AGENT` | `PARKED` | `STOPPED`.
    pub controller: &'static str,
    /// Grant end.
    pub grant_expires_ms: i64,
    /// Observation calls left.
    pub grant_calls_left: u32,
    /// Inputs so far.
    pub actions: u32,
    /// Screenshots so far.
    pub screenshots: u32,
    /// Observations so far.
    pub observations: u32,
}

impl ComputerRuntime {
    /// A runtime with no actuator.
    #[must_use]
    pub fn new(cfg: RuntimeConfig) -> Self {
        Self {
            shared: Arc::new(Shared {
                cfg,
                state: Mutex::new(State::default()),
                actuator: Mutex::new(None),
                config: Mutex::new(None),
                detail: Mutex::new("no actuator is configured".into()),
                fatal: AtomicBool::new(false),
                pending: Mutex::new(Vec::new()),
                epoch: AtomicU64::new(0),
                supervisor: AtomicBool::new(false),
                next_attempt: Mutex::new(Instant::now()),
                backoff: Mutex::new(Duration::from_millis(250)),
                last_ping: Mutex::new(Instant::now()),
            }),
        }
    }

    /// The configuration in force.
    #[must_use]
    pub fn config(&self) -> &RuntimeConfig {
        &self.shared.cfg
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, State> {
        self.shared.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Whether an actuator is configured.
    #[must_use]
    pub fn configured(&self) -> bool {
        self.shared
            .config
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
            || self
                .shared
                .actuator
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some()
    }

    /// Whether an actuator is attached now.
    #[must_use]
    pub fn attached(&self) -> bool {
        self.actuator().is_some()
    }

    /// Whether the tools are offered to a model: an actuator is configured
    /// and has not been found unusable. A configured actuator that is
    /// restarting still counts - its calls answer `ACTUATOR_UNAVAILABLE`.
    #[must_use]
    pub fn offered(&self) -> bool {
        self.attached()
            || (self.configured()
                && !self.shared.fatal.load(Ordering::SeqCst)
                && self.was_attached())
    }

    fn was_attached(&self) -> bool {
        self.shared.epoch.load(Ordering::SeqCst) > 0
    }

    /// Why no actuator is attached.
    #[must_use]
    pub fn detail(&self) -> String {
        self.shared
            .detail
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub(crate) fn actuator(&self) -> Option<Arc<dyn Actuator>> {
        let g = self
            .shared
            .actuator
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        g.as_ref()
            .filter(|a| a.actuator.is_alive())
            .map(|a| Arc::clone(&a.actuator))
    }

    pub(crate) fn epoch_now(&self) -> u64 {
        self.shared
            .actuator
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map_or(0, |a| a.epoch)
    }

    /// The attached actuator, waiting a few seconds for a restarting one: a
    /// call that arrives in the gap between an actuator ending and its
    /// relaunch is not refused for the gap (CUC-D02: only a failure provably
    /// before any write may relaunch; this is before one).
    pub(crate) async fn ready_actuator(&self) -> Result<Arc<dyn Actuator>, Refusal> {
        if let Some(a) = self.actuator() {
            return Ok(a);
        }
        if self.configured() && !self.shared.fatal.load(Ordering::SeqCst) {
            let deadline = Instant::now() + self.shared.cfg.restart_wait;
            while Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(50)).await;
                if let Some(a) = self.actuator() {
                    return Ok(a);
                }
            }
        }
        self.require_actuator()
    }

    pub(crate) fn require_actuator(&self) -> Result<Arc<dyn Actuator>, Refusal> {
        self.actuator().ok_or_else(|| {
            let detail = self.detail();
            Refusal::new(
                Code::ActuatorUnavailable,
                if detail.is_empty() {
                    "no actuator is attached".to_owned()
                } else {
                    format!("no actuator is attached ({detail})")
                },
            )
        })
    }

    /// Queue an event for the task's log.
    pub(crate) fn emit(
        &self,
        task_id: TaskId,
        session_id: SessionId,
        run_id: &str,
        event: TaskEvent,
    ) {
        self.shared
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(PendingEvent {
                task_id,
                session_id,
                run_id: run_id.to_owned(),
                event,
            });
    }

    /// Take the events queued so far, for the Core to append.
    #[must_use]
    pub fn drain_events(&self) -> Vec<PendingEvent> {
        std::mem::take(
            &mut *self
                .shared
                .pending
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
        )
    }

    /// Attach an actuator that is already connected (the launcher and the
    /// tests use this).
    pub fn install(&self, actuator: Arc<dyn Actuator>) {
        let epoch = self.shared.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        *self
            .shared
            .actuator
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(Attached {
            actuator: Arc::clone(&actuator),
            epoch,
        });
        self.shared.fatal.store(false, Ordering::SeqCst);
        *self.shared.detail.lock().unwrap_or_else(|e| e.into_inner()) = String::new();
        *self
            .shared
            .backoff
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Duration::from_millis(250);
        supervise::watch(self, actuator, epoch);
    }

    /// Remember how to launch the actuator.
    pub fn configure(&self, config: ActuatorConfig) {
        *self.shared.config.lock().unwrap_or_else(|e| e.into_inner()) = Some(config);
        *self.shared.detail.lock().unwrap_or_else(|e| e.into_inner()) =
            "the actuator has not started yet".into();
    }

    /// Launch the configured actuator now.
    ///
    /// # Errors
    /// Not configured, or the launch failed (a refused or unusable actuator
    /// also marks the configuration unusable, so it is not offered).
    pub async fn attach(&self) -> Result<(), crate::actuator::LaunchError> {
        let Some(config) = self
            .shared
            .config
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        else {
            return Err(crate::actuator::LaunchError::Spawn(
                "no actuator is configured".into(),
            ));
        };
        match crate::actuator::ProcessActuator::launch(&config, self.shared.cfg.handshake_timeout)
            .await
        {
            Ok(a) => {
                self.install(Arc::new(a));
                Ok(())
            }
            Err(e) => {
                *self.shared.detail.lock().unwrap_or_else(|x| x.into_inner()) = e.to_string();
                if matches!(
                    e,
                    crate::actuator::LaunchError::Refused(_)
                        | crate::actuator::LaunchError::Handshake(_)
                ) || matches!(&e, crate::actuator::LaunchError::Spawn(s) if s.contains("No such file") || s.contains("cannot find") || s.contains("not found"))
                {
                    self.shared.fatal.store(true, Ordering::SeqCst);
                }
                Err(e)
            }
        }
    }

    /// Start the supervisor: it relaunches a lost actuator with backoff,
    /// runs the watchdog and keeps the heartbeat. Idempotent.
    pub fn start_supervisor(&self) {
        if self.shared.supervisor.swap(true, Ordering::SeqCst) {
            return;
        }
        supervise::run(self.clone());
    }

    /// What a client reads for `task`.
    #[must_use]
    pub fn view(&self, task_id: TaskId, run_id: &str) -> RuntimeView {
        let st = self.lock();
        let session = st
            .sessions
            .values()
            .find(|s| s.task_id == task_id)
            .map(|s| SessionView {
                control_id: s.control_id.clone(),
                app: s.app.clone(),
                scope: s.scope,
                controller: match s.controller {
                    Controller::Agent => "AGENT",
                    Controller::Parked { .. } => "PARKED",
                    Controller::Stopped => "STOPPED",
                },
                grant_expires_ms: s.expires_ms,
                grant_calls_left: s.calls_left,
                actions: s.counts.actions_by_kind.values().sum(),
                screenshots: s.counts.screenshots,
                observations: s.counts.observations,
            });
        let user_aborted = st
            .aborted
            .get(&task_id)
            .is_some_and(|r| run_id.is_empty() || r == run_id);
        let latch = st.latches.get(&task_id).cloned();
        drop(st);
        RuntimeView {
            configured: self.configured(),
            attached: self.attached(),
            info: self
                .shared
                .actuator
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .map(|a| a.actuator.info().clone()),
            detail: self.detail(),
            session,
            latch,
            user_aborted,
        }
    }

    /// Restore what a restarted Core read from the task's log: the latch and
    /// the run the person stopped.
    pub fn hydrate(&self, task_id: TaskId, latch: Option<Latch>, aborted_run: Option<String>) {
        let mut st = self.lock();
        if let Some(l) = latch {
            st.latches.entry(task_id).or_insert(l);
        }
        if let Some(r) = aborted_run {
            st.aborted.entry(task_id).or_insert(r);
        }
    }

    /// The person resolves a latch (CUC-D03): the refusal that stood ends.
    /// The model still needs a session, an observation and an approval to act.
    pub fn resolve_latch(&self, task_id: TaskId, session_id: SessionId) -> bool {
        let latch = self.lock().latches.remove(&task_id);
        match latch {
            Some(l) => {
                self.emit(
                    task_id,
                    session_id,
                    "",
                    TaskEvent::ComputerLatchResolved {
                        control_id: String::new(),
                        was_tool_call_id: l.tool_call_id,
                        by: "person".into(),
                    },
                );
                true
            }
            None => false,
        }
    }

    /// Close the task's control sessions at the end of a run (the grant ends
    /// with the turn) and forget the run's stop.
    pub async fn end_run(&self, task_id: TaskId, run_id: &str, reason: &str) -> usize {
        let ids: Vec<String> = {
            let mut st = self.lock();
            if st.aborted.get(&task_id).is_some_and(|r| r == run_id) {
                st.aborted.remove(&task_id);
            }
            st.sessions
                .values()
                .filter(|s| s.task_id == task_id && (run_id.is_empty() || s.run_id == run_id))
                .map(|s| s.control_id.clone())
                .collect()
        };
        let n = ids.len();
        for id in ids {
            self.close_session(&id, reason, true).await;
        }
        n
    }

    /// The person pressed Stop (CUC-D05): the actuator stops injecting, the
    /// task's sessions close and the stop is sticky for the run.
    pub async fn stop_task(
        &self,
        task_id: TaskId,
        session_id: SessionId,
        run_hint: &str,
        reason: &str,
    ) -> (usize, bool) {
        let (ids, run) = {
            let mut st = self.lock();
            let ids: Vec<(String, String)> = st
                .sessions
                .values()
                .filter(|s| s.task_id == task_id)
                .map(|s| (s.control_id.clone(), s.run_id.clone()))
                .collect();
            let run = ids
                .first()
                .map(|(_, r)| r.clone())
                .filter(|r| !r.is_empty())
                .unwrap_or_else(|| run_hint.to_owned());
            st.aborted.insert(task_id, run.clone());
            for s in st.sessions.values_mut().filter(|s| s.task_id == task_id) {
                s.controller = Controller::Stopped;
            }
            (ids, run)
        };
        let stopped = self.stop_actuator(reason, "desktop").await;
        for (id, _) in &ids {
            self.close_session(id, "USER_ABORTED", true).await;
        }
        self.emit(
            task_id,
            session_id,
            &run,
            TaskEvent::ComputerUserAborted {
                run_id: run.clone(),
                reason: reason.to_owned(),
            },
        );
        (ids.len(), stopped)
    }

    /// The session's emergency stop: every control session of the session
    /// stops at once.
    pub async fn emergency_stop(&self, session_id: SessionId, reason: &str) -> usize {
        let tasks: Vec<TaskId> = {
            let st = self.lock();
            st.sessions
                .values()
                .filter(|s| s.session_id == session_id)
                .map(|s| s.task_id)
                .collect()
        };
        let mut n = 0;
        for t in tasks {
            n += self.stop_task(t, session_id, "", reason).await.0;
        }
        n
    }

    async fn stop_actuator(&self, reason: &str, source: &str) -> bool {
        let Some(a) = self.actuator() else {
            return false;
        };
        matches!(
            a.call(
                crate::actuator::Call::Stop(wire::StopRequest {
                    reason: reason.to_owned(),
                    source: source.to_owned(),
                }),
                Duration::from_millis(500),
            )
            .await,
            Ok(Reply::Stop(_))
        )
    }

    /// Close a session: release the actuator's lease (when asked), free the
    /// controller lock and queue the audit event.
    pub(crate) async fn close_session(&self, control_id: &str, reason: &str, release: bool) {
        let session = {
            let mut st = self.lock();
            let s = st.sessions.remove(control_id);
            if st.controller.as_deref() == Some(control_id) {
                st.controller = None;
            }
            s
        };
        let Some(s) = session else {
            return;
        };
        self.lock()
            .closed
            .insert(s.task_id, (reason.to_owned(), s.run_id.clone()));
        if release && let Some(a) = self.actuator() {
            let _ = a
                .call(
                    crate::actuator::Call::Release(wire::ReleaseRequest {
                        control_id: control_id.to_owned(),
                    }),
                    Duration::from_secs(2),
                )
                .await;
        }
        let latched = self.lock().latches.contains_key(&s.task_id);
        let outcome = match reason {
            "USER_ABORTED" | "EMERGENCY_STOP" => "STOPPED",
            _ if latched => "LATCHED",
            _ => "OK",
        };
        self.emit(
            s.task_id,
            s.session_id,
            &s.run_id,
            TaskEvent::ComputerSessionClosed {
                control_id: s.control_id.clone(),
                reason: reason.to_owned(),
                bundle_id: s.app.bundle_id.clone(),
                scope: s.scope.name().to_owned(),
                duration_ms: u64::try_from(s.started.elapsed().as_millis()).unwrap_or(u64::MAX),
                actions_by_kind: s.counts.actions_by_kind.clone(),
                screenshots: s.counts.screenshots,
                observations: s.counts.observations,
                outcome: outcome.to_owned(),
                approval_ids: s.counts.approvals.clone(),
            },
        );
    }

    /// Close sessions whose actuator connection ended (`epoch`).
    pub(crate) async fn actuator_lost(&self, epoch: u64) {
        {
            let mut g = self
                .shared
                .actuator
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if g.as_ref().is_some_and(|a| a.epoch == epoch) {
                *g = None;
            }
        }
        *self.shared.detail.lock().unwrap_or_else(|e| e.into_inner()) =
            "the actuator process ended; it is being restarted".into();
        let ids: Vec<String> = {
            let st = self.lock();
            st.sessions
                .values()
                .filter(|s| s.epoch == epoch && !s.in_flight)
                .map(|s| s.control_id.clone())
                .collect()
        };
        for id in ids {
            self.close_session(&id, "ACTUATOR_LOST", false).await;
        }
    }

    /// Latch the task: an input of unknown outcome (CUC-D03).
    pub(crate) fn latch(
        &self,
        ctx: &CallCtx,
        control_id: &str,
        tool: &str,
        action: &str,
        reason: &str,
    ) -> Latch {
        let l = Latch {
            control_id: control_id.to_owned(),
            tool_call_id: ctx.tool_call_id.clone(),
            tool: tool.to_owned(),
            action: action.to_owned(),
            reason: reason.to_owned(),
            at_ms: now_ms(),
        };
        self.lock().latches.insert(ctx.task_id, l.clone());
        self.emit(
            ctx.task_id,
            ctx.session_id,
            &ctx.run_id,
            TaskEvent::ComputerLatched {
                control_id: control_id.to_owned(),
                tool_call_id: ctx.tool_call_id.clone(),
                tool: tool.to_owned(),
                action: action.to_owned(),
                reason: reason.to_owned(),
            },
        );
        l
    }

    /// The structured answer of a latched refusal.
    pub(crate) fn latched_refusal(l: &Latch) -> Refusal {
        Refusal::new(
            Code::OutcomeUnknown,
            format!(
                "the {} `{}` may have happened ({}); no further input runs until a new control session observes the real state",
                l.tool, l.action, l.reason
            ),
        )
        .with_facts(json!({
            "latched": true,
            "input_sent": false,
            "since": {
                "tool": l.tool,
                "action": l.action,
                "reason": l.reason,
                "tool_call_id": l.tool_call_id,
                "control_id": l.control_id,
            },
        }))
    }
}

/// A refusal from an actuator's error answer.
pub(crate) fn remote_refusal(e: &wire::ActuatorError, fallback: Code) -> Refusal {
    let code = Code::parse(&e.code).unwrap_or(fallback);
    Refusal::new(
        code,
        if e.message.is_empty() {
            e.code.clone()
        } else {
            e.message.clone()
        },
    )
}

/// A refusal for a call that did not produce an answer.
pub(crate) fn call_refusal(e: &CallError, observe: bool) -> Refusal {
    match e {
        CallError::NotDelivered(m) => Refusal::new(Code::ActuatorUnavailable, m.clone()),
        CallError::TimedOut => Refusal::new(Code::Timeout, "the actuator did not answer in time"),
        CallError::Lost => Refusal::new(Code::ActuatorUnavailable, "the actuator connection ended"),
        CallError::Remote(r) => remote_refusal(
            r,
            if observe {
                Code::CaptureFailed
            } else {
                Code::InputFailed
            },
        ),
        CallError::Protocol(m) => Refusal::new(Code::UnsupportedRequest, m.clone()),
    }
}

pub(crate) fn rect_json(r: Rect) -> Value {
    json!({"x": r.x, "y": r.y, "width": r.width, "height": r.height})
}

pub(crate) fn window_json(w: &Window) -> Value {
    json!({"id": w.id, "title": w.title, "bounds": rect_json(w.bounds), "version": w.version, "modal": w.modal})
}

impl ComputerRuntime {
    /// Run a parsed call. `tool` is the tool name (for the record).
    ///
    /// # Errors
    /// A typed refusal, or an unknown outcome.
    pub async fn invoke(&self, ctx: &CallCtx, tool: &str, op: Op) -> Result<Answer, Failure> {
        match op {
            Op::Apps => self.apps(ctx).await.map_err(Failure::Refused),
            Op::Resolve { application, pid } => self
                .resolve(ctx, &application, pid)
                .await
                .map_err(Failure::Refused),
            Op::Start {
                application,
                pid,
                window,
                reason,
                scope,
            } => {
                self.start(ctx, &application, pid, window.as_deref(), &reason, scope)
                    .await
            }
            Op::Release => self.release(ctx).await.map_err(Failure::Refused),
            Op::State { window, max_nodes } => self
                .state(ctx, window.as_deref(), max_nodes)
                .await
                .map_err(Failure::Refused),
            Op::Screenshot { window } => self
                .screenshot(ctx, window.as_deref())
                .await
                .map_err(Failure::Refused),
            Op::Wait { ms, until_change } => self
                .wait(ctx, ms, until_change)
                .await
                .map_err(Failure::Refused),
            Op::Act(act) => self.act(ctx, tool, act).await,
        }
    }

    /// What an approval would bind for this call, after every refusal that
    /// can be decided without a person has been made (CUC-C01): `None` for a
    /// call that needs no approval.
    ///
    /// # Errors
    /// The refusal the call would get.
    pub async fn prepare(
        &self,
        ctx: &CallCtx,
        tool: &str,
        op: &Op,
    ) -> Result<Option<Value>, Refusal> {
        match op {
            Op::Start {
                application,
                pid,
                window,
                reason,
                scope,
            } => self
                .prepare_start(
                    ctx,
                    tool,
                    application,
                    *pid,
                    window.as_deref(),
                    reason,
                    *scope,
                )
                .await
                .map(Some),
            Op::Act(act) => self.prepare_act(ctx, tool, act).await.map(Some),
            _ => Ok(None),
        }
    }
}
