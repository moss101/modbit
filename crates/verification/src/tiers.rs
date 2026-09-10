//! Language tier conformance (PX-027, docs/76 "Language tiers"): the suites a
//! language must pass before the product claims anything about it, and the
//! record of what actually passed.
//!
//! A tier is a claim, so it is earned by evidence. Each tier names the checks
//! its claim rests on; a language enters a tier only when every check of that
//! tier and of every tier below it passed on a real fixture repository. A
//! skipped check is not a pass, a grammar is not a classification, and a
//! record that claims more than its checks show is refused by `verify_record`.

use serde::{Deserialize, Serialize};

/// A support tier (docs/76).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Tier {
    /// Text-safe support.
    C,
    /// Validated structural support.
    B,
    /// Full engineering intelligence.
    A,
}

impl Tier {
    /// The tier's name in the matrix.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::A => "Tier A — full engineering intelligence",
            Self::B => "Tier B — validated structural support",
            Self::C => "Tier C — text-safe support",
        }
    }

    /// Tiers from C up to and including this one.
    #[must_use]
    pub fn and_below(self) -> Vec<Tier> {
        match self {
            Self::A => vec![Tier::C, Tier::B, Tier::A],
            Self::B => vec![Tier::C, Tier::B],
            Self::C => vec![Tier::C],
        }
    }

    /// The next tier up, when there is one.
    #[must_use]
    pub fn next(self) -> Option<Tier> {
        match self {
            Self::C => Some(Tier::B),
            Self::B => Some(Tier::A),
            Self::A => None,
        }
    }
}

/// One check a tier's claim rests on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TierCheck {
    /// Stable id, used in records.
    pub id: &'static str,
    /// The tier it belongs to.
    pub tier: Tier,
    /// What passing it proves, in the matrix's own words.
    pub proves: &'static str,
}

/// The suites, in the order a runner should execute them (docs/76: the
/// conformance suite must prove exactly these things).
pub const CHECKS: &[TierCheck] = &[
    TierCheck {
        id: "c1_text_edit_preserves_encoding_and_line_endings",
        tier: Tier::C,
        proves: "edits never corrupt encoding or line endings",
    },
    TierCheck {
        id: "c2_exact_and_lexical_retrieval",
        tier: Tier::C,
        proves: "exact and BM25 retrieval return revision-bound hits for this language",
    },
    TierCheck {
        id: "c3_configured_command_evidence",
        tier: Tier::C,
        proves: "test or build evidence through a configured command is attributed to the task",
    },
    TierCheck {
        id: "b1_symbol_extraction",
        tier: Tier::B,
        proves: "tree-sitter symbol extraction finds the definitions of a real fixture file",
    },
    TierCheck {
        id: "b2_build_output_diagnostics",
        tier: Tier::B,
        proves: "diagnostics are parsed out of build or test output with a file and a line",
    },
    TierCheck {
        id: "b3_revision_bound_structural_edit",
        tier: Tier::B,
        proves: "an edit against a stale revision is refused rather than applied",
    },
    TierCheck {
        id: "a1_language_service_symbols_and_references",
        tier: Tier::A,
        proves: "the headless language service returns symbols and finds a known reference",
    },
    TierCheck {
        id: "a2_diagnostics_parity",
        tier: Tier::A,
        proves: "the language service reports the defect the build reports, in the same file",
    },
];

/// The checks of one tier.
#[must_use]
pub fn checks_of(tier: Tier) -> Vec<&'static TierCheck> {
    CHECKS.iter().filter(|c| c.tier == tier).collect()
}

/// What one check did when the suite ran.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckOutcome {
    /// Check id.
    pub id: String,
    /// Tier it belongs to.
    pub tier: Tier,
    /// `pass` | `fail` | `skip`.
    pub status: String,
    /// What happened, in one line (what was measured, or why it was skipped).
    pub detail: String,
}

impl CheckOutcome {
    /// Whether this outcome is a pass.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.status == "pass"
    }
}

/// A recorded conformance run for one language.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TierRecord {
    /// Language label (`rust`, `python`, `typescript`, …).
    pub language: String,
    /// The fixture repository the suite ran on.
    pub fixture: String,
    /// The tier this record claims.
    pub tier: Tier,
    /// Every check the suite ran, in order.
    pub checks: Vec<CheckOutcome>,
    /// The test that runs the suite, so a reader can rerun it.
    pub suite_test: String,
    /// What this record does not claim.
    pub not_claimed: Vec<String>,
}

/// Why a record was refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectedRecord {
    /// The record claims a tier its checks do not support.
    Unearned {
        /// The tier claimed.
        claimed: Tier,
        /// The tier the checks actually earn, when any.
        earned: Option<Tier>,
        /// The checks that stop it.
        missing: Vec<String>,
    },
    /// The record does not carry every check of the tier it claims.
    Incomplete {
        /// The checks that are absent from the record.
        absent: Vec<String>,
    },
    /// The record names a check that is not part of any suite.
    UnknownCheck {
        /// The id.
        id: String,
    },
}

/// The highest tier these outcomes earn: every check of that tier and of every
/// tier below it passed. A skipped check earns nothing.
#[must_use]
pub fn earned_tier(checks: &[CheckOutcome]) -> Option<Tier> {
    let passed = |tier: Tier| {
        checks_of(tier).iter().all(|want| {
            checks
                .iter()
                .any(|c| c.id == want.id && c.tier == want.tier && c.passed())
        })
    };
    [Tier::A, Tier::B, Tier::C]
        .into_iter()
        .find(|t| t.and_below().into_iter().all(passed))
}

/// The checks of `tier` and below that these outcomes do not show passing.
#[must_use]
pub fn missing_for(tier: Tier, checks: &[CheckOutcome]) -> Vec<String> {
    tier.and_below()
        .into_iter()
        .flat_map(|t| checks_of(t).into_iter())
        .filter(|want| !checks.iter().any(|c| c.id == want.id && c.passed()))
        .map(|want| want.id.to_owned())
        .collect()
}

/// Whether a record may stand: it names only real checks, carries every check
/// of the tier it claims and of the tiers below, and earns the tier it claims.
///
/// # Errors
/// The record is incomplete, names an unknown check, or claims more than its
/// checks show.
pub fn verify_record(record: &TierRecord) -> Result<(), RejectedRecord> {
    if let Some(unknown) = record
        .checks
        .iter()
        .find(|c| !CHECKS.iter().any(|k| k.id == c.id))
    {
        return Err(RejectedRecord::UnknownCheck {
            id: unknown.id.clone(),
        });
    }
    let absent: Vec<String> = record
        .tier
        .and_below()
        .into_iter()
        .flat_map(|t| checks_of(t).into_iter())
        .filter(|want| !record.checks.iter().any(|c| c.id == want.id))
        .map(|want| want.id.to_owned())
        .collect();
    if !absent.is_empty() {
        return Err(RejectedRecord::Incomplete { absent });
    }
    let earned = earned_tier(&record.checks);
    if earned != Some(record.tier) {
        return Err(RejectedRecord::Unearned {
            claimed: record.tier,
            earned,
            missing: missing_for(record.tier, &record.checks),
        });
    }
    Ok(())
}

/// The recorded conformance runs shipped with this build (PX-027: a language
/// enters a tier only through a recorded pass, and the suite test regenerates
/// this file, so it can never claim more than the suites proved).
const RECORDED: &str = include_str!("../language-tiers.json");

/// The recorded runs, parsed.
///
/// # Panics
/// The shipped file is malformed, which is a build error rather than a runtime
/// condition.
#[must_use]
pub fn recorded() -> Vec<TierRecord> {
    serde_json::from_str::<RecordFile>(RECORDED)
        .expect("language-tiers.json is part of the build")
        .records
}

/// The file's shape.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordFile {
    /// What the file is.
    pub note: String,
    /// One record per language that has passed a suite.
    pub records: Vec<TierRecord>,
}

/// The tier a language has earned, or `None` when nothing is recorded for it:
/// no record, no claim (docs/76: any language not in the table is
/// Unsupported, and a grammar alone classifies nothing).
#[must_use]
pub fn tier_of(language: &str) -> Option<Tier> {
    recorded()
        .into_iter()
        .find(|r| r.language == language && verify_record(r).is_ok())
        .map(|r| r.tier)
}
