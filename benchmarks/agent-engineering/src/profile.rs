//! Declarative harness profiles for the Adaptive Profile Evaluator
//! (EXPERIMENT; IMP-EV-0244 task-conditioned generation, IMP-EV-0246
//! bounded repair; docs/41 "Experiments — not production commitments").
//!
//! A profile is what the Eval Harness hands the headless CLI for one trial:
//! the run's budgets and the skills it selects — nothing the product's
//! policy, tools or prompts see as special. Variants exist only here: no
//! production crate depends on this one (`tools/architecture-lint`), so a
//! generated or repaired profile can shape an evaluation trial and nothing
//! else. A bundle measured under a variant is a `shadow:` experiment and is
//! refused as a baseline (`bundle.rs`).
//!
//! Hypotheses under test, not claims: that conditioning the budgets on the
//! task's tier, difficulty and language, and repairing a failing variant a
//! bounded number of times, finds a cheaper profile that verifies as often
//! as the pinned known-good one. The exit is an ADR with measured benefit,
//! or removal.

use serde::{Deserialize, Serialize};

use crate::suite::{SuiteProtocol, TaskSpec};

/// The pinned known-good profile's id: the frozen `direct` protocol.
pub const KNOWN_GOOD: &str = "direct/known-good";

/// Most bounded repairs of one variant (REQ-EV-0246).
pub const MAX_PROFILE_REPAIRS: u32 = 2;

/// Bounds every profile is validated against.
const TURNS: std::ops::RangeInclusive<u32> = 1..=64;
const TOOL_CALLS: std::ops::RangeInclusive<u32> = 0..=256;
const NO_PROGRESS: std::ops::RangeInclusive<u32> = 0..=12;
/// The Core's own no-progress budget (`RepairPolicy::default`), which a
/// profile's 0 leaves in force.
const CORE_NO_PROGRESS: u32 = 3;

/// A declarative trial profile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessProfile {
    /// `direct/known-good`, `generated/<task>` or `repaired/<task>/<n>`.
    pub id: String,
    /// The profile it was generated or repaired from (its digest).
    pub derived_from: Option<String>,
    /// Model turns the run may take.
    pub max_turns: u32,
    /// Tool calls the run may make (0: the Core's default).
    pub max_tool_calls: u32,
    /// Turns without progress before the run escalates (0: the Core's
    /// default).
    pub max_no_progress_turns: u32,
    /// Skills the trial selects explicitly.
    pub skills: Vec<String>,
    /// Repairs applied so far (0 for generated and known-good profiles).
    pub repairs: u32,
    /// Why each field is what it is.
    pub rationale: Vec<String>,
}

/// Why a profile or a repair is refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProfileRefused {
    /// A budget outside its bound.
    #[error("{field} = {value} is outside {min}..={max}")]
    OutOfBounds {
        /// Field.
        field: &'static str,
        /// Value.
        value: u32,
        /// Lower bound.
        min: u32,
        /// Upper bound.
        max: u32,
    },
    /// A skill name that is not a plain name.
    #[error("skill `{0}` is not a plain name")]
    BadSkill(String),
    /// The variant has been repaired as often as allowed.
    #[error(
        "repair {attempt} refused: at most {MAX_PROFILE_REPAIRS} repairs; run the known-good fallback"
    )]
    RepairLimit {
        /// The attempt that was refused.
        attempt: u32,
    },
    /// The known-good profile is never repaired: it is the fallback.
    #[error("the known-good profile is the fallback and is not repaired")]
    KnownGood,
}

impl HarnessProfile {
    /// The pinned fallback: the frozen protocol's budgets and nothing else.
    #[must_use]
    pub fn known_good(protocol: &SuiteProtocol) -> Self {
        Self {
            id: KNOWN_GOOD.into(),
            derived_from: None,
            max_turns: protocol.max_turns,
            max_tool_calls: 0,
            max_no_progress_turns: 0,
            skills: vec![],
            repairs: 0,
            rationale: vec![
                "the frozen protocol's max_turns; the Core's defaults otherwise".into(),
            ],
        }
    }

    /// Digest over the profile's canonical JSON.
    #[must_use]
    pub fn digest(&self) -> String {
        crate::sha256_hex(&serde_json::to_vec(self).unwrap_or_default())
    }

    /// Whether this is the pinned fallback.
    #[must_use]
    pub fn is_known_good(&self) -> bool {
        self.id == KNOWN_GOOD
    }

    /// Check every field against its bound.
    ///
    /// # Errors
    /// [`ProfileRefused`] naming the first field out of bounds.
    pub fn validate(&self) -> Result<(), ProfileRefused> {
        for (field, value, range) in [
            ("max_turns", self.max_turns, &TURNS),
            ("max_tool_calls", self.max_tool_calls, &TOOL_CALLS),
            (
                "max_no_progress_turns",
                self.max_no_progress_turns,
                &NO_PROGRESS,
            ),
        ] {
            if !range.contains(&value) {
                return Err(ProfileRefused::OutOfBounds {
                    field,
                    value,
                    min: *range.start(),
                    max: *range.end(),
                });
            }
        }
        if let Some(bad) = self.skills.iter().find(|s| {
            s.is_empty()
                || !s
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        }) {
            return Err(ProfileRefused::BadSkill(bad.clone()));
        }
        Ok(())
    }

    /// The `task run` flags this profile sets; a default (0) is not passed,
    /// so the known-good profile's flags are the frozen protocol's.
    #[must_use]
    pub fn run_args(&self) -> Vec<String> {
        let mut a = vec!["--max-turns".to_owned(), self.max_turns.to_string()];
        if self.max_no_progress_turns > 0 {
            a.extend([
                "--max-no-progress-turns".to_owned(),
                self.max_no_progress_turns.to_string(),
            ]);
        }
        if self.max_tool_calls > 0 {
            a.extend([
                "--max-tool-calls".to_owned(),
                self.max_tool_calls.to_string(),
            ]);
        }
        for s in &self.skills {
            a.extend(["--skill".to_owned(), s.clone()]);
        }
        a
    }

    /// How this run is labelled in an experiment report.
    #[must_use]
    pub fn configuration(&self) -> String {
        if self.is_known_good() {
            "direct".into()
        } else {
            format!("shadow:{}", self.digest())
        }
    }
}

fn effective_no_progress(p: &HarnessProfile) -> u32 {
    if p.max_no_progress_turns == 0 {
        CORE_NO_PROGRESS
    } else {
        p.max_no_progress_turns
    }
}

/// A task-conditioned variant of `base` (IMP-EV-0244): deterministic — the
/// same task and base give the same profile, byte for byte. Easy tasks get a
/// tighter turn budget, hard or tier-C tasks a looser one and
/// more patience; nothing widens authority (budgets and skill selection
/// only).
#[must_use]
pub fn generate(task: &TaskSpec, base: &HarnessProfile) -> HarnessProfile {
    let mut p = HarnessProfile {
        id: format!("generated/{}", task.id),
        derived_from: Some(base.digest()),
        repairs: 0,
        rationale: vec![],
        ..base.clone()
    };
    let hard = task.difficulty == "hard" || task.tier == "C";
    let easy = task.difficulty == "easy" && !hard;
    if easy {
        p.max_turns = (base.max_turns / 2).max(*TURNS.start());
        p.max_no_progress_turns = 2;
        p.rationale.push(format!(
            "tier {} {}: half the turns, less patience",
            task.tier, task.difficulty
        ));
    } else if hard {
        p.max_turns = (base.max_turns + base.max_turns / 2).min(*TURNS.end());
        p.max_no_progress_turns = (effective_no_progress(base) + 1).min(*NO_PROGRESS.end());
        p.rationale.push(format!(
            "tier {} {}: half again the turns, more patience",
            task.tier, task.difficulty
        ));
    } else {
        p.rationale
            .push("neither easy nor hard: the base budgets".into());
    }
    if task.language == "python" {
        p.rationale
            .push("python: no language-specific change".into());
    }
    p
}

/// Repair a failing variant once (IMP-EV-0246), from the failure features
/// the harness observed (`budget:max_turns`, `boundary:no_progress`): each
/// repair moves one bound by a fixed step and never widens authority. The
/// third repair is refused; the caller runs the known-good fallback.
///
/// # Errors
/// [`ProfileRefused::RepairLimit`] past [`MAX_PROFILE_REPAIRS`];
/// [`ProfileRefused::KnownGood`] for the fallback itself.
pub fn repair(p: &HarnessProfile, features: &[String]) -> Result<HarnessProfile, ProfileRefused> {
    if p.is_known_good() {
        return Err(ProfileRefused::KnownGood);
    }
    let attempt = p.repairs + 1;
    if attempt > MAX_PROFILE_REPAIRS {
        return Err(ProfileRefused::RepairLimit { attempt });
    }
    // The task id itself may hold slashes: `generated/<task>` and
    // `repaired/<task>/<n>` name it whole.
    let task = if let Some(t) = p.id.strip_prefix("generated/") {
        t.to_owned()
    } else if let Some(t) = p.id.strip_prefix("repaired/") {
        t.rsplit_once('/').map_or(t, |(task, _)| task).to_owned()
    } else {
        p.id.clone()
    };
    let mut r = HarnessProfile {
        id: format!("repaired/{task}/{attempt}"),
        derived_from: Some(p.digest()),
        repairs: attempt,
        ..p.clone()
    };
    if features.iter().any(|f| f == "boundary:no_progress") {
        r.max_no_progress_turns = (effective_no_progress(&r) + 1).min(*NO_PROGRESS.end());
        r.rationale
            .push(format!("repair {attempt}: one more turn of patience"));
    } else {
        r.max_turns = (r.max_turns + 2).min(*TURNS.end());
        r.rationale
            .push(format!("repair {attempt}: two more turns"));
    }
    r.validate()?;
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn protocol() -> SuiteProtocol {
        serde_json::from_value(serde_json::json!({
            "trials": 1, "max_turns": 24, "question_answer": "first", "approvals": "approve"
        }))
        .unwrap()
    }

    fn task(tier: &str, difficulty: &str) -> TaskSpec {
        serde_json::from_value(serde_json::json!({
            "id": "t", "fixture": "f", "language": "python", "tier": tier, "difficulty": difficulty,
            "goal": "g", "setup": [], "acceptance": {"commands": [["true"]]}
        }))
        .unwrap()
    }

    #[test]
    fn a_variant_is_conditioned_on_the_task_deterministic_and_within_bounds() {
        let base = HarnessProfile::known_good(&protocol());
        base.validate().unwrap();
        assert_eq!(
            base.run_args(),
            ["--max-turns", "24"],
            "the frozen protocol's flags"
        );
        let easy = generate(&task("A", "easy"), &base);
        let hard = generate(&task("C", "hard"), &base);
        assert_eq!((easy.max_turns, hard.max_turns), (12, 36));
        assert_eq!(easy, generate(&task("A", "easy"), &base), "deterministic");
        assert_eq!(easy.derived_from.as_deref(), Some(base.digest().as_str()));
        assert!(easy.configuration().starts_with("shadow:"));
        assert_eq!(base.configuration(), "direct");
        easy.validate().unwrap();
        let mut bad = easy.clone();
        bad.max_turns = 0;
        assert!(matches!(
            bad.validate(),
            Err(ProfileRefused::OutOfBounds {
                field: "max_turns",
                ..
            })
        ));
        bad = easy.clone();
        bad.skills = vec!["../x".into()];
        assert!(matches!(bad.validate(), Err(ProfileRefused::BadSkill(_))));
    }

    #[test]
    fn a_variant_is_repaired_twice_and_the_third_repair_is_refused() {
        let base = HarnessProfile::known_good(&protocol());
        let v = generate(&task("A", "easy"), &base);
        let r1 = repair(&v, &["budget:max_turns".into()]).unwrap();
        let r2 = repair(&r1, &["boundary:no_progress".into()]).unwrap();
        assert_eq!((r1.max_turns, r1.repairs), (14, 1));
        assert_eq!(
            (r1.id.as_str(), r2.id.as_str()),
            ("repaired/t/1", "repaired/t/2")
        );
        assert_eq!((r2.max_no_progress_turns, r2.repairs), (3, 2));
        assert_eq!(
            repair(&r2, &[]),
            Err(ProfileRefused::RepairLimit { attempt: 3 })
        );
        assert_eq!(repair(&base, &[]), Err(ProfileRefused::KnownGood));
    }
}
