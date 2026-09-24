//! Importing another agent's configuration (REQ-EV-0137, REQ-EV-0183):
//! instructions, commands, agents, skills and MCP servers written for other
//! coding agents, turned into one Modbit extension — never run as the other
//! agent's runtime — with a migration report that labels every item
//! `MAPPED`, `SKIPPED` or `CONFLICT` and says why.
//!
//! What it reads, relative to the source root:
//!
//! - instruction manifests — `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`,
//!   `.cursorrules`, `.windsurfrules`, `.github/copilot-instructions.md`,
//!   `.cursor/rules/*.mdc`, and an `AGENTS.md`/`CLAUDE.md` in a
//!   subdirectory (scoped to it) — become rules;
//! - `.claude/commands/**/*.md` become commands (`$ARGUMENTS` →
//!   `{{arguments}}`); one that runs shell commands when expanded is
//!   skipped;
//! - `.claude/agents/*.md` become agent profiles, through the canonical
//!   importer that refuses any key that would widen authority;
//! - `.claude/skills/<name>/SKILL.md` become skills (incubator until
//!   evaluated and signed, like any skill); their scripts are skipped;
//! - `.mcp.json`, `.cursor/mcp.json`, `.gemini/settings.json` and
//!   `.vscode/mcp.json` servers become tool servers — stdio only, no
//!   credential copied;
//! - hooks and permissions in `.claude/settings*.json` are skipped by name:
//!   their semantics are not Modbit's.
//!
//! The extension is written unsigned, with every file listed by digest in
//! its manifest, so loading it quarantines it — its servers and everything
//! else inert — until the person has seen what it would do and trusts it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Digest;

/// The report file written beside the manifest.
pub const IMPORT_REPORT: &str = "IMPORT_REPORT.json";

/// What an item is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    /// Instructions.
    Rule,
    /// A command template.
    Command,
    /// An agent profile.
    Agent,
    /// A skill package.
    Skill,
    /// An MCP server.
    McpServer,
    /// A lifecycle hook.
    Hook,
    /// A setting.
    Setting,
    /// A file no importer here knows.
    Other,
}

/// What became of an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ItemStatus {
    /// Imported.
    Mapped,
    /// Not imported, with why.
    Skipped,
    /// Not imported: it collides with something already there, or with an
    /// item imported before it.
    Conflict,
}

/// One line of the migration report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportItem {
    /// Kind.
    pub kind: ItemKind,
    /// Where it came from: a path under the source root, with `#key` for an
    /// entry inside a file.
    pub source: String,
    /// What it became, under the extension (`rules/…`, `command:…`,
    /// `tool:…`); empty when nothing was written.
    pub target: String,
    /// Status.
    pub status: ItemStatus,
    /// Why, in words.
    pub reason: String,
}

/// The migration report.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReport {
    /// The extension's name.
    pub extension: String,
    /// The source root, as given.
    pub source_root: String,
    /// The formats recognized (`agents-md`, `claude`, `cursor`, `gemini`,
    /// `copilot`, `windsurf`, `vscode`).
    pub formats: Vec<String>,
    /// Every item, in the order it was read.
    pub items: Vec<ImportItem>,
}

impl ImportReport {
    /// How many items have `status`.
    #[must_use]
    pub fn count(&self, status: ItemStatus) -> usize {
        self.items.iter().filter(|i| i.status == status).count()
    }
}

/// What the destination already has, by kind and name: an imported item of
/// the same name is a conflict, and what is already there wins.
#[derive(Clone, Debug, Default)]
pub struct Existing {
    /// Rule ids.
    pub rules: BTreeSet<String>,
    /// Agent profile names.
    pub agents: BTreeSet<String>,
    /// Skill names.
    pub skills: BTreeSet<String>,
    /// MCP server names.
    pub servers: BTreeSet<String>,
}

/// Why nothing was imported.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    /// The source root is not a directory.
    #[error("the source `{0}` is not a directory")]
    NoSource(String),
    /// The destination exists and `replace` was not asked for.
    #[error("`{0}` already exists")]
    Exists(String),
    /// The name is not an extension name.
    #[error("`{0}` is not an extension name (1–64 of A–Z, a–z, 0–9, `-`, `_`)")]
    BadName(String),
    /// Nothing the importer knows was found.
    #[error("no configuration this importer knows was found under `{0}`")]
    Nothing(String),
    /// Writing the extension failed.
    #[error("writing the extension: {0}")]
    Io(String),
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(sha2::Sha256::digest(bytes))
}

/// Split `---` front matter (simple `key: value` lines; `[a, b]` lists)
/// from a Markdown body.
fn front_matter(text: &str) -> (BTreeMap<String, String>, String) {
    let mut map = BTreeMap::new();
    let t = text.strip_prefix('\u{feff}').unwrap_or(text);
    let Some(rest) = t
        .strip_prefix("---\n")
        .or_else(|| t.strip_prefix("---\r\n"))
    else {
        return (map, text.to_owned());
    };
    let Some(end) = rest.find("\n---") else {
        return (map, text.to_owned());
    };
    for line in rest[..end].lines() {
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim();
            if !k.is_empty() && !k.starts_with('#') {
                map.insert(
                    k.to_owned(),
                    v.trim().trim_matches('"').trim_matches('\'').to_owned(),
                );
            }
        }
    }
    let body = rest[end + 4..]
        .trim_start_matches(['-'])
        .trim_start_matches(['\r', '\n']);
    (map, body.to_owned())
}

fn yaml_list(v: &str) -> Vec<String> {
    v.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

fn slug(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out.trim_matches('-').chars().take(64).collect()
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

fn files_under(dir: &Path, depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            if depth > 0 {
                out.extend(files_under(&p, depth - 1));
            }
        } else {
            out.push(p);
        }
    }
    out
}

/// A rule file in Modbit's format (its front matter holds only what the
/// rules owner reads; where it came from is a comment in the body).
fn rule_text(id: &str, paths: &[String], source: &str, body: &str) -> String {
    let mut fm = format!("---\nid: {id}\n");
    if !paths.is_empty() {
        fm.push_str(&format!("paths: [{}]\n", paths.join(", ")));
    }
    fm.push_str("---\n");
    format!("{fm}<!-- {source} -->\n{}\n", body.trim_end())
}

struct Builder<'a> {
    root: &'a Path,
    existing: &'a Existing,
    report: ImportReport,
    files: BTreeMap<String, Vec<u8>>,
    commands: Vec<Value>,
    tools: Vec<Value>,
    tool_defs: BTreeMap<String, (String, Value)>,
    names: BTreeSet<String>,
}

impl Builder<'_> {
    fn item(
        &mut self,
        kind: ItemKind,
        source: &str,
        target: &str,
        status: ItemStatus,
        reason: &str,
    ) {
        self.report.items.push(ImportItem {
            kind,
            source: source.to_owned(),
            target: target.to_owned(),
            status,
            reason: reason.to_owned(),
        });
    }

    fn format(&mut self, f: &str) {
        if !self.report.formats.iter().any(|x| x == f) {
            self.report.formats.push(f.to_owned());
        }
    }

    /// Claim `kind:name` for this import; `false` when an item before it
    /// took it.
    fn claim(&mut self, kind: &str, name: &str) -> bool {
        self.names.insert(format!("{kind}:{name}"))
    }

    fn rule(&mut self, source: &str, id: &str, paths: &[String], body: &str, reason: &str) {
        let target = format!("rules/{id}.md");
        if self.existing.rules.contains(id) {
            self.item(
                ItemKind::Rule,
                source,
                "",
                ItemStatus::Conflict,
                &format!("a rule `{id}` is already configured; it wins"),
            );
            return;
        }
        if !self.claim("rule", id) {
            self.item(
                ItemKind::Rule,
                source,
                "",
                ItemStatus::Conflict,
                &format!("another imported file already became rule `{id}`"),
            );
            return;
        }
        if body.trim().is_empty() {
            self.item(ItemKind::Rule, source, "", ItemStatus::Skipped, "empty");
            return;
        }
        self.files.insert(
            target.clone(),
            rule_text(id, paths, &format!("import:{source}"), body).into_bytes(),
        );
        self.item(ItemKind::Rule, source, &target, ItemStatus::Mapped, reason);
    }

    fn instructions(&mut self) {
        let top: [(&str, &str, &str); 6] = [
            ("AGENTS.md", "agents-md", "agents"),
            ("CLAUDE.md", "claude", "claude"),
            ("GEMINI.md", "gemini", "gemini"),
            (".cursorrules", "cursor", "cursorrules"),
            (".windsurfrules", "windsurf", "windsurfrules"),
            (
                ".github/copilot-instructions.md",
                "copilot",
                "copilot-instructions",
            ),
        ];
        for (path, format, id) in top {
            let p = self.root.join(path);
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            self.format(format);
            let follows_imports = text.lines().any(|l| {
                let l = l.trim();
                l.starts_with('@') && l.len() > 1 && !l.contains(' ')
            });
            self.rule(
                path,
                &format!("imported-{id}"),
                &[],
                &text,
                if follows_imports {
                    "always in force; its `@file` imports are not followed — import those files as rules of their own"
                } else {
                    "always in force (no path scope)"
                },
            );
        }
        // Nested instruction files apply to their directory.
        for p in files_under(self.root, 3) {
            let r = rel(self.root, &p);
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if !(name == "AGENTS.md" || name == "CLAUDE.md") || !r.contains('/') {
                continue;
            }
            if r.starts_with('.')
                || r.split('/')
                    .any(|seg| seg == "node_modules" || seg == "target")
            {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            let dir = r.rsplit_once('/').map(|(d, _)| d).unwrap_or_default();
            self.format(if name == "AGENTS.md" {
                "agents-md"
            } else {
                "claude"
            });
            self.rule(
                &r,
                &format!("imported-{}", slug(&r.replace(".md", ""))),
                &[format!("{dir}/**")],
                &text,
                &format!("in force for paths under `{dir}/`"),
            );
        }
        for p in files_under(&self.root.join(".cursor/rules"), 2) {
            if p.extension().and_then(|e| e.to_str()) != Some("mdc") {
                continue;
            }
            self.format("cursor");
            let r = rel(self.root, &p);
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            let (fm, body) = front_matter(&text);
            let globs = fm.get("globs").map(|g| yaml_list(g)).unwrap_or_default();
            let always = fm.get("alwaysApply").is_some_and(|v| v == "true");
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("rule");
            let (paths, reason) = if always || globs.is_empty() {
                (vec![], "always in force".to_owned())
            } else {
                (globs.clone(), format!("in force for {}", globs.join(", ")))
            };
            self.rule(
                &r,
                &format!("imported-cursor-{}", slug(stem)),
                &paths,
                &body,
                &reason,
            );
        }
    }

    fn commands(&mut self) {
        let base = self.root.join(".claude/commands");
        for p in files_under(&base, 4) {
            if p.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            self.format("claude");
            let r = rel(self.root, &p);
            let name = slug(&rel(&base, &p).trim_end_matches(".md").replace('/', "-"));
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            let (fm, body) = front_matter(&text);
            // `!`cmd`` runs a shell command when the command is expanded:
            // that is code, and a command here is only text.
            if body.contains("!`") {
                self.item(
                    ItemKind::Command,
                    &r,
                    "",
                    ItemStatus::Skipped,
                    "it runs shell commands when expanded (!`…`); a Modbit command is only text",
                );
                continue;
            }
            if !self.claim("command", &name) {
                self.item(
                    ItemKind::Command,
                    &r,
                    "",
                    ItemStatus::Conflict,
                    &format!("another imported command is already `{name}`"),
                );
                continue;
            }
            let positional = (1..=9).any(|i| body.contains(&format!("${i}")));
            let template = body.replace("$ARGUMENTS", "{{arguments}}");
            if template.trim().is_empty() {
                self.item(ItemKind::Command, &r, "", ItemStatus::Skipped, "empty");
                continue;
            }
            self.commands.push(json!({
                "name": name,
                "description": fm.get("description").cloned().unwrap_or_default(),
                "template": template,
            }));
            let mut reason = "`$ARGUMENTS` became `{{arguments}}`".to_owned();
            if positional {
                reason.push_str("; positional `$1`… are left as written");
            }
            if fm.contains_key("allowed-tools") {
                reason.push_str("; `allowed-tools` is ignored — a command grants nothing");
            }
            self.item(
                ItemKind::Command,
                &r,
                &format!("command:{name}"),
                ItemStatus::Mapped,
                &reason,
            );
        }
        for p in files_under(&self.root.join(".gemini/commands"), 3) {
            self.format("gemini");
            self.item(
                ItemKind::Command,
                &rel(self.root, &p),
                "",
                ItemStatus::Skipped,
                "TOML commands are not imported; re-declare the prompt as a Markdown command",
            );
        }
    }

    fn agents(&mut self) {
        for p in files_under(&self.root.join(".claude/agents"), 1) {
            if p.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            self.format("claude");
            let r = rel(self.root, &p);
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            match modbit_domain::agent_profile::import_claude_agent(&text) {
                Err(e) => self.item(ItemKind::Agent, &r, "", ItemStatus::Skipped, &e.to_string()),
                Ok(profile) => {
                    let name = profile.name.clone();
                    if self.existing.agents.contains(&name) || !self.claim("agent", &name) {
                        self.item(
                            ItemKind::Agent,
                            &r,
                            "",
                            ItemStatus::Conflict,
                            &format!("an agent profile `{name}` already exists; it wins"),
                        );
                        continue;
                    }
                    let target = format!("agents/{name}.md");
                    self.files.insert(
                        target.clone(),
                        modbit_domain::agent_profile::canonical_text(&profile).into_bytes(),
                    );
                    self.item(
                        ItemKind::Agent,
                        &r,
                        &target,
                        ItemStatus::Mapped,
                        "tool names mapped to canonical tools; a profile can only narrow",
                    );
                }
            }
        }
    }

    fn skills(&mut self) {
        let base = self.root.join(".claude/skills");
        let Ok(entries) = std::fs::read_dir(&base) else {
            return;
        };
        let mut dirs: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            self.format("claude");
            let r = rel(self.root, &dir);
            let Ok(text) = std::fs::read_to_string(dir.join("SKILL.md")) else {
                self.item(ItemKind::Skill, &r, "", ItemStatus::Skipped, "no SKILL.md");
                continue;
            };
            let (fm, body) = front_matter(&text);
            let name = slug(fm.get("name").map_or_else(
                || dir.file_name().and_then(|n| n.to_str()).unwrap_or("skill"),
                String::as_str,
            ));
            let description = fm.get("description").cloned().unwrap_or_default();
            if description.trim().is_empty() {
                self.item(
                    ItemKind::Skill,
                    &r,
                    "",
                    ItemStatus::Skipped,
                    "no description",
                );
                continue;
            }
            if self.existing.skills.contains(&name) || !self.claim("skill", &name) {
                self.item(
                    ItemKind::Skill,
                    &r,
                    "",
                    ItemStatus::Conflict,
                    &format!("a skill `{name}` already exists; it wins"),
                );
                continue;
            }
            let target = format!("skills/{name}");
            self.files.insert(
                format!("{target}/SKILL.md"),
                format!(
                    "---\nname: {name}\nversion: imported\ndescription: {}\nprovenance.source: import:{r}\n---\n{}\n",
                    description.replace('\n', " "),
                    body.trim_end()
                )
                .into_bytes(),
            );
            let mut skipped_scripts = Vec::new();
            for f in files_under(&dir, 3) {
                let fr = rel(&dir, &f);
                if fr == "SKILL.md" {
                    continue;
                }
                let script = fr.starts_with("scripts/")
                    || matches!(
                        f.extension().and_then(|e| e.to_str()),
                        Some("sh" | "py" | "js" | "ts" | "rb" | "ps1" | "bat" | "exe")
                    );
                if script {
                    skipped_scripts.push(fr);
                    continue;
                }
                if let Ok(bytes) = std::fs::read(&f) {
                    self.files.insert(format!("{target}/resources/{fr}"), bytes);
                }
            }
            let mut reason =
                "incubator until it is evaluated and signed, like any skill".to_owned();
            if !skipped_scripts.is_empty() {
                reason.push_str(&format!(
                    "; its scripts were not imported ({}) — a Modbit skill runs procedures in the isolate, not host scripts",
                    skipped_scripts.join(", ")
                ));
            }
            self.item(ItemKind::Skill, &r, &target, ItemStatus::Mapped, &reason);
        }
    }

    fn servers(&mut self) {
        let sources: [(&str, &str, &str); 4] = [
            (".mcp.json", "claude", "mcpServers"),
            (".cursor/mcp.json", "cursor", "mcpServers"),
            (".gemini/settings.json", "gemini", "mcpServers"),
            (".vscode/mcp.json", "vscode", "servers"),
        ];
        for (path, format, key) in sources {
            let Ok(text) = std::fs::read_to_string(self.root.join(path)) else {
                continue;
            };
            let Ok(doc) = serde_json::from_str::<Value>(&text) else {
                self.format(format);
                self.item(
                    ItemKind::McpServer,
                    path,
                    "",
                    ItemStatus::Skipped,
                    "not JSON",
                );
                continue;
            };
            let Some(servers) = doc.get(key).and_then(Value::as_object) else {
                continue;
            };
            self.format(format);
            for (raw_name, def) in servers {
                let source = format!("{path}#{raw_name}");
                let name = slug(raw_name).replace('-', "_");
                let name = if name.is_empty() {
                    "server".to_owned()
                } else {
                    name
                };
                let Some(command) = def.get("command").and_then(Value::as_str) else {
                    self.item(
                        ItemKind::McpServer,
                        &source,
                        "",
                        ItemStatus::Skipped,
                        "not a stdio server (url/http/sse); only stdio servers are imported",
                    );
                    continue;
                };
                let args: Vec<String> = def
                    .get("args")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default();
                let mut env = serde_json::Map::new();
                let mut dropped = Vec::new();
                for (k, v) in def
                    .get("env")
                    .and_then(Value::as_object)
                    .into_iter()
                    .flatten()
                {
                    let upper = k.to_ascii_uppercase();
                    let secretish = ["KEY", "TOKEN", "SECRET", "PASSWORD", "AUTH", "CREDENTIAL"]
                        .iter()
                        .any(|s| upper.contains(s));
                    let value = v.as_str().unwrap_or_default();
                    if secretish || value.starts_with("${") {
                        dropped.push(k.clone());
                    } else {
                        env.insert(k.clone(), Value::String(value.to_owned()));
                    }
                }
                let definition = json!({
                    "name": name,
                    "transport": {"kind": "stdio", "command": command, "args": args},
                    "env": env,
                });
                if self.existing.servers.contains(&name) {
                    self.item(
                        ItemKind::McpServer,
                        &source,
                        "",
                        ItemStatus::Conflict,
                        &format!("a server `{name}` is already configured; it wins"),
                    );
                    continue;
                }
                if let Some((first, def0)) = self.tool_defs.get(&name) {
                    let same = def0 == &definition;
                    let first = first.clone();
                    self.item(
                        ItemKind::McpServer,
                        &source,
                        "",
                        if same {
                            ItemStatus::Skipped
                        } else {
                            ItemStatus::Conflict
                        },
                        &if same {
                            format!("the same server as {first}")
                        } else {
                            format!("{first} defines `{name}` differently; the first wins")
                        },
                    );
                    continue;
                }
                self.tool_defs
                    .insert(name.clone(), (source.clone(), definition.clone()));
                self.tools.push(definition);
                let mut reason = format!(
                    "runs `{command} {}` once trusted; inert until the person trusts the extension",
                    args.join(" ")
                );
                if !dropped.is_empty() {
                    reason.push_str(&format!(
                        "; credentials not imported ({}): put them in the Core's custody",
                        dropped.join(", ")
                    ));
                }
                self.item(
                    ItemKind::McpServer,
                    &source,
                    &format!("tool:{name}"),
                    ItemStatus::Mapped,
                    &reason,
                );
            }
        }
    }

    fn settings(&mut self) {
        for path in [".claude/settings.json", ".claude/settings.local.json"] {
            let Ok(text) = std::fs::read_to_string(self.root.join(path)) else {
                continue;
            };
            self.format("claude");
            let Ok(doc) = serde_json::from_str::<Value>(&text) else {
                self.item(ItemKind::Setting, path, "", ItemStatus::Skipped, "not JSON");
                continue;
            };
            for (k, v) in doc.as_object().into_iter().flatten() {
                let source = format!("{path}#{k}");
                match k.as_str() {
                    "hooks" => {
                        for (event, _) in v.as_object().into_iter().flatten() {
                            self.item(
                                ItemKind::Hook,
                                &format!("{source}.{event}"),
                                "",
                                ItemStatus::Skipped,
                                "its hooks decide by exit code under another agent's lifecycle; re-declare it as a Modbit hook (typed request, continue/deny/mutate)",
                            );
                        }
                    }
                    "permissions" => self.item(
                        ItemKind::Setting,
                        &source,
                        "",
                        ItemStatus::Skipped,
                        "permissions come from Modbit's policy layers, never from an import",
                    ),
                    "mcpServers" => {}
                    _ => self.item(
                        ItemKind::Setting,
                        &source,
                        "",
                        ItemStatus::Skipped,
                        "not a setting this importer maps",
                    ),
                }
            }
        }
    }
}

/// Import the configuration under `source_root` into a new extension
/// directory `out` named `name`. Writes the manifest (unsigned, every file
/// listed by digest), the rules, agents and skills, and the report.
///
/// # Errors
/// The source is not a directory, the name is not a name, `out` exists and
/// `replace` is false, nothing was recognized, or writing failed.
pub fn import(
    source_root: &Path,
    name: &str,
    out: &Path,
    existing: &Existing,
    replace: bool,
) -> Result<ImportReport, ImportError> {
    let ok_name = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    if !ok_name {
        return Err(ImportError::BadName(name.to_owned()));
    }
    if !source_root.is_dir() {
        return Err(ImportError::NoSource(source_root.display().to_string()));
    }
    if out.exists() && !replace {
        return Err(ImportError::Exists(out.display().to_string()));
    }
    let mut b = Builder {
        root: source_root,
        existing,
        report: ImportReport {
            extension: name.to_owned(),
            source_root: source_root.display().to_string(),
            ..ImportReport::default()
        },
        files: BTreeMap::new(),
        commands: Vec::new(),
        tools: Vec::new(),
        tool_defs: BTreeMap::new(),
        names: BTreeSet::new(),
    };
    b.instructions();
    b.commands();
    b.agents();
    b.skills();
    b.servers();
    b.settings();
    if b.report.items.is_empty() {
        return Err(ImportError::Nothing(source_root.display().to_string()));
    }
    // The report travels with the extension, listed like every other file.
    b.files.insert(
        IMPORT_REPORT.to_owned(),
        serde_json::to_string_pretty(&b.report)
            .unwrap_or_default()
            .into_bytes(),
    );
    let io = |e: std::io::Error| ImportError::Io(e.to_string());
    if out.exists() {
        std::fs::remove_dir_all(out).map_err(io)?;
    }
    std::fs::create_dir_all(out).map_err(io)?;
    let mut listed = serde_json::Map::new();
    for (path, bytes) in &b.files {
        let dest = out.join(path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        std::fs::write(&dest, bytes).map_err(io)?;
        listed.insert(path.clone(), Value::String(sha256_hex(bytes)));
    }
    let manifest = json!({
        "name": name,
        "version": "imported",
        "publisher": "",
        "source": format!("import:{}:{}", b.report.formats.join("+"), source_root.display()),
        "commands": b.commands,
        "tools": b.tools,
        "files": listed,
    });
    std::fs::write(
        out.join("modbit-extension.json"),
        serde_json::to_string_pretty(&manifest).unwrap_or_default(),
    )
    .map_err(io)?;
    Ok(b.report)
}
