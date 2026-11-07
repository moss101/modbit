//! Trajectory metrics: a read-only reading of the task and tool-call
//! projections that says where agent work is being wasted — calls that failed,
//! identical calls repeated, tasks that ended badly — and which goals keep
//! coming back. Nothing here writes or decides: it takes the projection rows
//! the event store already holds and returns plain numbers, so the same rows
//! always yield the same report.
//!
//! A goal that recurs (the same request, digits aside, issued three or more
//! times) is the signal that a skill or an automation is missing; a task that
//! repeats one call with unchanged arguments is the signal of a loop the
//! harness did not break.

use std::collections::BTreeMap;

use modbit_domain::TaskId;
use modbit_domain::task::{Task, TaskState};
use modbit_domain::toolcall::{ToolCall, ToolCallState};

/// Times the same normalized goal must appear before it is called recurring.
pub const RECURRING_MIN: usize = 3;

/// Characters of a goal that identify it; enough to tell tasks apart, short
/// enough that a trailing detail does not split a family.
const GOAL_KEY_CHARS: usize = 60;

/// How one task's tool calls went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskMetrics {
    /// The task.
    pub task_id: TaskId,
    /// All calls the task made.
    pub calls: usize,
    /// Calls that ended `Failed`.
    pub failed_calls: usize,
    /// Calls that ended `Cancelled`.
    pub cancelled_calls: usize,
    /// Calls whose effect is unknown and must be reconciled before a retry.
    pub unknown_outcome_calls: usize,
    /// Calls beyond the first that repeat an earlier call's tool and
    /// argument hash exactly: the same question asked again.
    pub repeated_calls: usize,
    /// The task ended `Failed` or `Cancelled`.
    pub ended_badly: bool,
}

/// A goal issued [`RECURRING_MIN`] or more times.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecurringGoal {
    /// The normalized opening of the goal.
    pub key: String,
    /// The tasks that share it, in the order given.
    pub task_ids: Vec<TaskId>,
    /// How many of them ended `Failed` or `Cancelled`.
    pub ended_badly: usize,
}

/// The goal with digits collapsed to `#`, whitespace folded and case
/// dropped, cut to [`GOAL_KEY_CHARS`]. Two requests that differ only in an
/// issue number or a line count share a key.
#[must_use]
pub fn goal_key(goal: &str) -> String {
    let mut out = String::new();
    let mut last_digit = false;
    let mut last_space = true;
    for ch in goal.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_digit() {
            if !last_digit {
                out.push('#');
            }
            last_digit = true;
            last_space = false;
        } else if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
            last_digit = false;
        } else {
            out.push(ch);
            last_digit = false;
            last_space = false;
        }
    }
    out.trim_end().chars().take(GOAL_KEY_CHARS).collect()
}

fn ended_badly(state: &TaskState) -> bool {
    matches!(state, TaskState::Failed | TaskState::Cancelled)
}

/// Measure one task from its tool calls.
#[must_use]
pub fn task_metrics(task: &Task, calls: &[ToolCall]) -> TaskMetrics {
    let mut seen: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    let mut m = TaskMetrics {
        task_id: task.task_id,
        calls: calls.len(),
        failed_calls: 0,
        cancelled_calls: 0,
        unknown_outcome_calls: 0,
        repeated_calls: 0,
        ended_badly: ended_badly(&task.state),
    };
    for call in calls {
        match call.state {
            ToolCallState::Failed => m.failed_calls += 1,
            ToolCallState::Cancelled => m.cancelled_calls += 1,
            ToolCallState::UnknownOutcome => m.unknown_outcome_calls += 1,
            _ => {}
        }
        let n = seen
            .entry((call.tool_name.as_str(), call.arguments_hash.as_str()))
            .or_insert(0);
        if *n > 0 {
            m.repeated_calls += 1;
        }
        *n += 1;
    }
    m
}

/// The goals that recur at least `min` times (never fewer than two), most
/// frequent first; ties fall back to the key, so the order is deterministic.
#[must_use]
pub fn recurring_goals(tasks: &[Task], min: usize) -> Vec<RecurringGoal> {
    let mut groups: BTreeMap<String, Vec<&Task>> = BTreeMap::new();
    for task in tasks {
        let key = goal_key(&task.goal_text);
        if !key.is_empty() {
            groups.entry(key).or_default().push(task);
        }
    }
    let mut out: Vec<RecurringGoal> = groups
        .into_iter()
        .filter(|(_, v)| v.len() >= min.max(2))
        .map(|(key, v)| RecurringGoal {
            key,
            ended_badly: v.iter().filter(|t| ended_badly(&t.state)).count(),
            task_ids: v.iter().map(|t| t.task_id).collect(),
        })
        .collect();
    out.sort_by(|a, b| {
        b.task_ids
            .len()
            .cmp(&a.task_ids.len())
            .then_with(|| a.key.cmp(&b.key))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use modbit_domain::task::TaskOrigin;
    use modbit_domain::toolcall::EffectClass;
    use modbit_domain::{SessionId, Timestamp, WorkspaceId};

    fn task(goal: &str, state: TaskState) -> Task {
        Task {
            task_id: TaskId::new(),
            session_id: SessionId::new(),
            goal_text: goal.into(),
            workspace_id: WorkspaceId::new(),
            workspace_root: None,
            base_revision: None,
            execution_profile: "local_trusted".into(),
            policy_profile_id: None,
            origin: TaskOrigin::Cli,
            state,
            generation: 1,
            created_at: Timestamp(0),
            started_at: None,
            completed_at: None,
            failure_code: None,
        }
    }

    fn call(task: &Task, tool: &str, args: &str, state: ToolCallState) -> ToolCall {
        ToolCall {
            tool_call_id: modbit_domain::ToolCallId::new(),
            task_id: task.task_id,
            step_id: None,
            tool_name: tool.into(),
            tool_version: "1".into(),
            effect_class: EffectClass::ReadOnly,
            capability_lease_id: None,
            arguments_hash: args.into(),
            state,
            generation: 1,
            dispatched_at: None,
            completed_at: None,
            result_ref: None,
            unknown_outcome_reason: None,
            policy_decision: None,
            approval_id: None,
            run_id: None,
            turn_id: None,
            call_id: None,
            arguments_ref: None,
        }
    }

    #[test]
    fn digits_case_and_spacing_do_not_split_a_goal() {
        assert_eq!(
            goal_key("Fix  issue 1234 in   Parser"),
            goal_key("fix issue 7 in parser")
        );
        assert_eq!(goal_key("fix issue 1234"), "fix issue #");
        assert_ne!(goal_key("fix the parser"), goal_key("fix the lexer"));
        assert_eq!(goal_key("   "), "");
    }

    #[test]
    fn goal_key_is_cut_on_a_char_boundary() {
        let long = "修".repeat(200);
        assert_eq!(goal_key(&long).chars().count(), GOAL_KEY_CHARS);
    }

    #[test]
    fn metrics_count_failures_and_exact_repeats_only() {
        let t = task("g", TaskState::Failed);
        let calls = [
            call(&t, "fs.read", "h1", ToolCallState::Succeeded),
            call(&t, "fs.read", "h1", ToolCallState::Succeeded), // repeat
            call(&t, "fs.read", "h2", ToolCallState::Succeeded), // new args
            call(&t, "shell.exec", "h1", ToolCallState::Failed), // other tool
            call(&t, "shell.exec", "h1", ToolCallState::Failed), // repeat
            call(&t, "shell.exec", "h1", ToolCallState::Cancelled), // repeat
            call(&t, "fs.write", "h9", ToolCallState::UnknownOutcome),
        ];
        let m = task_metrics(&t, &calls);
        assert_eq!(m.calls, 7);
        assert_eq!(m.failed_calls, 2);
        assert_eq!(m.cancelled_calls, 1);
        assert_eq!(m.unknown_outcome_calls, 1);
        assert_eq!(m.repeated_calls, 3);
        assert!(m.ended_badly);
        let ok = task_metrics(&task("g", TaskState::Completed), &[]);
        assert_eq!((ok.calls, ok.repeated_calls, ok.ended_badly), (0, 0, false));
    }

    #[test]
    fn recurring_goals_need_the_minimum_and_rank_by_count() {
        let tasks = vec![
            task("Fix flaky test 12", TaskState::Failed),
            task("fix flaky test 99", TaskState::Completed),
            task("fix flaky test 5", TaskState::Cancelled),
            task("bump deps", TaskState::Completed),
            task("bump deps", TaskState::Completed),
            task("write a haiku", TaskState::Completed),
            task("", TaskState::Completed),
            task("", TaskState::Completed),
            task("", TaskState::Completed),
        ];
        let r = recurring_goals(&tasks, RECURRING_MIN);
        assert_eq!(r.len(), 1, "{r:?}");
        assert_eq!(r[0].key, "fix flaky test #");
        assert_eq!(r[0].task_ids.len(), 3);
        assert_eq!(r[0].ended_badly, 2);
        // A lower bar admits the pair, ranked after the triple; never below 2.
        let r = recurring_goals(&tasks, 0);
        assert_eq!(
            r.iter().map(|g| g.key.as_str()).collect::<Vec<_>>(),
            ["fix flaky test #", "bump deps"]
        );
    }
}
