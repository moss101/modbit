//! `AGENTS.md` and `CLAUDE.md` as a native rules layer (REQ-PX-107, ADC-B01).
//!
//! A repository's own instruction files are project guidance its authors
//! wrote for the agents that work in it. This module finds them and turns
//! them into [`Rule`]s of the `repo-instructions` layer; whether they may
//! enter a prompt at all (the workspace trust decision) is the caller's, and
//! an untrusted repository's files are only *listed*, never read into the
//! prompt ([`present`]).
//!
//! # Which files, in which order
//!
//! * `AGENTS.md` and `CLAUDE.md` in the workspace root and in every
//!   directory above it up to the repository root (the first directory that
//!   holds a `.git`, or the workspace root itself when there is none): always
//!   active.
//! * The same two names in a directory *below* the workspace root, once the
//!   task has made a path under that directory active (read, retrieved,
//!   written or planned): the existing path-scoped lazy loading
//!   (`<dir>/**`).
//! * Precedence is deterministic: the nearer directory wins over the
//!   farther one (a nested file over the workspace root's, the workspace
//!   root's over a parent's), `AGENTS.md` before `CLAUDE.md` in one
//!   directory, and the repository's `.modbit/rules` above all of them.
//!   Files are listed in precedence order, the winner first.
//!
//! # Bounds
//!
//! One file contributes at most [`FILE_CAP`] bytes and all of them together
//! at most [`AGGREGATE_CAP`]; a longer text is cut at a character boundary
//! and says so with a visible marker, and a file left out says why. At most
//! [`NESTED_DIR_CAP`] nested directories are looked at in a turn.
//!
//! # Confinement
//!
//! A file is read only when it resolves — symbolic links followed — to a
//! regular file inside the repository root. A link that leaves the
//! repository is reported and not read; nothing outside the repository is
//! ever opened. The text is hashed (sha256 over the bytes read) so the
//! prompt, the log and the Inspector can name exactly what was in force.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::rules::{INSTRUCTIONS_LAYER, Rule};

/// The instruction file names, in precedence order within one directory.
pub const FILE_NAMES: [&str; 2] = ["AGENTS.md", "CLAUDE.md"];
/// The most bytes of one file that enter the prompt.
pub const FILE_CAP: usize = 32 * 1024;
/// The most bytes of all instruction files together that enter the prompt.
pub const AGGREGATE_CAP: usize = 64 * 1024;
/// The most nested directories searched in one turn.
pub const NESTED_DIR_CAP: usize = 24;
/// The most bytes read from one file, for its hash. A larger file is
/// hashed over this prefix and says it was truncated.
const HARD_READ_CAP: u64 = 1024 * 1024;
/// A file's share of the aggregate below which it is left out rather than
/// shown as a stump.
const MIN_USEFUL_BYTES: usize = 256;

/// What a discovery found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Discovery {
    /// The files that are in force, as rules of the `repo-instructions`
    /// layer (their `layer_index` is the caller's to set).
    pub rules: Vec<Rule>,
    /// The files that exist and are not in force, with the reason.
    pub not_loaded: Vec<(String, String)>,
}

/// The repository root of `workspace`: the nearest directory at or above it
/// that holds a `.git`, else the workspace itself. Both are canonical.
fn repo_root_of(workspace: &Path) -> PathBuf {
    workspace
        .ancestors()
        .find(|d| d.join(".git").exists())
        .unwrap_or(workspace)
        .to_path_buf()
}

/// `dir/name` as the user would read it: relative to the workspace root,
/// `../` for a directory above it.
fn display_of(workspace: &Path, dir: &Path, name: &str) -> String {
    if let Ok(rel) = dir.strip_prefix(workspace) {
        let rel = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        return if rel.is_empty() {
            name.to_owned()
        } else {
            format!("{}/{name}", rel.join("/"))
        };
    }
    let ups = workspace
        .ancestors()
        .position(|a| a == dir)
        .unwrap_or_default();
    format!("{}{name}", "../".repeat(ups))
}

/// The depth of `dir` below the repository root.
fn level_of(repo: &Path, dir: &Path) -> usize {
    dir.strip_prefix(repo)
        .map(|r| r.components().count())
        .unwrap_or_default()
}

/// The longest prefix of `s` that is at most `max` bytes and ends on a
/// character boundary.
fn cut(s: &str, max: usize) -> &str {
    let mut end = max.min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn marker(shown: usize, total: u64, cap: usize) -> String {
    format!("\n[... truncated by Modbit: {shown} of {total} bytes shown (cap {cap} bytes)]")
}

/// One file, read under the confinement rules; `Ok(None)` when the name is
/// not there.
fn load_one(
    repo: &Path,
    workspace: &Path,
    dir: &Path,
    name: &str,
    seen: &mut BTreeSet<PathBuf>,
) -> Result<Option<Rule>, (String, String)> {
    let candidate = dir.join(name);
    let source = display_of(workspace, dir, name);
    let not_loaded = |why: String| Err((source.clone(), why));
    if std::fs::symlink_metadata(&candidate).is_err() {
        return Ok(None);
    }
    let resolved = match std::fs::canonicalize(&candidate) {
        Ok(r) => r,
        Err(e) => return not_loaded(format!("not loaded: it does not resolve ({e})")),
    };
    // Compared as paths, never as strings (the Windows verbatim form).
    if !resolved.starts_with(repo) {
        return not_loaded(
            "not loaded: it resolves outside the repository (symbolic links are followed and confined to the repository root)"
                .into(),
        );
    }
    if !seen.insert(resolved.clone()) {
        // The same file under another name (CLAUDE.md linked to AGENTS.md):
        // it is in force once, under the name that wins.
        return Ok(None);
    }
    let Ok(meta) = std::fs::metadata(&resolved) else {
        return not_loaded("not loaded: unreadable".into());
    };
    if !meta.is_file() {
        return not_loaded("not loaded: it is not a regular file".into());
    }
    let mut bytes = Vec::new();
    let read = std::fs::File::open(&resolved)
        .and_then(|f| f.take(HARD_READ_CAP).read_to_end(&mut bytes))
        .map(|_| ());
    if let Err(e) = read {
        return not_loaded(format!("not loaded: unreadable ({e})"));
    }
    let total = meta.len();
    let hash = hex::encode(Sha256::digest(&bytes));
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{feff}').trim();
    if text.is_empty() {
        return not_loaded("not loaded: it is empty".into());
    }
    let mut truncated = total > bytes.len() as u64;
    let body = if text.len() > FILE_CAP {
        truncated = true;
        let shown = cut(text, FILE_CAP);
        format!(
            "{}{}",
            shown.trim_end(),
            marker(shown.len(), total, FILE_CAP)
        )
    } else {
        text.to_owned()
    };
    let level = level_of(repo, dir);
    let scope = dir
        .strip_prefix(workspace)
        .ok()
        .filter(|r| !r.as_os_str().is_empty());
    Ok(Some(Rule {
        id: source.clone(),
        layer: INSTRUCTIONS_LAYER.into(),
        layer_index: 0,
        source,
        paths: scope
            .map(|r| {
                vec![format!(
                    "{}/**",
                    r.components()
                        .map(|c| c.as_os_str().to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join("/")
                )]
            })
            .unwrap_or_default(),
        // Deeper wins; `AGENTS.md` before `CLAUDE.md` at one depth.
        priority: i64::try_from(level * 2).unwrap_or(i64::MAX - 1)
            + i64::from(name == FILE_NAMES[0]),
        expires_at_ms: None,
        body,
        hash,
        bytes: total,
        truncated,
        findings: Vec::new(),
    }))
}

/// Normal components of a root-relative path, or `None` when it climbs out
/// of the root or is absolute.
fn relative_components(path: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for c in Path::new(path).components() {
        match c {
            Component::Normal(n) => out.push(n.to_string_lossy().into_owned()),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

/// Find the instruction files in force for a workspace and the paths its
/// task has made active, applying the caps; see the module documentation.
/// Reads the files every call: an edit between two turns changes the next
/// one, and its hash.
#[must_use]
pub fn discover(workspace_root: &Path, active_paths: &[String]) -> Discovery {
    let mut out = Discovery::default();
    let Ok(workspace) = std::fs::canonicalize(workspace_root) else {
        return out;
    };
    let repo = repo_root_of(&workspace);
    let mut seen = BTreeSet::new();
    let mut dirs: Vec<PathBuf> = workspace
        .ancestors()
        .take_while(|d| d.starts_with(&repo))
        .map(Path::to_path_buf)
        .collect();
    // Nested directories on the way to the active paths, each once.
    let mut nested: BTreeSet<PathBuf> = BTreeSet::new();
    for p in active_paths {
        let Some(parts) = relative_components(p) else {
            continue;
        };
        for i in 1..parts.len() {
            let mut dir = workspace.clone();
            dir.extend(&parts[..i]);
            nested.insert(dir);
        }
    }
    dirs.extend(nested.into_iter().take(NESTED_DIR_CAP));
    for dir in &dirs {
        for name in FILE_NAMES {
            match load_one(&repo, &workspace, dir, name, &mut seen) {
                Ok(Some(rule)) => out.rules.push(rule),
                Ok(None) => {}
                Err(refusal) => out.not_loaded.push(refusal),
            }
        }
    }
    // The aggregate cap, spent on the files that win first.
    out.rules
        .sort_by(|a, b| b.priority.cmp(&a.priority).then(a.source.cmp(&b.source)));
    let mut left = AGGREGATE_CAP;
    let mut kept = Vec::new();
    for mut rule in std::mem::take(&mut out.rules) {
        if left < MIN_USEFUL_BYTES {
            out.not_loaded.push((
                rule.source,
                format!(
                    "not loaded: the aggregate cap of {AGGREGATE_CAP} bytes was spent on the files that take precedence"
                ),
            ));
            continue;
        }
        if rule.body.len() > left {
            let shown = cut(&rule.body, left).trim_end().to_owned();
            rule.body = format!("{shown}{}", marker(shown.len(), rule.bytes, left));
            rule.truncated = true;
        }
        left = left.saturating_sub(rule.body.len());
        kept.push(rule);
    }
    out.rules = kept;
    out
}

/// The instruction files that exist in the workspace root and above it, up
/// to the repository root, without reading any of them: what an untrusted
/// repository holds and the Core does not load.
#[must_use]
pub fn present(workspace_root: &Path) -> Vec<String> {
    let Ok(workspace) = std::fs::canonicalize(workspace_root) else {
        return Vec::new();
    };
    let repo = repo_root_of(&workspace);
    let mut out = Vec::new();
    for dir in workspace.ancestors().take_while(|d| d.starts_with(&repo)) {
        for name in FILE_NAMES {
            if std::fs::symlink_metadata(dir.join(name)).is_ok() {
                out.push(display_of(&workspace, dir, name));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::RuleSet;

    fn write(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    fn repo() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join(".git")).unwrap();
        d
    }

    fn select(d: &Discovery, active: &[&str]) -> (RuleSet, Vec<String>) {
        let mut set = RuleSet {
            rules: d.rules.clone(),
            invalid: vec![],
        };
        for r in &mut set.rules {
            r.layer_index = 1;
        }
        let active: Vec<String> = active.iter().map(|s| (*s).to_owned()).collect();
        let s = set.select(&active, 0);
        (set.clone(), set.texts(&s))
    }

    #[test]
    fn the_nearer_directory_wins_and_agents_precedes_claude() {
        let r = repo();
        write(r.path(), "AGENTS.md", "ROOT-AGENTS");
        write(r.path(), "CLAUDE.md", "ROOT-CLAUDE");
        write(r.path(), "pkg/AGENTS.md", "PKG-AGENTS");
        write(r.path(), "pkg/CLAUDE.md", "PKG-CLAUDE");
        write(r.path(), "pkg/deep/AGENTS.md", "DEEP-AGENTS");
        // From the root, nothing nested is active yet.
        let d = discover(r.path(), &[]);
        let sources: Vec<&str> = d.rules.iter().map(|x| x.source.as_str()).collect();
        assert_eq!(sources, ["AGENTS.md", "CLAUDE.md"]);
        let (_, texts) = select(&d, &[]);
        assert!(texts[0].contains("Repository instruction files"));
        assert!(texts[1].contains("ROOT-AGENTS") && texts[2].contains("ROOT-CLAUDE"));
        // A touched path under pkg/deep activates its nested files: the
        // deepest is listed first, the root's last.
        let d = discover(r.path(), &["pkg/deep/x.rs".into()]);
        let (_, texts) = select(&d, &["pkg/deep/x.rs"]);
        let order: Vec<&str> = texts
            .iter()
            .filter_map(|t| {
                [
                    "DEEP-AGENTS",
                    "PKG-AGENTS",
                    "PKG-CLAUDE",
                    "ROOT-AGENTS",
                    "ROOT-CLAUDE",
                ]
                .into_iter()
                .find(|k| t.contains(k))
            })
            .collect();
        assert_eq!(
            order,
            [
                "DEEP-AGENTS",
                "PKG-AGENTS",
                "PKG-CLAUDE",
                "ROOT-AGENTS",
                "ROOT-CLAUDE"
            ]
        );
        // A path elsewhere leaves the nested ones dormant.
        let (_, texts) = select(&d, &["README.md"]);
        assert!(!texts.iter().any(|t| t.contains("PKG-AGENTS")));
    }

    #[test]
    fn a_workspace_below_the_repository_root_reads_its_ancestors_and_stops_at_the_root() {
        let outer = tempfile::tempdir().unwrap();
        write(outer.path(), "AGENTS.md", "ABOVE-THE-REPO");
        let repo = outer.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        write(&repo, "AGENTS.md", "REPO-ROOT");
        write(&repo, "apps/web/AGENTS.md", "WORKSPACE");
        let d = discover(&repo.join("apps/web"), &[]);
        let sources: Vec<&str> = d.rules.iter().map(|x| x.source.as_str()).collect();
        assert_eq!(sources, ["AGENTS.md", "../../AGENTS.md"]);
        let bodies: String = d.rules.iter().map(|r| r.body.as_str()).collect();
        assert!(bodies.contains("WORKSPACE") && bodies.contains("REPO-ROOT"));
        assert!(
            !bodies.contains("ABOVE-THE-REPO"),
            "nothing above the repository root is read"
        );
        // Without a `.git` anywhere the workspace is its own root.
        let lone = tempfile::tempdir().unwrap();
        write(lone.path(), "AGENTS.md", "LONE");
        write(lone.path(), "sub/AGENTS.md", "SUB");
        let d = discover(&lone.path().join("sub"), &[]);
        let bodies: String = d.rules.iter().map(|r| r.body.as_str()).collect();
        assert!(
            bodies.contains("SUB") && !bodies.contains("LONE"),
            "{bodies}"
        );
    }

    #[test]
    fn a_file_over_the_cap_is_cut_with_a_marker_and_the_total_stays_bounded() {
        let r = repo();
        write(r.path(), "AGENTS.md", &"a".repeat(FILE_CAP * 2));
        let d = discover(r.path(), &[]);
        let rule = &d.rules[0];
        assert!(rule.truncated && rule.bytes == (FILE_CAP * 2) as u64);
        assert!(rule.body.contains("truncated by Modbit"), "{}", rule.body);
        assert!(rule.body.len() < FILE_CAP + 200);
        // Many near-cap files together stay under the aggregate cap.
        write(r.path(), "CLAUDE.md", &"b".repeat(FILE_CAP - 10));
        for i in 0..4 {
            write(
                r.path(),
                &format!("d{i}/AGENTS.md"),
                &"c".repeat(FILE_CAP - 10),
            );
        }
        let active: Vec<String> = (0..4).map(|i| format!("d{i}/f.rs")).collect();
        let d = discover(r.path(), &active);
        let total: usize = d.rules.iter().map(|r| r.body.len()).sum();
        assert!(total <= AGGREGATE_CAP + 600, "{total}");
        assert!(
            d.not_loaded
                .iter()
                .any(|(_, why)| why.contains("aggregate cap")),
            "{:?}",
            d.not_loaded
        );
        // The ones that win are the nearest.
        assert!(d.rules[0].source.contains('/'), "{:?}", d.rules[0].source);
    }

    #[test]
    fn the_hash_is_over_the_bytes_and_changes_when_the_file_does() {
        let r = repo();
        write(r.path(), "AGENTS.md", "one");
        let h1 = discover(r.path(), &[]).rules[0].hash.clone();
        assert_eq!(h1, hex::encode(Sha256::digest(b"one")));
        write(r.path(), "AGENTS.md", "two");
        let h2 = discover(r.path(), &[]).rules[0].hash.clone();
        assert_ne!(h1, h2);
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_links_are_followed_inside_the_repository_and_refused_outside_it() {
        use std::os::unix::fs::symlink;
        let outside = tempfile::tempdir().unwrap();
        write(outside.path(), "secret.md", "OUTSIDE-TEXT");
        let r = repo();
        write(r.path(), "docs/shared.md", "SHARED-INSIDE");
        symlink(r.path().join("docs/shared.md"), r.path().join("AGENTS.md")).unwrap();
        symlink(outside.path().join("secret.md"), r.path().join("CLAUDE.md")).unwrap();
        let d = discover(r.path(), &[]);
        assert_eq!(d.rules.len(), 1, "{d:?}");
        assert!(d.rules[0].body.contains("SHARED-INSIDE"));
        let (src, why) = &d.not_loaded[0];
        assert_eq!(src, "CLAUDE.md");
        assert!(why.contains("outside the repository"), "{why}");
        assert!(
            !d.rules.iter().any(|r| r.body.contains("OUTSIDE-TEXT")),
            "a link out of the repository was read"
        );
        // A nested directory that is a link out is refused as well.
        symlink(outside.path(), r.path().join("evil")).unwrap();
        write(outside.path(), "AGENTS.md", "OUTSIDE-AGENTS");
        let d = discover(r.path(), &["evil/x.rs".into()]);
        assert!(!d.rules.iter().any(|r| r.body.contains("OUTSIDE-AGENTS")));
        assert!(d.not_loaded.iter().any(|(s, _)| s == "evil/AGENTS.md"));
        // CLAUDE.md linked to AGENTS.md is one file, in force once.
        let r2 = repo();
        write(r2.path(), "AGENTS.md", "SAME");
        symlink(r2.path().join("AGENTS.md"), r2.path().join("CLAUDE.md")).unwrap();
        assert_eq!(discover(r2.path(), &[]).rules.len(), 1);
    }

    #[test]
    fn a_hostile_active_path_cannot_climb_out_of_the_workspace() {
        let outer = tempfile::tempdir().unwrap();
        write(outer.path(), "up/AGENTS.md", "CLIMBED");
        let r = outer.path().join("ws");
        std::fs::create_dir_all(r.join(".git")).unwrap();
        let d = discover(&r, &["../up/x.rs".into(), "/etc/x".into()]);
        assert!(d.rules.is_empty() && d.not_loaded.is_empty(), "{d:?}");
    }

    #[test]
    fn present_lists_the_files_without_reading_them() {
        let r = repo();
        write(r.path(), "AGENTS.md", "x");
        write(r.path(), "CLAUDE.md", "y");
        assert_eq!(present(r.path()), ["AGENTS.md", "CLAUDE.md"]);
        assert!(present(&r.path().join("nope")).is_empty());
    }
}
