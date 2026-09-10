//! Diff invariants DI-1..DI-9 (docs/64 §4). Evaluated per ChangeTransaction
//! (one path, old and new text) and over a whole unified diff at COMPLETION.
//! Conservative by construction: unclassifiable test-file changes FLAG.

use serde::{Deserialize, Serialize};

/// Violation class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Class {
    /// Reject the transaction.
    Deny,
    /// Block the SelfReview until resolved or justified.
    Flag,
}

/// One violation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Violation {
    /// `DI-1` .. `DI-9`.
    pub id: String,
    /// Class.
    pub class: Class,
    /// Paths.
    pub paths: Vec<String>,
    /// Evidence.
    pub evidence: String,
}

/// What the evaluator knows about the task.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvariantContext {
    /// Current plan write set (`None` = no plan yet).
    pub write_set: Option<Vec<String>>,
    /// Paths the plan explicitly declares (generated files, manifests, tests).
    pub plan_entries: Vec<String>,
    /// Test files / check symbols named by the task's acceptance.
    pub acceptance_named: Vec<String>,
    /// Test symbols failing at baseline.
    pub baseline_failing: Vec<String>,
    /// Protected paths from policy (prefixes/globs, simple prefix match).
    pub protected_paths: Vec<String>,
    /// Formatting churn threshold (lines).
    pub formatting_churn_lines: usize,
    /// Declared candidate revision the change binds to (DI-8).
    pub expected_revision: Option<u64>,
}

/// One changed file for whole-diff evaluation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedFile {
    /// Path.
    pub path: String,
    /// Old text (`None` = created).
    pub old: Option<String>,
    /// New text (`None` = deleted).
    pub new: Option<String>,
}

const GENERATED: &[&str] = &[
    "Cargo.lock",
    "pnpm-lock.yaml",
    "package-lock.json",
    "yarn.lock",
    "poetry.lock",
    "go.sum",
];
const MANIFESTS: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "requirements.txt",
    "go.mod",
];
const TEST_MARKERS: &[&str] = &[
    "#[ignore",
    ".skip(",
    ".only(",
    "it.skip",
    "test.skip",
    "describe.skip",
    "xit(",
    "xtest(",
    "@pytest.mark.skip",
    "@pytest.mark.xfail",
    "pytest.skip(",
    "@retry",
    "retries:",
    "retry(",
    "#[should_panic",
];
const DEBUG_MARKERS: &[&str] = &[
    "println!(\"DEBUG",
    "dbg!(",
    "console.log(",
    "debugger;",
    "print(\"DEBUG",
    "breakpoint()",
    "TODO REMOVE",
];
const SECRET_MARKERS: &[&str] = &[
    "sk-",
    "AKIA",
    "-----BEGIN PRIVATE KEY",
    "ghp_",
    "xoxb-",
    "AIza",
];

fn is_test_path(p: &str) -> bool {
    let lower = p.to_ascii_lowercase();
    lower.contains("/tests/")
        || lower.starts_with("tests/")
        || lower.contains("/test/")
        || lower.starts_with("test/")
        || lower.contains(".test.")
        || lower.contains("_test.")
        || lower.contains("test_")
        || lower.ends_with("_spec.rb")
        || lower.contains(".spec.")
}

fn is_generated(p: &str) -> bool {
    GENERATED.iter().any(|g| p.ends_with(g))
        || p.contains("/gen/")
        || p.contains("/generated/")
        || p.contains("/migrations/")
}

fn is_manifest(p: &str) -> bool {
    MANIFESTS.iter().any(|m| p.ends_with(m))
}

fn in_set(set: &[String], p: &str) -> bool {
    set.iter().any(|s| {
        s == p
            || (s.ends_with("/**") && p.starts_with(&s[..s.len() - 3]))
            || (s.ends_with('*') && p.starts_with(&s[..s.len() - 1]))
    })
}

fn added_lines<'a>(old: Option<&'a str>, new: Option<&'a str>) -> Vec<&'a str> {
    let old_set: std::collections::HashSet<&str> = old.unwrap_or("").lines().collect();
    new.unwrap_or("")
        .lines()
        .filter(|l| !old_set.contains(l))
        .collect()
}

fn removed_lines<'a>(old: Option<&'a str>, new: Option<&'a str>) -> Vec<&'a str> {
    let new_set: std::collections::HashSet<&str> = new.unwrap_or("").lines().collect();
    old.unwrap_or("")
        .lines()
        .filter(|l| !new_set.contains(l))
        .collect()
}

/// Evaluate every invariant over one changed file.
#[must_use]
pub fn evaluate_file(
    ctx: &InvariantContext,
    f: &ChangedFile,
    candidate_revision: Option<u64>,
) -> Vec<Violation> {
    let mut out = Vec::new();
    let p = f.path.as_str();
    let planned = ctx
        .plan_entries
        .iter()
        .any(|e| in_set(std::slice::from_ref(e), p));
    // DI-1 write set
    if let Some(ws) = &ctx.write_set
        && !in_set(ws, p)
        && !planned
    {
        out.push(Violation {
            id: "DI-1".into(),
            class: Class::Deny,
            paths: vec![p.into()],
            evidence: "path outside the plan's write set with no PlanRevised scope delta".into(),
        });
    }
    // DI-2 generated
    if is_generated(p) && !planned {
        out.push(Violation {
            id: "DI-2".into(),
            class: Class::Deny,
            paths: vec![p.into()],
            evidence:
                "generated artifact / lockfile / migration changed by hand without a plan entry"
                    .into(),
        });
    } else if is_generated(p) && planned {
        // docs/28 §3 (PX-016): with a plan entry the change is allowed but
        // flagged — generated files change through their generators.
        out.push(Violation {
            id: "DI-2".into(),
            class: Class::Flag,
            paths: vec![p.into()],
            evidence: "generated artifact / lockfile / migration changed by hand (plan entry present; regenerate through its generator or justify in review)".into(),
        });
    }
    // DI-7 manifests
    if is_manifest(p) && !planned {
        out.push(Violation {
            id: "DI-7".into(),
            class: Class::Deny,
            paths: vec![p.into()],
            evidence: "dependency manifest changed without a plan entry".into(),
        });
    }
    // DI-9 protected
    if ctx
        .protected_paths
        .iter()
        .any(|pp| p.starts_with(pp.as_str()))
    {
        out.push(Violation {
            id: "DI-9".into(),
            class: Class::Deny,
            paths: vec![p.into()],
            evidence: "protected path (CI/policy/deployment) changes only after a typed question"
                .into(),
        });
    }
    // DI-8 revision binding
    if let (Some(exp), Some(cand)) = (ctx.expected_revision, candidate_revision)
        && exp != cand
    {
        out.push(Violation {
            id: "DI-8".into(),
            class: Class::Deny,
            paths: vec![p.into()],
            evidence: format!("change bound to revision {exp}, candidate is {cand}"),
        });
    }
    let added = added_lines(f.old.as_deref(), f.new.as_deref());
    let removed = removed_lines(f.old.as_deref(), f.new.as_deref());
    // DI-3 test integrity
    if is_test_path(p) {
        let protected_symbol = |text: &str| {
            ctx.acceptance_named
                .iter()
                .chain(ctx.baseline_failing.iter())
                .any(|n| text.contains(n.as_str()) || p.contains(n.as_str()))
        };
        let touches_protected = f.new.is_none()
            || protected_symbol(f.old.as_deref().unwrap_or(""))
            || protected_symbol(f.new.as_deref().unwrap_or(""));
        let marker = added
            .iter()
            .find(|l| TEST_MARKERS.iter().any(|m| l.contains(m)));
        let weakened = removed.iter().any(|l| l.contains("assert"))
            && !added.iter().any(|l| l.contains("assert"));
        let expected_edit = removed.iter().any(|l| l.contains("assert"))
            && added.iter().any(|l| l.contains("assert"))
            && touches_protected;
        if f.new.is_none() {
            out.push(Violation {
                id: "DI-3".into(),
                class: if touches_protected {
                    Class::Deny
                } else {
                    Class::Flag
                },
                paths: vec![p.into()],
                evidence: "test file deleted".into(),
            });
        } else if let Some(m) = marker {
            out.push(Violation {
                id: "DI-3".into(),
                class: if touches_protected {
                    Class::Deny
                } else {
                    Class::Flag
                },
                paths: vec![p.into()],
                evidence: format!("skip/only/xfail/retry marker added: {}", m.trim()),
            });
        } else if weakened || expected_edit {
            out.push(Violation {
                id: "DI-3".into(),
                class: if touches_protected {
                    Class::Deny
                } else {
                    Class::Flag
                },
                paths: vec![p.into()],
                evidence: if weakened {
                    "assertion removed".into()
                } else {
                    "assertion / expected value edited in a protected test".into()
                },
            });
        } else if !planned && !added.is_empty() {
            out.push(Violation {
                id: "DI-3".into(),
                class: Class::Flag,
                paths: vec![p.into()],
                evidence: "test-file change without a plan entry declaring it".into(),
            });
        }
    } else {
        // DI-4 debug leftovers (non-test code)
        if let Some(l) = added
            .iter()
            .find(|l| DEBUG_MARKERS.iter().any(|m| l.contains(m)))
        {
            out.push(Violation {
                id: "DI-4".into(),
                class: Class::Flag,
                paths: vec![p.into()],
                evidence: format!("debug leftover: {}", l.trim()),
            });
        }
    }
    // DI-5 secrets
    if let Some(l) = added
        .iter()
        .find(|l| SECRET_MARKERS.iter().any(|m| l.contains(m)) && l.len() > 24)
    {
        out.push(Violation {
            id: "DI-5".into(),
            class: Class::Deny,
            paths: vec![p.into()],
            evidence: format!(
                "credential-like token in diff: {}…",
                &l.trim()[..l.trim().len().min(12)]
            ),
        });
    }
    // DI-6 formatting churn outside the plan
    if ctx.write_set.as_ref().is_some_and(|ws| !in_set(ws, p)) && !planned {
        let ws_only = added
            .iter()
            .filter(|l| {
                l.trim().is_empty()
                    || removed
                        .iter()
                        .any(|r| r.split_whitespace().eq(l.split_whitespace()))
            })
            .count();
        if ws_only > ctx.formatting_churn_lines {
            out.push(Violation {
                id: "DI-6".into(),
                class: Class::Flag,
                paths: vec![p.into()],
                evidence: format!("{ws_only} formatting-only lines outside the plan"),
            });
        }
    }
    out
}

/// Evaluate over a whole diff.
#[must_use]
pub fn evaluate_diff(
    ctx: &InvariantContext,
    files: &[ChangedFile],
    candidate_revision: Option<u64>,
) -> Vec<Violation> {
    files
        .iter()
        .flat_map(|f| evaluate_file(ctx, f, candidate_revision))
        .collect()
}

/// Whether any violation denies.
#[must_use]
pub fn denies(v: &[Violation]) -> bool {
    v.iter().any(|x| x.class == Class::Deny)
}
