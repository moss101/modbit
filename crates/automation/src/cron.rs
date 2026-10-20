//! A five-field cron expression in UTC (docs/68 AUT-B01): minute, hour,
//! day of month, month, day of week. Supports `*`, lists, ranges, steps and
//! month / weekday names. Parsing is total and bounded; nothing is executed.
//!
//! Day of month and day of week follow the classic rule: when both are
//! restricted a day matches if either does.

use serde::{Deserialize, Serialize};

const MINUTE_MS: i64 = 60_000;
const DAY_MS: i64 = 86_400_000;

/// A parsed expression.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cron {
    minutes: u64,
    hours: u32,
    dom: u32,
    months: u16,
    dow: u8,
    dom_star: bool,
    dow_star: bool,
}

/// Why an expression was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CronError(pub String);

impl std::fmt::Display for CronError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn err<T>(m: impl Into<String>) -> Result<T, CronError> {
    Err(CronError(m.into()))
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];
const DAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

fn value(tok: &str, lo: u32, hi: u32, names: &[&str], name_base: u32) -> Result<u32, CronError> {
    if let Ok(n) = tok.parse::<u32>() {
        return if (lo..=hi).contains(&n) {
            Ok(n)
        } else {
            err(format!("`{tok}` is outside {lo}-{hi}"))
        };
    }
    let lower = tok.to_ascii_lowercase();
    match names.iter().position(|n| *n == lower) {
        Some(i) => Ok(i as u32 + name_base),
        None => err(format!("`{tok}` is not a number or a name")),
    }
}

/// Bits set for one field. `dow` accepts 0-7 (both 0 and 7 are Sunday).
fn field(
    text: &str,
    lo: u32,
    hi: u32,
    names: &[&str],
    name_base: u32,
) -> Result<(u64, bool), CronError> {
    if text.is_empty() {
        return err("an empty field");
    }
    let mut bits = 0u64;
    let mut star = false;
    for part in text.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => {
                let step: u32 = s
                    .parse()
                    .map_err(|_| CronError(format!("`{s}` is not a step")))?;
                if step == 0 || step > hi.max(1) {
                    return err(format!("step `{s}` is outside 1-{hi}"));
                }
                (r, step)
            }
            None => (part, 1),
        };
        let (from, to) = if range == "*" {
            if step == 1 && text == "*" {
                star = true;
            }
            (lo, hi)
        } else if let Some((a, b)) = range.split_once('-') {
            let (a, b) = (
                value(a, lo, hi, names, name_base)?,
                value(b, lo, hi, names, name_base)?,
            );
            if a > b {
                return err(format!("the range `{range}` runs backwards"));
            }
            (a, b)
        } else {
            let v = value(range, lo, hi, names, name_base)?;
            // `5/10` means from 5 to the end of the field.
            if step > 1 { (v, hi) } else { (v, v) }
        };
        let mut v = from;
        while v <= to {
            bits |= 1u64 << v;
            v += step;
        }
    }
    Ok((bits, star))
}

impl Cron {
    /// Parse five space-separated fields.
    ///
    /// # Errors
    /// A typed reason when the text is not a five-field expression.
    pub fn parse(text: &str) -> Result<Self, CronError> {
        if text.len() > 128 {
            return err("an expression is at most 128 bytes");
        }
        let fields: Vec<&str> = text.split_whitespace().collect();
        if fields.len() != 5 {
            return err(format!(
                "expected five fields (minute hour day-of-month month day-of-week), found {}",
                fields.len()
            ));
        }
        let (minutes, _) = field(fields[0], 0, 59, &[], 0)?;
        let (hours, _) = field(fields[1], 0, 23, &[], 0)?;
        let (dom, dom_star) = field(fields[2], 1, 31, &[], 0)?;
        let (months, _) = field(fields[3], 1, 12, &MONTHS, 1)?;
        let (mut dow, dow_star) = field(fields[4], 0, 7, &DAYS, 0)?;
        if dow & (1 << 7) != 0 {
            dow = (dow | 1) & !(1 << 7);
        }
        Ok(Self {
            minutes,
            hours: hours as u32,
            dom: dom as u32,
            months: months as u16,
            dow: dow as u8,
            dom_star,
            dow_star,
        })
    }

    fn day_matches(&self, month: u32, day: u32, days_since_epoch: i64) -> bool {
        if self.months & (1 << month) == 0 {
            return false;
        }
        let weekday = (days_since_epoch + 4).rem_euclid(7) as u8; // 1970-01-01 was a Thursday
        let dom_hit = self.dom & (1 << day) != 0;
        let dow_hit = self.dow & (1 << weekday) != 0;
        match (self.dom_star, self.dow_star) {
            (true, true) => true,
            (false, true) => dom_hit,
            (true, false) => dow_hit,
            (false, false) => dom_hit || dow_hit,
        }
    }

    /// The first firing strictly after `after_ms` (milliseconds since the
    /// epoch, UTC), or `None` when none exists within eight years.
    #[must_use]
    pub fn next_after(&self, after_ms: i64) -> Option<i64> {
        let start_minute = after_ms.div_euclid(MINUTE_MS) + 1;
        let first_day = (start_minute * MINUTE_MS).div_euclid(DAY_MS);
        for day in first_day..first_day + 366 * 8 {
            let (_, m, d) = civil_from_days(day);
            if !self.day_matches(m, d, day) {
                continue;
            }
            let day_start_minute = day * 1440;
            for hour in 0..24u32 {
                if self.hours & (1 << hour) == 0 {
                    continue;
                }
                for minute in 0..60u32 {
                    if self.minutes & (1 << minute) == 0 {
                        continue;
                    }
                    let at = day_start_minute + i64::from(hour) * 60 + i64::from(minute);
                    if at >= start_minute {
                        return Some(at * MINUTE_MS);
                    }
                }
            }
        }
        None
    }

    /// The shortest gap, in minutes, between two consecutive firings over a
    /// leap year, or `None` when it never fires twice. Used for the floor of
    /// one run per five minutes.
    #[must_use]
    pub fn min_gap_minutes(&self) -> Option<i64> {
        // 2024-01-01T00:00:00Z, a leap year.
        let origin = 1_704_067_200_000i64;
        let mut prev: Option<i64> = None;
        let mut best: Option<i64> = None;
        let mut cursor = origin - 1;
        for _ in 0..200_000 {
            let Some(at) = self.next_after(cursor) else {
                break;
            };
            if at >= origin + 366 * DAY_MS {
                break;
            }
            if let Some(p) = prev {
                let gap = (at - p) / MINUTE_MS;
                best = Some(best.map_or(gap, |b| b.min(gap)));
                if gap < 5 {
                    return best;
                }
            }
            prev = Some(at);
            cursor = at;
        }
        best
    }
}

/// Days since 1970-01-01 to (year, month 1-12, day 1-31).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// (year, month, day, hour, minute) of a UTC instant, for display and tests.
#[must_use]
pub fn utc_parts(ms: i64) -> (i64, u32, u32, u32, u32) {
    let days = ms.div_euclid(DAY_MS);
    let (y, m, d) = civil_from_days(days);
    let minute_of_day = ms.rem_euclid(DAY_MS) / MINUTE_MS;
    (
        y,
        m,
        d,
        (minute_of_day / 60) as u32,
        (minute_of_day % 60) as u32,
    )
}

/// Milliseconds since the epoch of a UTC civil time (tests and fixtures).
#[must_use]
pub fn utc_ms(year: i64, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = i64::from(if month > 2 { month - 3 } else { month + 9 });
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * DAY_MS + i64::from(hour) * 3_600_000 + i64::from(minute) * MINUTE_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_firings_follow_the_expression() {
        let c = Cron::parse("30 3 * * *").unwrap();
        let t = utc_ms(2026, 10, 8, 12, 0);
        assert_eq!(c.next_after(t), Some(utc_ms(2026, 10, 9, 3, 30)));
        assert_eq!(
            c.next_after(utc_ms(2026, 10, 9, 3, 30)),
            Some(utc_ms(2026, 10, 10, 3, 30))
        );
        let every_15 = Cron::parse("*/15 * * * *").unwrap();
        assert_eq!(every_15.next_after(t), Some(utc_ms(2026, 10, 8, 12, 15)));
        assert_eq!(every_15.min_gap_minutes(), Some(15));
    }

    #[test]
    fn day_of_week_names_and_either_rule() {
        let mon = Cron::parse("0 9 * * MON").unwrap();
        // 2026-10-08 is a Thursday; the next Monday is the 12th.
        assert_eq!(
            mon.next_after(utc_ms(2026, 10, 8, 0, 0)),
            Some(utc_ms(2026, 10, 12, 9, 0))
        );
        // dom and dow both restricted: either matches (the 13th, or a Friday).
        let either = Cron::parse("0 0 13 * 5").unwrap();
        assert_eq!(
            either.next_after(utc_ms(2026, 10, 8, 0, 0)),
            Some(utc_ms(2026, 10, 9, 0, 0))
        );
        let sunday7 = Cron::parse("0 0 * * 7").unwrap();
        assert_eq!(
            sunday7.next_after(utc_ms(2026, 10, 8, 0, 0)),
            Some(utc_ms(2026, 10, 11, 0, 0))
        );
    }

    #[test]
    fn the_floor_is_measured_not_guessed() {
        assert_eq!(Cron::parse("* * * * *").unwrap().min_gap_minutes(), Some(1));
        assert_eq!(
            Cron::parse("0,2 * * * *").unwrap().min_gap_minutes(),
            Some(2)
        );
        assert_eq!(
            Cron::parse("0 * * * *").unwrap().min_gap_minutes(),
            Some(60)
        );
    }

    #[test]
    fn malformed_expressions_are_refused() {
        for bad in [
            "",
            "* * * *",
            "* * * * * *",
            "60 * * * *",
            "* 24 * * *",
            "* * 0 * *",
            "* * * 13 *",
            "5-1 * * * *",
            "*/0 * * * *",
            "a b c d e",
            "@daily",
        ] {
            assert!(Cron::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn civil_arithmetic_round_trips() {
        for ms in [0, utc_ms(2024, 2, 29, 23, 59), utc_ms(2026, 12, 31, 0, 0)] {
            let (y, m, d, h, mi) = utc_parts(ms);
            assert_eq!(utc_ms(y, m, d, h, mi), ms);
        }
    }
}
