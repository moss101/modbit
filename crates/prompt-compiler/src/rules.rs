//! Workspace and user rules (REQ-EV-0059 path-scoped lazy rules, REQ-EV-0129
//! instruction layering with precedence, TTL and conflict diagnostics;
//! REQ-EV-0105 explicit injection): repository instructions the prompt
//! carries only when they apply, with the manifest saying which, from where,
//! and why.
//!
//! A rule is a Markdown file with front matter under a layer's directory —
//! the workspace's `.modbit/rules/` (project) or the profile's `rules/`
//! (user). `id` names it (the file stem by default); `paths` scopes it to
//! globs and keeps it dormant until a matching path becomes active (read,
//! written or planned); `priority` orders rules inside a layer;
//! `expires_at_ms` is its TTL. Precedence is deterministic: project over
//! user, then priority, then file name; a rule that loses to another with
//! the same id is a recorded conflict with the winner and its source.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A layer of rules: its name decides precedence (lower index wins).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layer {
    /// `project` | `user`.
    pub name: String,
    /// The directory of `*.md` rule files.
    pub dir: PathBuf,
}

/// One rule as loaded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// Id (front matter `id`, or the file stem).
    pub id: String,
    /// Layer name.
    pub layer: String,
    /// Layer index (precedence).
    pub layer_index: usize,
    /// Source path.
    pub source: String,
    /// Path globs; empty = global.
    pub paths: Vec<String>,
    /// Priority inside the layer (higher wins).
    pub priority: i64,
    /// TTL, epoch milliseconds.
    pub expires_at_ms: Option<i64>,
    /// The instruction text.
    pub body: String,
    /// sha256 of the file.
    pub hash: String,
}

/// Why a rule is in the prompt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActivationReason {
    /// A global rule (no path scope).
    Global,
    /// A scoped rule matched an active path.
    Path {
        /// The active path that matched.
        path: String,
        /// The glob it matched.
        glob: String,
    },
}

/// A rule in the prompt, with its provenance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveRule {
    /// Id.
    pub id: String,
    /// Layer.
    pub layer: String,
    /// Source path.
    pub source: String,
    /// File hash.
    pub hash: String,
    /// Why.
    pub reason: ActivationReason,
}

/// Two rules with one id: the winner and the loser, with why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    /// Id.
    pub id: String,
    /// Winner's source.
    pub winner: String,
    /// Winner's layer.
    pub winner_layer: String,
    /// Loser's source.
    pub loser: String,
    /// Loser's layer.
    pub loser_layer: String,
    /// `LAYER` | `PRIORITY` | `NAME`.
    pub decided_by: String,
}

/// What the compiler selected this turn.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RulesSelection {
    /// Rules in the prompt, in precedence order.
    pub active: Vec<ActiveRule>,
    /// Scoped rules whose paths are not active yet.
    pub dormant: Vec<String>,
    /// Rules past their TTL (id and source).
    pub expired: Vec<(String, String)>,
    /// Conflicts resolved.
    pub conflicts: Vec<Conflict>,
    /// Files that were not rules (source and why).
    pub invalid: Vec<(String, String)>,
}

/// The loaded rules of every layer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSet {
    /// Every rule that parsed.
    pub rules: Vec<Rule>,
    /// Files that did not parse.
    pub invalid: Vec<(String, String)>,
}

fn sha_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(bytes))
}

/// A rule file's parsed parts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedRule {
    /// Id.
    pub id: String,
    /// Path globs.
    pub paths: Vec<String>,
    /// Priority.
    pub priority: i64,
    /// TTL.
    pub expires_at_ms: Option<i64>,
    /// Body.
    pub body: String,
}

/// Parse one rule file.
///
/// # Errors
/// Malformed front matter.
pub fn parse_rule(text: &str, stem: &str) -> Result<ParsedRule, String> {
    let text = text.trim_start_matches('\u{feff}');
    let mut id = stem.to_owned();
    let mut paths = Vec::new();
    let mut priority = 0i64;
    let mut expires = None;
    let body;
    if text.starts_with("---") {
        let mut lines = text.lines();
        lines.next();
        let mut closed = false;
        for l in lines.by_ref() {
            if l.trim() == "---" {
                closed = true;
                break;
            }
            let line = l.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once(':') else {
                return Err(format!("expected `key: value`, got `{line}`"));
            };
            let v = v.trim();
            match k.trim() {
                "id" => id = v.trim_matches('"').to_owned(),
                "paths" => {
                    paths = v
                        .trim_start_matches('[')
                        .trim_end_matches(']')
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_owned())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "priority" => {
                    priority = v
                        .parse()
                        .map_err(|_| format!("priority `{v}` is not a number"))?;
                }
                "expires_at_ms" => {
                    expires = Some(
                        v.parse()
                            .map_err(|_| format!("expires_at_ms `{v}` is not a number"))?,
                    );
                }
                other => return Err(format!("unknown front matter key `{other}`")),
            }
        }
        if !closed {
            return Err("unterminated front matter".into());
        }
        body = lines.collect::<Vec<_>>().join("\n").trim().to_owned();
    } else {
        body = text.trim().to_owned();
    }
    if id.is_empty() {
        return Err("empty id".into());
    }
    if body.is_empty() {
        return Err("empty rule body".into());
    }
    Ok(ParsedRule {
        id,
        paths,
        priority,
        expires_at_ms: expires,
        body,
    })
}

/// Load every `*.md` under each layer's directory (missing directories are
/// empty layers).
#[must_use]
pub fn load(layers: &[Layer]) -> RuleSet {
    let mut set = RuleSet::default();
    for (index, layer) in layers.iter().enumerate() {
        let Ok(entries) = std::fs::read_dir(&layer.dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "md"))
            .collect();
        files.sort();
        for file in files {
            let source = file.display().to_string();
            let Ok(bytes) = std::fs::read(&file) else {
                set.invalid.push((source, "unreadable".into()));
                continue;
            };
            let stem = file
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            match parse_rule(&String::from_utf8_lossy(&bytes), &stem) {
                Ok(parsed) => set.rules.push(Rule {
                    id: parsed.id,
                    layer: layer.name.clone(),
                    layer_index: index,
                    source,
                    paths: parsed.paths,
                    priority: parsed.priority,
                    expires_at_ms: parsed.expires_at_ms,
                    body: parsed.body,
                    hash: sha_hex(&bytes),
                }),
                Err(e) => set.invalid.push((source, e)),
            }
        }
    }
    set
}

fn glob_matches(glob: &str, path: &str) -> bool {
    globset::GlobBuilder::new(glob)
        .literal_separator(false)
        .build()
        .map(|g| g.compile_matcher().is_match(Path::new(path)))
        .unwrap_or(false)
}

impl RuleSet {
    /// Select the rules for a turn: the active paths decide scoped rules,
    /// `now_ms` decides TTL, and one id yields one rule — the winner by
    /// layer, then priority, then file name — with the loss recorded.
    #[must_use]
    pub fn select(&self, active_paths: &[String], now_ms: i64) -> RulesSelection {
        let mut out = RulesSelection {
            invalid: self.invalid.clone(),
            ..RulesSelection::default()
        };
        let mut candidates: Vec<(&Rule, ActivationReason)> = Vec::new();
        for r in &self.rules {
            if let Some(exp) = r.expires_at_ms
                && now_ms >= exp
            {
                out.expired.push((r.id.clone(), r.source.clone()));
                continue;
            }
            if r.paths.is_empty() {
                candidates.push((r, ActivationReason::Global));
                continue;
            }
            let hit = active_paths.iter().find_map(|p| {
                r.paths
                    .iter()
                    .find(|g| glob_matches(g, p))
                    .map(|g| (p.clone(), g.clone()))
            });
            match hit {
                Some((path, glob)) => candidates.push((r, ActivationReason::Path { path, glob })),
                None => out.dormant.push(r.id.clone()),
            }
        }
        // Precedence: layer index ascending, priority descending, source name.
        candidates.sort_by(|(a, _), (b, _)| {
            a.layer_index
                .cmp(&b.layer_index)
                .then(b.priority.cmp(&a.priority))
                .then(a.source.cmp(&b.source))
        });
        let mut seen: Vec<&Rule> = Vec::new();
        for (r, reason) in candidates {
            if let Some(winner) = seen.iter().find(|w| w.id == r.id) {
                let decided_by = if winner.layer_index != r.layer_index {
                    "LAYER"
                } else if winner.priority != r.priority {
                    "PRIORITY"
                } else {
                    "NAME"
                };
                out.conflicts.push(Conflict {
                    id: r.id.clone(),
                    winner: winner.source.clone(),
                    winner_layer: winner.layer.clone(),
                    loser: r.source.clone(),
                    loser_layer: r.layer.clone(),
                    decided_by: decided_by.into(),
                });
                continue;
            }
            seen.push(r);
            out.active.push(ActiveRule {
                id: r.id.clone(),
                layer: r.layer.clone(),
                source: r.source.clone(),
                hash: r.hash.clone(),
                reason,
            });
        }
        out
    }

    /// The prompt texts of a selection, in precedence order, each headed
    /// by its id and layer.
    #[must_use]
    pub fn texts(&self, selection: &RulesSelection) -> Vec<String> {
        selection
            .active
            .iter()
            .filter_map(|a| self.rules.iter().find(|r| r.source == a.source))
            .map(|r| format!("[rule {} — {}]\n{}", r.id, r.layer, r.body))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, text: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(name), text).unwrap();
    }

    #[test]
    fn scoped_rules_stay_dormant_until_a_matching_path_is_active_and_layers_decide_conflicts() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let user = root.path().join("user");
        write(&project, "general.md", "Keep commits small.");
        write(
            &project,
            "billing.md",
            "---\nid: billing\npaths: [src/billing/**, \"**/*.sql\"]\n---\nMoney is in minor units.",
        );
        write(
            &project,
            "style.md",
            "---\nid: style\npriority: 5\n---\nProject style.",
        );
        write(
            &user,
            "style.md",
            "---\nid: style\npriority: 50\n---\nUser style.",
        );
        write(
            &user,
            "stale.md",
            "---\nid: stale\nexpires_at_ms: 1000\n---\nOld advice.",
        );
        write(&user, "broken.md", "---\nid: x\nbogus: 1\n---\nnope");
        let set = load(&[
            Layer {
                name: "project".into(),
                dir: project.clone(),
            },
            Layer {
                name: "user".into(),
                dir: user.clone(),
            },
        ]);
        assert_eq!(set.rules.len(), 5);
        assert_eq!(set.invalid.len(), 1);
        assert!(set.invalid[0].1.contains("unknown front matter key"));
        let s = set.select(&["README.md".into()], 5_000);
        assert_eq!(
            s.active.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
            ["style", "general"],
            "priority orders inside the layer"
        );
        assert_eq!(s.dormant, ["billing"]);
        assert_eq!(s.expired.len(), 1);
        assert_eq!(s.conflicts.len(), 1);
        let c = &s.conflicts[0];
        assert_eq!(
            (
                c.id.as_str(),
                c.winner_layer.as_str(),
                c.loser_layer.as_str(),
                c.decided_by.as_str()
            ),
            ("style", "project", "user", "LAYER")
        );
        assert!(c.winner.ends_with("style.md") && c.winner.contains("project"));
        let texts = set.texts(&s);
        assert_eq!(texts[0], "[rule style — project]\nProject style.");
        // A billing path activates the scoped rule; the reason names the
        // path and the glob.
        let s = set.select(
            &["README.md".into(), "src/billing/invoice.rs".into()],
            5_000,
        );
        let billing = s.active.iter().find(|a| a.id == "billing").unwrap();
        assert_eq!(
            billing.reason,
            ActivationReason::Path {
                path: "src/billing/invoice.rs".into(),
                glob: "src/billing/**".into()
            }
        );
        assert!(s.dormant.is_empty());
        // Before the TTL the stale rule is active.
        let s = set.select(&[], 500);
        assert!(s.active.iter().any(|a| a.id == "stale"));
        assert!(s.expired.is_empty());
    }

    #[test]
    fn a_rule_without_front_matter_is_global_and_named_by_its_file() {
        let r = parse_rule("Just a rule.", "plain").unwrap();
        assert_eq!(
            (
                r.id.as_str(),
                r.paths.len(),
                r.priority,
                r.expires_at_ms,
                r.body.as_str()
            ),
            ("plain", 0, 0, None, "Just a rule.")
        );
        assert!(
            parse_rule("---\nid: a\n---\n", "a")
                .unwrap_err()
                .contains("empty rule body")
        );
        assert!(
            parse_rule("---\nid: a\npriority: x\n---\nb", "a")
                .unwrap_err()
                .contains("not a number")
        );
        assert!(
            parse_rule("---\nid: a\n", "a")
                .unwrap_err()
                .contains("unterminated")
        );
    }
}
