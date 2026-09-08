//! Module registration check (M0.4, docs/45 "CI rule to implement", docs/81).
//!
//! Every workspace member (Rust crate via `[package.metadata.modbit]`, TypeScript
//! package via a `"modbit"` object in package.json) must declare:
//! - `owner`: a subsystem node id present in `graph/project-graph.json`;
//! - `canonical`: doc 81 single-owner systems it is the implementation of;
//!   each system has at most one claimant unless `migration_adr` names an
//!   accepted Decision Record;
//! - `capabilities`: `{ id, requirements = [REQ-…] }` entries; every production
//!   capability needs at least one requirement id that exists in the graph.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cargo_metadata::MetadataCommand;
use serde::Deserialize;

use crate::Rules;
use crate::locked::is_accepted_record;

/// One registered production capability.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Capability {
    /// Capability id (e.g. `fs.read`).
    pub id: String,
    /// Requirement ids (`REQ-EV-*`, `REQ-EPR-*`, `REQ-PX-*`).
    #[serde(default)]
    pub requirements: Vec<String>,
}

/// Registration block of one module.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Registration {
    /// Subsystem node id.
    pub owner: String,
    /// Canonical single-owner systems claimed.
    #[serde(default)]
    pub canonical: Vec<String>,
    /// Production capabilities.
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    /// Accepted Decision Record allowing a second claimant during migration.
    #[serde(default)]
    pub migration_adr: Option<String>,
}

/// A workspace member and its registration (if any).
#[derive(Debug, Clone)]
pub struct Module {
    /// Package name.
    pub name: String,
    /// Manifest path (Cargo.toml or package.json).
    pub manifest: PathBuf,
    /// Parsed registration, `None` when missing.
    pub registration: Option<Registration>,
}

/// Graph facts needed by the check.
#[derive(Debug, Clone, Default)]
pub struct GraphFacts {
    /// Subsystem node ids.
    pub subsystems: BTreeSet<String>,
    /// Requirement node ids.
    pub requirements: BTreeSet<String>,
}

impl GraphFacts {
    /// Load from `graph/project-graph.json`.
    pub fn load(path: &Path) -> Result<Self> {
        #[derive(Deserialize)]
        struct Node {
            id: String,
            #[serde(rename = "type")]
            kind: String,
        }
        #[derive(Deserialize)]
        struct G {
            nodes: Vec<Node>,
        }
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let g: G = serde_json::from_str(&text).context("parsing project graph")?;
        let mut f = GraphFacts::default();
        for n in g.nodes {
            match n.kind.as_str() {
                "subsystem" => {
                    f.subsystems.insert(n.id);
                }
                "requirement" => {
                    f.requirements.insert(n.id);
                }
                _ => {}
            }
        }
        Ok(f)
    }
}

/// Collect Rust workspace members (via `cargo metadata`) and TypeScript
/// packages (every `package.json` listed in `ts_manifests`).
pub fn collect(manifest_path: &Path, ts_manifests: &[PathBuf]) -> Result<Vec<Module>> {
    let metadata = MetadataCommand::new()
        .manifest_path(manifest_path)
        .no_deps()
        .exec()
        .with_context(|| format!("running cargo metadata for {}", manifest_path.display()))?;
    let mut out = Vec::new();
    for p in metadata.workspace_packages() {
        let registration = match p.metadata.get("modbit") {
            Some(v) => Some(
                serde_json::from_value::<Registration>(v.clone())
                    .with_context(|| format!("{}: invalid [package.metadata.modbit]", p.name))?,
            ),
            None => None,
        };
        out.push(Module {
            name: p.name.to_string(),
            manifest: p.manifest_path.clone().into_std_path_buf(),
            registration,
        });
    }
    for m in ts_manifests {
        let text =
            std::fs::read_to_string(m).with_context(|| format!("reading {}", m.display()))?;
        let v: serde_json::Value =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", m.display()))?;
        let name = v
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("<unnamed>")
            .to_owned();
        let registration = match v.get("modbit") {
            Some(r) => Some(
                serde_json::from_value::<Registration>(r.clone())
                    .with_context(|| format!("{}: invalid \"modbit\" registration", m.display()))?,
            ),
            None => None,
        };
        out.push(Module {
            name,
            manifest: m.clone(),
            registration,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// One module-registration violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleViolation {
    /// Module name.
    pub module: String,
    /// What is wrong.
    pub problem: String,
}

impl std::fmt::Display for ModuleViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MODULE {} : {}", self.module, self.problem)
    }
}

fn adr_accepted(repo_root: &Path, rel: &str, decisions_dir: &str) -> bool {
    rel.starts_with(&format!("{decisions_dir}/"))
        && std::fs::read_to_string(repo_root.join(rel))
            .map(|t| is_accepted_record(&t))
            .unwrap_or(false)
}

/// Evaluate registrations against the graph facts and canonical-system rules.
pub fn check_modules(
    repo_root: &Path,
    modules: &[Module],
    facts: &GraphFacts,
    rules: &Rules,
) -> Vec<ModuleViolation> {
    let mut out = Vec::new();
    let known: BTreeSet<&str> = rules.canonical.iter().map(|c| c.id.as_str()).collect();
    let mut claims: BTreeMap<&str, Vec<&Module>> = BTreeMap::new();
    for m in modules {
        let Some(r) = &m.registration else {
            out.push(ModuleViolation {
                module: m.name.clone(),
                problem: "no modbit registration (owner/canonical/capabilities)".into(),
            });
            continue;
        };
        if !facts.subsystems.contains(&r.owner) {
            out.push(ModuleViolation {
                module: m.name.clone(),
                problem: format!(
                    "owner `{}` is not a subsystem node in the project graph",
                    r.owner
                ),
            });
        }
        for c in &r.canonical {
            if !known.contains(c.as_str()) {
                out.push(ModuleViolation {
                    module: m.name.clone(),
                    problem: format!(
                        "canonical system `{c}` is not declared in rules.toml [[canonical]]"
                    ),
                });
            }
            claims.entry(c.as_str()).or_default().push(m);
        }
        for cap in &r.capabilities {
            if cap.requirements.is_empty() {
                out.push(ModuleViolation {
                    module: m.name.clone(),
                    problem: format!("capability `{}` registers no requirement id", cap.id),
                });
            }
            for req in &cap.requirements {
                if !facts.requirements.contains(req) {
                    out.push(ModuleViolation {
                        module: m.name.clone(),
                        problem: format!(
                            "capability `{}` names unknown requirement `{req}`",
                            cap.id
                        ),
                    });
                }
            }
        }
        if let Some(adr) = &r.migration_adr
            && !adr_accepted(repo_root, adr, &rules.locked_policy.decisions_dir)
        {
            out.push(ModuleViolation {
                module: m.name.clone(),
                problem: format!("migration_adr `{adr}` is not an accepted Decision Record"),
            });
        }
    }
    for (system, claimants) in &claims {
        if claimants.len() < 2 {
            continue;
        }
        let migrating = claimants
            .iter()
            .filter(|m| {
                m.registration
                    .as_ref()
                    .and_then(|r| r.migration_adr.as_deref())
                    .is_some_and(|a| adr_accepted(repo_root, a, &rules.locked_policy.decisions_dir))
            })
            .count();
        // Exactly one legacy owner and one migrating successor is the only allowed pair.
        if claimants.len() != 2 || migrating != 1 {
            let names: Vec<_> = claimants.iter().map(|m| m.name.as_str()).collect();
            out.push(ModuleViolation {
                module: names.join(", "),
                problem: format!("canonical system `{system}` has {} active implementations without an accepted migration ADR (docs/81: parallel implementations may not exist)", claimants.len()),
            });
        }
    }
    out
}
