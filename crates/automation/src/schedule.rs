//! Schedule evaluation (AUT-B01, AUT-B03): which slots fire, which are
//! skipped as missed, and where the cursor moves to. Pure arithmetic over a
//! time the caller supplies, so the Core's clock can be a real one, a
//! monotonic-safe one, or a controlled one in a test.
//!
//! Missed slots are never replayed in bulk: at most one run results from a
//! whole missed window (`run_once`), or none (`skip`), and the missed window
//! is recorded once.

use crate::cron::Cron;
use crate::definition::{MissedPolicy, Trigger};

/// A schedule trigger, ready to evaluate.
#[derive(Clone, Debug)]
pub enum Spec {
    /// A cron expression.
    Cron(Cron),
    /// A fixed interval from an anchor (the time the definition was enabled).
    Every {
        /// Interval in milliseconds.
        interval_ms: i64,
        /// Slots fall at `anchor + n * interval`.
        anchor_ms: i64,
    },
}

impl Spec {
    /// The spec of a schedule trigger, anchored at `anchor_ms` for intervals.
    /// `None` for any other trigger or an expression that does not parse.
    #[must_use]
    pub fn of(trigger: &Trigger, anchor_ms: i64) -> Option<Self> {
        match trigger {
            Trigger::Schedule {
                cron: Some(c),
                every_minutes: None,
                ..
            } => Cron::parse(c).ok().map(Self::Cron),
            Trigger::Schedule {
                cron: None,
                every_minutes: Some(m),
                ..
            } => Some(Self::Every {
                interval_ms: i64::from(*m) * 60_000,
                anchor_ms,
            }),
            _ => None,
        }
    }

    /// The first slot strictly after `after_ms`.
    #[must_use]
    pub fn next_after(&self, after_ms: i64) -> Option<i64> {
        match self {
            Self::Cron(c) => c.next_after(after_ms),
            Self::Every {
                interval_ms,
                anchor_ms,
            } => {
                if after_ms < *anchor_ms {
                    Some(anchor_ms + interval_ms)
                } else {
                    Some(anchor_ms + ((after_ms - anchor_ms) / interval_ms + 1) * interval_ms)
                }
            }
        }
    }
}

/// One slot to fire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    /// The slot's instant (UTC, ms).
    pub slot_ms: i64,
    /// This is the one run that stands for a missed window.
    pub catch_up: bool,
    /// How many earlier missed slots it stands for (0 when on time).
    pub missed_before: u64,
}

/// The result of evaluating a schedule at an instant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// Slots to fire now (at most a handful; usually one).
    pub fire: Vec<Slot>,
    /// Missed slots recorded as skipped, as a count and a window.
    pub skipped: u64,
    /// First skipped slot.
    pub skipped_first_ms: Option<i64>,
    /// Last skipped slot.
    pub skipped_last_ms: Option<i64>,
    /// The cursor after this plan: every slot up to here has been handled.
    pub through_ms: i64,
    /// The next slot after `now`.
    pub next_due_ms: Option<i64>,
}

/// Most slots examined in one evaluation (a very long downtime of a
/// five-minute schedule is counted, not enumerated forever).
pub const MAX_SLOTS_EXAMINED: u64 = 50_000;

/// Evaluate `spec` at `now_ms`, having handled every slot up to `cursor_ms`.
///
/// A slot is on time when `now - slot <= grace_ms`; an older one was missed.
/// `catch_up_window_ms` bounds how far back a missed slot still earns the
/// single catch-up run.
#[must_use]
pub fn plan(
    spec: &Spec,
    cursor_ms: i64,
    now_ms: i64,
    grace_ms: i64,
    policy: MissedPolicy,
    catch_up_window_ms: i64,
) -> Plan {
    let mut on_time = Vec::new();
    let mut missed: Vec<i64> = Vec::new();
    let mut missed_count = 0u64;
    let mut first_missed = None;
    let mut last_missed = None;
    let mut through = cursor_ms;
    let mut examined = 0u64;
    let mut at = cursor_ms;
    while let Some(slot) = spec.next_after(at) {
        if slot > now_ms || examined >= MAX_SLOTS_EXAMINED {
            break;
        }
        examined += 1;
        through = slot;
        at = slot;
        if now_ms - slot <= grace_ms {
            on_time.push(slot);
        } else {
            missed_count += 1;
            first_missed.get_or_insert(slot);
            last_missed = Some(slot);
            // Only the newest few are kept to choose a catch-up from.
            missed.push(slot);
            if missed.len() > 2 {
                missed.remove(0);
            }
        }
    }
    let next_due_ms = spec.next_after(now_ms.max(through));
    let mut fire: Vec<Slot> = on_time
        .iter()
        .map(|s| Slot {
            slot_ms: *s,
            catch_up: false,
            missed_before: 0,
        })
        .collect();
    let mut skipped = missed_count;
    if policy == MissedPolicy::RunOnce
        && fire.is_empty()
        && let Some(latest) = last_missed
        && now_ms - latest <= catch_up_window_ms
    {
        fire.push(Slot {
            slot_ms: latest,
            catch_up: true,
            missed_before: missed_count - 1,
        });
        skipped = missed_count - 1;
    }
    let (skipped_first_ms, skipped_last_ms) = if skipped == 0 {
        (None, None)
    } else if fire.iter().any(|s| s.catch_up) {
        // Everything before the catch-up slot.
        (
            first_missed,
            missed.iter().rev().nth(1).copied().or(first_missed),
        )
    } else {
        (first_missed, last_missed)
    };
    Plan {
        fire,
        skipped,
        skipped_first_ms,
        skipped_last_ms,
        through_ms: through,
        next_due_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cron::utc_ms;

    fn hourly() -> Spec {
        Spec::Cron(Cron::parse("0 * * * *").unwrap())
    }

    #[test]
    fn an_on_time_slot_fires_once_and_the_cursor_moves() {
        let cursor = utc_ms(2026, 10, 8, 9, 0);
        let now = utc_ms(2026, 10, 8, 10, 0) + 1_000;
        let p = plan(
            &hourly(),
            cursor,
            now,
            90_000,
            MissedPolicy::Skip,
            86_400_000,
        );
        assert_eq!(p.fire.len(), 1);
        assert_eq!(p.fire[0].slot_ms, utc_ms(2026, 10, 8, 10, 0));
        assert!(!p.fire[0].catch_up);
        assert_eq!(p.skipped, 0);
        assert_eq!(p.through_ms, utc_ms(2026, 10, 8, 10, 0));
        assert_eq!(p.next_due_ms, Some(utc_ms(2026, 10, 8, 11, 0)));
        // Evaluating again at the same instant fires nothing.
        let again = plan(
            &hourly(),
            p.through_ms,
            now,
            90_000,
            MissedPolicy::Skip,
            86_400_000,
        );
        assert!(again.fire.is_empty());
        assert_eq!(again.skipped, 0);
    }

    #[test]
    fn a_downtime_skips_the_window_once_or_catches_up_once() {
        let cursor = utc_ms(2026, 10, 8, 1, 0);
        let now = utc_ms(2026, 10, 8, 6, 30);
        let skip = plan(
            &hourly(),
            cursor,
            now,
            90_000,
            MissedPolicy::Skip,
            86_400_000,
        );
        assert!(skip.fire.is_empty());
        assert_eq!(skip.skipped, 5);
        assert_eq!(skip.skipped_first_ms, Some(utc_ms(2026, 10, 8, 2, 0)));
        assert_eq!(skip.skipped_last_ms, Some(utc_ms(2026, 10, 8, 6, 0)));
        assert_eq!(skip.through_ms, utc_ms(2026, 10, 8, 6, 0));

        let once = plan(
            &hourly(),
            cursor,
            now,
            90_000,
            MissedPolicy::RunOnce,
            86_400_000,
        );
        assert_eq!(once.fire.len(), 1);
        assert!(once.fire[0].catch_up);
        assert_eq!(once.fire[0].slot_ms, utc_ms(2026, 10, 8, 6, 0));
        assert_eq!(once.fire[0].missed_before, 4);
        assert_eq!(once.skipped, 4);
        assert_eq!(once.skipped_last_ms, Some(utc_ms(2026, 10, 8, 5, 0)));

        // Past the catch-up bound nothing runs.
        let late = plan(
            &hourly(),
            cursor,
            now,
            90_000,
            MissedPolicy::RunOnce,
            60_000,
        );
        assert!(late.fire.is_empty());
        assert_eq!(late.skipped, 5);
    }

    #[test]
    fn a_long_downtime_is_bounded_not_replayed() {
        let spec = Spec::Every {
            interval_ms: 5 * 60_000,
            anchor_ms: 0,
        };
        let now = 400 * 86_400_000;
        let p = plan(&spec, 0, now, 90_000, MissedPolicy::RunOnce, 86_400_000);
        assert!(p.fire.len() <= 1);
        assert!(p.skipped <= MAX_SLOTS_EXAMINED);
    }

    #[test]
    fn interval_slots_fall_on_the_anchor_grid() {
        let spec = Spec::Every {
            interval_ms: 600_000,
            anchor_ms: 1_000,
        };
        assert_eq!(spec.next_after(0), Some(601_000));
        assert_eq!(spec.next_after(601_000), Some(1_201_000));
        assert_eq!(spec.next_after(601_001), Some(1_201_000));
    }

    #[test]
    fn a_clock_that_went_backwards_fires_nothing() {
        let p = plan(
            &hourly(),
            utc_ms(2026, 10, 8, 10, 0),
            utc_ms(2026, 10, 8, 9, 0),
            90_000,
            MissedPolicy::RunOnce,
            86_400_000,
        );
        assert!(p.fire.is_empty());
        assert_eq!(p.skipped, 0);
        assert_eq!(p.through_ms, utc_ms(2026, 10, 8, 10, 0));
    }
}
