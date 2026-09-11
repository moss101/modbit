//! The Request Profiler (REQ-EPR-003, docs/27 and docs/38): what a request
//! and its repository intrinsically demand, and how likely the product is to
//! meet the floor on it — measured, bounded and catalog-independent.
//!
//! Three boundaries define it.
//!
//! Features are intrinsic. They come from the request text and the repository
//! in front of it, and from nothing else: no price, no provider, no cache
//! state and no allowlist reaches the extractor, so a configuration change
//! cannot move a feature. The extractor takes only `RequestFacts`, which has
//! no field for any of them.
//!
//! Extraction is bounded and deterministic. The goal text is read up to a
//! fixed ceiling and the repository up to a fixed number of paths; there is no
//! model call anywhere in it. The same inputs always produce the same features
//! and the same digest.
//!
//! A probability is only as good as the cohort behind it. `p_floor_success`
//! comes from a versioned calibration cohort of recorded outcomes, shrunk
//! toward a conservative prior by how little support the slice has, and a
//! slice the cohort has not seen is out of distribution and says so. When the
//! profiler is disabled, or the repository metadata is stale or missing, the
//! answer is the conservative profile rather than a guess.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Feature schema version, pinned in every profile.
pub const PROFILER_VERSION: &str = "profiler-1";

/// Bytes of goal text the extractor reads.
pub const GOAL_CEILING_BYTES: usize = 8 * 1024;

/// Repository paths the extractor reads.
pub const PATH_CEILING: usize = 2_000;

/// Support below which a slice is out of distribution: the cohort has not seen
/// enough of it to say anything.
pub const OOD_SUPPORT: u32 = 5;

/// Support at which a slice's own rate is trusted without shrinkage.
pub const FULL_SUPPORT: u32 = 50;

/// The conservative prior a thin slice is shrunk toward. It is deliberately
/// low: absent evidence, the product does not claim it will meet the floor.
pub const CONSERVATIVE_PRIOR: f64 = 0.2;

/// What a request is asking for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskDemand {
    /// Fix behaviour that is wrong.
    BugFix,
    /// Add behaviour that is not there.
    Feature,
    /// Change structure without changing behaviour.
    Refactor,
    /// Add or change tests.
    Test,
    /// Change prose.
    Docs,
    /// Answer something without changing the repository.
    Question,
}

/// What the repository is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainDemand {
    /// Systems code.
    Systems,
    /// Web or application code.
    Web,
    /// Scripting or services.
    Scripting,
    /// Text and data.
    Text,
    /// Nothing the extractor recognises.
    Unknown,
}

/// What makes a request harder than its kind alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModifierDemand {
    /// It names more than one file, or the repository is large.
    CrossFile,
    /// It asks for evidence: a test, a check, a reproduction.
    NeedsVerification,
    /// It is under-specified: no file, no symbol, no acceptance.
    Underspecified,
    /// It touches something the repository marks as sensitive.
    Sensitive,
}

/// What the extractor is allowed to see. There is deliberately no field here
/// for a price, a provider, a cache or an allowlist.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RequestFacts<'a> {
    /// The request text.
    pub goal: &'a str,
    /// Repository-relative paths, as the index has them.
    pub paths: Vec<String>,
    /// The languages the index identified, with a file count each.
    pub languages: Vec<(String, u32)>,
    /// Whether the repository has a configured verification command.
    pub has_configured_verification: bool,
    /// Whether the repository metadata is current. Stale or missing metadata
    /// is not a feature: it is a reason to fall back.
    pub metadata_fresh: bool,
}

/// The intrinsic features of a request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Features {
    /// What is being asked for.
    pub task: TaskDemand,
    /// What kind of repository it is.
    pub domain: DomainDemand,
    /// What makes it harder, in a stable order.
    pub modifiers: Vec<ModifierDemand>,
    /// Files named in the request that exist in the repository.
    pub named_paths: u32,
    /// Size band of the repository: 0 `<50`, 1 `<500`, 2 `<5000`, 3 above.
    pub size_band: u8,
    /// Length band of the request: 0 `<200` bytes, 1 `<1000`, 2 above.
    pub length_band: u8,
    /// The version of the extractor that produced this.
    pub profiler_version: String,
}

impl Features {
    /// The slice this request belongs to, which is what the cohort is keyed
    /// by. Deliberately coarse: a slice nobody has observations for is worth
    /// nothing, so the slice is the part of the feature set with support.
    #[must_use]
    pub fn slice(&self) -> String {
        let modifiers: Vec<String> = self
            .modifiers
            .iter()
            .map(|m| {
                serde_json::to_string(m)
                    .unwrap_or_default()
                    .replace('"', "")
            })
            .collect();
        format!(
            "{}|{}|{}",
            serde_json::to_string(&self.task)
                .unwrap_or_default()
                .replace('"', ""),
            serde_json::to_string(&self.domain)
                .unwrap_or_default()
                .replace('"', ""),
            modifiers.join("+")
        )
    }

    /// Digest of every feature, so a caller can prove nothing moved.
    #[must_use]
    pub fn digest(&self) -> String {
        let mut h = Sha256::new();
        h.update(serde_json::to_vec(self).unwrap_or_default());
        hex::encode(h.finalize())
    }
}

/// One recorded slice of the calibration cohort.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CohortSlice {
    /// The slice key.
    pub slice: String,
    /// Requests observed in it.
    pub observations: u32,
    /// Of those, the ones that met the floor.
    pub met_floor: u32,
}

/// A versioned cohort of recorded outcomes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrationCohort {
    /// The version a profile pins.
    pub cohort_version: String,
    /// What the cohort is, and what it is not.
    pub note: String,
    /// Where the observations came from: baseline bundle digests.
    pub source_digests: Vec<String>,
    /// The slices, in slice order.
    pub slices: Vec<CohortSlice>,
    /// The slices held out of calibration, used only to measure it.
    pub holdout: Vec<CohortSlice>,
}

/// The cohort shipped with this build.
const RECORDED_COHORT: &str = include_str!("../calibration-cohort.json");

/// The recorded cohort.
///
/// # Panics
/// The shipped cohort is malformed, which is a build error.
#[must_use]
pub fn cohort() -> CalibrationCohort {
    serde_json::from_str(RECORDED_COHORT).expect("calibration-cohort.json is part of the build")
}

/// What the profiler says about one request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// The intrinsic features.
    pub features: Features,
    /// The slice they fall in.
    pub slice: String,
    /// Probability the floor is met, from the cohort, shrunk by support.
    pub p_floor_success: f64,
    /// How much the cohort supports that number, in `[0, 1]`.
    pub confidence: f64,
    /// The cohort has too little of this slice to say anything.
    pub ood: bool,
    /// The cohort this came from.
    pub cohort_version: String,
    /// Whether this is the conservative fallback rather than a measurement.
    pub fallback: bool,
    /// Why, when it is.
    pub fallback_reason: String,
}

impl Profile {
    /// Whether a caller may use this profile to prefer a cheaper plan. A
    /// fallback or an out-of-distribution profile may not: it is a reason to
    /// be conservative, never a licence to economise.
    #[must_use]
    pub fn may_economise(&self, floor: f64) -> bool {
        !self.fallback && !self.ood && self.p_floor_success >= floor
    }
}

/// The conservative profile: what the product says when it does not know.
#[must_use]
pub fn conservative(features: Features, cohort_version: &str, reason: &str) -> Profile {
    Profile {
        slice: features.slice(),
        features,
        p_floor_success: 0.0,
        confidence: 0.0,
        ood: true,
        cohort_version: cohort_version.to_owned(),
        fallback: true,
        fallback_reason: reason.to_owned(),
    }
}

fn band(n: usize, edges: [usize; 3]) -> u8 {
    match n {
        _ if n < edges[0] => 0,
        _ if n < edges[1] => 1,
        _ if n < edges[2] => 2,
        _ => 3,
    }
}

/// Extract the intrinsic features of a request. Bounded, deterministic, and
/// blind to anything the catalog decides.
#[must_use]
pub fn extract(facts: &RequestFacts<'_>) -> Features {
    let goal = &facts.goal[..facts.goal.len().min(GOAL_CEILING_BYTES)];
    let lower = goal.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    let task = if has(&[
        "fix",
        "bug",
        "broken",
        "regression",
        "fails",
        "error",
        "crash",
    ]) {
        TaskDemand::BugFix
    } else if has(&["test", "coverage", "assert"]) {
        TaskDemand::Test
    } else if has(&["refactor", "rename", "extract", "tidy", "clean up"]) {
        TaskDemand::Refactor
    } else if has(&["document", "readme", "docs", "comment"]) {
        TaskDemand::Docs
    } else if has(&["why", "what", "how does", "explain", "where is"]) {
        TaskDemand::Question
    } else {
        TaskDemand::Feature
    };
    let paths: Vec<&String> = facts.paths.iter().take(PATH_CEILING).collect();
    let mut by_count = facts.languages.clone();
    by_count.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let domain = match by_count.first().map(|(l, _)| l.as_str()) {
        Some("rust" | "c" | "cpp" | "go") => DomainDemand::Systems,
        Some("typescript" | "javascript" | "tsx" | "html" | "css") => DomainDemand::Web,
        Some("python" | "ruby" | "shell") => DomainDemand::Scripting,
        Some("markdown" | "text" | "json" | "toml" | "yaml") => DomainDemand::Text,
        _ => DomainDemand::Unknown,
    };
    let named_paths = u32::try_from(
        paths
            .iter()
            .filter(|p| {
                let name = p.rsplit('/').next().unwrap_or(p);
                !name.is_empty() && lower.contains(&name.to_ascii_lowercase())
            })
            .count(),
    )
    .unwrap_or(u32::MAX);
    let mut modifiers = Vec::new();
    if named_paths > 1 || paths.len() >= 500 {
        modifiers.push(ModifierDemand::CrossFile);
    }
    if facts.has_configured_verification || has(&["test", "verify", "prove", "reproduce"]) {
        modifiers.push(ModifierDemand::NeedsVerification);
    }
    if named_paths == 0 && goal.len() < 200 {
        modifiers.push(ModifierDemand::Underspecified);
    }
    if paths.iter().any(|p| {
        let p = p.to_ascii_lowercase();
        p.contains("secret") || p.contains("credential") || p.contains(".env")
    }) {
        modifiers.push(ModifierDemand::Sensitive);
    }
    modifiers.sort();
    modifiers.dedup();
    Features {
        task,
        domain,
        modifiers,
        named_paths,
        size_band: band(paths.len(), [50, 500, 5_000]),
        length_band: band(goal.len(), [200, 1_000, usize::MAX]),
        profiler_version: PROFILER_VERSION.to_owned(),
    }
}

/// Profile a request against a cohort.
///
/// `enabled` is the shadow switch: a disabled profiler answers with the
/// conservative profile rather than with nothing, so a caller always has a
/// defined answer to plan from.
#[must_use]
pub fn profile(facts: &RequestFacts<'_>, cohort: &CalibrationCohort, enabled: bool) -> Profile {
    let features = extract(facts);
    if !enabled {
        return conservative(features, &cohort.cohort_version, "the profiler is disabled");
    }
    if !facts.metadata_fresh {
        return conservative(
            features,
            &cohort.cohort_version,
            "the repository metadata is stale or missing, so the features cannot be trusted",
        );
    }
    let slice = features.slice();
    let Some(found) = cohort.slices.iter().find(|s| s.slice == slice) else {
        let mut p = conservative(
            features,
            &cohort.cohort_version,
            "the cohort has no observations of this slice",
        );
        p.fallback = false;
        return p;
    };
    if found.observations < OOD_SUPPORT {
        let mut p = conservative(
            features,
            &cohort.cohort_version,
            "the cohort has too few observations of this slice",
        );
        p.fallback = false;
        return p;
    }
    // Shrink the slice's own rate toward the conservative prior by how little
    // support it has: a rate over eight observations is mostly prior.
    let n = f64::from(found.observations);
    let rate = f64::from(found.met_floor) / n;
    let weight = (n / f64::from(FULL_SUPPORT)).min(1.0);
    Profile {
        slice,
        features,
        p_floor_success: weight.mul_add(rate, (1.0 - weight) * CONSERVATIVE_PRIOR),
        confidence: weight,
        ood: false,
        cohort_version: cohort.cohort_version.clone(),
        fallback: false,
        fallback_reason: String::new(),
    }
}

/// How well the cohort's own numbers predicted the holdout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Calibration {
    /// Slices measured.
    pub slices: usize,
    /// Observations behind them.
    pub observations: u32,
    /// Brier score: mean squared error of the prediction. Lower is better.
    pub brier: f64,
    /// Expected calibration error: mean gap between predicted and observed.
    pub ece: f64,
    /// Slices the calibration set had nothing for.
    pub ood_slices: usize,
    /// Whether this is enough to promote the profiler out of shadow.
    pub promotable: bool,
    /// Why not, when it is not.
    pub blockers: Vec<String>,
}

/// The support a cohort needs before its calibration means anything.
pub const PROMOTION_MIN_OBSERVATIONS: u32 = 200;

/// The Brier score above which calibration is too poor to promote.
pub const PROMOTION_MAX_BRIER: f64 = 0.15;

/// Measure the cohort's calibration against its untouched holdout.
#[must_use]
pub fn calibration(cohort: &CalibrationCohort) -> Calibration {
    let mut squared = 0.0;
    let mut gap = 0.0;
    let mut observations = 0u32;
    let mut measured = 0usize;
    let mut ood = 0usize;
    for h in &cohort.holdout {
        let Some(train) = cohort.slices.iter().find(|s| s.slice == h.slice) else {
            ood += 1;
            continue;
        };
        if train.observations == 0 || h.observations == 0 {
            ood += 1;
            continue;
        }
        let predicted = f64::from(train.met_floor) / f64::from(train.observations);
        let observed = f64::from(h.met_floor) / f64::from(h.observations);
        let n = f64::from(h.observations);
        squared += n * (predicted - observed).powi(2);
        gap += n * (predicted - observed).abs();
        observations += h.observations;
        measured += 1;
    }
    let total = f64::from(observations).max(1.0);
    let brier = squared / total;
    let ece = gap / total;
    let mut blockers = Vec::new();
    if observations < PROMOTION_MIN_OBSERVATIONS {
        blockers.push(format!(
            "the holdout has {observations} observations; promotion needs at least {PROMOTION_MIN_OBSERVATIONS}"
        ));
    }
    if measured == 0 {
        blockers.push("no holdout slice is covered by the calibration set".to_owned());
    }
    if brier > PROMOTION_MAX_BRIER && measured > 0 {
        blockers.push(format!(
            "the Brier score is {brier:.3}; promotion needs at most {PROMOTION_MAX_BRIER}"
        ));
    }
    if ood > 0 {
        blockers.push(format!(
            "{ood} holdout slice(s) are out of distribution for the calibration set"
        ));
    }
    Calibration {
        slices: measured,
        observations,
        brier,
        ece,
        ood_slices: ood,
        promotable: blockers.is_empty(),
        blockers,
    }
}
