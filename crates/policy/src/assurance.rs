//! Factual, policy-owned assurance (REQ-EPR-008; docs/27 §5.1 and §9.3,
//! docs/38 "DeriveRealizedRisk", docs/23): the assurance section of the
//! PolicyEnvelope — minimum assurance, protected surfaces, blast-radius
//! thresholds, review and human conditions, forbidden effects — and the
//! deterministic derivation of a `RealizedRisk` from the facts of a
//! candidate change under revision lock.
//!
//! The derivation takes facts, never scores: which paths changed and how
//! much, what the plan said would change, which effects were requested. A
//! learned risk scalar, a reviewer's confidence and a passing test suite
//! are not inputs — an `Advisory` may be recorded beside the facts, and
//! the rule set is bound to ignore it. Layers only strengthen: a
//! repository or organization layer can add surfaces and raise minima,
//! never remove or lower what the base policy says.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Schema version of the policy and risk records.
pub const ASSURANCE_SCHEMA_VERSION: u32 = 1;
/// Version of the derivation rules below; bumps when a rule changes.
pub const REALIZED_RISK_RULES_VERSION: &str = "risk-rules-1";

/// How much assurance a candidate needs before acceptance (docs/27 §5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssuranceLevel {
    /// Deterministic checks suffice.
    Fast,
    /// Deterministic checks plus the standard completion evidence.
    Standard,
    /// Independent review before acceptance.
    Governed,
    /// Independent review and a human decision before acceptance.
    HighAssurance,
}

impl AssuranceLevel {
    /// Stable label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fast => "FAST",
            Self::Standard => "STANDARD",
            Self::Governed => "GOVERNED",
            Self::HighAssurance => "HIGH_ASSURANCE",
        }
    }
}

/// Factual risk level of a candidate change (docs/27 §9.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskLevel {
    /// Nothing sensitive, small.
    Low,
    /// Some scope, dependency or coverage concern.
    Medium,
    /// A protected surface, a large blast radius or an external effect.
    High,
    /// Auth, secrets, deployment: never accepted without a human.
    Critical,
}

impl RiskLevel {
    /// Stable label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
            Self::Critical => "CRITICAL",
        }
    }

    /// The least assurance this level needs.
    #[must_use]
    pub const fn minimum_assurance(self) -> AssuranceLevel {
        match self {
            Self::Low => AssuranceLevel::Fast,
            Self::Medium => AssuranceLevel::Standard,
            Self::High => AssuranceLevel::Governed,
            Self::Critical => AssuranceLevel::HighAssurance,
        }
    }
}

/// What a protected surface is (docs/27 §9.3 reasons).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SurfaceKind {
    /// Authentication.
    Auth,
    /// Authorization.
    Authorization,
    /// Secrets and credentials.
    Secret,
    /// CI/CD definitions.
    CiCd,
    /// Infrastructure as code.
    Infrastructure,
    /// Deployment.
    Deploy,
    /// Database or schema migrations.
    Migration,
    /// Dependency manifests.
    Dependency,
    /// Lockfiles.
    Lockfile,
    /// Public API surface.
    PublicApi,
    /// The product's own policy and configuration.
    Policy,
}

impl SurfaceKind {
    /// Stable label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Auth => "AUTH",
            Self::Authorization => "AUTHORIZATION",
            Self::Secret => "SECRET",
            Self::CiCd => "CI_CD",
            Self::Infrastructure => "INFRASTRUCTURE",
            Self::Deploy => "DEPLOY",
            Self::Migration => "MIGRATION",
            Self::Dependency => "DEPENDENCY",
            Self::Lockfile => "LOCKFILE",
            Self::PublicApi => "PUBLIC_API",
            Self::Policy => "POLICY",
        }
    }
}

/// A protected surface: paths that match any pattern carry at least the
/// level and the obligations named here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtectedSurface {
    /// Kind.
    pub kind: SurfaceKind,
    /// Patterns: a directory prefix (`.github/`), a path segment
    /// (`/migrations/`), a suffix (`.lock`, `.pem`) or a basename
    /// (`Dockerfile`). Matched against the repository-relative path with
    /// `/` separators.
    pub patterns: Vec<String>,
    /// The least risk level a change here carries.
    pub level: RiskLevel,
    /// Whether independent review is required for a change here.
    pub review_required: bool,
    /// Whether a human decision is required for a change here.
    pub human_required: bool,
    /// Whether the change engine must see a typed question before a write
    /// here (docs/64 DI-9).
    pub question_required: bool,
}

impl ProtectedSurface {
    /// Whether `path` is on this surface.
    #[must_use]
    pub fn matches(&self, path: &str) -> bool {
        let path = path.replace('\\', "/");
        let base = path.rsplit('/').next().unwrap_or(&path);
        self.patterns.iter().any(|p| {
            if let Some(dir) = p.strip_suffix('/') {
                if let Some(seg) = dir.strip_prefix('/') {
                    // `/migrations/`: a segment anywhere.
                    path.split('/').any(|s| s == seg)
                } else {
                    path == dir || path.starts_with(p.as_str())
                }
            } else if let Some(suffix) = p.strip_prefix('*') {
                path.ends_with(suffix)
            } else if p.starts_with('.') && !p.contains('/') {
                path.ends_with(p.as_str())
            } else {
                base == p || path == *p
            }
        })
    }
}

/// Blast-radius thresholds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlastRadius {
    /// Files changed at or above which the change is MEDIUM.
    pub medium_files: u32,
    /// Files changed at or above which the change is HIGH.
    pub high_files: u32,
    /// Lines added plus removed at or above which the change is HIGH.
    pub high_lines: u32,
}

/// The assurance section of the PolicyEnvelope (docs/27 §5.1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssurancePolicy {
    /// Schema version.
    pub schema_version: u32,
    /// The least assurance any candidate needs.
    pub minimum_assurance: AssuranceLevel,
    /// Protected surfaces.
    pub protected_surfaces: Vec<ProtectedSurface>,
    /// Blast-radius thresholds.
    pub blast_radius: BlastRadius,
    /// Risk level at or above which independent review is required.
    pub review_required_at: RiskLevel,
    /// Risk level at or above which a human decision is required.
    pub human_required_at: RiskLevel,
    /// Check ids that must be present and passing at COMPLETION.
    pub required_checks: Vec<String>,
    /// Capabilities never permitted, whatever the profile.
    pub forbidden_effects: Vec<String>,
    /// Whether a change outside the plan's write set requires review.
    pub review_on_unexpected_scope: bool,
    /// Whether an external or destructive effect request requires review.
    pub review_on_external_effect: bool,
}

impl Default for AssurancePolicy {
    fn default() -> Self {
        let s = |kind: SurfaceKind,
                 patterns: &[&str],
                 level: RiskLevel,
                 review: bool,
                 human: bool,
                 question: bool| ProtectedSurface {
            kind,
            patterns: patterns.iter().map(|p| (*p).to_owned()).collect(),
            level,
            review_required: review,
            human_required: human,
            question_required: question,
        };
        Self {
            schema_version: ASSURANCE_SCHEMA_VERSION,
            minimum_assurance: AssuranceLevel::Standard,
            protected_surfaces: vec![
                s(
                    SurfaceKind::Auth,
                    &["/auth/", "/authn/", "/login/", "/oauth/", "/sso/"],
                    RiskLevel::Critical,
                    true,
                    true,
                    false,
                ),
                s(
                    SurfaceKind::Authorization,
                    &["/authz/", "/permissions/", "/rbac/", "/policies/"],
                    RiskLevel::Critical,
                    true,
                    true,
                    false,
                ),
                s(
                    SurfaceKind::Secret,
                    &[
                        ".env",
                        ".pem",
                        ".key",
                        ".p12",
                        ".pfx",
                        "/secrets/",
                        "credentials",
                    ],
                    RiskLevel::Critical,
                    true,
                    true,
                    false,
                ),
                s(
                    SurfaceKind::CiCd,
                    &[
                        ".github/",
                        ".gitlab-ci.yml",
                        ".circleci/",
                        "Jenkinsfile",
                        ".buildkite/",
                    ],
                    RiskLevel::High,
                    true,
                    false,
                    true,
                ),
                s(
                    SurfaceKind::Infrastructure,
                    &[
                        ".tf",
                        "/terraform/",
                        "/infra/",
                        "/k8s/",
                        "/helm/",
                        "Dockerfile",
                        "docker-compose.yml",
                    ],
                    RiskLevel::High,
                    true,
                    false,
                    true,
                ),
                s(
                    SurfaceKind::Deploy,
                    &["/deploy/", "/deployment/", "/release/"],
                    RiskLevel::Critical,
                    true,
                    true,
                    true,
                ),
                s(
                    SurfaceKind::Migration,
                    &["/migrations/", "/migrate/", "/schema/"],
                    RiskLevel::High,
                    true,
                    false,
                    false,
                ),
                s(
                    SurfaceKind::Dependency,
                    &[
                        "Cargo.toml",
                        "package.json",
                        "pyproject.toml",
                        "requirements.txt",
                        "go.mod",
                        "Gemfile",
                        "pom.xml",
                        "build.gradle",
                    ],
                    RiskLevel::Medium,
                    false,
                    false,
                    false,
                ),
                s(
                    SurfaceKind::Lockfile,
                    &[
                        "Cargo.lock",
                        "package-lock.json",
                        "pnpm-lock.yaml",
                        "yarn.lock",
                        "poetry.lock",
                        "go.sum",
                        "Gemfile.lock",
                    ],
                    RiskLevel::Medium,
                    true,
                    false,
                    false,
                ),
                s(
                    SurfaceKind::Policy,
                    &[".modbit/"],
                    RiskLevel::High,
                    true,
                    false,
                    true,
                ),
            ],
            blast_radius: BlastRadius {
                medium_files: 10,
                high_files: 30,
                high_lines: 800,
            },
            review_required_at: RiskLevel::High,
            human_required_at: RiskLevel::Critical,
            required_checks: vec![],
            forbidden_effects: vec![],
            review_on_unexpected_scope: true,
            review_on_external_effect: true,
        }
    }
}

/// A layer an organization or a repository adds on top of the base policy.
/// Every field only strengthens: surfaces are added, minima are raised,
/// thresholds are lowered, obligations are turned on. Nothing here can
/// remove a surface or lower a minimum.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssuranceLayer {
    /// Raise the minimum assurance to at least this.
    #[serde(default)]
    pub minimum_assurance: Option<AssuranceLevel>,
    /// Surfaces to add.
    #[serde(default)]
    pub protected_surfaces: Vec<ProtectedSurface>,
    /// Lower the blast-radius thresholds to at most these.
    #[serde(default)]
    pub blast_radius: Option<BlastRadius>,
    /// Lower the level at which review is required.
    #[serde(default)]
    pub review_required_at: Option<RiskLevel>,
    /// Lower the level at which a human is required.
    #[serde(default)]
    pub human_required_at: Option<RiskLevel>,
    /// Checks to require.
    #[serde(default)]
    pub required_checks: Vec<String>,
    /// Capabilities to forbid.
    #[serde(default)]
    pub forbidden_effects: Vec<String>,
    /// Anything a layer says that would weaken the policy is recorded here
    /// by `strengthen_with` and ignored — a repository cannot lower what an
    /// organization set.
    #[serde(default)]
    pub weaken_attempts: Vec<String>,
}

impl AssurancePolicy {
    /// Apply a layer; returns the strengthened policy and what the layer
    /// tried to weaken (ignored).
    #[must_use]
    pub fn strengthen_with(&self, layer: &AssuranceLayer) -> (Self, Vec<String>) {
        let mut out = self.clone();
        let mut ignored = Vec::new();
        if let Some(m) = layer.minimum_assurance {
            if m > out.minimum_assurance {
                out.minimum_assurance = m;
            } else if m < out.minimum_assurance {
                ignored.push(format!(
                    "minimum_assurance {} below {}",
                    m.label(),
                    out.minimum_assurance.label()
                ));
            }
        }
        for s in &layer.protected_surfaces {
            if !out.protected_surfaces.contains(s) {
                out.protected_surfaces.push(s.clone());
            }
        }
        if let Some(b) = &layer.blast_radius {
            if b.medium_files > out.blast_radius.medium_files
                || b.high_files > out.blast_radius.high_files
                || b.high_lines > out.blast_radius.high_lines
            {
                ignored.push("blast_radius thresholds above the base".into());
            }
            out.blast_radius.medium_files = out.blast_radius.medium_files.min(b.medium_files);
            out.blast_radius.high_files = out.blast_radius.high_files.min(b.high_files);
            out.blast_radius.high_lines = out.blast_radius.high_lines.min(b.high_lines);
        }
        if let Some(r) = layer.review_required_at {
            if r > out.review_required_at {
                ignored.push(format!(
                    "review_required_at {} above {}",
                    r.label(),
                    out.review_required_at.label()
                ));
            }
            out.review_required_at = out.review_required_at.min(r);
        }
        if let Some(h) = layer.human_required_at {
            if h > out.human_required_at {
                ignored.push(format!(
                    "human_required_at {} above {}",
                    h.label(),
                    out.human_required_at.label()
                ));
            }
            out.human_required_at = out.human_required_at.min(h);
        }
        for c in &layer.required_checks {
            if !out.required_checks.contains(c) {
                out.required_checks.push(c.clone());
            }
        }
        for e in &layer.forbidden_effects {
            if !out.forbidden_effects.contains(e) {
                out.forbidden_effects.push(e.clone());
            }
        }
        (out, ignored)
    }

    /// Content digest of the policy: its version.
    #[must_use]
    pub fn version(&self) -> String {
        let bytes = serde_json::to_vec(self).unwrap_or_default();
        format!("assurance-{}", &hex::encode(Sha256::digest(&bytes))[..16])
    }

    /// Paths the change engine must not write without a typed question
    /// (docs/64 DI-9): every pattern of a surface with `question_required`.
    #[must_use]
    pub fn question_required_patterns(&self) -> Vec<String> {
        self.protected_surfaces
            .iter()
            .filter(|s| s.question_required)
            .flat_map(|s| s.patterns.iter().cloned())
            .collect()
    }

    /// The surfaces a path is on.
    #[must_use]
    pub fn surfaces_of(&self, path: &str) -> Vec<&ProtectedSurface> {
        self.protected_surfaces
            .iter()
            .filter(|s| s.matches(path))
            .collect()
    }
}

/// How a path changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ChangeKind {
    /// Created.
    Created,
    /// Modified.
    Modified,
    /// Deleted.
    Deleted,
}

/// One changed path, as a fact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedPath {
    /// Repository-relative path.
    pub path: String,
    /// How.
    pub change: ChangeKind,
    /// Lines added.
    pub lines_added: u32,
    /// Lines removed.
    pub lines_removed: u32,
}

/// An effect the candidate requested (an approval opened for it, or a
/// protected capability invoked).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestedEffect {
    /// Tool.
    pub tool: String,
    /// Effect class label (`Destructive`, `ExternalSideEffect`, ...).
    pub effect_class: String,
    /// Capability id, when known.
    pub capability: String,
    /// Whether an approval resolved it.
    pub approved: bool,
}

/// Signals the derivation is bound to ignore. They are recorded so an
/// audit can see what was on the table, and so a test can prove they
/// changed nothing.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Advisory {
    /// A learned risk score, if some component produced one.
    pub learned_risk: Option<f64>,
    /// A reviewer's or a model's confidence.
    pub confidence: Option<f64>,
    /// Whether the checks passed.
    pub checks_passed: Option<bool>,
}

/// The facts of a candidate change under revision lock.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CandidateFacts {
    /// Candidate workspace revision.
    pub candidate_revision: u64,
    /// Every changed path.
    pub changed: Vec<ChangedPath>,
    /// The plan's write set, when a plan exists.
    pub plan_write_set: Option<Vec<String>>,
    /// Effects requested during the candidate's runs.
    pub requested_effects: Vec<RequestedEffect>,
    /// Advisory signals, recorded and ignored.
    pub advisory: Advisory,
}

/// One reason a candidate carries risk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskReason {
    /// Stable code: `PROTECTED_SURFACE`, `BLAST_RADIUS`, `UNEXPECTED_SCOPE`,
    /// `SPARSE_COVERAGE`, `EXTERNAL_EFFECT`, `FORBIDDEN_EFFECT`.
    pub code: String,
    /// The surface kind, for `PROTECTED_SURFACE`.
    pub surface: Option<SurfaceKind>,
    /// The level this reason alone implies.
    pub level: RiskLevel,
    /// Paths (or tools) concerned.
    pub paths: Vec<String>,
    /// Detail.
    pub detail: String,
}

/// The factual, revision-bound risk of a candidate (docs/27 §9.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealizedRisk {
    /// Schema version.
    pub schema_version: u32,
    /// Rules version.
    pub realized_risk_version: String,
    /// Policy version the risk was derived under.
    pub policy_version: String,
    /// Candidate revision.
    pub candidate_revision: u64,
    /// Level.
    pub level: RiskLevel,
    /// Reasons, sorted by code then path.
    pub reasons: Vec<RiskReason>,
    /// The least assurance acceptance needs: the stricter of the policy's
    /// minimum and what the facts imply.
    pub minimum_assurance: AssuranceLevel,
    /// Independent review required.
    pub independent_review_required: bool,
    /// Human decision required.
    pub human_required: bool,
    /// Forbidden effects the candidate requested (never acceptable).
    pub forbidden_effects_requested: Vec<String>,
    /// Evidence: the facts digest and what was ignored.
    pub evidence_refs: Vec<String>,
    /// sha256 of the facts (without the advisory).
    pub facts_digest: String,
}

fn is_test_path(p: &str) -> bool {
    let p = p.replace('\\', "/");
    let base = p.rsplit('/').next().unwrap_or(&p);
    p.contains("/tests/")
        || p.contains("/test/")
        || p.contains("/__tests__/")
        || p.starts_with("tests/")
        || p.starts_with("test/")
        || base.starts_with("test_")
        || base.contains(".test.")
        || base.contains(".spec.")
        || base.ends_with("_test.go")
        || base.ends_with("_test.py")
        || base.ends_with("_test.rs")
}

fn is_source_path(p: &str) -> bool {
    let ext = p.rsplit('.').next().unwrap_or("");
    matches!(
        ext,
        "rs" | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "py"
            | "go"
            | "java"
            | "kt"
            | "rb"
            | "cs"
            | "c"
            | "cc"
            | "cpp"
            | "h"
            | "hpp"
            | "swift"
            | "scala"
            | "php"
            | "ex"
            | "exs"
    ) && !is_test_path(p)
}

/// Derive the realized risk of a candidate. Deterministic in the facts;
/// the advisory is recorded and ignored.
#[must_use]
pub fn derive_realized_risk(policy: &AssurancePolicy, facts: &CandidateFacts) -> RealizedRisk {
    let mut reasons: Vec<RiskReason> = Vec::new();
    let mut review = false;
    let mut human = false;
    // 1. Protected surfaces.
    for c in &facts.changed {
        for s in policy.surfaces_of(&c.path) {
            reasons.push(RiskReason {
                code: "PROTECTED_SURFACE".into(),
                surface: Some(s.kind),
                level: s.level,
                paths: vec![c.path.clone()],
                detail: format!("{} surface: {}", s.kind.label(), c.path),
            });
            review |= s.review_required;
            human |= s.human_required;
        }
    }
    // 2. Blast radius.
    let files = facts.changed.len() as u32;
    let lines: u32 = facts
        .changed
        .iter()
        .map(|c| c.lines_added + c.lines_removed)
        .sum();
    if files >= policy.blast_radius.high_files || lines >= policy.blast_radius.high_lines {
        reasons.push(RiskReason {
            code: "BLAST_RADIUS".into(),
            surface: None,
            level: RiskLevel::High,
            paths: vec![],
            detail: format!("{files} files, {lines} lines changed"),
        });
    } else if files >= policy.blast_radius.medium_files {
        reasons.push(RiskReason {
            code: "BLAST_RADIUS".into(),
            surface: None,
            level: RiskLevel::Medium,
            paths: vec![],
            detail: format!("{files} files changed"),
        });
    }
    // 3. Unexpected scope: changes the plan did not declare.
    if let Some(ws) = &facts.plan_write_set {
        let declared = |p: &str| {
            ws.iter()
                .any(|e| e == p || (e.ends_with('/') && p.starts_with(e.as_str())))
        };
        let outside: Vec<String> = facts
            .changed
            .iter()
            .filter(|c| !declared(&c.path))
            .map(|c| c.path.clone())
            .collect();
        if !outside.is_empty() {
            let n = outside.len();
            reasons.push(RiskReason {
                code: "UNEXPECTED_SCOPE".into(),
                surface: None,
                level: if n >= 3 {
                    RiskLevel::High
                } else {
                    RiskLevel::Medium
                },
                paths: outside,
                detail: format!("{n} path(s) outside the plan's write set"),
            });
            review |= policy.review_on_unexpected_scope;
        }
    }
    // 4. Sparse coverage: source changed, no test changed.
    let source_changed: Vec<String> = facts
        .changed
        .iter()
        .filter(|c| c.change != ChangeKind::Deleted && is_source_path(&c.path))
        .map(|c| c.path.clone())
        .collect();
    let tests_changed = facts.changed.iter().any(|c| is_test_path(&c.path));
    if !source_changed.is_empty() && !tests_changed {
        reasons.push(RiskReason {
            code: "SPARSE_COVERAGE".into(),
            surface: None,
            level: RiskLevel::Medium,
            paths: source_changed,
            detail: "source changed without a test change".into(),
        });
    }
    // 5. External or destructive effects; forbidden effects.
    let mut forbidden = Vec::new();
    for e in &facts.requested_effects {
        if policy
            .forbidden_effects
            .iter()
            .any(|f| f == &e.capability || f == &e.tool)
        {
            forbidden.push(e.tool.clone());
            reasons.push(RiskReason {
                code: "FORBIDDEN_EFFECT".into(),
                surface: None,
                level: RiskLevel::Critical,
                paths: vec![e.tool.clone()],
                detail: format!("`{}` is forbidden by policy", e.tool),
            });
            human = true;
        } else if matches!(
            e.effect_class.as_str(),
            "ExternalSideEffect" | "Destructive"
        ) {
            reasons.push(RiskReason {
                code: "EXTERNAL_EFFECT".into(),
                surface: None,
                level: RiskLevel::High,
                paths: vec![e.tool.clone()],
                detail: format!(
                    "{} effect requested ({}){}",
                    e.effect_class,
                    e.tool,
                    if e.approved {
                        ", approved"
                    } else {
                        ", not approved"
                    }
                ),
            });
            review |= policy.review_on_external_effect;
        }
    }
    let level = reasons
        .iter()
        .map(|r| r.level)
        .max()
        .unwrap_or(RiskLevel::Low);
    review |= level >= policy.review_required_at;
    human |= level >= policy.human_required_at;
    let minimum_assurance = policy
        .minimum_assurance
        .max(level.minimum_assurance())
        .max(if human {
            AssuranceLevel::HighAssurance
        } else if review {
            AssuranceLevel::Governed
        } else {
            AssuranceLevel::Fast
        });
    reasons.sort_by(|a, b| {
        a.code
            .cmp(&b.code)
            .then_with(|| a.paths.cmp(&b.paths))
            .then_with(|| a.surface.cmp(&b.surface))
    });
    reasons.dedup();
    let facts_digest = {
        let mut without = facts.clone();
        without.advisory = Advisory::default();
        hex::encode(Sha256::digest(
            serde_json::to_vec(&without).unwrap_or_default(),
        ))
    };
    let mut evidence_refs = vec![format!("facts:{facts_digest}")];
    if facts.advisory != Advisory::default() {
        evidence_refs.push(format!(
            "advisory_ignored:{}",
            serde_json::to_string(&facts.advisory).unwrap_or_default()
        ));
    }
    RealizedRisk {
        schema_version: ASSURANCE_SCHEMA_VERSION,
        realized_risk_version: REALIZED_RISK_RULES_VERSION.into(),
        policy_version: policy.version(),
        candidate_revision: facts.candidate_revision,
        level,
        reasons,
        minimum_assurance,
        independent_review_required: review,
        human_required: human,
        forbidden_effects_requested: forbidden,
        evidence_refs,
        facts_digest,
    }
}

/// Post-draft facts may strengthen assurance but never weaken it
/// (docs/27 §9.3): the result of a later derivation is joined with the
/// earlier one — the higher level, the union of reasons, every obligation
/// that either required.
#[must_use]
pub fn strengthen_only(earlier: &RealizedRisk, later: &RealizedRisk) -> RealizedRisk {
    let mut out = later.clone();
    out.level = earlier.level.max(later.level);
    out.minimum_assurance = earlier.minimum_assurance.max(later.minimum_assurance);
    out.independent_review_required =
        earlier.independent_review_required || later.independent_review_required;
    out.human_required = earlier.human_required || later.human_required;
    for r in &earlier.reasons {
        if !out.reasons.contains(r) {
            out.reasons.push(r.clone());
        }
    }
    out.reasons
        .sort_by(|a, b| a.code.cmp(&b.code).then_with(|| a.paths.cmp(&b.paths)));
    let mut forbidden: BTreeSet<String> = out.forbidden_effects_requested.iter().cloned().collect();
    forbidden.extend(earlier.forbidden_effects_requested.iter().cloned());
    out.forbidden_effects_requested = forbidden.into_iter().collect();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed(path: &str, added: u32, removed: u32) -> ChangedPath {
        ChangedPath {
            path: path.into(),
            change: ChangeKind::Modified,
            lines_added: added,
            lines_removed: removed,
        }
    }

    #[test]
    fn surfaces_match_prefixes_segments_suffixes_and_basenames() {
        let p = AssurancePolicy::default();
        let kinds = |path: &str| -> Vec<SurfaceKind> {
            p.surfaces_of(path).iter().map(|s| s.kind).collect()
        };
        assert_eq!(kinds(".github/workflows/ci.yml"), vec![SurfaceKind::CiCd]);
        assert_eq!(kinds("src/auth/login.py"), vec![SurfaceKind::Auth]);
        assert_eq!(kinds("db/migrations/002.sql"), vec![SurfaceKind::Migration]);
        assert_eq!(kinds("Cargo.lock"), vec![SurfaceKind::Lockfile]);
        assert_eq!(kinds("apps/x/Cargo.toml"), vec![SurfaceKind::Dependency]);
        assert_eq!(kinds("config/prod.pem"), vec![SurfaceKind::Secret]);
        assert_eq!(kinds("src/lib.rs"), vec![]);
        assert_eq!(kinds("src\\auth\\login.py"), vec![SurfaceKind::Auth]);
    }

    #[test]
    fn a_plain_change_is_low_and_passing_tests_or_confidence_change_nothing() {
        let p = AssurancePolicy::default();
        let mut facts = CandidateFacts {
            candidate_revision: 4,
            changed: vec![
                changed("src/lib.rs", 3, 1),
                changed("tests/lib_test.rs", 5, 0),
            ],
            plan_write_set: Some(vec!["src/lib.rs".into(), "tests/lib_test.rs".into()]),
            requested_effects: vec![],
            advisory: Advisory::default(),
        };
        let a = derive_realized_risk(&p, &facts);
        assert_eq!(a.level, RiskLevel::Low);
        assert_eq!(a.minimum_assurance, AssuranceLevel::Standard);
        assert!(!a.independent_review_required && !a.human_required);
        facts.advisory = Advisory {
            learned_risk: Some(0.0),
            confidence: Some(0.99),
            checks_passed: Some(true),
        };
        let b = derive_realized_risk(&p, &facts);
        assert_eq!(
            (
                b.level,
                b.minimum_assurance,
                b.independent_review_required,
                b.human_required
            ),
            (
                a.level,
                a.minimum_assurance,
                a.independent_review_required,
                a.human_required
            )
        );
        assert_eq!(b.reasons, a.reasons);
        assert_eq!(b.facts_digest, a.facts_digest, "the advisory is not a fact");
        assert!(
            b.evidence_refs
                .iter()
                .any(|e| e.starts_with("advisory_ignored:"))
        );
    }

    #[test]
    fn a_critical_surface_needs_a_human_whatever_else_is_true() {
        let p = AssurancePolicy::default();
        let facts = CandidateFacts {
            candidate_revision: 9,
            changed: vec![
                changed("src/auth/login.py", 2, 2),
                changed("tests/test_login.py", 4, 0),
            ],
            plan_write_set: Some(vec![
                "src/auth/login.py".into(),
                "tests/test_login.py".into(),
            ]),
            requested_effects: vec![],
            advisory: Advisory {
                learned_risk: Some(0.01),
                confidence: Some(1.0),
                checks_passed: Some(true),
            },
        };
        let r = derive_realized_risk(&p, &facts);
        assert_eq!(r.level, RiskLevel::Critical);
        assert_eq!(r.minimum_assurance, AssuranceLevel::HighAssurance);
        assert!(r.independent_review_required && r.human_required);
        assert_eq!(r.reasons[0].code, "PROTECTED_SURFACE");
        assert_eq!(r.reasons[0].surface, Some(SurfaceKind::Auth));
    }

    #[test]
    fn layers_only_strengthen() {
        let base = AssurancePolicy::default();
        let weaker = AssuranceLayer {
            minimum_assurance: Some(AssuranceLevel::Fast),
            blast_radius: Some(BlastRadius {
                medium_files: 100,
                high_files: 1000,
                high_lines: 100_000,
            }),
            review_required_at: Some(RiskLevel::Critical),
            human_required_at: Some(RiskLevel::Critical),
            protected_surfaces: vec![],
            required_checks: vec![],
            forbidden_effects: vec![],
            weaken_attempts: vec![],
        };
        let (p, ignored) = base.strengthen_with(&weaker);
        assert_eq!(p.minimum_assurance, base.minimum_assurance);
        assert_eq!(p.blast_radius, base.blast_radius);
        assert_eq!(p.review_required_at, base.review_required_at);
        assert_eq!(ignored.len(), 3, "{ignored:?}");
        let stronger = AssuranceLayer {
            minimum_assurance: Some(AssuranceLevel::Governed),
            protected_surfaces: vec![ProtectedSurface {
                kind: SurfaceKind::PublicApi,
                patterns: vec!["/api/".into()],
                level: RiskLevel::High,
                review_required: true,
                human_required: false,
                question_required: false,
            }],
            review_required_at: Some(RiskLevel::Medium),
            forbidden_effects: vec!["deploy".into()],
            ..Default::default()
        };
        let (p, ignored) = base.strengthen_with(&stronger);
        assert!(ignored.is_empty());
        assert_eq!(p.minimum_assurance, AssuranceLevel::Governed);
        assert_eq!(p.review_required_at, RiskLevel::Medium);
        assert_eq!(
            p.surfaces_of("src/api/v1.rs")[0].kind,
            SurfaceKind::PublicApi
        );
        assert_ne!(p.version(), base.version());
        let facts = CandidateFacts {
            candidate_revision: 1,
            changed: vec![changed("src/lib.rs", 1, 1)],
            plan_write_set: None,
            requested_effects: vec![RequestedEffect {
                tool: "deploy".into(),
                effect_class: "ExternalSideEffect".into(),
                capability: "deploy".into(),
                approved: true,
            }],
            advisory: Advisory::default(),
        };
        let r = derive_realized_risk(&p, &facts);
        assert_eq!(r.forbidden_effects_requested, vec!["deploy".to_owned()]);
        assert_eq!(r.level, RiskLevel::Critical);
        assert!(r.human_required);
    }

    #[test]
    fn later_facts_strengthen_and_never_weaken() {
        let p = AssurancePolicy::default();
        let critical = derive_realized_risk(
            &p,
            &CandidateFacts {
                candidate_revision: 2,
                changed: vec![changed("secrets/prod.env", 1, 0)],
                ..Default::default()
            },
        );
        let benign = derive_realized_risk(
            &p,
            &CandidateFacts {
                candidate_revision: 3,
                changed: vec![changed("README.md", 1, 0)],
                ..Default::default()
            },
        );
        let joined = strengthen_only(&critical, &benign);
        assert_eq!(joined.level, RiskLevel::Critical);
        assert!(joined.human_required);
        assert_eq!(joined.candidate_revision, 3);
        assert!(
            joined
                .reasons
                .iter()
                .any(|r| r.surface == Some(SurfaceKind::Secret))
        );
    }
}
