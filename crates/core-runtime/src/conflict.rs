//! Semantic write-conflict detection (docs/14 "Semantic conflict
//! detection"; M6.4): before parallel builders start, their write sets are
//! checked against the workers already admitted — explicit path overlap,
//! the same public symbol or interface owned from the AST/LSP symbol
//! graph, dependency hot spots, migration/schema/shared configuration
//! files, generated files and lockfiles, and test fixtures likely to
//! conflict. Pure: the Core hands in what its index knows; this module
//! says what conflicts and why, and admission refuses or isolates.

use serde::{Deserialize, Serialize};

/// What the repository knows about its files, as the detector needs it.
pub trait RepoFacts {
    /// Every indexed path (root-relative, `/`-separated).
    fn paths(&self) -> Vec<String>;
    /// Public symbols a file defines: `(name, kind, container)`.
    fn symbols_of(&self, path: &str) -> Vec<(String, String, Option<String>)>;
    /// Files that import `path` (direct importers).
    fn importers_of(&self, path: &str) -> Vec<String>;
}

/// The facts as plain data, for callers that gathered them already.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facts {
    /// Paths.
    pub paths: Vec<String>,
    /// Path → symbols.
    pub symbols: Vec<(String, Vec<(String, String, Option<String>)>)>,
    /// Path → importers.
    pub importers: Vec<(String, Vec<String>)>,
}

impl RepoFacts for Facts {
    fn paths(&self) -> Vec<String> {
        self.paths.clone()
    }
    fn symbols_of(&self, path: &str) -> Vec<(String, String, Option<String>)> {
        self.symbols
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, s)| s.clone())
            .unwrap_or_default()
    }
    fn importers_of(&self, path: &str) -> Vec<String> {
        self.importers
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, s)| s.clone())
            .unwrap_or_default()
    }
}

/// The rules and their thresholds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictPolicy {
    /// A file imported by at least this many others is a hot spot.
    pub hot_spot_importers: usize,
    /// Patterns of files only one worker may hold at a time: migrations,
    /// schemas, shared configuration, CI, lockfiles.
    pub shared_patterns: Vec<String>,
    /// Patterns of generated files: never two workers.
    pub generated_patterns: Vec<String>,
    /// Patterns of test fixtures: shared per directory.
    pub fixture_patterns: Vec<String>,
}

impl Default for ConflictPolicy {
    fn default() -> Self {
        Self {
            hot_spot_importers: 5,
            shared_patterns: [
                "Cargo.lock",
                "Cargo.toml",
                "package.json",
                "package-lock.json",
                "pnpm-lock.yaml",
                "yarn.lock",
                "go.mod",
                "go.sum",
                "poetry.lock",
                "requirements.txt",
                "tsconfig.json",
                "migrations/**",
                "**/migrations/**",
                "schema.*",
                "**/schema.*",
                ".github/workflows/**",
                "*.lock",
            ]
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
            generated_patterns: [
                "**/generated/**",
                "**/gen/**",
                "*_pb.*",
                "**/*_pb.*",
                "*.gen.*",
                "**/*.gen.*",
            ]
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
            fixture_patterns: [
                "tests/fixtures/**",
                "**/fixtures/**",
                "**/__snapshots__/**",
                "**/testdata/**",
            ]
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        }
    }
}

/// Which rule fired.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Rule {
    /// The scopes cover the same path.
    PathOverlap,
    /// The scopes define the same public symbol or interface.
    SymbolOwnership,
    /// One scope holds a file the other's scope imports heavily.
    HotSpot,
    /// A migration, schema, shared configuration or lockfile is in the
    /// candidate's scope while another worker is live.
    SharedFile,
    /// A generated file or lockfile is in both scopes' reach.
    GeneratedFile,
    /// Test fixtures under the same directory in both scopes.
    TestFixture,
}

/// Whether the rule blocks admission or only warns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Severity {
    /// Admission refuses (or replans/isolates).
    Block,
    /// Admission proceeds; the parent is told.
    Warn,
}

/// One conflict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    /// Rule.
    pub rule: Rule,
    /// Severity.
    pub severity: Severity,
    /// The admitted worker it conflicts with (empty for a rule about the
    /// candidate alone).
    pub with: String,
    /// The path or symbol.
    pub subject: String,
    /// Why.
    pub detail: String,
}

fn norm(p: &str) -> String {
    p.trim()
        .trim_start_matches("./")
        .trim_end_matches('/')
        .replace('\\', "/")
}

/// Whether a scope entry (a path, a prefix or a glob) covers a path.
#[must_use]
pub fn scope_covers(entry: &str, path: &str) -> bool {
    let (e, p) = (norm(entry), norm(path));
    if e.is_empty() {
        return false;
    }
    if e == "**" || e == "*" {
        return true;
    }
    if let Some(prefix) = e.strip_suffix("/**").or_else(|| e.strip_suffix("/*")) {
        return p == prefix || p.starts_with(&format!("{prefix}/"));
    }
    if e.contains('*') {
        return glob_match(&e, &p);
    }
    e == p || p.starts_with(&format!("{e}/"))
}

/// A small glob: `**` any path, `*` within a segment, `?` one char.
#[must_use]
pub fn glob_match(pattern: &str, path: &str) -> bool {
    fn rec(p: &[u8], s: &[u8]) -> bool {
        match p.first() {
            None => s.is_empty(),
            Some(b'*') if p.get(1) == Some(&b'*') => {
                let rest = if p.get(2) == Some(&b'/') {
                    &p[3..]
                } else {
                    &p[2..]
                };
                (0..=s.len()).any(|i| rec(rest, &s[i..]))
            }
            Some(b'*') => {
                let rest = &p[1..];
                (0..=s.len())
                    .take_while(|&i| i == 0 || s[i - 1] != b'/')
                    .any(|i| rec(rest, &s[i..]))
            }
            Some(b'?') => !s.is_empty() && s[0] != b'/' && rec(&p[1..], &s[1..]),
            Some(c) => !s.is_empty() && s[0] == *c && rec(&p[1..], &s[1..]),
        }
    }
    // A pattern without a slash matches the file name in any directory.
    if !pattern.contains('/') {
        let name = path.rsplit('/').next().unwrap_or(path);
        return rec(pattern.as_bytes(), name.as_bytes());
    }
    rec(pattern.as_bytes(), path.as_bytes())
}

fn any_pattern(patterns: &[String], path: &str) -> bool {
    patterns.iter().any(|p| glob_match(p, &norm(path)))
}

/// The indexed files a scope covers.
fn files_in(facts: &dyn RepoFacts, scope: &[String]) -> Vec<String> {
    facts
        .paths()
        .into_iter()
        .filter(|p| scope.iter().any(|s| scope_covers(s, p)))
        .collect()
}

/// Detect the conflicts of `candidate` against the `admitted` write sets
/// (name, scope). Every rule is reported; the caller blocks on any
/// `Block`.
#[must_use]
pub fn detect(
    facts: &dyn RepoFacts,
    candidate: &[String],
    admitted: &[(String, Vec<String>)],
    policy: &ConflictPolicy,
) -> Vec<Conflict> {
    let mut out = Vec::new();
    let cand_files = files_in(facts, candidate);
    // 1. Explicit overlap of the scopes themselves (pattern against
    //    pattern, so an empty repository still refuses the obvious).
    for (who, scope) in admitted {
        for c in candidate {
            for o in scope {
                if scope_covers(c, o) || scope_covers(o, c) {
                    out.push(Conflict {
                        rule: Rule::PathOverlap,
                        severity: Severity::Block,
                        with: who.clone(),
                        subject: c.clone(),
                        detail: format!("`{c}` and `{o}` cover the same paths"),
                    });
                }
            }
        }
    }
    // 2. Symbol ownership: a public symbol defined in both scopes' files.
    let mut cand_symbols: Vec<(String, String)> = Vec::new();
    for f in &cand_files {
        for (name, kind, container) in facts.symbols_of(f) {
            if is_public_kind(&kind) {
                let key = container.map(|c| format!("{c}::{name}")).unwrap_or(name);
                cand_symbols.push((key, f.clone()));
            }
        }
    }
    for (who, scope) in admitted {
        let other_files = files_in(facts, scope);
        for f in &other_files {
            if cand_files.contains(f) {
                continue; // already a path overlap
            }
            for (name, kind, container) in facts.symbols_of(f) {
                if !is_public_kind(&kind) {
                    continue;
                }
                let key = container.map(|c| format!("{c}::{name}")).unwrap_or(name);
                if let Some((_, cf)) = cand_symbols.iter().find(|(k, _)| *k == key) {
                    out.push(Conflict {
                        rule: Rule::SymbolOwnership,
                        severity: Severity::Block,
                        with: who.clone(),
                        subject: key.clone(),
                        detail: format!(
                            "`{key}` is defined in `{cf}` (candidate) and `{f}` ({who})"
                        ),
                    });
                }
            }
        }
        // 3. Hot spots: a file in one scope imported by files in the other's.
        for f in &cand_files {
            let importers = facts.importers_of(f);
            if importers.len() >= policy.hot_spot_importers
                && importers.iter().any(|i| other_files.contains(i))
            {
                out.push(Conflict {
                    rule: Rule::HotSpot,
                    severity: Severity::Block,
                    with: who.clone(),
                    subject: f.clone(),
                    detail: format!(
                        "`{f}` is imported by {} files, some in {who}'s scope",
                        importers.len()
                    ),
                });
            }
        }
        for f in &other_files {
            let importers = facts.importers_of(f);
            if importers.len() >= policy.hot_spot_importers
                && importers.iter().any(|i| cand_files.contains(i))
            {
                out.push(Conflict {
                    rule: Rule::HotSpot,
                    severity: Severity::Warn,
                    with: who.clone(),
                    subject: f.clone(),
                    detail: format!(
                        "{who} holds `{f}`, imported by {} files, some in the candidate's scope; verify after the merge",
                        importers.len()
                    ),
                });
            }
        }
        // 6. Fixtures under the same directory.
        for f in &cand_files {
            if !any_pattern(&policy.fixture_patterns, f) {
                continue;
            }
            let dir = f.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
            if other_files.iter().any(|o| {
                any_pattern(&policy.fixture_patterns, o)
                    && o.rsplit_once('/').map(|(d, _)| d).unwrap_or("") == dir
            }) {
                out.push(Conflict {
                    rule: Rule::TestFixture,
                    severity: Severity::Block,
                    with: who.clone(),
                    subject: f.clone(),
                    detail: format!("fixtures under `{dir}` are in both scopes"),
                });
            }
        }
    }
    // 4/5. Shared and generated files: one worker at a time, whoever else
    //      is live; the scope patterns count too, not only indexed files.
    if !admitted.is_empty() {
        let mut shared: Vec<String> = cand_files
            .iter()
            .filter(|f| any_pattern(&policy.shared_patterns, f))
            .cloned()
            .collect();
        shared.extend(
            candidate
                .iter()
                .filter(|c| any_pattern(&policy.shared_patterns, c))
                .cloned(),
        );
        shared.sort();
        shared.dedup();
        for f in shared {
            out.push(Conflict {
                rule: Rule::SharedFile,
                severity: Severity::Block,
                with: admitted.iter().map(|(w, _)| w.clone()).collect::<Vec<_>>().join(","),
                subject: f.clone(),
                detail: format!("`{f}` is a migration, schema, shared configuration or lockfile; one worker at a time"),
            });
        }
        let mut generated: Vec<String> = cand_files
            .iter()
            .filter(|f| any_pattern(&policy.generated_patterns, f))
            .cloned()
            .collect();
        generated.extend(
            candidate
                .iter()
                .filter(|c| any_pattern(&policy.generated_patterns, c))
                .cloned(),
        );
        generated.sort();
        generated.dedup();
        for f in generated {
            out.push(Conflict {
                rule: Rule::GeneratedFile,
                severity: Severity::Block,
                with: admitted
                    .iter()
                    .map(|(w, _)| w.clone())
                    .collect::<Vec<_>>()
                    .join(","),
                subject: f.clone(),
                detail: format!("`{f}` is generated; regenerate once, after the merge"),
            });
        }
    }
    out
}

fn is_public_kind(kind: &str) -> bool {
    matches!(
        kind,
        "function"
            | "method"
            | "class"
            | "struct"
            | "enum"
            | "interface"
            | "trait"
            | "type"
            | "const"
            | "module"
    )
}

/// Whether any conflict blocks.
#[must_use]
pub fn blocks(conflicts: &[Conflict]) -> bool {
    conflicts.iter().any(|c| c.severity == Severity::Block)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts {
            paths: vec![
                "src/api/users.rs".into(),
                "src/api/mod.rs".into(),
                "src/db/schema.rs".into(),
                "src/core/types.rs".into(),
                "src/a/one.rs".into(),
                "src/b/two.rs".into(),
                "src/c/three.rs".into(),
                "src/d/four.rs".into(),
                "src/e/five.rs".into(),
                "src/f/six.rs".into(),
                "Cargo.lock".into(),
                "migrations/0001_init.sql".into(),
                "packages/proto/gen/surface_pb.js".into(),
                "tests/fixtures/repos/a.json".into(),
                "tests/fixtures/repos/b.json".into(),
            ],
            symbols: vec![
                (
                    "src/api/users.rs".into(),
                    vec![
                        ("User".into(), "struct".into(), None),
                        ("list".into(), "function".into(), None),
                    ],
                ),
                (
                    "src/core/types.rs".into(),
                    vec![("User".into(), "struct".into(), None)],
                ),
                (
                    "src/a/one.rs".into(),
                    vec![("helper".into(), "function".into(), None)],
                ),
            ],
            importers: vec![(
                "src/core/types.rs".into(),
                vec![
                    "src/a/one.rs".into(),
                    "src/b/two.rs".into(),
                    "src/c/three.rs".into(),
                    "src/d/four.rs".into(),
                    "src/e/five.rs".into(),
                ],
            )],
        }
    }

    fn admitted(name: &str, scope: &[&str]) -> (String, Vec<String>) {
        (name.into(), scope.iter().map(|s| (*s).to_owned()).collect())
    }

    #[test]
    fn disjoint_scopes_pass_and_overlapping_ones_block() {
        let f = facts();
        let p = ConflictPolicy::default();
        let none = detect(&f, &["src/b/".into()], &[admitted("w1", &["src/a/"])], &p);
        assert!(none.is_empty(), "{none:?}");
        let hit = detect(
            &f,
            &["src/a/one.rs".into()],
            &[admitted("w1", &["src/a/"])],
            &p,
        );
        assert!(
            hit.iter().any(|c| c.rule == Rule::PathOverlap
                && c.severity == Severity::Block
                && c.with == "w1"),
            "{hit:?}"
        );
        assert!(blocks(&hit));
    }

    #[test]
    fn the_same_public_symbol_in_two_scopes_is_an_ownership_conflict() {
        let f = facts();
        let p = ConflictPolicy::default();
        let hit = detect(
            &f,
            &["src/api/".into()],
            &[admitted("w1", &["src/core/"])],
            &p,
        );
        assert!(
            hit.iter()
                .any(|c| c.rule == Rule::SymbolOwnership && c.subject == "User"),
            "{hit:?}"
        );
    }

    #[test]
    fn hot_spots_block_the_holder_and_warn_the_dependent() {
        let f = facts();
        let p = ConflictPolicy::default();
        // The candidate holds the hot spot; w1's files import it: block.
        let hit = detect(
            &f,
            &["src/core/".into()],
            &[admitted("w1", &["src/a/"])],
            &p,
        );
        assert!(
            hit.iter()
                .any(|c| c.rule == Rule::HotSpot && c.severity == Severity::Block),
            "{hit:?}"
        );
        // w1 holds the hot spot; the candidate depends on it: a warning.
        let warn = detect(
            &f,
            &["src/b/".into()],
            &[admitted("w1", &["src/core/"])],
            &p,
        );
        assert!(
            warn.iter()
                .any(|c| c.rule == Rule::HotSpot && c.severity == Severity::Warn),
            "{warn:?}"
        );
        assert!(!blocks(&warn));
    }

    #[test]
    fn shared_generated_and_fixture_files_are_one_worker_at_a_time() {
        let f = facts();
        let p = ConflictPolicy::default();
        let lock = detect(
            &f,
            &["Cargo.lock".into()],
            &[admitted("w1", &["src/a/"])],
            &p,
        );
        assert!(lock.iter().any(|c| c.rule == Rule::SharedFile), "{lock:?}");
        let mig = detect(
            &f,
            &["migrations/".into()],
            &[admitted("w1", &["src/a/"])],
            &p,
        );
        assert!(mig.iter().any(|c| c.rule == Rule::SharedFile), "{mig:?}");
        let generated = detect(
            &f,
            &["packages/proto/gen/".into()],
            &[admitted("w1", &["src/a/"])],
            &p,
        );
        assert!(
            generated.iter().any(|c| c.rule == Rule::GeneratedFile),
            "{generated:?}"
        );
        let fx = detect(
            &f,
            &["tests/fixtures/repos/a.json".into()],
            &[admitted("w1", &["tests/fixtures/repos/b.json"])],
            &p,
        );
        assert!(fx.iter().any(|c| c.rule == Rule::TestFixture), "{fx:?}");
        // Alone, the same scopes are fine: the rules are about concurrency.
        assert!(detect(&f, &["Cargo.lock".into()], &[], &p).is_empty());
    }

    #[test]
    fn globs_match_names_and_paths() {
        assert!(glob_match("*.lock", "Cargo.lock"));
        assert!(glob_match("*.lock", "deep/dir/x.lock"));
        assert!(glob_match("**/migrations/**", "db/migrations/0001.sql"));
        assert!(glob_match("migrations/**", "migrations/0001.sql"));
        assert!(!glob_match("migrations/**", "src/migrations_helper.rs"));
        assert!(glob_match("*_pb.*", "packages/x/surface_pb.js"));
        assert!(scope_covers("src/api/", "src/api/users.rs"));
        assert!(scope_covers("src/**", "src/a/b.rs"));
        assert!(!scope_covers("src/api", "src/apix.rs"));
    }
}
