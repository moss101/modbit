//! Dependency / Git / test / runtime-evidence graph (docs/18 "Index inputs":
//! import/dependency edges, Git ownership/change history and changed-line
//! context, test mappings and recent runtime evidence; L2/L3 expansion; M3.6).
//!
//! An in-memory adjacency store per workspace, rebuilt per changed path:
//! `imports` from the real tree-sitter parse of each file, resolved to
//! workspace paths; `cochange`, `owners` and `commits` from the real recent
//! Git history; `changed_lines` from the real worktree diff; `tests` from
//! test files whose imports reach the path; `evidence` from the verification
//! checks the host attributes. Every view names the revision it was built at.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Parser};

/// A commit with the paths it touched (mirrors the git crate's record).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitRecord {
    /// Sha.
    pub sha: String,
    /// Author.
    pub author: String,
    /// Date.
    pub date: String,
    /// Subject.
    pub subject: String,
    /// Paths.
    pub files: Vec<String>,
}

/// A query.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphQuery {
    /// Root-relative path.
    pub path: String,
    /// `imports` | `importers` | `cochange` | `owners` | `commits` |
    /// `changed_lines` | `tests` | `evidence` | `all`.
    pub relation: String,
    /// Expansion depth for import relations (1..3).
    pub depth: u32,
    /// Ceiling (0 = 50).
    pub max: usize,
}

/// The answer for one path.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphView {
    /// Path.
    pub path: String,
    /// Revision the graph was built at.
    pub revision: u64,
    /// Files this path imports (transitively to `depth`), with distance.
    pub imports: Vec<(String, u32)>,
    /// Files importing this path (transitively to `depth`), with distance.
    pub importers: Vec<(String, u32)>,
    /// Files changed together with this one in recent commits (count).
    pub cochange: Vec<(String, u32)>,
    /// Authors by commit count.
    pub owners: Vec<(String, u32)>,
    /// Recent commits touching the path.
    pub commits: Vec<CommitRecord>,
    /// Changed line ranges in the worktree versus HEAD (1-based, inclusive).
    pub changed_lines: Vec<(u32, u32)>,
    /// Test files whose imports reach this path.
    pub tests: Vec<String>,
    /// Verification checks (check id, status) attributed to this path.
    pub evidence: Vec<(String, String)>,
}

/// Changed line ranges per path.
pub type ChangedLines = BTreeMap<String, Vec<(u32, u32)>>;
/// A refreshed path: `None` removes; `Some((text, language))` re-indexes.
pub type ChangedFile<'a> = (&'a str, Option<(&'a str, Option<&'a str>)>);

/// The evidence graph of one workspace.
#[derive(Debug, Default)]
pub struct EvidenceGraph {
    revision: u64,
    imports: BTreeMap<String, BTreeSet<String>>,
    importers: BTreeMap<String, BTreeSet<String>>,
    test_files: BTreeSet<String>,
    commits: Vec<CommitRecord>,
    changed_lines: ChangedLines,
    evidence: BTreeMap<String, Vec<(String, String)>>,
    paths: BTreeSet<String>,
}

fn grammar(language: &str, path: &str) -> Option<tree_sitter::Language> {
    Some(match language {
        "rust" => tree_sitter_rust::LANGUAGE.into(),
        "python" => tree_sitter_python::LANGUAGE.into(),
        "javascript" => tree_sitter_javascript::LANGUAGE.into(),
        "typescript" if path.ends_with(".tsx") => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "typescript" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        _ => return None,
    })
}

fn unquote(t: &str) -> String {
    t.trim_matches(|c| c == '"' || c == '\'').to_owned()
}

/// Import specifiers of a file, as written (module paths / specifiers),
/// from the real tree-sitter parse.
#[must_use]
pub fn import_specifiers(language: &str, path: &str, text: &str) -> Vec<String> {
    let Some(lang) = grammar(language, path) else {
        return vec![];
    };
    let mut parser = Parser::new();
    if parser.set_language(&lang).is_err() {
        return vec![];
    }
    let Some(tree) = parser.parse(text, None) else {
        return vec![];
    };
    let mut out = Vec::new();
    let mut stack: Vec<Node> = vec![tree.root_node()];
    let bytes = text.as_bytes();
    while let Some(node) = stack.pop() {
        match (language, node.kind()) {
            ("rust", "use_declaration") => {
                if let Some(arg) = node.child_by_field_name("argument")
                    && let Ok(t) = arg.utf8_text(bytes)
                {
                    out.push(t.to_owned());
                }
            }
            ("rust", "mod_item") if node.child_by_field_name("body").is_none() => {
                if let Some(n) = node.child_by_field_name("name")
                    && let Ok(t) = n.utf8_text(bytes)
                {
                    out.push(format!("mod {t}"));
                }
            }
            ("python", "import_statement" | "import_from_statement") => {
                if let Some(n) = node.child_by_field_name("module_name") {
                    if let Ok(t) = n.utf8_text(bytes) {
                        out.push(t.to_owned());
                    }
                } else {
                    let mut c = node.walk();
                    for ch in node.named_children(&mut c) {
                        if (ch.kind() == "dotted_name" || ch.kind() == "aliased_import")
                            && let Ok(t) = ch.utf8_text(bytes)
                        {
                            out.push(t.split_whitespace().next().unwrap_or_default().to_owned());
                        }
                    }
                }
            }
            ("javascript" | "typescript", "import_statement" | "export_statement") => {
                if let Some(src) = node.child_by_field_name("source")
                    && let Ok(t) = src.utf8_text(bytes)
                {
                    out.push(unquote(t));
                }
            }
            ("javascript" | "typescript", "call_expression") => {
                if let Some(f) = node.child_by_field_name("function")
                    && f.utf8_text(bytes).ok() == Some("require")
                    && let Some(args) = node.child_by_field_name("arguments")
                    && let Some(first) = args.named_child(0)
                    && let Ok(t) = first.utf8_text(bytes)
                {
                    out.push(unquote(t));
                }
            }
            _ => {}
        }
        let mut c = node.walk();
        let children: Vec<Node> = node.children(&mut c).collect();
        for ch in children.into_iter().rev() {
            stack.push(ch);
        }
    }
    out
}

fn dirname(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

fn join(dir: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = if dir.is_empty() {
        vec![]
    } else {
        dir.split('/').collect()
    };
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// Resolve an import specifier of `from` to a workspace path in `paths`.
#[must_use]
pub fn resolve_import(
    language: &str,
    from: &str,
    spec: &str,
    paths: &BTreeSet<String>,
) -> Option<String> {
    let dir = dirname(from);
    let candidates: Vec<String> = match language {
        "rust" => {
            if let Some(m) = spec.strip_prefix("mod ") {
                vec![
                    join(dir, &format!("{m}.rs")),
                    join(dir, &format!("{m}/mod.rs")),
                ]
            } else {
                let segs: Vec<&str> = spec
                    .trim_start_matches("crate::")
                    .trim_start_matches("super::")
                    .trim_start_matches("self::")
                    .split("::")
                    .collect();
                let crate_root = from.find("src/").map_or(dir, |i| &from[..i + 3]);
                let mut c = Vec::new();
                for n in (1..=segs.len().min(3)).rev() {
                    let p = segs[..n].join("/");
                    c.push(format!("{crate_root}/{p}.rs"));
                    c.push(format!("{crate_root}/{p}/mod.rs"));
                    c.push(join(dir, &format!("{p}.rs")));
                }
                if segs.len() > 1 && !matches!(segs[0], "std" | "core" | "alloc") {
                    // `use <own crate>::…` from tests/examples: the library root.
                    for n in (2..=segs.len().min(3)).rev() {
                        let p = segs[1..n].join("/");
                        c.push(format!("src/{p}.rs"));
                        c.push(format!("src/{p}/mod.rs"));
                    }
                    c.push("src/lib.rs".to_owned());
                    // A workspace sibling crate: `<name>` or `modbit_<name>` under
                    // crates/<name-with-dashes>/src (module file, then the root).
                    let bare = segs[0].strip_prefix("modbit_").unwrap_or(segs[0]);
                    for dir in [segs[0].replace('_', "-"), bare.replace('_', "-")] {
                        for n in (2..=segs.len().min(3)).rev() {
                            let p = segs[1..n].join("/");
                            c.push(format!("crates/{dir}/src/{p}.rs"));
                            c.push(format!("crates/{dir}/src/{p}/mod.rs"));
                        }
                        c.push(format!("crates/{dir}/src/lib.rs"));
                    }
                }
                c
            }
        }
        "python" => {
            let rel = spec.trim_start_matches('.');
            let dots = spec.len() - rel.len();
            let mut base = dir.to_owned();
            for _ in 1..dots.max(1) {
                base = dirname(&base).to_owned();
            }
            let modpath = rel.replace('.', "/");
            vec![
                join(&base, &format!("{modpath}.py")),
                join(&base, &format!("{modpath}/__init__.py")),
                format!("{modpath}.py"),
                format!("{modpath}/__init__.py"),
                format!("src/{modpath}.py"),
            ]
        }
        _ => {
            if !(spec.starts_with('.') || spec.starts_with('/')) {
                return None; // package import
            }
            let base = join(dir, spec);
            let base = base
                .trim_end_matches(".js")
                .trim_end_matches(".ts")
                .to_owned();
            let mut c = Vec::new();
            for ext in ["ts", "tsx", "js", "jsx", "mjs", "cjs"] {
                c.push(format!("{base}.{ext}"));
                c.push(format!("{base}/index.{ext}"));
            }
            c.push(join(dir, spec));
            c
        }
    };
    candidates.into_iter().find(|c| paths.contains(c))
}

/// Whether a path is a test file by convention.
#[must_use]
pub fn is_test_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    path.starts_with("tests/")
        || path.contains("/tests/")
        || path.starts_with("test/")
        || path.contains("/test/")
        || path.contains("__tests__/")
        || name.starts_with("test_")
        || name.ends_with("_test.py")
        || name.ends_with("_test.rs")
        || name.contains(".test.")
        || name.contains(".spec.")
}

impl EvidenceGraph {
    /// Build from `(path, text, language)` at `revision`, the recent commits
    /// and the worktree's changed line ranges.
    pub fn build<'a>(
        files: impl Iterator<Item = (&'a str, &'a str, Option<&'a str>)>,
        commits: Vec<CommitRecord>,
        changed_lines: ChangedLines,
        revision: u64,
    ) -> Self {
        let files: Vec<(String, String, Option<String>)> = files
            .map(|(p, t, l)| (p.to_owned(), t.to_owned(), l.map(str::to_owned)))
            .collect();
        let mut g = Self {
            revision,
            commits,
            changed_lines,
            ..Self::default()
        };
        g.paths = files.iter().map(|(p, _, _)| p.clone()).collect();
        for (p, t, l) in &files {
            g.index_file(p, t, l.as_deref());
        }
        g
    }

    /// Revision.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Number of import edges.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.imports.values().map(BTreeSet::len).sum()
    }

    fn drop_imports(&mut self, path: &str) {
        if let Some(old) = self.imports.remove(path) {
            for t in old {
                if let Some(set) = self.importers.get_mut(&t) {
                    set.remove(path);
                }
            }
        }
    }

    fn index_file(&mut self, path: &str, text: &str, language: Option<&str>) {
        self.drop_imports(path);
        self.paths.insert(path.to_owned());
        if is_test_path(path) {
            self.test_files.insert(path.to_owned());
        } else {
            self.test_files.remove(path);
        }
        let Some(language) = language else { return };
        let mut targets = BTreeSet::new();
        for spec in import_specifiers(language, path, text) {
            if let Some(t) = resolve_import(language, path, &spec, &self.paths)
                && t != path
            {
                targets.insert(t);
            }
        }
        for t in &targets {
            self.importers
                .entry(t.clone())
                .or_default()
                .insert(path.to_owned());
        }
        self.imports.insert(path.to_owned(), targets);
    }

    /// Refresh changed paths at `revision`, with the current worktree
    /// changed lines and, when given, refreshed commits.
    pub fn refresh<'a>(
        &mut self,
        changed: impl Iterator<Item = ChangedFile<'a>>,
        changed_lines: ChangedLines,
        commits: Option<Vec<CommitRecord>>,
        revision: u64,
    ) {
        for (p, content) in changed {
            match content {
                Some((t, l)) => self.index_file(p, t, l),
                None => {
                    self.drop_imports(p);
                    self.importers.remove(p);
                    self.paths.remove(p);
                    self.test_files.remove(p);
                    self.evidence.remove(p);
                }
            }
        }
        self.changed_lines = changed_lines;
        if let Some(c) = commits {
            self.commits = c;
        }
        self.revision = revision;
    }

    /// Ingest verification evidence: (path, check id, status). Replaces the
    /// evidence of the paths mentioned.
    pub fn set_evidence(&mut self, items: Vec<(String, String, String)>) {
        let mut by_path: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        for (p, id, st) in items {
            by_path.entry(p).or_default().push((id, st));
        }
        for (p, v) in by_path {
            self.evidence.insert(p, v);
        }
    }

    fn expand(&self, start: &str, forward: bool, depth: u32, max: usize) -> Vec<(String, u32)> {
        let map = if forward {
            &self.imports
        } else {
            &self.importers
        };
        let mut seen: BTreeMap<String, u32> = BTreeMap::new();
        let mut frontier = vec![start.to_owned()];
        for d in 1..=depth.clamp(1, 3) {
            let mut next = Vec::new();
            for f in &frontier {
                for t in map.get(f).into_iter().flatten() {
                    if t != start && !seen.contains_key(t) {
                        seen.insert(t.clone(), d);
                        next.push(t.clone());
                    }
                }
            }
            frontier = next;
            if frontier.is_empty() {
                break;
            }
        }
        let mut v: Vec<(String, u32)> = seen.into_iter().collect();
        v.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
        v.truncate(max);
        v
    }

    /// Answer a query.
    #[must_use]
    pub fn query(&self, q: &GraphQuery) -> GraphView {
        let max = if q.max == 0 { 50 } else { q.max };
        let depth = q.depth.max(1);
        let all = q.relation == "all" || q.relation.is_empty();
        let want = |r: &str| all || q.relation == r;
        let mut v = GraphView {
            path: q.path.clone(),
            revision: self.revision,
            ..GraphView::default()
        };
        if want("imports") {
            v.imports = self.expand(&q.path, true, depth, max);
        }
        if want("importers") {
            v.importers = self.expand(&q.path, false, depth, max);
        }
        if want("cochange") || want("owners") || want("commits") {
            let mut co: BTreeMap<String, u32> = BTreeMap::new();
            let mut owners: BTreeMap<String, u32> = BTreeMap::new();
            for c in &self.commits {
                if !c.files.iter().any(|f| f == &q.path) {
                    continue;
                }
                if want("commits") && v.commits.len() < max {
                    v.commits.push(c.clone());
                }
                *owners.entry(c.author.clone()).or_default() += 1;
                for f in &c.files {
                    if f != &q.path {
                        *co.entry(f.clone()).or_default() += 1;
                    }
                }
            }
            if want("cochange") {
                let mut co: Vec<(String, u32)> = co.into_iter().collect();
                co.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                co.truncate(max);
                v.cochange = co;
            }
            if want("owners") {
                let mut o: Vec<(String, u32)> = owners.into_iter().collect();
                o.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                v.owners = o;
            }
        }
        if want("changed_lines") {
            v.changed_lines = self.changed_lines.get(&q.path).cloned().unwrap_or_default();
        }
        if want("tests") {
            let importers = self.expand(&q.path, false, 3, usize::MAX);
            v.tests = importers
                .into_iter()
                .map(|(p, _)| p)
                .filter(|p| self.test_files.contains(p))
                .take(max)
                .collect();
            if self.test_files.contains(&q.path) {
                v.tests.insert(0, q.path.clone());
            }
        }
        if want("evidence") {
            v.evidence = self.evidence.get(&q.path).cloned().unwrap_or_default();
            v.evidence.truncate(max);
        }
        v
    }
}

/// Changed line ranges per file from a unified diff (new-side hunks).
#[must_use]
pub fn changed_lines_from_unified(unified: &str) -> ChangedLines {
    let mut out: ChangedLines = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in unified.lines() {
        if let Some(p) = line.strip_prefix("+++ ") {
            let p = p.trim_start_matches("b/");
            current = (p != "/dev/null").then(|| p.to_owned());
        } else if let Some(rest) = line.strip_prefix("@@ ")
            && let Some(cur) = &current
            && let Some(plus) = rest.split_whitespace().nth(1)
            && let Some(plus) = plus.strip_prefix('+')
        {
            let mut it = plus.split(',');
            let start: u32 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let count: u32 = it.next().and_then(|s| s.parse().ok()).unwrap_or(1);
            if count > 0 {
                out.entry(cur.clone())
                    .or_default()
                    .push((start, start + count - 1));
            }
        }
    }
    out
}
