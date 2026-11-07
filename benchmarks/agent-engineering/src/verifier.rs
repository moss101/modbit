//! A task's own verifier (PX-184, QUAL-PX-184): what the harness checks after
//! a trial, from observable outcomes only — the workspace's files, the tool
//! results the Core stored in its object store, and the Core's own event log
//! (`MemoryUsed`, `MemoryProposed`, `SkillSelected`, `SubagentAdmitted`,
//! `SubagentResultRecorded`, `MergeTransactionRecorded`). Nothing the model
//! says about its work is read.
//!
//! The seeds a task needs (curated memory, owner-trusted skills) are put in
//! place through the product's own commands before the run; the verifier
//! names them by key and the harness resolves each key to the id or content
//! hash the Core assigned.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::events::Event;

/// What must hold after the trial. Every listed check must pass.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Verifier {
    /// Stable id of this verifier; unique in its suite.
    pub id: String,
    /// Workspace files and their content.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileCheck>,
    /// Tool calls of the trial's task that the Core recorded as succeeded.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolCheck>,
    /// Engineering memory the task used or recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<MemoryCheck>,
    /// Seeded skills whose instructions reached the task (by name).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills_used: Vec<String>,
    /// Children the task delegated to and integrated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagents: Option<SubagentCheck>,
}

/// A workspace file after the run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileCheck {
    /// Root-relative path.
    pub path: String,
    /// Exact content (line endings normalized to LF).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equals: Option<String>,
    /// Substrings the content must contain.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contains: Vec<String>,
}

/// Succeeded calls of one tool, from the Core's stored tool results.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCheck {
    /// Tool name (`fs.read`, `search.exact`, ...).
    pub tool: String,
    /// Least number of succeeded calls.
    #[serde(default = "one")]
    pub min: u32,
    /// Text one of those calls' structured output must contain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_contains: Option<String>,
}

fn one() -> u32 {
    1
}

/// Memory checks; keys name the task's memory seeds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryCheck {
    /// Seeds whose curated item reached the task: injected into a prompt
    /// (`MemoryUsed`) or returned by a `memory.query` the task made.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub used: Vec<String>,
    /// Seeds whose item must not have reached the task (a forgotten one).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_used: Vec<String>,
    /// Items the agent itself proposed (`MemoryProposed` by an `agent:` author).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proposed: Vec<ProposalCheck>,
}

/// One proposal the agent must have recorded.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalCheck {
    /// Topic (compared trimmed and case-insensitively).
    pub topic: String,
    /// Record type, when it matters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_type: Option<String>,
    /// Text the content must contain (case-insensitive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_contains: Option<String>,
}

/// Delegation checks on the trial's task.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentCheck {
    /// Least number of children admitted (`SubagentAdmitted`).
    pub admitted: u32,
    /// Least number of result envelopes recorded (`SubagentResultRecorded`).
    pub results: u32,
    /// Least number of distinct children merged (`MergeTransactionRecorded`
    /// in state `COMMITTED`).
    pub merged: u32,
}

/// What the harness puts in place before the run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seed {
    /// Curated memory: proposed and promoted with `memory propose|promote`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub memory: Vec<MemorySeed>,
    /// Skills written to the profile and trusted with `skill trust name@hash`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<SkillSeed>,
}

/// A memory item the person curated before the run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySeed {
    /// Key the verifier names it by.
    pub key: String,
    /// Scope kind (`repository`, `session`, ...).
    #[serde(default = "repository")]
    pub scope: String,
    /// Record type (`fact`, `decision`, `convention`, ...).
    pub record_type: String,
    /// Topic.
    pub topic: String,
    /// Content.
    pub content: String,
    /// Forgotten after promotion (`memory forget`): it must not reach the task.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub forget: bool,
}

fn repository() -> String {
    "repository".into()
}

/// A skill the owner reviewed and trusted by content hash before the run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillSeed {
    /// Skill name (the directory and the front matter's `name`).
    pub name: String,
    /// One-line description (the index line).
    pub description: String,
    /// The instructions.
    pub body: String,
}

impl Seed {
    /// Nothing to seed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.memory.is_empty() && self.skills.is_empty()
    }

    /// Structural checks.
    ///
    /// # Errors
    /// A duplicate key or name, or an empty field.
    pub fn check(&self) -> Result<(), String> {
        let mut keys = std::collections::BTreeSet::new();
        for m in &self.memory {
            if m.key.trim().is_empty() || m.topic.trim().is_empty() || m.content.trim().is_empty() {
                return Err(format!("memory seed {:?} has an empty field", m.key));
            }
            if !keys.insert(m.key.as_str()) {
                return Err(format!("duplicate memory seed {}", m.key));
            }
        }
        let mut names = std::collections::BTreeSet::new();
        for s in &self.skills {
            let ok = !s.name.is_empty()
                && s.name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
            if !ok || s.body.trim().is_empty() || s.description.trim().is_empty() {
                return Err(format!("skill seed {:?} is malformed", s.name));
            }
            if !names.insert(s.name.as_str()) {
                return Err(format!("duplicate skill seed {}", s.name));
            }
        }
        Ok(())
    }

    /// The `SKILL.md` the harness writes for a seeded skill.
    #[must_use]
    pub fn skill_file(s: &SkillSeed) -> String {
        format!(
            "---\nname: {}\nversion: 1.0.0\ndescription: {}\n---\n{}\n",
            s.name,
            s.description,
            s.body.trim_end()
        )
    }
}

impl Verifier {
    /// Number of individual checks.
    #[must_use]
    pub fn check_count(&self) -> usize {
        let m = self
            .memory
            .as_ref()
            .map_or(0, |m| m.used.len() + m.not_used.len() + m.proposed.len());
        self.files.len()
            + self.tools.len()
            + m
            + self.skills_used.len()
            + usize::from(self.subagents.is_some())
    }

    /// The checks without the id: two tasks with the same checks share a
    /// verifier, which QUAL-PX-184 refuses.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        let mut v = self.clone();
        v.id.clear();
        serde_json::to_string(&v).unwrap_or_default()
    }

    /// Structural checks.
    ///
    /// # Errors
    /// No id, no check, or a check that cannot fail.
    pub fn check(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("verifier has no id".into());
        }
        if self.check_count() == 0 {
            return Err(format!("verifier {} checks nothing", self.id));
        }
        for f in &self.files {
            if f.path.trim().is_empty() || (f.equals.is_none() && f.contains.is_empty()) {
                return Err(format!(
                    "verifier {}: file check {:?} states no content",
                    self.id, f.path
                ));
            }
        }
        for t in &self.tools {
            if t.min == 0 {
                return Err(format!(
                    "verifier {}: tool check {} with min 0 cannot fail",
                    self.id, t.tool
                ));
            }
        }
        if let Some(s) = &self.subagents
            && s.admitted == 0
            && s.results == 0
            && s.merged == 0
        {
            return Err(format!(
                "verifier {}: a sub-agent check of zeros cannot fail",
                self.id
            ));
        }
        Ok(())
    }
}

/// A tool result as the Core stored it (`ToolCallSucceeded.result_ref`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolResultRecord {
    /// Tool name.
    pub tool_name: String,
    /// `SUCCESS`, `FAILED`, ...
    pub status: String,
    /// Structured output.
    #[serde(default)]
    pub structured_output: Value,
}

/// What one check found.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    /// The check, as text.
    pub check: String,
    /// Whether it held.
    pub passed: bool,
    /// What was observed.
    pub detail: String,
}

/// A verifier's verdict on one trial.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifierOutcome {
    /// Verifier id.
    pub id: String,
    /// Every check held.
    pub passed: bool,
    /// Each check.
    pub checks: Vec<CheckResult>,
}

/// What the harness observed after a trial.
pub struct Observed<'a> {
    /// The trial session's whole event log.
    pub events: &'a [Event],
    /// The trial's task (hex, with or without dashes).
    pub task: &'a str,
    /// Tool results of the trial's task, from the Core's object store.
    pub tool_results: &'a [ToolResultRecord],
    /// The workspace root.
    pub workspace: &'a Path,
    /// Memory seed key → the id the Core assigned.
    pub memory_ids: &'a BTreeMap<String, String>,
    /// Skill seed name → the content hash the owner trusted.
    pub skill_hashes: &'a BTreeMap<String, String>,
}

fn norm(id: &str) -> String {
    id.replace('-', "").to_ascii_lowercase()
}

impl Observed<'_> {
    fn of_task(&self) -> impl Iterator<Item = &Event> {
        let want = norm(self.task);
        self.events
            .iter()
            .filter(move |e| e.task_id.as_deref().is_some_and(|t| norm(t) == want))
    }

    fn succeeded(&self, tool: &str) -> impl Iterator<Item = &ToolResultRecord> {
        let tool = tool.to_owned();
        self.tool_results
            .iter()
            .filter(move |r| r.tool_name == tool && r.status == "SUCCESS")
    }

    fn memory_reached(&self, id: &str) -> bool {
        let injected = self
            .of_task()
            .any(|e| e.event_type == "MemoryUsed" && e.payload["memory_id"].as_str() == Some(id));
        let queried = self
            .succeeded("memory.query")
            .any(|r| r.structured_output.to_string().contains(id));
        injected || queried
    }
}

/// Evaluate a verifier against what a trial left behind.
#[must_use]
pub fn evaluate(v: &Verifier, o: &Observed<'_>) -> VerifierOutcome {
    let mut checks = Vec::new();
    let mut push = |check: String, passed: bool, detail: String| {
        checks.push(CheckResult {
            check,
            passed,
            detail,
        });
    };
    for f in &v.files {
        let content = std::fs::read_to_string(o.workspace.join(&f.path))
            .ok()
            .map(|c| c.replace("\r\n", "\n"));
        let (passed, detail) = match &content {
            None => (false, "missing".to_owned()),
            Some(c) => {
                let eq = f.equals.as_ref().is_none_or(|e| c == e);
                let missing: Vec<&String> = f
                    .contains
                    .iter()
                    .filter(|s| !c.contains(s.as_str()))
                    .collect();
                (
                    eq && missing.is_empty(),
                    format!(
                        "{} bytes{}{}",
                        c.len(),
                        if eq { "" } else { "; content differs" },
                        if missing.is_empty() {
                            String::new()
                        } else {
                            format!("; lacks {missing:?}")
                        }
                    ),
                )
            }
        };
        push(format!("file {}", f.path), passed, detail);
    }
    for t in &v.tools {
        let ok: Vec<&ToolResultRecord> = o.succeeded(&t.tool).collect();
        let matching = ok
            .iter()
            .filter(|r| {
                t.output_contains
                    .as_ref()
                    .is_none_or(|s| r.structured_output.to_string().contains(s.as_str()))
            })
            .count();
        let enough = u32::try_from(ok.len()).unwrap_or(u32::MAX) >= t.min;
        push(
            format!(
                "tool {} succeeded >= {}{}",
                t.tool,
                t.min,
                t.output_contains
                    .as_ref()
                    .map(|s| format!(" with output containing {s:?}"))
                    .unwrap_or_default()
            ),
            enough && matching > 0,
            format!("{} succeeded, {matching} matching", ok.len()),
        );
    }
    if let Some(m) = &v.memory {
        for key in &m.used {
            let (passed, detail) = match o.memory_ids.get(key) {
                None => (false, "seed was not recorded".to_owned()),
                Some(id) => (
                    o.memory_reached(id),
                    format!("memory {}", &id[..id.len().min(12)]),
                ),
            };
            push(format!("memory {key} reached the task"), passed, detail);
        }
        for key in &m.not_used {
            let (passed, detail) = match o.memory_ids.get(key) {
                None => (false, "seed was not recorded".to_owned()),
                Some(id) => (
                    !o.memory_reached(id),
                    format!("memory {}", &id[..id.len().min(12)]),
                ),
            };
            push(format!("memory {key} kept out of the task"), passed, detail);
        }
        for p in &m.proposed {
            let found = o
                .of_task()
                .filter(|e| e.event_type == "MemoryProposed")
                .any(|e| {
                    let doc: Value = e.payload["row"]["doc"]
                        .as_str()
                        .and_then(|d| serde_json::from_str(d).ok())
                        .unwrap_or_default();
                    doc["author"]
                        .as_str()
                        .is_some_and(|a| a.starts_with("agent:"))
                        && doc["topic"]
                            .as_str()
                            .is_some_and(|t| t.trim().eq_ignore_ascii_case(p.topic.trim()))
                        && p.record_type
                            .as_ref()
                            .is_none_or(|r| doc["record_type"].as_str() == Some(r.as_str()))
                        && p.content_contains.as_ref().is_none_or(|c| {
                            doc["content"]
                                .as_str()
                                .is_some_and(|x| x.to_lowercase().contains(&c.to_lowercase()))
                        })
                });
            push(
                format!("agent proposed memory {:?}", p.topic),
                found,
                String::new(),
            );
        }
    }
    for name in &v.skills_used {
        let (passed, detail) = match o.skill_hashes.get(name) {
            None => (false, "skill was not trusted before the run".to_owned()),
            Some(hash) => {
                let loaded = o.succeeded("skill.load").any(|r| {
                    r.structured_output["name"].as_str() == Some(name.as_str())
                        && r.structured_output["content_hash"].as_str() == Some(hash.as_str())
                });
                let selected = o.of_task().any(|e| {
                    e.event_type == "SkillSelected"
                        && e.payload["name"].as_str() == Some(name.as_str())
                        && e.payload["content_hash"].as_str() == Some(hash.as_str())
                });
                (
                    loaded || selected,
                    format!("loaded={loaded} selected={selected}"),
                )
            }
        };
        push(format!("skill {name} reached the task"), passed, detail);
    }
    if let Some(s) = &v.subagents {
        let admitted = o
            .of_task()
            .filter(|e| e.event_type == "SubagentAdmitted")
            .count();
        let results = o
            .of_task()
            .filter(|e| e.event_type == "SubagentResultRecorded")
            .count();
        let merged: std::collections::BTreeSet<String> = o
            .of_task()
            .filter(|e| {
                e.event_type == "MergeTransactionRecorded"
                    && e.payload["state"].as_str() == Some("COMMITTED")
            })
            .filter_map(|e| e.payload["child_task_id"].as_str().map(norm))
            .collect();
        let n = |x: usize| u32::try_from(x).unwrap_or(u32::MAX);
        push(
            format!(
                "children admitted >= {}, results >= {}, merged >= {}",
                s.admitted, s.results, s.merged
            ),
            n(admitted) >= s.admitted && n(results) >= s.results && n(merged.len()) >= s.merged,
            format!(
                "admitted={admitted} results={results} merged={}",
                merged.len()
            ),
        );
    }
    let passed = !checks.is_empty() && checks.iter().all(|c| c.passed);
    VerifierOutcome {
        id: v.id.clone(),
        passed,
        checks,
    }
}
