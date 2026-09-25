//! The cloud SLO event ladder (IMP-EV-0023, REQ-EV-0023; docs/34
//! "Metrics": sandbox provisioning duration, model first-token latency;
//! docs/71 "sandbox provisioning p95"). A cloud task's start is recorded as
//! rungs on its log — `REQUESTED`, `PREWARM`, `SANDBOX_REQUESTED`,
//! `SANDBOX_READY`, `FIRST_TOKEN`, `FIRST_TOOL` — and this derives, per
//! start, the latencies between them and whether it was cold or warm. A rung
//! that was never reached is absent, never zero.

use serde::{Deserialize, Serialize};

/// Rung names, in ladder order.
pub const STAGES: [&str; 6] = [
    "REQUESTED",
    "PREWARM",
    "SANDBOX_REQUESTED",
    "SANDBOX_READY",
    "FIRST_TOKEN",
    "FIRST_TOOL",
];

/// One recorded rung.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage {
    /// One of [`STAGES`].
    pub stage: String,
    /// When (ms since the epoch).
    pub at_ms: i64,
    /// The run, once known.
    pub run_id: Option<String>,
    /// For `PREWARM` and `SANDBOX_READY`.
    pub warm: Option<bool>,
    /// What the rung saw.
    pub detail: String,
}

/// One start of a task: its rungs and what they add up to.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Start {
    /// The run the start became (from its first run-bearing rung).
    pub run_id: Option<String>,
    /// Rung timestamps.
    pub requested_at_ms: Option<i64>,
    /// What the warm pool offered (`NONE` when there is none).
    pub prewarm: Option<String>,
    /// Asked the gateway for a sandbox.
    pub sandbox_requested_at_ms: Option<i64>,
    /// The sandbox was ready.
    pub sandbox_ready_at_ms: Option<i64>,
    /// The model's first output.
    pub first_token_at_ms: Option<i64>,
    /// The first tool dispatch.
    pub first_tool_at_ms: Option<i64>,
    /// The task's own sandbox was reused (`Some(true)`), newly provisioned
    /// (`Some(false)`), or never reached (`None`).
    pub warm: Option<bool>,
    /// Sandbox requested → ready.
    pub provision_ms: Option<u64>,
    /// Requested → sandbox ready.
    pub ready_ms: Option<u64>,
    /// Requested → first token.
    pub first_token_ms: Option<u64>,
    /// Requested → first tool.
    pub first_tool_ms: Option<u64>,
}

/// Latency figures over a set of starts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Figures {
    /// Starts counted.
    pub starts: u64,
    /// Requested → sandbox ready: median and largest.
    pub ready_p50_ms: Option<u64>,
    /// Largest.
    pub ready_max_ms: Option<u64>,
    /// Requested → first token: median and largest.
    pub first_token_p50_ms: Option<u64>,
    /// Largest.
    pub first_token_max_ms: Option<u64>,
}

/// Cold and warm figures.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    /// Starts that provisioned a sandbox.
    pub cold: Figures,
    /// Starts that reused the task's sandbox.
    pub warm: Figures,
}

fn span(from: Option<i64>, to: Option<i64>) -> Option<u64> {
    let (a, b) = (from?, to?);
    u64::try_from(b - a).ok()
}

/// Group rungs, in log order, into starts: each `REQUESTED` opens one; a
/// rung before any `REQUESTED` (a log from before the ladder) is ignored;
/// only the first of each rung per start counts.
#[must_use]
pub fn ladder(stages: &[Stage]) -> Vec<Start> {
    let mut out: Vec<Start> = Vec::new();
    for s in stages {
        if s.stage == "REQUESTED" {
            out.push(Start {
                requested_at_ms: Some(s.at_ms),
                run_id: s.run_id.clone(),
                ..Start::default()
            });
            continue;
        }
        let Some(cur) = out.last_mut() else {
            continue;
        };
        if cur.run_id.is_none() {
            cur.run_id.clone_from(&s.run_id);
        }
        match s.stage.as_str() {
            "PREWARM" if cur.prewarm.is_none() => cur.prewarm = Some(s.detail.clone()),
            "SANDBOX_REQUESTED" if cur.sandbox_requested_at_ms.is_none() => {
                cur.sandbox_requested_at_ms = Some(s.at_ms);
            }
            "SANDBOX_READY" if cur.sandbox_ready_at_ms.is_none() => {
                cur.sandbox_ready_at_ms = Some(s.at_ms);
                cur.warm = s.warm;
            }
            "FIRST_TOKEN" if cur.first_token_at_ms.is_none() => {
                cur.first_token_at_ms = Some(s.at_ms);
            }
            "FIRST_TOOL" if cur.first_tool_at_ms.is_none() => cur.first_tool_at_ms = Some(s.at_ms),
            _ => {}
        }
    }
    for s in &mut out {
        s.provision_ms = span(s.sandbox_requested_at_ms, s.sandbox_ready_at_ms);
        s.ready_ms = span(s.requested_at_ms, s.sandbox_ready_at_ms);
        s.first_token_ms = span(s.requested_at_ms, s.first_token_at_ms);
        s.first_tool_ms = span(s.requested_at_ms, s.first_tool_at_ms);
    }
    out
}

fn figures<'a>(starts: impl Iterator<Item = &'a Start>) -> Figures {
    let starts: Vec<&Start> = starts.collect();
    let stat = |mut v: Vec<u64>| -> (Option<u64>, Option<u64>) {
        v.sort_unstable();
        (
            v.get((v.len().saturating_sub(1)) / 2).copied(),
            v.last().copied(),
        )
    };
    let (ready_p50_ms, ready_max_ms) = stat(starts.iter().filter_map(|s| s.ready_ms).collect());
    let (first_token_p50_ms, first_token_max_ms) =
        stat(starts.iter().filter_map(|s| s.first_token_ms).collect());
    Figures {
        starts: starts.len() as u64,
        ready_p50_ms,
        ready_max_ms,
        first_token_p50_ms,
        first_token_max_ms,
    }
}

/// Cold and warm figures over a ladder.
#[must_use]
pub fn summary(starts: &[Start]) -> Summary {
    Summary {
        cold: figures(starts.iter().filter(|s| s.warm == Some(false))),
        warm: figures(starts.iter().filter(|s| s.warm == Some(true))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(stage: &str, at: i64, run: Option<&str>, warm: Option<bool>) -> Stage {
        Stage {
            stage: stage.into(),
            at_ms: at,
            run_id: run.map(str::to_owned),
            warm,
            detail: if stage == "PREWARM" {
                "NONE".into()
            } else {
                String::new()
            },
        }
    }

    #[test]
    fn a_ladder_derives_each_starts_latencies_and_separates_cold_from_warm() {
        let stages = vec![
            st("FIRST_TOKEN", 1, Some("old"), None), // before any REQUESTED: ignored
            st("REQUESTED", 1_000, None, None),
            st("PREWARM", 1_000, None, Some(false)),
            st("SANDBOX_REQUESTED", 1_010, None, None),
            st("SANDBOX_READY", 1_900, None, Some(false)),
            st("FIRST_TOKEN", 2_300, Some("r1"), None),
            st("FIRST_TOOL", 2_500, Some("r1"), None),
            st("FIRST_TOKEN", 2_600, Some("r1"), None), // a later token: ignored
            st("REQUESTED", 5_000, None, None),
            st("PREWARM", 5_000, None, Some(false)),
            st("SANDBOX_REQUESTED", 5_001, None, None),
            st("SANDBOX_READY", 5_001, None, Some(true)),
            st("FIRST_TOKEN", 5_200, Some("r2"), None),
        ];
        let l = ladder(&stages);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].run_id.as_deref(), Some("r1"));
        assert_eq!(
            (
                l[0].provision_ms,
                l[0].ready_ms,
                l[0].first_token_ms,
                l[0].first_tool_ms
            ),
            (Some(890), Some(900), Some(1_300), Some(1_500))
        );
        assert_eq!(
            (l[0].warm, l[0].prewarm.as_deref()),
            (Some(false), Some("NONE"))
        );
        assert_eq!(
            (l[1].warm, l[1].ready_ms, l[1].first_tool_ms),
            (Some(true), Some(1), None)
        );
        let s = summary(&l);
        assert_eq!((s.cold.starts, s.cold.first_token_max_ms), (1, Some(1_300)));
        assert_eq!((s.warm.starts, s.warm.first_token_p50_ms), (1, Some(200)));
    }
}
