//! PX-085 (docs/68, AUT-B03, AUT-B05): the cloud time source. The worker's
//! existing claim loop is the only time evaluator: each pass reads the
//! database clock, claims due schedule triggers with `FOR UPDATE SKIP
//! LOCKED` (so workers racing on a slot produce one firing), applies the
//! definition's missed-run policy through the same `plan` arithmetic the
//! local Core uses, and creates the tasks of the firings that are to run.
//! There is no scheduler here beyond that loop.

use modbit_automation::definition::{MissedPolicy, parse_and_validate};
use modbit_automation::schedule::{Spec, plan};
use modbit_event_store::cloud::CloudStore;
use modbit_event_store::cloud::automation::{
    DueSchedule, ScheduleDecision, ScheduleFire, TickReport,
};

/// How late a slot may be and still count as on time, in milliseconds.
pub const GRACE_MS: i64 = 90_000;

/// Firings pending for longer than this are finished by the next pass: what
/// a process that died between recording and creating the task left.
const RECOVERY_AFTER_MS: i64 = 5_000;

/// What to fire for one claimed, due trigger: the definition's schedule, the
/// cursor, and its missed-run policy, through `plan`.
fn decide(due: &DueSchedule, now_ms: i64) -> ScheduleDecision {
    let nothing = |through| ScheduleDecision {
        fires: vec![],
        skipped: None,
        through_ms: through,
        next_due_ms: None,
    };
    let Ok(def) = parse_and_validate(&due.definition_json) else {
        return nothing(due.cursor_ms);
    };
    let Some(trigger) = def.trigger(&due.trigger_id) else {
        return nothing(due.cursor_ms);
    };
    let Some(spec) = Spec::of(trigger, due.anchor_ms) else {
        return nothing(due.cursor_ms);
    };
    let policy = if due.controls.missed_policy == "run_once" {
        MissedPolicy::RunOnce
    } else {
        MissedPolicy::Skip
    };
    let p = plan(
        &spec,
        due.cursor_ms,
        now_ms,
        GRACE_MS,
        policy,
        due.controls.catch_up_window_ms,
    );
    ScheduleDecision {
        fires: p
            .fire
            .iter()
            .map(|s| ScheduleFire {
                slot_ms: s.slot_ms,
                catch_up: s.catch_up,
                missed_before: s.missed_before,
            })
            .collect(),
        skipped: match (p.skipped, p.skipped_first_ms, p.skipped_last_ms) {
            (n, Some(first), Some(last)) if n > 0 => Some((first, last, n)),
            _ => None,
        },
        through_ms: p.through_ms,
        next_due_ms: p.next_due_ms,
    }
}

/// One pass of the cloud time source at `now_ms`: evaluate the due
/// schedules, let queued firings through, finish the tasks of firings that
/// are to run. Exposed so a test (or a second worker) can drive it with a
/// controlled clock; the worker's loop calls it with the database clock.
///
/// # Errors
/// The store failed.
pub async fn automation_tick(store: &CloudStore, now_ms: i64) -> anyhow::Result<TickReport> {
    let report = store
        .automation_schedule_tick(now_ms, |due| decide(due, now_ms))
        .await?;
    for key in &report.pending {
        store.dispatch_firing(key).await?;
    }
    for key in store.promote_queued_firings(now_ms).await? {
        store.dispatch_firing(&key).await?;
    }
    for key in store
        .pending_firings(now_ms - RECOVERY_AFTER_MS, 50)
        .await?
    {
        store.dispatch_firing(&key).await?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use modbit_event_store::cloud::automation::RunControls;

    fn due(missed: &str, cursor_ms: i64) -> DueSchedule {
        DueSchedule {
            automation_id: uuid::Uuid::nil(),
            tenant: modbit_domain::TenantId::new(),
            version: 1,
            trigger_id: "hourly".into(),
            anchor_ms: 0,
            cursor_ms,
            definition_json: serde_json::json!({
                "schema": "modbit.automation/1", "name": "t", "prompt": "p",
                "triggers": [{"kind": "schedule", "id": "hourly", "cron": "0 * * * *"}],
            })
            .to_string(),
            controls: RunControls {
                prompt: "p".into(),
                effects: "read_only".into(),
                profile: "plan".into(),
                concurrency: "skip".into(),
                queue_max: 3,
                max_runs_per_hour: 12,
                daily_budget_minor: None,
                max_turns: 40,
                max_tool_calls: 200,
                max_cost_minor: None,
                max_wall_ms: 1_800_000,
                approval_wait_minutes: 1440,
                missed_policy: missed.into(),
                catch_up_window_ms: 86_400_000,
            },
        }
    }

    #[test]
    fn an_on_time_slot_fires_and_a_downtime_follows_the_policy_once() {
        const H: i64 = 3_600_000;
        // 1970-01-01 01:00 is a slot; the cursor is the epoch.
        let on_time = decide(&due("skip", 0), H + 1_000);
        assert_eq!(on_time.fires.len(), 1);
        assert_eq!(on_time.fires[0].slot_ms, H);
        assert_eq!(on_time.skipped, None);
        assert_eq!(on_time.next_due_ms, Some(2 * H));
        // Five hours of downtime: skip records the window once, fires nothing.
        let skip = decide(&due("skip", 0), 5 * H + 1_800_000);
        assert!(skip.fires.is_empty());
        assert_eq!(skip.skipped.map(|s| s.2), Some(5));
        // run_once fires the newest slot once and records the rest.
        let once = decide(&due("run_once", 0), 5 * H + 1_800_000);
        assert_eq!(once.fires.len(), 1);
        assert!(once.fires[0].catch_up);
        assert_eq!(once.skipped.map(|s| s.2), Some(4));
    }
}
