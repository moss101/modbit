//! The automation ledger: definitions, enable approvals, kill switches and
//! run history as events, and the fold that turns them into state (AUT-A01,
//! AUT-E01). The Core appends these events to its one event store and keeps
//! a [`Registry`] folded from them; nothing here schedules, dispatches or
//! executes. A rebuilt registry equals the live one, so a restart loses
//! nothing (docs/13 event-sourced aggregates).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::definition::{Definition, Effects};

/// Where a definition came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    /// Authored through the Core by a person.
    Local,
    /// Supplied by a file in a repository: data, never enabled until an
    /// owner approves the exact content (AUT-A04).
    Repository {
        /// The repository root the file was read from.
        root: String,
        /// The file, relative to the root (`.modbit/automations/x.json`).
        path: String,
        /// The repository revision when it was read.
        revision: String,
        /// sha256 of the file's bytes: a changed byte changes it.
        source_sha256: String,
    },
}

/// The status of a run record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Recorded, not yet dispatched (a restart finishes the dispatch).
    Pending,
    /// Held by the concurrency policy until the active run ends.
    Queued,
    /// A task is running.
    Running,
    /// The task completed.
    Succeeded,
    /// The task failed or a limit stopped it.
    Failed,
    /// Nothing ran; the reason says why.
    Skipped,
    /// Stopped by a person, a kill switch, a replacement or an expiry.
    Cancelled,
}

impl RunStatus {
    /// Whether the run is over.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Skipped | Self::Cancelled
        )
    }

    /// Whether the run holds a slot of the concurrency policy.
    #[must_use]
    pub fn is_active(self) -> bool {
        matches!(self, Self::Pending | Self::Running)
    }
}

/// The fields every record of a firing carries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Firing {
    /// The definition.
    pub automation_id: String,
    /// Its version at the time.
    pub version: u32,
    /// The trigger that fired.
    pub trigger_id: String,
    /// `schedule`, `manual`, `event`, `webhook`.
    pub trigger_kind: String,
    /// The delivery id or the schedule slot id.
    pub event_id: String,
    /// `sha256(automation_id | version | event_id)`: one run per key.
    pub dispatch_key: String,
    /// For a schedule, the slot.
    #[serde(default)]
    pub slot_ms: Option<i64>,
    /// This run stands for a missed window.
    #[serde(default)]
    pub catch_up: bool,
    /// How many missed slots it stands for.
    #[serde(default)]
    pub missed: u64,
    /// Whose authority the run takes (`user:...` / `service:...`).
    pub principal: String,
    /// A test run (dry-run posture): protected effects are denied.
    #[serde(default)]
    pub test: bool,
    /// sha256 of the trigger payload, when there was one.
    #[serde(default)]
    pub payload_sha256: Option<String>,
    /// Instruction-shaped passages found in the payload.
    #[serde(default)]
    pub findings: u32,
    /// The resolved typed inputs.
    #[serde(default)]
    pub inputs: serde_json::Value,
    /// When.
    pub at_ms: i64,
}

/// An event of the automation ledger.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum AutomationEvent {
    /// A version of a definition exists (creation, an edit, a repository
    /// reload). A new version is disabled until an owner approves its hash.
    AutomationDefined {
        /// Identity.
        automation_id: String,
        /// Version, from 1.
        version: u32,
        /// The definition.
        definition: Box<Definition>,
        /// Its canonical hash.
        definition_hash: String,
        /// Where it came from.
        source: Source,
        /// The workspace the runs act in.
        workspace_root: String,
        /// Who defined it.
        created_by: String,
        /// When.
        at_ms: i64,
    },
    /// An owner approved this exact version and hash, listing what it may
    /// do. The definition is enabled from here (AUT-D03).
    AutomationEnableApproved {
        /// Identity.
        automation_id: String,
        /// Version.
        version: u32,
        /// The hash approved.
        definition_hash: String,
        /// The effect ceiling listed.
        effects: Effects,
        /// The capabilities listed.
        capabilities: Vec<String>,
        /// The paths listed.
        paths: Vec<String>,
        /// The hosts listed.
        hosts: Vec<String>,
        /// For a repository definition: the bytes' hash approved.
        #[serde(default)]
        source_sha256: Option<String>,
        /// For a repository definition: the revision approved.
        #[serde(default)]
        repo_revision: Option<String>,
        /// Who approved.
        approver: String,
        /// Interval schedules count from here.
        anchor_ms: i64,
        /// When.
        at_ms: i64,
    },
    /// The definition stopped being enabled.
    AutomationDisabled {
        /// Identity.
        automation_id: String,
        /// A stable code (`OWNER`, `CONSECUTIVE_FAILURES`, `SOURCE_CHANGED`, ...).
        reason: String,
        /// In words.
        detail: String,
        /// When.
        at_ms: i64,
    },
    /// A pause switch: one definition, or all (`automation_id` absent).
    AutomationPauseSet {
        /// The definition, or `None` for the global switch.
        automation_id: Option<String>,
        /// On or off.
        paused: bool,
        /// Who.
        by: String,
        /// Why.
        note: String,
        /// When.
        at_ms: i64,
    },
    /// A firing was recorded; the dispatch follows (and is finished by a
    /// restart if the Core dies in between).
    AutomationFired {
        /// The firing.
        #[serde(flatten)]
        firing: Firing,
        /// `pending` or `queued`.
        initial: RunStatus,
    },
    /// The firing became a task.
    AutomationDispatched {
        /// The key.
        dispatch_key: String,
        /// The task.
        task_id: String,
        /// The run's own session.
        session_id: String,
        /// When.
        at_ms: i64,
    },
    /// A firing produced no run, with a typed reason (duplicate, paused,
    /// budget, concurrency, missed, policy, filter, expired).
    AutomationRunSkipped {
        /// The firing.
        #[serde(flatten)]
        firing: Firing,
        /// The typed reason.
        reason: String,
        /// In words.
        detail: String,
    },
    /// A run ended.
    AutomationRunFinished {
        /// The key.
        dispatch_key: String,
        /// `succeeded`, `failed` or `cancelled`.
        status: RunStatus,
        /// The typed reason (`TASK_COMPLETED`, `BUDGET_EXHAUSTED`,
        /// `APPROVAL_EXPIRED`, `KILLED`, `REPLACED`, ...).
        reason: String,
        /// In words.
        detail: String,
        /// Cost the run incurred, minor units, when known.
        #[serde(default)]
        cost_minor: Option<u64>,
        /// The run's typed outputs.
        #[serde(default)]
        outputs: serde_json::Value,
        /// When.
        at_ms: i64,
    },
    /// Schedule slots were missed and recorded once (not replayed).
    AutomationSlotsSkipped {
        /// Identity.
        automation_id: String,
        /// Version.
        version: u32,
        /// The trigger.
        trigger_id: String,
        /// How many.
        count: u64,
        /// First.
        first_ms: i64,
        /// Last.
        last_ms: i64,
        /// The cursor after.
        through_ms: i64,
        /// When.
        at_ms: i64,
    },
    /// A person saw a failed or parked run.
    AutomationAttentionAcknowledged {
        /// The key.
        dispatch_key: String,
        /// Who.
        by: String,
        /// When.
        at_ms: i64,
    },
}

/// Every event type name of the ledger (used to read them back).
pub const EVENT_TYPES: &[&str] = &[
    "AutomationDefined",
    "AutomationEnableApproved",
    "AutomationDisabled",
    "AutomationPauseSet",
    "AutomationFired",
    "AutomationDispatched",
    "AutomationRunSkipped",
    "AutomationRunFinished",
    "AutomationSlotsSkipped",
    "AutomationAttentionAcknowledged",
];

impl AutomationEvent {
    /// The event's type name.
    #[must_use]
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::AutomationDefined { .. } => "AutomationDefined",
            Self::AutomationEnableApproved { .. } => "AutomationEnableApproved",
            Self::AutomationDisabled { .. } => "AutomationDisabled",
            Self::AutomationPauseSet { .. } => "AutomationPauseSet",
            Self::AutomationFired { .. } => "AutomationFired",
            Self::AutomationDispatched { .. } => "AutomationDispatched",
            Self::AutomationRunSkipped { .. } => "AutomationRunSkipped",
            Self::AutomationRunFinished { .. } => "AutomationRunFinished",
            Self::AutomationSlotsSkipped { .. } => "AutomationSlotsSkipped",
            Self::AutomationAttentionAcknowledged { .. } => "AutomationAttentionAcknowledged",
        }
    }
}

/// One stored version of a definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    /// Version number.
    pub version: u32,
    /// The definition.
    pub definition: Definition,
    /// Its hash.
    pub hash: String,
    /// Where it came from.
    pub source: Source,
    /// The workspace.
    pub workspace_root: String,
    /// Who defined it.
    pub created_by: String,
    /// When.
    pub created_ms: i64,
}

/// The enable approval in force.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Enabled {
    /// The version approved.
    pub version: u32,
    /// The hash approved.
    pub hash: String,
    /// The effect ceiling listed.
    pub effects: Effects,
    /// Capabilities listed.
    pub capabilities: Vec<String>,
    /// Paths listed.
    pub paths: Vec<String>,
    /// Hosts listed.
    pub hosts: Vec<String>,
    /// The bytes' hash approved, for a repository definition.
    pub source_sha256: Option<String>,
    /// The revision approved.
    pub repo_revision: Option<String>,
    /// Who approved.
    pub approver: String,
    /// Interval anchor.
    pub anchor_ms: i64,
    /// When.
    pub approved_ms: i64,
}

/// One definition's state.
#[derive(Clone, Debug, Default)]
pub struct State {
    /// Identity.
    pub id: String,
    /// Every version, oldest first.
    pub versions: Vec<Version>,
    /// The enable approval in force, if any (an edit clears it).
    pub enabled: Option<Enabled>,
    /// Per-definition kill switch.
    pub paused: bool,
    /// Why it was last disabled.
    pub disabled_reason: Option<(String, String)>,
    /// Schedule cursors by trigger id: every slot up to here is handled.
    pub cursors: BTreeMap<String, i64>,
    /// Consecutive failed runs (a success resets it).
    pub consecutive_failures: u32,
}

impl State {
    /// The newest version.
    #[must_use]
    pub fn current(&self) -> Option<&Version> {
        self.versions.last()
    }

    /// The version a run would use right now: the enabled one, if the
    /// approval still names the current version and hash.
    #[must_use]
    pub fn live(&self) -> Option<&Version> {
        let e = self.enabled.as_ref()?;
        let v = self.versions.iter().find(|v| v.version == e.version)?;
        (v.hash == e.hash && self.current().is_some_and(|c| c.version == e.version)).then_some(v)
    }
}

/// One run record.
#[derive(Clone, Debug)]
pub struct Run {
    /// The firing it came from.
    pub firing: Firing,
    /// Status.
    pub status: RunStatus,
    /// Typed reason for a skip, failure or cancellation.
    pub reason: String,
    /// In words.
    pub detail: String,
    /// The task, once dispatched.
    pub task_id: Option<String>,
    /// The run's session.
    pub session_id: Option<String>,
    /// Dispatch time.
    pub dispatched_ms: Option<i64>,
    /// End time.
    pub finished_ms: Option<i64>,
    /// Cost.
    pub cost_minor: Option<u64>,
    /// Typed outputs.
    pub outputs: serde_json::Value,
    /// A person has seen it.
    pub acknowledged: bool,
    /// Arrival order, for queues.
    pub seq: u64,
}

/// The folded ledger.
#[derive(Clone, Debug, Default)]
pub struct Registry {
    /// Definitions by id.
    pub defs: BTreeMap<String, State>,
    /// Global kill switch.
    pub global_paused: bool,
    /// Run records by dispatch key.
    pub runs: BTreeMap<String, Run>,
    next_seq: u64,
}

impl Registry {
    /// Fold one event. Folding is deterministic and idempotent per key: a
    /// second firing with a dispatch key already recorded changes nothing.
    pub fn apply(&mut self, e: &AutomationEvent) {
        match e {
            AutomationEvent::AutomationDefined {
                automation_id,
                version,
                definition,
                definition_hash,
                source,
                workspace_root,
                created_by,
                at_ms,
            } => {
                let st = self.defs.entry(automation_id.clone()).or_default();
                st.id.clone_from(automation_id);
                if st.versions.iter().any(|v| v.version == *version) {
                    return;
                }
                st.versions.push(Version {
                    version: *version,
                    definition: (**definition).clone(),
                    hash: definition_hash.clone(),
                    source: source.clone(),
                    workspace_root: workspace_root.clone(),
                    created_by: created_by.clone(),
                    created_ms: *at_ms,
                });
                // An edit creates a version that is disabled until re-approved.
                if st.enabled.as_ref().is_some_and(|en| en.version != *version) {
                    st.enabled = None;
                    st.disabled_reason = Some((
                        "EDITED".into(),
                        format!("version {version} needs its own approval"),
                    ));
                }
            }
            AutomationEvent::AutomationEnableApproved {
                automation_id,
                version,
                definition_hash,
                effects,
                capabilities,
                paths,
                hosts,
                source_sha256,
                repo_revision,
                approver,
                anchor_ms,
                at_ms,
            } => {
                if let Some(st) = self.defs.get_mut(automation_id) {
                    st.enabled = Some(Enabled {
                        version: *version,
                        hash: definition_hash.clone(),
                        effects: *effects,
                        capabilities: capabilities.clone(),
                        paths: paths.clone(),
                        hosts: hosts.clone(),
                        source_sha256: source_sha256.clone(),
                        repo_revision: repo_revision.clone(),
                        approver: approver.clone(),
                        anchor_ms: *anchor_ms,
                        approved_ms: *at_ms,
                    });
                    st.disabled_reason = None;
                    st.consecutive_failures = 0;
                    // Slots before the approval are not owed.
                    for c in st.cursors.values_mut() {
                        *c = (*c).max(*anchor_ms);
                    }
                }
            }
            AutomationEvent::AutomationDisabled {
                automation_id,
                reason,
                detail,
                ..
            } => {
                if let Some(st) = self.defs.get_mut(automation_id) {
                    st.enabled = None;
                    st.disabled_reason = Some((reason.clone(), detail.clone()));
                }
            }
            AutomationEvent::AutomationPauseSet {
                automation_id,
                paused,
                ..
            } => match automation_id {
                Some(id) => {
                    if let Some(st) = self.defs.get_mut(id) {
                        st.paused = *paused;
                    }
                }
                None => self.global_paused = *paused,
            },
            AutomationEvent::AutomationFired { firing, initial } => {
                if self.runs.contains_key(&firing.dispatch_key) {
                    return;
                }
                if let Some(st) = self.defs.get_mut(&firing.automation_id)
                    && let Some(slot) = firing.slot_ms
                {
                    let c = st.cursors.entry(firing.trigger_id.clone()).or_insert(slot);
                    *c = (*c).max(slot);
                }
                self.insert_run(firing.clone(), *initial, String::new(), String::new());
            }
            AutomationEvent::AutomationRunSkipped {
                firing,
                reason,
                detail,
            } => {
                if self.runs.contains_key(&firing.dispatch_key) {
                    return;
                }
                if let Some(st) = self.defs.get_mut(&firing.automation_id)
                    && let Some(slot) = firing.slot_ms
                {
                    let c = st.cursors.entry(firing.trigger_id.clone()).or_insert(slot);
                    *c = (*c).max(slot);
                }
                self.insert_run(
                    firing.clone(),
                    RunStatus::Skipped,
                    reason.clone(),
                    detail.clone(),
                );
            }
            AutomationEvent::AutomationDispatched {
                dispatch_key,
                task_id,
                session_id,
                at_ms,
            } => {
                if let Some(r) = self.runs.get_mut(dispatch_key)
                    && !r.status.is_terminal()
                {
                    r.status = RunStatus::Running;
                    r.task_id = Some(task_id.clone());
                    r.session_id = Some(session_id.clone());
                    r.dispatched_ms = Some(*at_ms);
                }
            }
            AutomationEvent::AutomationRunFinished {
                dispatch_key,
                status,
                reason,
                detail,
                cost_minor,
                outputs,
                at_ms,
            } => {
                let Some(r) = self.runs.get_mut(dispatch_key) else {
                    return;
                };
                if r.status.is_terminal() {
                    return;
                }
                r.status = *status;
                r.reason.clone_from(reason);
                r.detail.clone_from(detail);
                r.cost_minor = *cost_minor;
                r.outputs = outputs.clone();
                r.finished_ms = Some(*at_ms);
                let id = r.firing.automation_id.clone();
                if let Some(st) = self.defs.get_mut(&id) {
                    match status {
                        RunStatus::Failed => st.consecutive_failures += 1,
                        RunStatus::Succeeded => st.consecutive_failures = 0,
                        _ => {}
                    }
                }
            }
            AutomationEvent::AutomationSlotsSkipped {
                automation_id,
                version,
                trigger_id,
                count,
                first_ms,
                last_ms,
                through_ms,
                at_ms,
            } => {
                if let Some(st) = self.defs.get_mut(automation_id) {
                    let c = st.cursors.entry(trigger_id.clone()).or_insert(*through_ms);
                    *c = (*c).max(*through_ms);
                }
                // One history row for the whole window: missed slots are
                // never replayed, and never silent.
                let event_id = format!("missed:{trigger_id}@{first_ms}..{last_ms}");
                let key = dispatch_key(automation_id, *version, &event_id);
                if !self.runs.contains_key(&key) {
                    let firing = Firing {
                        automation_id: automation_id.clone(),
                        version: *version,
                        trigger_id: trigger_id.clone(),
                        trigger_kind: "schedule".into(),
                        event_id,
                        dispatch_key: key,
                        slot_ms: None,
                        catch_up: false,
                        missed: *count,
                        principal: String::new(),
                        test: false,
                        payload_sha256: None,
                        findings: 0,
                        inputs: serde_json::Value::Null,
                        at_ms: *at_ms,
                    };
                    self.insert_run(
                        firing,
                        RunStatus::Skipped,
                        "MISSED".into(),
                        format!("{count} scheduled slot(s) passed while the Core was not running; not replayed"),
                    );
                }
            }
            AutomationEvent::AutomationAttentionAcknowledged { dispatch_key, .. } => {
                if let Some(r) = self.runs.get_mut(dispatch_key) {
                    r.acknowledged = true;
                }
            }
        }
    }

    fn insert_run(&mut self, firing: Firing, status: RunStatus, reason: String, detail: String) {
        self.next_seq += 1;
        let finished = status.is_terminal().then_some(firing.at_ms);
        self.runs.insert(
            firing.dispatch_key.clone(),
            Run {
                firing,
                status,
                reason,
                detail,
                task_id: None,
                session_id: None,
                dispatched_ms: None,
                finished_ms: finished,
                cost_minor: None,
                outputs: serde_json::Value::Null,
                acknowledged: false,
                seq: self.next_seq,
            },
        );
    }

    /// Runs of a definition, newest first.
    #[must_use]
    pub fn runs_of(&self, automation_id: &str) -> Vec<&Run> {
        let mut v: Vec<&Run> = self
            .runs
            .values()
            .filter(|r| r.firing.automation_id == automation_id)
            .collect();
        v.sort_by_key(|r| std::cmp::Reverse(r.seq));
        v
    }

    /// Runs holding a slot of the definition's concurrency policy.
    #[must_use]
    pub fn active_runs(&self, automation_id: &str) -> Vec<&Run> {
        self.runs_of(automation_id)
            .into_iter()
            .filter(|r| r.status.is_active())
            .collect()
    }

    /// Queued runs of a definition, oldest first.
    #[must_use]
    pub fn queued_runs(&self, automation_id: &str) -> Vec<&Run> {
        let mut v: Vec<&Run> = self
            .runs
            .values()
            .filter(|r| r.firing.automation_id == automation_id && r.status == RunStatus::Queued)
            .collect();
        v.sort_by_key(|r| r.seq);
        v
    }

    /// Runs started since `since_ms` (not skipped), for the rate limit.
    #[must_use]
    pub fn started_since(&self, automation_id: Option<&str>, since_ms: i64) -> usize {
        self.runs
            .values()
            .filter(|r| {
                !matches!(r.status, RunStatus::Skipped)
                    && r.firing.at_ms >= since_ms
                    && automation_id.is_none_or(|a| r.firing.automation_id == a)
            })
            .count()
    }
}

/// `sha256(automation_id | version | event_id)` as hex: the key under which
/// a firing creates at most one run (AUT-B05).
#[must_use]
pub fn dispatch_key(automation_id: &str, version: u32, event_id: &str) -> String {
    crate::definition::sha256_hex(format!("{automation_id}\n{version}\n{event_id}").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::parse_and_validate;

    fn def() -> Definition {
        parse_and_validate(
            &serde_json::json!({
                "schema": "modbit.automation/1",
                "name": "d",
                "prompt": "p",
                "triggers": [{"kind": "manual", "id": "m"}]
            })
            .to_string(),
        )
        .unwrap()
    }

    fn defined(version: u32) -> AutomationEvent {
        let d = def();
        AutomationEvent::AutomationDefined {
            automation_id: "a1".into(),
            version,
            definition_hash: d.hash(),
            definition: Box::new(d),
            source: Source::Local,
            workspace_root: "/w".into(),
            created_by: "user:x".into(),
            at_ms: 1,
        }
    }

    fn approve(version: u32) -> AutomationEvent {
        AutomationEvent::AutomationEnableApproved {
            automation_id: "a1".into(),
            version,
            definition_hash: def().hash(),
            effects: Effects::ReadOnly,
            capabilities: vec![],
            paths: vec![],
            hosts: vec![],
            source_sha256: None,
            repo_revision: None,
            approver: "user:x".into(),
            anchor_ms: 5,
            at_ms: 5,
        }
    }

    fn firing(event_id: &str) -> Firing {
        Firing {
            automation_id: "a1".into(),
            version: 1,
            trigger_id: "m".into(),
            trigger_kind: "manual".into(),
            event_id: event_id.into(),
            dispatch_key: dispatch_key("a1", 1, event_id),
            slot_ms: None,
            catch_up: false,
            missed: 0,
            principal: "user:x".into(),
            test: false,
            payload_sha256: None,
            findings: 0,
            inputs: serde_json::Value::Null,
            at_ms: 10,
        }
    }

    #[test]
    fn an_edit_disables_until_the_new_version_is_approved() {
        let mut r = Registry::default();
        r.apply(&defined(1));
        r.apply(&approve(1));
        assert!(r.defs["a1"].live().is_some());
        r.apply(&defined(2));
        assert!(r.defs["a1"].live().is_none());
        assert_eq!(r.defs["a1"].disabled_reason.as_ref().unwrap().0, "EDITED");
        r.apply(&approve(2));
        assert_eq!(r.defs["a1"].live().unwrap().version, 2);
    }

    #[test]
    fn a_firing_key_creates_one_run() {
        let mut r = Registry::default();
        r.apply(&defined(1));
        let f = AutomationEvent::AutomationFired {
            firing: firing("e1"),
            initial: RunStatus::Pending,
        };
        r.apply(&f);
        r.apply(&f);
        assert_eq!(r.runs.len(), 1);
        r.apply(&AutomationEvent::AutomationDispatched {
            dispatch_key: dispatch_key("a1", 1, "e1"),
            task_id: "t".into(),
            session_id: "s".into(),
            at_ms: 11,
        });
        assert_eq!(r.runs_of("a1")[0].status, RunStatus::Running);
        let fin = AutomationEvent::AutomationRunFinished {
            dispatch_key: dispatch_key("a1", 1, "e1"),
            status: RunStatus::Failed,
            reason: "X".into(),
            detail: String::new(),
            cost_minor: Some(3),
            outputs: serde_json::Value::Null,
            at_ms: 12,
        };
        r.apply(&fin);
        r.apply(&fin);
        assert_eq!(r.defs["a1"].consecutive_failures, 1);
    }

    #[test]
    fn replaying_the_same_events_rebuilds_the_same_state() {
        let events = vec![
            defined(1),
            approve(1),
            AutomationEvent::AutomationFired {
                firing: firing("e1"),
                initial: RunStatus::Pending,
            },
            AutomationEvent::AutomationPauseSet {
                automation_id: None,
                paused: true,
                by: "u".into(),
                note: String::new(),
                at_ms: 20,
            },
        ];
        let mut a = Registry::default();
        let mut b = Registry::default();
        for e in &events {
            a.apply(e);
            let round: AutomationEvent =
                serde_json::from_str(&serde_json::to_string(e).unwrap()).unwrap();
            b.apply(&round);
        }
        assert_eq!(a.runs.len(), b.runs.len());
        assert_eq!(a.global_paused, b.global_paused);
        assert!(b.global_paused);
    }
}
