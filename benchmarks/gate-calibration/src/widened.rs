//! Widened gate calibration (PX-135, REQ-PX-135; docs/62, ADC-I03).
//!
//! EPR-019's held-out corpora were eight Rust cases and eighteen
//! language-blind risk cases. This widens the *data* to the Tier A languages
//! (Rust, Python, JavaScript, TypeScript) and measures the Acceptance Gate
//! twice on every case, from one real verification run:
//!
//! - **previous**: the gate as EPR-017 sealed it, the evidence EPR-019
//!   measured;
//! - **widened**: the same gate, the same algorithm and thresholds, handed
//!   the generated adversarial checks of `modbit_verification::adversarial`
//!   as additional evidence.
//!
//! Every case carries a hidden oracle that neither the gate nor the checks
//! can read: it is copied into a separate tree after the gate has decided,
//! and a leak search over everything the gate and the checks read (gate
//! input, changed files, the tree, retained raw output) looks for the
//! oracle's canaries, file names and label keys.
//!
//! The report lists false-accept, false-reject and realized-risk
//! false-negative rates with Wilson intervals and sample counts per language
//! and slice, separately from any router quality. Thresholds are the owner's
//! approved profile; this module neither sets nor changes them and never
//! attests a gate: with no profile the release check fails by design.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use modbit_policy::AssurancePolicy;
use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::{
    AcceptanceCase, AcceptanceCorpus, AcceptanceOutcome, AdversarialConfig, FileOp, Metrics,
    Oracle, Rate, Refusal, ReleaseCheck, RiskCase, RiskCorpus, RiskOutcome, WidenedGate,
    check_separation, metrics, release_check, run_case, run_risk_case, sha256_hex, tuning_lineages,
    wilson,
};

/// Suite version of the widened report.
pub const WIDENED_SUITE_VERSION: &str = "gate-calibration-widened/1";

/// The Tier A languages, in report order.
pub const LANGUAGES: [&str; 4] = ["rust", "python", "javascript", "typescript"];

/// The failure classes seeded as incorrect candidates, in report order.
pub const INCORRECT_CLASSES: [&str; 4] = ["overfit", "offbyone", "weakening", "vacuous"];

/// The corpus-size floors this harness holds the corpora to, per language.
///
/// The dossier fixes no numeric minima (docs/61: "No numeric empirical pass
/// rate is invented by this dossier"; the owner's profile carries the
/// statistical `min_samples`). These are floors on how much *data* a slice
/// must have before its rate is reported as measured, so a slice thinner than
/// this is reported as unmet rather than silently small. They are not a
/// threshold on any rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Minima {
    /// Correct candidates per language.
    pub correct: u32,
    /// Incorrect candidates per failure class per language.
    pub incorrect_per_class: u32,
    /// Risk cases whose oracle obliges review or a human, per language.
    pub risk_obliged: u32,
    /// Risk cases whose oracle obliges nothing, per language.
    pub risk_benign: u32,
    /// Risk cases that touch a critical surface, per language.
    pub risk_critical: u32,
}

/// The floors in force.
pub const MINIMA: Minima = Minima {
    correct: 8,
    incorrect_per_class: 8,
    risk_obliged: 8,
    risk_benign: 4,
    risk_critical: 6,
};

// ---- Corpus ----------------------------------------------------------------

/// One widened acceptance case and its slice coordinates.
#[derive(Clone, Debug)]
pub struct WidenedCase {
    /// The case the runner takes.
    pub case: AcceptanceCase,
    /// `rust` | `python` | `javascript` | `typescript`.
    pub language: String,
    /// `correct` | `overfit` | `offbyone` | `weakening` | `vacuous`.
    pub class: String,
    /// `a` | `b`.
    pub flavor: String,
    /// What a model is told to do in live mode: the specification in words
    /// and the failing visible test. Never an oracle or a label.
    pub goal: String,
}

/// One widened risk case.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WidenedRiskCase {
    /// Language.
    pub language: String,
    /// The case.
    #[serde(flatten)]
    pub case: RiskCase,
}

/// The widened corpora, loaded.
#[derive(Clone, Debug)]
pub struct WidenedCorpus {
    /// Acceptance cases, in manifest order.
    pub cases: Vec<WidenedCase>,
    /// Risk cases.
    pub risk: Vec<WidenedRiskCase>,
    /// sha256 over every corpus file (generator excluded), name-ordered.
    pub digest: String,
    /// Fixture-independent oracle needles for the leak search.
    pub needles: Vec<String>,
}

#[derive(Deserialize)]
struct ManifestCase {
    id: String,
    language: String,
    lineage: String,
    class: String,
    flavor: String,
}

#[derive(Deserialize)]
struct ManifestLanguage {
    id: String,
    fixture: String,
}

#[derive(Deserialize)]
struct Manifest {
    languages: Vec<ManifestLanguage>,
    cases: Vec<ManifestCase>,
}

#[derive(Deserialize)]
struct LineageFile {
    lineage: String,
    #[serde(default)]
    goal: String,
    acceptance: Vec<String>,
    seed: Vec<String>,
    oracle: Oracle,
}

#[derive(Deserialize)]
struct CaseFile {
    leg: String,
    write_set: Vec<String>,
    declared: String,
}

fn unreadable(p: &Path, e: impl std::fmt::Display) -> Refusal {
    Refusal::Unreadable {
        detail: format!("{}: {e}", p.display()),
    }
}

fn read_text(p: &Path) -> Result<String, Refusal> {
    std::fs::read_to_string(p).map_err(|e| unreadable(p, e))
}

fn read_json<T: serde::de::DeserializeOwned>(p: &Path) -> Result<T, Refusal> {
    serde_json::from_str(&read_text(p)?).map_err(|e| unreadable(p, e))
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                walk(&p, out);
            } else {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The strings that must appear in nothing the gate reads: every oracle
/// canary, every oracle file name and the label keys of the corpus.
fn oracle_needles(oracles: &Path) -> Vec<String> {
    let mut out = vec![
        "\"declared\"".to_owned(),
        "oracle_correct".to_owned(),
        "MODBIT-ORACLE-CANARY".to_owned(),
    ];
    for p in files_under(oracles) {
        let name = p
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        if !name.is_empty() {
            out.push(name);
        }
        for tok in read_text(&p).unwrap_or_default().split_whitespace() {
            if tok.starts_with("MODBIT-ORACLE-CANARY-") {
                out.push(
                    tok.trim_matches(|c: char| !(c.is_alphanumeric() || c == '-'))
                        .to_owned(),
                );
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Load the widened corpora from `widened_dir` (`corpora/widened`).
///
/// # Errors
/// A corpus file is unreadable or malformed.
pub fn load(widened_dir: &Path) -> Result<WidenedCorpus, Refusal> {
    let manifest: Manifest = read_json(&widened_dir.join("manifest.json"))?;
    let fixtures: BTreeMap<&str, &str> = manifest
        .languages
        .iter()
        .map(|l| (l.id.as_str(), l.fixture.as_str()))
        .collect();
    let mut cases = Vec::new();
    for m in &manifest.cases {
        let ldir = widened_dir.join(&m.language).join(&m.lineage);
        let lineage: LineageFile = read_json(&ldir.join("lineage.json"))?;
        let cdir = ldir.join("cases").join(&m.id);
        let cf: CaseFile = read_json(&cdir.join("case.json"))?;
        let mut seed = Vec::new();
        for path in &lineage.seed {
            seed.push(FileOp::Write {
                path: path.clone(),
                content: read_text(&ldir.join("seed").join(path))?,
            });
        }
        let mut candidate = Vec::new();
        let fdir = cdir.join("files");
        for p in files_under(&fdir) {
            candidate.push(FileOp::Write {
                path: rel(&fdir, &p),
                content: read_text(&p)?,
            });
        }
        let fixture = fixtures
            .get(m.language.as_str())
            .ok_or_else(|| Refusal::Unreadable {
                detail: format!("no fixture for language {}", m.language),
            })?;
        cases.push(WidenedCase {
            case: AcceptanceCase {
                id: m.id.clone(),
                lineage: lineage.lineage.clone(),
                fixture: (*fixture).to_owned(),
                leg: cf.leg,
                acceptance: lineage.acceptance.clone(),
                write_set: cf.write_set,
                candidate,
                oracle: lineage.oracle.clone(),
                declared: cf.declared,
                seed,
            },
            language: m.language.clone(),
            class: m.class.clone(),
            flavor: m.flavor.clone(),
            goal: lineage.goal.clone(),
        });
    }
    let risk_corpus: RiskFile = read_json(&widened_dir.join("risk-widened.json"))?;
    let mut h = sha2::Sha256::new();
    for p in files_under(widened_dir) {
        if p.file_name().and_then(|n| n.to_str()) == Some("generate.py") {
            continue;
        }
        h.update(rel(widened_dir, &p).as_bytes());
        h.update(std::fs::read(&p).map_err(|e| unreadable(&p, e))?);
    }
    Ok(WidenedCorpus {
        cases,
        risk: risk_corpus.cases,
        digest: hex::encode(h.finalize()),
        needles: oracle_needles(&widened_dir.join("oracles")),
    })
}

#[derive(Deserialize)]
struct RiskFile {
    cases: Vec<WidenedRiskCase>,
}

// ---- Toolchains ------------------------------------------------------------------

/// Whether a language's real verification command can run here.
///
/// # Errors
/// What is missing, as a sentence for the report.
pub fn toolchain(language: &str) -> Result<(), String> {
    let ok = |argv: &[&str]| {
        Command::new(argv[0])
            .args(&argv[1..])
            .output()
            .is_ok_and(|o| o.status.success())
    };
    match language {
        "rust" if ok(&["cargo", "--version"]) => Ok(()),
        "rust" => Err("cargo is not installed".into()),
        "python" if ok(&["python3", "-m", "pytest", "--version"]) => Ok(()),
        "python" => Err("python3 with pytest is not installed".into()),
        "javascript" => {
            let out = Command::new("node").arg("--version").output();
            let major = out
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .and_then(|v| {
                    v.trim()
                        .trim_start_matches('v')
                        .split('.')
                        .next()
                        .and_then(|m| m.parse::<u32>().ok())
                });
            match major {
                Some(m) if m >= 22 => Ok(()),
                Some(m) => Err(format!(
                    "node {m} is older than 22 (needs `node --test` globs)"
                )),
                None => Err("node is not installed".into()),
            }
        }
        "typescript" => {
            toolchain("javascript")?;
            let dir = std::env::temp_dir().join(format!("modbit-ts-probe-{}", std::process::id()));
            let _ = std::fs::create_dir_all(&dir);
            let f = dir.join("probe.ts");
            let _ = std::fs::write(&f, "const x: number = 41;\nconsole.log(x + 1);\n");
            let out = Command::new("node").arg(&f).output();
            let _ = std::fs::remove_dir_all(&dir);
            match out {
                Ok(o)
                    if o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "42" =>
                {
                    Ok(())
                }
                _ => Err("this node cannot run TypeScript by type stripping".into()),
            }
        }
        other => Err(format!("`{other}` is not a Tier A language of this suite")),
    }
}

// ---- Outcomes and report ----------------------------------------------------------

/// A verdict and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateRow {
    /// `ACCEPT` | `REJECT` | `INCONCLUSIVE`.
    pub verdict: String,
    /// Reject reasons.
    pub reasons: Vec<String>,
    /// Missing evidence.
    pub missing: Vec<String>,
}

/// One case before and after.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairRow {
    /// Case id.
    pub id: String,
    /// `rust` ... or `rust-legacy` (EPR-019's own corpus).
    pub language: String,
    /// Lineage.
    pub lineage: String,
    /// Candidate class.
    pub class: String,
    /// Flavor.
    pub flavor: String,
    /// The hidden oracle accepted the candidate. Recorded after both gates
    /// decided.
    pub oracle_correct: bool,
    /// The previous gate.
    pub previous: GateRow,
    /// The widened gate.
    pub widened: GateRow,
    /// Findings of the generated checks that failed (`check id: finding`).
    pub findings: Vec<String>,
    /// sha256 of the decision: verdicts, reasons, missing evidence and
    /// findings. (The engine's run ids are not in it; they differ per run.)
    pub decision_digest: String,
}

impl PairRow {
    fn outcome(&self, widened: bool) -> AcceptanceOutcome {
        let g = if widened {
            &self.widened
        } else {
            &self.previous
        };
        AcceptanceOutcome {
            id: self.id.clone(),
            lineage: self.lineage.clone(),
            leg: "initial".into(),
            verdict: g.verdict.clone(),
            oracle_correct: self.oracle_correct,
            missing_evidence: g.missing.clone(),
            reject_reasons: g.reasons.clone(),
            risk_level: String::new(),
            gate_result_digest: String::new(),
        }
    }
}

/// A rate under both gates, with the samples behind it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SliceRate {
    /// `false_accept` (incorrect candidates) | `false_reject` (correct ones).
    pub measure: String,
    /// The slice: a failure class, `correct` or `all_incorrect`.
    pub slice: String,
    /// Samples in the slice.
    pub n: u32,
    /// The data floor for the slice (0 for aggregate slices).
    pub minimum: u32,
    /// Whether `n` reaches the floor.
    pub minimum_met: bool,
    /// The previous gate.
    pub previous: Rate,
    /// The widened gate.
    pub widened: Rate,
}

/// Realized-risk rates for one language.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RiskRates {
    /// The oracle obliges more than the rules did (over obliged cases).
    pub false_negative: Rate,
    /// The rules obliged what the oracle did not (over benign cases).
    pub false_positive: Rate,
    /// A critical surface the rules did not send to a human.
    pub critical_miss: Rate,
    /// Whether the three denominators reach their floors.
    pub minimum_met: bool,
    /// The cases that missed (`id: what`).
    pub misses: Vec<String>,
}

/// One language's section.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LanguageReport {
    /// Language.
    pub language: String,
    /// `MEASURED` | `UNAVAILABLE`.
    pub status: String,
    /// Why unavailable.
    pub reason: Option<String>,
    /// Acceptance cases run.
    pub cases: u32,
    /// Rates per slice.
    pub slices: Vec<SliceRate>,
    /// Risk rates.
    pub risk: Option<RiskRates>,
}

/// Seeded candidates of one class, accepted by each gate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassRow {
    /// Class.
    pub class: String,
    /// Incorrect candidates of the class (the oracle rejects them).
    pub seeded: u32,
    /// Accepted by the previous gate.
    pub accepted_by_previous: u32,
    /// Accepted by the widened gate.
    pub accepted_by_widened: u32,
    /// Accepted by the previous gate and rejected (REJECT) by the widened.
    pub newly_rejected: u32,
}

/// The oracle-leak search.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeakReport {
    /// What was searched for.
    pub needles: Vec<String>,
    /// Cases whose gate inputs, files, trees and retained output were searched.
    pub cases_searched: u32,
    /// Where a needle was found (`case: source: needle`); empty when clean.
    pub hits: Vec<String>,
}

/// The whole widened calibration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WidenedReport {
    /// Suite version.
    pub suite_version: String,
    /// Generated-check generator version.
    pub generator_version: String,
    /// Gate rules version (unchanged by this row).
    pub gate_version: String,
    /// Realized-risk rules version.
    pub risk_version: String,
    /// Corpus digest.
    pub corpus_digest: String,
    /// Tuning-lineage digest.
    pub tuning_digest: String,
    /// The floors in force.
    pub minima: Minima,
    /// Every floor met for every language (an unavailable language is not).
    pub minima_met: bool,
    /// Per language.
    pub languages: Vec<LanguageReport>,
    /// Seeded candidates, accepted by each gate, per class (all languages).
    pub before_after: Vec<ClassRow>,
    /// Acceptance rates over every measured widened case, both gates.
    pub overall: BTreeMap<String, Rate>,
    /// The same, for the previous gate.
    pub overall_previous: BTreeMap<String, Rate>,
    /// EPR-019's own eight cases under both gates.
    pub legacy: Vec<PairRow>,
    /// Every widened case.
    pub rows: Vec<PairRow>,
    /// The leak search.
    pub leak: LeakReport,
    /// The release check against no profile: it fails by design.
    pub release: ReleaseCheck,
    /// Gates are never attested by this suite.
    pub gates_attested: bool,
    /// sha256 of this report with this field empty.
    pub report_digest: String,
}

fn gate_row(g: &WidenedGate) -> GateRow {
    GateRow {
        verdict: g.verdict.clone(),
        reasons: g.reject_reasons.clone(),
        missing: g.missing_evidence.clone(),
    }
}

/// One case's before-and-after row, from a run.
///
/// # Errors
/// The run carried no widened gate.
pub fn pair_row(
    wc: &WidenedCase,
    language: &str,
    run: &crate::CaseRun,
) -> Result<PairRow, Refusal> {
    let w = run.widened.as_ref().ok_or_else(|| Refusal::CaseFailed {
        case: wc.case.id.clone(),
        detail: "the widened gate did not run".into(),
    })?;
    let previous = GateRow {
        verdict: run.outcome.verdict.clone(),
        reasons: run.outcome.reject_reasons.clone(),
        missing: run.outcome.missing_evidence.clone(),
    };
    let widened = gate_row(w);
    let findings: Vec<String> = w
        .generated
        .iter()
        .filter(|g| g.status == modbit_verification::adversarial::Status::Fail)
        .flat_map(|g| {
            g.findings
                .iter()
                .map(|f| format!("{}: {f}", g.class.check_id()))
                .collect::<Vec<_>>()
        })
        .collect();
    let decision = serde_json::to_vec(&(&previous, &widened, &findings, w.generated.len()))
        .unwrap_or_default();
    Ok(PairRow {
        id: wc.case.id.clone(),
        language: language.to_owned(),
        lineage: wc.case.lineage.clone(),
        class: wc.class.clone(),
        flavor: wc.flavor.clone(),
        oracle_correct: run.outcome.oracle_correct,
        previous,
        widened,
        findings,
        decision_digest: sha256_hex(&decision),
    })
}

pub(crate) fn false_accepts(rows: &[&PairRow], widened: bool) -> Rate {
    let bad: Vec<&&PairRow> = rows.iter().filter(|r| !r.oracle_correct).collect();
    let fa = bad
        .iter()
        .filter(|r| r.outcome(widened).false_accept())
        .count();
    wilson(
        u32::try_from(fa).unwrap_or(u32::MAX),
        u32::try_from(bad.len()).unwrap_or(u32::MAX),
    )
}

pub(crate) fn false_rejects(rows: &[&PairRow], widened: bool) -> Rate {
    let good: Vec<&&PairRow> = rows.iter().filter(|r| r.oracle_correct).collect();
    let fr = good
        .iter()
        .filter(|r| r.outcome(widened).false_reject())
        .count();
    wilson(
        u32::try_from(fr).unwrap_or(u32::MAX),
        u32::try_from(good.len()).unwrap_or(u32::MAX),
    )
}

fn slices(rows: &[PairRow]) -> Vec<SliceRate> {
    let mut out = Vec::new();
    let correct: Vec<&PairRow> = rows.iter().filter(|r| r.class == "correct").collect();
    let n = u32::try_from(correct.len()).unwrap_or(u32::MAX);
    out.push(SliceRate {
        measure: "false_reject".into(),
        slice: "correct".into(),
        n,
        minimum: MINIMA.correct,
        minimum_met: n >= MINIMA.correct,
        previous: false_rejects(&correct, false),
        widened: false_rejects(&correct, true),
    });
    for class in INCORRECT_CLASSES {
        let of: Vec<&PairRow> = rows.iter().filter(|r| r.class == class).collect();
        let n = u32::try_from(of.len()).unwrap_or(u32::MAX);
        out.push(SliceRate {
            measure: "false_accept".into(),
            slice: class.into(),
            n,
            minimum: MINIMA.incorrect_per_class,
            minimum_met: n >= MINIMA.incorrect_per_class,
            previous: false_accepts(&of, false),
            widened: false_accepts(&of, true),
        });
    }
    let all: Vec<&PairRow> = rows.iter().filter(|r| r.class != "correct").collect();
    let n = u32::try_from(all.len()).unwrap_or(u32::MAX);
    out.push(SliceRate {
        measure: "false_accept".into(),
        slice: "all_incorrect".into(),
        n,
        minimum: 0,
        minimum_met: true,
        previous: false_accepts(&all, false),
        widened: false_accepts(&all, true),
    });
    out
}

pub(crate) fn risk_rates(outcomes: &[RiskOutcome]) -> RiskRates {
    let m = metrics(&[], outcomes);
    let fnr = m.rates["realized_risk_false_negative_rate"].clone();
    let fpr = m.rates["realized_risk_false_positive_rate"].clone();
    let crit = m.rates["critical_surface_miss_rate"].clone();
    let mut misses = Vec::new();
    for r in outcomes {
        if r.false_negative {
            misses.push(format!("{}: obliged review or human not derived", r.id));
        }
        if r.false_positive {
            misses.push(format!(
                "{}: obligation derived that the oracle does not warrant",
                r.id
            ));
        }
        if r.critical_miss {
            misses.push(format!("{}: critical surface not sent to a human", r.id));
        }
    }
    RiskRates {
        minimum_met: fnr.n >= MINIMA.risk_obliged
            && fpr.n >= MINIMA.risk_benign
            && crit.n >= MINIMA.risk_critical,
        false_negative: fnr,
        false_positive: fpr,
        critical_miss: crit,
        misses,
    }
}

/// How a widened calibration is run.
#[derive(Clone, Debug, Default)]
pub struct RunOptions {
    /// Languages to leave out although their toolchain is present (tests).
    pub skip_languages: Vec<String>,
    /// Run only these case ids (tests); empty runs all.
    pub only_cases: Vec<String>,
    /// Also run EPR-019's eight cases under both gates.
    pub include_legacy: bool,
}

/// The widened gate configuration for a corpus.
#[must_use]
pub fn adversarial_config(corpus: &WidenedCorpus) -> AdversarialConfig {
    AdversarialConfig {
        needles: corpus.needles.clone(),
        expose_oracle_to_gate: false,
    }
}

/// Run the widened calibration.
///
/// # Errors
/// A contaminated holdout, a mislabeled case or a case that could not run.
pub async fn run(
    repo_root: &Path,
    corpora: &Path,
    work: &Path,
    opts: &RunOptions,
) -> Result<WidenedReport, Refusal> {
    let widened_dir = corpora.join("widened");
    let corpus = load(&widened_dir)?;
    // Held out from the tuning sets and from EPR-019's holdout, as a whole.
    let legacy_acc: AcceptanceCorpus = read_json(&corpora.join("acceptance-holdout.json"))?;
    let legacy_risk: RiskCorpus = read_json(&corpora.join("risk-holdout.json"))?;
    let mut acc_all = legacy_acc.clone();
    acc_all
        .cases
        .extend(corpus.cases.iter().map(|c| c.case.clone()));
    let mut risk_all = legacy_risk.clone();
    risk_all
        .cases
        .extend(corpus.risk.iter().map(|r| r.case.clone()));
    let tuning = tuning_lineages(repo_root);
    check_separation(&acc_all, &risk_all, &tuning)?;

    let policy = AssurancePolicy::default();
    let adv = adversarial_config(&corpus);
    let widened_oracles = widened_dir.clone();

    // Languages run concurrently; the cases of one language run in turn.
    let mut available: BTreeMap<String, Result<(), String>> = BTreeMap::new();
    for l in LANGUAGES {
        let r = if opts.skip_languages.iter().any(|s| s == l) {
            Err("skipped by the caller".to_owned())
        } else {
            toolchain(l)
        };
        available.insert(l.to_owned(), r);
    }
    let mut set = tokio::task::JoinSet::new();
    for l in LANGUAGES {
        if available[l].is_err() {
            continue;
        }
        let cases: Vec<WidenedCase> = corpus
            .cases
            .iter()
            .filter(|c| {
                c.language == l
                    && (opts.only_cases.is_empty() || opts.only_cases.contains(&c.case.id))
            })
            .cloned()
            .collect();
        let (repo_root, dir, policy, adv, oracles) = (
            repo_root.to_path_buf(),
            work.join(l),
            policy.clone(),
            adv.clone(),
            widened_oracles.clone(),
        );
        set.spawn(async move {
            let mut rows = Vec::new();
            let _ = std::fs::create_dir_all(&dir);
            for wc in cases {
                let run =
                    run_case(&wc.case, &repo_root, &oracles, &dir, &policy, Some(&adv)).await?;
                let leaks = run
                    .widened
                    .as_ref()
                    .map(|w| w.leaks.clone())
                    .unwrap_or_default();
                rows.push((pair_row(&wc, &wc.language, &run)?, leaks));
            }
            Ok::<_, Refusal>(rows)
        });
    }
    let mut measured: Vec<(PairRow, Vec<String>)> = Vec::new();
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok(Ok(rows)) => measured.extend(rows),
            Ok(Err(refusal)) => return Err(refusal),
            Err(e) => {
                return Err(Refusal::CaseFailed {
                    case: "widened".into(),
                    detail: format!("a language task failed: {e}"),
                });
            }
        }
    }
    // Manifest order, whatever order the languages finished in.
    let order: BTreeMap<&str, usize> = corpus
        .cases
        .iter()
        .enumerate()
        .map(|(i, c)| (c.case.id.as_str(), i))
        .collect();
    measured.sort_by_key(|(r, _)| order.get(r.id.as_str()).copied().unwrap_or(usize::MAX));
    let mut leak_hits: Vec<String> = measured
        .iter()
        .flat_map(|(r, hits)| hits.iter().map(|h| format!("{}: {h}", r.id)))
        .collect();

    // EPR-019's own eight cases under both gates.
    let mut legacy_rows = Vec::new();
    if opts.include_legacy
        && available["rust"].is_ok()
        && !opts.skip_languages.iter().any(|s| s == "rust")
    {
        let dir = work.join("legacy");
        let _ = std::fs::create_dir_all(&dir);
        for case in &legacy_acc.cases {
            let wc = WidenedCase {
                case: case.clone(),
                language: "rust-legacy".into(),
                class: case.id.split('-').skip(1).collect::<Vec<_>>().join("-"),
                flavor: String::new(),
                goal: String::new(),
            };
            let run = run_case(case, repo_root, corpora, &dir, &policy, Some(&adv)).await?;
            if let Some(w) = &run.widened {
                leak_hits.extend(w.leaks.iter().map(|h| format!("{}: {h}", case.id)));
            }
            legacy_rows.push(pair_row(&wc, "rust-legacy", &run)?);
        }
    }

    // Risk, per language.
    let mut risk_by_lang: BTreeMap<String, Vec<RiskOutcome>> = BTreeMap::new();
    for r in &corpus.risk {
        risk_by_lang
            .entry(r.language.clone())
            .or_default()
            .push(run_risk_case(&r.case, &policy));
    }

    let rows: Vec<PairRow> = measured.iter().map(|(r, _)| r.clone()).collect();
    let mut languages = Vec::new();
    let mut minima_met = true;
    for l in LANGUAGES {
        match &available[l] {
            Err(reason) => {
                minima_met = false;
                languages.push(LanguageReport {
                    language: l.into(),
                    status: "UNAVAILABLE".into(),
                    reason: Some(reason.clone()),
                    cases: 0,
                    slices: vec![],
                    risk: risk_by_lang.get(l).map(|o| risk_rates(o)),
                });
            }
            Ok(()) => {
                let mine: Vec<PairRow> = rows.iter().filter(|r| r.language == l).cloned().collect();
                let s = slices(&mine);
                let risk = risk_by_lang.get(l).map(|o| risk_rates(o));
                minima_met &=
                    s.iter().all(|x| x.minimum_met) && risk.as_ref().is_some_and(|r| r.minimum_met);
                languages.push(LanguageReport {
                    language: l.into(),
                    status: "MEASURED".into(),
                    reason: None,
                    cases: u32::try_from(mine.len()).unwrap_or(u32::MAX),
                    slices: s,
                    risk,
                });
            }
        }
    }

    let mut before_after = Vec::new();
    for class in INCORRECT_CLASSES {
        let of: Vec<&PairRow> = rows
            .iter()
            .filter(|r| r.class == class && !r.oracle_correct)
            .collect();
        let count = |f: &dyn Fn(&PairRow) -> bool| {
            u32::try_from(of.iter().filter(|r| f(r)).count()).unwrap_or(u32::MAX)
        };
        before_after.push(ClassRow {
            class: class.into(),
            seeded: u32::try_from(of.len()).unwrap_or(u32::MAX),
            accepted_by_previous: count(&|r| r.previous.verdict == "ACCEPT"),
            accepted_by_widened: count(&|r| r.widened.verdict == "ACCEPT"),
            newly_rejected: count(&|r| {
                r.previous.verdict == "ACCEPT" && r.widened.verdict == "REJECT"
            }),
        });
    }
    let overall_of = |widened: bool| -> (Metrics, BTreeMap<String, Rate>) {
        let outs: Vec<AcceptanceOutcome> = rows.iter().map(|r| r.outcome(widened)).collect();
        let m = metrics(&outs, &[]);
        let rates = m
            .rates
            .iter()
            .filter(|(k, _)| k.starts_with("acceptance_"))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        (m, rates)
    };
    let (m_wide, overall) = overall_of(true);
    let (_, overall_previous) = overall_of(false);
    let release = release_check(&m_wide, None, 0);

    let tuning_digest = sha256_hex(
        tuning
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
            .as_bytes(),
    );
    let mut report = WidenedReport {
        suite_version: WIDENED_SUITE_VERSION.into(),
        generator_version: modbit_verification::adversarial::GENERATOR_VERSION.into(),
        gate_version: modbit_verification::gate::GATE_VERSION.into(),
        risk_version: modbit_policy::assurance::REALIZED_RISK_RULES_VERSION.into(),
        corpus_digest: corpus.digest.clone(),
        tuning_digest,
        minima: MINIMA,
        minima_met,
        languages,
        before_after,
        overall,
        overall_previous,
        legacy: legacy_rows,
        rows: rows.clone(),
        leak: LeakReport {
            needles: corpus.needles.clone(),
            cases_searched: u32::try_from(
                rows.len() + opts.include_legacy as usize * legacy_acc.cases.len(),
            )
            .unwrap_or(u32::MAX),
            hits: leak_hits,
        },
        release,
        gates_attested: false,
        report_digest: String::new(),
    };
    report.report_digest = digest_of(&report);
    Ok(report)
}

/// The digest a report carries: sha256 of its JSON with the digest empty.
#[must_use]
pub fn digest_of(report: &WidenedReport) -> String {
    let mut copy = report.clone();
    copy.report_digest = String::new();
    sha256_hex(&serde_json::to_vec(&copy).unwrap_or_default())
}
