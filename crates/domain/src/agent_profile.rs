//! Declarative agent profiles (REQ-EV-0115, REQ-EV-0182, REQ-EV-0241;
//! docs/14 "Agent profiles"): a profile is compiled configuration — a name,
//! the tools it asks for, a model preference, a default write scope,
//! budgets and domain context — that the Core compiles into an
//! `AgentExecutionCapsule` at admission. It can only narrow: a tool the
//! surface does not serve, a tool above a child's ceiling or the agent
//! family is dropped and named; a key that would widen authority (a lease,
//! an effect ceiling, an execution profile, permissions, network) is
//! rejected at validation, never honoured.

use serde::{Deserialize, Serialize};

/// A profile as declared (Markdown with `---` front matter; the body is the
/// domain context the child starts with).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProfile {
    /// Lowercase name (`[a-z0-9-]+`).
    pub name: String,
    /// Version text.
    #[serde(default)]
    pub version: String,
    /// One line.
    #[serde(default)]
    pub description: String,
    /// Tools asked for, by canonical name; empty = the child's default
    /// projection.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Model preference (the binding's model when the gateway serves it).
    #[serde(default)]
    pub model: String,
    /// Default write scope when the spawn names none.
    #[serde(default)]
    pub write_scope: Vec<String>,
    /// Turn budget (0 = default).
    #[serde(default)]
    pub max_turns: u32,
    /// Tool-call budget (0 = default).
    #[serde(default)]
    pub max_tool_calls: u32,
    /// Where it came from (`modbit` | `claude` | …), for an import.
    #[serde(default)]
    pub source: String,
    /// The body: domain context for the child.
    #[serde(default)]
    pub context: String,
}

/// Why a profile is refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum ProfileError {
    /// Front matter missing or unreadable.
    #[error("malformed profile: {detail}")]
    Malformed {
        /// Detail.
        detail: String,
    },
    /// A required field is empty.
    #[error("profile field `{field}` is required")]
    MissingField {
        /// Field.
        field: String,
    },
    /// A key that would widen authority (REQ-EV-0241).
    #[error("profile key `{key}` would expand capability and is rejected: {why}")]
    UnsafeExpansion {
        /// The key.
        key: String,
        /// Why.
        why: String,
    },
    /// The name is not a name.
    #[error("profile name `{name}` must be lowercase letters, digits and dashes")]
    BadName {
        /// Name.
        name: String,
    },
}

/// Keys a profile may not carry: each one is authority, not configuration.
pub const UNSAFE_KEYS: &[(&str, &str)] = &[
    (
        "effect_ceiling",
        "the effect ceiling is the lease's, capped by the parent's",
    ),
    ("lease", "leases are minted by the Core at admission"),
    ("resources", "lease resources are minted at admission"),
    ("operations", "lease operations are minted at admission"),
    ("execution_profile", "the execution profile is the parent's"),
    ("permissions", "permissions come from policy, not a profile"),
    ("allow", "permissions come from policy, not a profile"),
    ("network", "network access is a policy decision"),
    ("secrets", "secrets never enter a profile"),
    ("credentials", "credentials never enter a profile"),
    (
        "depth",
        "delegation depth is policy (MODBIT_AGENT_MAX_DEPTH)",
    ),
    (
        "nesting",
        "delegation depth is policy (MODBIT_AGENT_MAX_DEPTH)",
    ),
];

/// Parse a canonical Modbit profile (`---` front matter, then the body).
///
/// # Errors
/// Malformed front matter, a missing name, a bad name, or an unsafe key.
pub fn parse_profile(text: &str) -> Result<AgentProfile, ProfileError> {
    let (map, body) = front_matter(text)?;
    for (key, why) in UNSAFE_KEYS {
        if map.contains_key(*key) {
            return Err(ProfileError::UnsafeExpansion {
                key: (*key).to_owned(),
                why: (*why).to_owned(),
            });
        }
    }
    // `version: 1` reads as a number; versions are text.
    let mut map = map;
    if let Some(v) = map.get("version").cloned()
        && !v.is_string()
    {
        map.insert("version".into(), serde_json::Value::String(v.to_string()));
    }
    let mut p: AgentProfile =
        serde_json::from_value(serde_json::Value::Object(map)).map_err(|e| {
            ProfileError::Malformed {
                detail: e.to_string(),
            }
        })?;
    p.context = body;
    if p.source.is_empty() {
        p.source = "modbit".into();
    }
    validate(&p)?;
    Ok(p)
}

/// Import a Claude-style agent file (`.claude/agents/<name>.md`: `name`,
/// `description`, `tools: Read, Grep, Bash`, `model`) into the canonical
/// schema (REQ-EV-0182). Tool names are mapped; unknown ones are kept as
/// declared so the compiler narrows and names them.
///
/// # Errors
/// As `parse_profile`, plus an unsafe key in the foreign file.
pub fn import_claude_agent(text: &str) -> Result<AgentProfile, ProfileError> {
    let (map, body) = front_matter(text)?;
    for (key, why) in UNSAFE_KEYS {
        if map.contains_key(*key) {
            return Err(ProfileError::UnsafeExpansion {
                key: (*key).to_owned(),
                why: (*why).to_owned(),
            });
        }
    }
    let str_of = |k: &str| {
        map.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    let tools: Vec<String> = match map.get("tools") {
        Some(serde_json::Value::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(map_claude_tool)
            .collect(),
        Some(serde_json::Value::String(s)) => s
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(map_claude_tool)
            .collect(),
        _ => vec![],
    };
    let p = AgentProfile {
        name: str_of("name").to_lowercase().replace('_', "-"),
        version: {
            let v = str_of("version");
            if v.is_empty() { "imported".into() } else { v }
        },
        description: str_of("description"),
        tools,
        model: str_of("model"),
        write_scope: vec![],
        max_turns: 0,
        max_tool_calls: 0,
        source: "claude".into(),
        context: body,
    };
    validate(&p)?;
    Ok(p)
}

/// The canonical name of a Claude Code tool, when there is one; the
/// declared name otherwise (the compiler narrows it and says so).
#[must_use]
pub fn map_claude_tool(name: &str) -> String {
    match name {
        "Read" => "fs.read",
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => "change.apply",
        "Grep" => "search.regex",
        "Glob" | "LS" => "search.paths",
        "Bash" => "proc.exec",
        "WebFetch" => "browser.fetch",
        "WebSearch" => "browser.search",
        "Task" | "Agent" => "agent.spawn",
        "TodoWrite" => "plan.update",
        other => other,
    }
    .to_owned()
}

/// A profile on disk with where it was read from.
pub type Listed = (AgentProfile, std::path::PathBuf);
/// A file that is not a valid profile, with why.
pub type Rejected = (std::path::PathBuf, ProfileError);

/// Every profile under the roots, by name (the first root wins), with the
/// rejected ones and why.
#[must_use]
pub fn list(roots: &[std::path::PathBuf]) -> (Vec<Listed>, Vec<Rejected>) {
    let mut ok = Vec::new();
    let mut rejected = Vec::new();
    for r in roots {
        let Ok(rd) = std::fs::read_dir(r) else {
            continue;
        };
        let mut paths: Vec<std::path::PathBuf> = rd.flatten().map(|e| e.path()).collect();
        paths.sort();
        for p in paths {
            if p.extension().is_none_or(|e| e != "md") {
                continue;
            }
            match std::fs::read_to_string(&p)
                .map_err(|e| ProfileError::Malformed {
                    detail: e.to_string(),
                })
                .and_then(|t| parse_profile(&t))
            {
                Ok(prof) => {
                    if !ok
                        .iter()
                        .any(|(x, _): &(AgentProfile, std::path::PathBuf)| x.name == prof.name)
                    {
                        ok.push((prof, p));
                    }
                }
                Err(e) => rejected.push((p, e)),
            }
        }
    }
    (ok, rejected)
}

/// Install a profile file into `root`, validated first; `from` names a
/// foreign format to import (`claude`). An unsafe key is refused before
/// anything is written (REQ-EV-0241).
///
/// # Errors
/// The file is unreadable, invalid or unsafe; or a profile of that name is
/// already installed and `replace` is false.
pub fn install(
    file: &std::path::Path,
    root: &std::path::Path,
    from: Option<&str>,
    replace: bool,
) -> Result<(AgentProfile, std::path::PathBuf), ProfileError> {
    let text = std::fs::read_to_string(file).map_err(|e| ProfileError::Malformed {
        detail: format!("{}: {e}", file.display()),
    })?;
    let profile = match from {
        Some("claude") => import_claude_agent(&text)?,
        Some(other) => {
            return Err(ProfileError::Malformed {
                detail: format!("unknown import format `{other}` (known: claude)"),
            });
        }
        None => parse_profile(&text)?,
    };
    std::fs::create_dir_all(root).map_err(|e| ProfileError::Malformed {
        detail: e.to_string(),
    })?;
    let dest = root.join(format!("{}.md", profile.name));
    if dest.exists() && !replace {
        return Err(ProfileError::Malformed {
            detail: format!(
                "profile `{}` is already installed (--replace)",
                profile.name
            ),
        });
    }
    // Always written in the canonical form, whatever was imported.
    std::fs::write(&dest, canonical_text(&profile)).map_err(|e| ProfileError::Malformed {
        detail: e.to_string(),
    })?;
    Ok((profile, dest))
}

/// The canonical file for a profile.
#[must_use]
pub fn canonical_text(p: &AgentProfile) -> String {
    let list = |v: &[String]| format!("[{}]", v.join(", "));
    format!(
        "---\nname: {}\nversion: {}\ndescription: {}\ntools: {}\nmodel: {}\nwrite_scope: {}\nmax_turns: {}\nmax_tool_calls: {}\nsource: {}\n---\n{}\n",
        p.name,
        if p.version.is_empty() {
            "0"
        } else {
            &p.version
        },
        p.description,
        list(&p.tools),
        p.model,
        list(&p.write_scope),
        p.max_turns,
        p.max_tool_calls,
        if p.source.is_empty() {
            "modbit"
        } else {
            &p.source
        },
        p.context
    )
}

fn validate(p: &AgentProfile) -> Result<(), ProfileError> {
    if p.name.trim().is_empty() {
        return Err(ProfileError::MissingField {
            field: "name".into(),
        });
    }
    if !p
        .name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(ProfileError::BadName {
            name: p.name.clone(),
        });
    }
    Ok(())
}

fn front_matter(
    text: &str,
) -> Result<(serde_json::Map<String, serde_json::Value>, String), ProfileError> {
    let text = text.trim_start_matches('\u{feff}');
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Err(ProfileError::Malformed {
            detail: "a profile starts with a `---` front matter fence".into(),
        });
    }
    let mut front: Vec<&str> = Vec::new();
    let mut closed = false;
    for l in lines.by_ref() {
        if l.trim() == "---" {
            closed = true;
            break;
        }
        front.push(l);
    }
    if !closed {
        return Err(ProfileError::Malformed {
            detail: "unterminated front matter".into(),
        });
    }
    let body: String = lines.collect::<Vec<_>>().join("\n").trim().to_owned();
    let mut map = serde_json::Map::new();
    for raw in front {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            return Err(ProfileError::Malformed {
                detail: format!("expected `key: value`, got `{line}`"),
            });
        };
        map.insert(k.trim().to_owned(), scalar(v.trim()));
    }
    Ok((map, body))
}

fn scalar(v: &str) -> serde_json::Value {
    if let Some(inner) = v.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return serde_json::Value::Array(
            inner
                .split(',')
                .map(|s| s.trim().trim_matches('"').trim_matches('\''))
                .filter(|s| !s.is_empty())
                .map(|s| serde_json::Value::String(s.to_owned()))
                .collect(),
        );
    }
    if let Ok(n) = v.parse::<u64>() {
        return serde_json::Value::Number(n.into());
    }
    if v == "true" || v == "false" {
        return serde_json::Value::Bool(v == "true");
    }
    serde_json::Value::String(v.trim_matches('"').trim_matches('\'').to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_parses_and_an_unsafe_key_is_rejected() {
        let p = parse_profile(
            "---\nname: researcher\nversion: 1\ndescription: reads\ntools: [fs.read, search.exact]\nmodel: gpt-5-mini\nmax_turns: 6\n---\nRead first; report what you find.\n",
        )
        .unwrap();
        assert_eq!(p.name, "researcher");
        assert_eq!(p.tools, vec!["fs.read", "search.exact"]);
        assert_eq!(p.max_turns, 6);
        assert_eq!(p.context, "Read first; report what you find.");
        assert_eq!(p.source, "modbit");
        let e = parse_profile("---\nname: root\neffect_ceiling: DESTRUCTIVE\n---\n").unwrap_err();
        assert!(
            matches!(e, ProfileError::UnsafeExpansion { ref key, .. } if key == "effect_ceiling")
        );
        let e = parse_profile("---\nname: Bad Name\n---\n").unwrap_err();
        assert!(matches!(e, ProfileError::BadName { .. }));
    }

    #[test]
    fn a_claude_agent_imports_with_mapped_tools() {
        let p = import_claude_agent(
            "---\nname: code-reviewer\ndescription: Reviews code\ntools: Read, Grep, Bash, Task, Mystery\nmodel: sonnet\n---\nYou review code.\n",
        )
        .unwrap();
        assert_eq!(p.source, "claude");
        assert_eq!(
            p.tools,
            vec![
                "fs.read",
                "search.regex",
                "proc.exec",
                "agent.spawn",
                "Mystery"
            ]
        );
        assert_eq!(p.model, "sonnet");
        assert!(import_claude_agent("---\nname: x\npermissions: [all]\n---\n").is_err());
    }
}
