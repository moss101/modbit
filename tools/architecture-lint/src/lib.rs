//! Architecture dependency lint for the Modbit workspace.
//!
//! Reads `cargo metadata`, builds the dependency graph between packages and
//! evaluates the rules in `rules.toml` (docs/12, docs/81, docs/35). Forbidden
//! edges are reported with the concrete dependency path that realises them.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use cargo_metadata::{DependencyKind, MetadataCommand};
use globset::{Glob, GlobMatcher};
use serde::Deserialize;

/// A rule forbidding any (transitive) dependency from `from` to `to`.
#[derive(Debug, Clone, Deserialize)]
pub struct ForbidRule {
    /// Glob over workspace member package names.
    pub from: String,
    /// Glob over package names (workspace or external).
    pub to: String,
    /// Authority reference explaining the rule.
    pub reason: String,
}

/// A rule confining direct use of an external package to listed members.
#[derive(Debug, Clone, Deserialize)]
pub struct ConfineRule {
    /// Glob over package names.
    pub package: String,
    /// Workspace member package names allowed to depend on it directly.
    pub allowed: Vec<String>,
    /// Authority reference explaining the rule.
    pub reason: String,
}

/// The rule file.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Rules {
    /// Forbidden edges.
    #[serde(default)]
    pub forbid: Vec<ForbidRule>,
    /// Confined external packages.
    #[serde(default)]
    pub confine: Vec<ConfineRule>,
}

impl Rules {
    /// Parse a TOML rule file.
    pub fn parse(text: &str) -> Result<Self> {
        let rules: Rules = toml::from_str(text).context("parsing rules TOML")?;
        for r in &rules.forbid {
            Glob::new(&r.from).with_context(|| format!("invalid glob `{}`", r.from))?;
            Glob::new(&r.to).with_context(|| format!("invalid glob `{}`", r.to))?;
        }
        for r in &rules.confine {
            Glob::new(&r.package).with_context(|| format!("invalid glob `{}`", r.package))?;
        }
        Ok(rules)
    }

    /// Load and parse a rule file from disk.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading rules file {}", path.display()))?;
        Self::parse(&text)
    }
}

/// Package dependency graph keyed by package name.
#[derive(Debug, Clone, Default)]
pub struct Graph {
    /// Names of workspace members.
    pub members: BTreeSet<String>,
    /// Direct dependency edges (normal and build kinds unless dev included).
    pub edges: BTreeMap<String, BTreeSet<String>>,
}

impl Graph {
    /// Build a graph from an explicit edge list (used by tests and tooling).
    pub fn from_edges<'a>(
        members: impl IntoIterator<Item = &'a str>,
        edges: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Self {
        let mut g = Graph {
            members: members.into_iter().map(str::to_owned).collect(),
            edges: BTreeMap::new(),
        };
        for (a, b) in edges {
            g.edges
                .entry(a.to_owned())
                .or_default()
                .insert(b.to_owned());
        }
        g
    }

    /// Build the graph from `cargo metadata` of the workspace at `manifest_path`.
    pub fn from_cargo_metadata(manifest_path: &Path, include_dev: bool) -> Result<Self> {
        let metadata = MetadataCommand::new()
            .manifest_path(manifest_path)
            .exec()
            .with_context(|| format!("running cargo metadata for {}", manifest_path.display()))?;
        let names: HashMap<_, _> = metadata
            .packages
            .iter()
            .map(|p| (p.id.clone(), p.name.to_string()))
            .collect();
        let members: BTreeSet<String> = metadata
            .workspace_members
            .iter()
            .filter_map(|id| names.get(id).cloned())
            .collect();
        let resolve = metadata
            .resolve
            .ok_or_else(|| anyhow!("cargo metadata returned no resolve graph"))?;
        let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for node in &resolve.nodes {
            let Some(from) = names.get(&node.id) else {
                continue;
            };
            for dep in &node.deps {
                let keep = dep.dep_kinds.iter().any(|k| match k.kind {
                    DependencyKind::Normal | DependencyKind::Build => true,
                    DependencyKind::Development => include_dev,
                    _ => false,
                });
                if !keep {
                    continue;
                }
                if let Some(to) = names.get(&dep.pkg) {
                    edges.entry(from.clone()).or_default().insert(to.clone());
                }
            }
        }
        Ok(Graph { members, edges })
    }

    /// Shortest dependency path from `from` to `to` (inclusive), if reachable.
    pub fn path(&self, from: &str, to: &str) -> Option<Vec<String>> {
        let mut prev: HashMap<&str, &str> = HashMap::new();
        let mut queue = VecDeque::from([from]);
        let mut seen: BTreeSet<&str> = BTreeSet::from([from]);
        while let Some(cur) = queue.pop_front() {
            if cur == to && cur != from {
                let mut out = vec![cur.to_owned()];
                let mut c = cur;
                while let Some(&p) = prev.get(c) {
                    out.push(p.to_owned());
                    c = p;
                }
                out.reverse();
                return Some(out);
            }
            if let Some(next) = self.edges.get(cur) {
                for n in next {
                    if seen.insert(n.as_str()) {
                        prev.insert(n.as_str(), cur);
                        queue.push_back(n.as_str());
                    }
                }
            }
        }
        None
    }

    /// Every package name present in the graph.
    fn all_packages(&self) -> BTreeSet<&str> {
        let mut s: BTreeSet<&str> = self.members.iter().map(String::as_str).collect();
        for (a, bs) in &self.edges {
            s.insert(a);
            s.extend(bs.iter().map(String::as_str));
        }
        s
    }
}

/// One rule violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// Workspace member the violation originates from.
    pub from: String,
    /// Package that must not be reached.
    pub to: String,
    /// Dependency path realising the edge, `from` first.
    pub path: Vec<String>,
    /// Authority reference of the violated rule.
    pub reason: String,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "FORBIDDEN {} -> {} via {} ({})",
            self.from,
            self.to,
            self.path.join(" -> "),
            self.reason
        )
    }
}

fn matcher(glob: &str) -> GlobMatcher {
    // Globs were validated when the rules were parsed.
    Glob::new(glob).expect("validated glob").compile_matcher()
}

/// Evaluate `rules` against `graph`, returning every violation found.
pub fn check(graph: &Graph, rules: &Rules) -> Vec<Violation> {
    let mut out = Vec::new();
    let packages = graph.all_packages();
    for rule in &rules.forbid {
        let from_m = matcher(&rule.from);
        let to_m = matcher(&rule.to);
        for from in graph.members.iter().filter(|m| from_m.is_match(m)) {
            for to in packages
                .iter()
                .filter(|p| **p != from.as_str() && to_m.is_match(p))
            {
                if let Some(path) = graph.path(from, to) {
                    out.push(Violation {
                        from: from.clone(),
                        to: (*to).to_owned(),
                        path,
                        reason: rule.reason.clone(),
                    });
                }
            }
        }
    }
    for rule in &rules.confine {
        let pkg_m = matcher(&rule.package);
        for from in &graph.members {
            if rule.allowed.iter().any(|a| a == from) {
                continue;
            }
            if let Some(direct) = graph.edges.get(from) {
                for to in direct.iter().filter(|d| pkg_m.is_match(d)) {
                    out.push(Violation {
                        from: from.clone(),
                        to: to.clone(),
                        path: vec![from.clone(), to.clone()],
                        reason: rule.reason.clone(),
                    });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Rules {
        Rules::parse(
            r#"
            [[forbid]]
            from = "a"
            to = "c"
            reason = "a must not reach c"
            [[forbid]]
            from = "d"
            to = "modbit-*"
            reason = "d is a leaf"
            [[confine]]
            package = "vendor*"
            allowed = ["adapter"]
            reason = "vendor confined to adapter"
            "#,
        )
        .unwrap()
    }

    #[test]
    fn transitive_edge_is_reported_with_path() {
        let g = Graph::from_edges(["a", "b", "c"], [("a", "b"), ("b", "c")]);
        let v = check(&g, &rules());
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].path, vec!["a", "b", "c"]);
        assert_eq!(v[0].reason, "a must not reach c");
    }

    #[test]
    fn allowed_direction_passes() {
        let g = Graph::from_edges(["a", "b", "c"], [("c", "b"), ("b", "a")]);
        assert!(check(&g, &rules()).is_empty());
    }

    #[test]
    fn glob_targets_and_self_exclusion() {
        let g = Graph::from_edges(["d", "modbit-x"], [("d", "modbit-x")]);
        let v = check(&g, &rules());
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].to, "modbit-x");
        let g = Graph::from_edges(["modbit-d"], []);
        assert!(check(&g, &rules()).is_empty());
    }

    #[test]
    fn confine_rejects_direct_use_outside_allowed_members() {
        let g = Graph::from_edges(
            ["adapter", "other"],
            [("adapter", "vendor-sdk"), ("other", "vendor-sdk")],
        );
        let v = check(&g, &rules());
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].from, "other");
        assert_eq!(v[0].to, "vendor-sdk");
    }

    #[test]
    fn invalid_glob_is_rejected_at_parse_time() {
        let err = Rules::parse("[[forbid]]\nfrom = \"a[\"\nto = \"b\"\nreason = \"x\"\n");
        assert!(err.is_err());
    }
}
