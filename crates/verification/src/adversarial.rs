//! Generated adversarial checks (PX-135, REQ-EPR-017 evidence): additional
//! evidence the Verification Engine derives from a candidate and the tree it
//! lands in, aimed at the false-accept classes the first calibration
//! observed (docs/61 "Gate calibration", audit BLD-28):
//!
//! - **test weakening** (`adv:test-weakening`): a test removed, an assertion
//!   removed or edited, a skip marker added, in a test the task did not name
//!   (DI-3 flags those and a flag is advisory; this is evidence);
//! - **vacuous pass** (`adv:vacuous-pass`): a test that cannot fail because
//!   it asserts nothing, asserts a tautology, or returns before it asserts;
//! - **overfit to the visible test** (`adv:overfit-literal`): source that
//!   special-cases a literal which only the visible tests contain;
//! - **off-by-one** (`adv:boundary-pinned`): a changed comparison against a
//!   number is mutated (operator weakened or strengthened, the number moved
//!   by one) and the repository's own check commands are re-run; a mutant
//!   that survives means no test pins that boundary.
//!
//! The checks are derived from the candidate's files, the tests already in
//! the tree and the repository's own commands. They never read an oracle,
//! a label or a hidden test: they have no such input. Each becomes a
//! [`CheckEvidence`] of kind `adversarial`, which the unchanged Acceptance
//! Gate weighs like any other check (a `FAIL` rejects). No gate rule and no
//! threshold changes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::engine::CommandRunner;
use crate::gate::CheckEvidence;
use crate::invariants::{ChangedFile, TEST_MARKERS, is_test_path};

/// The `CheckEvidence::kind` of every generated check.
pub const CHECK_KIND: &str = "adversarial";

/// Generator version, in every report that carries generated evidence.
pub const GENERATOR_VERSION: &str = "adversarial-checks/1";

/// Most mutants one candidate is asked to survive.
pub const MAX_MUTANTS: usize = 12;

/// The failure class a generated check targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    /// A test removed, an assertion removed or edited.
    TestWeakening,
    /// A test that cannot fail.
    VacuousPass,
    /// A literal only the visible tests contain, special-cased in source.
    OverfitLiteral,
    /// A changed boundary no test pins.
    BoundaryNotPinned,
}

impl Class {
    /// Every class.
    pub const ALL: [Class; 4] = [
        Class::TestWeakening,
        Class::VacuousPass,
        Class::OverfitLiteral,
        Class::BoundaryNotPinned,
    ];

    /// The check id the evidence carries.
    #[must_use]
    pub const fn check_id(self) -> &'static str {
        match self {
            Self::TestWeakening => "adv:test-weakening",
            Self::VacuousPass => "adv:vacuous-pass",
            Self::OverfitLiteral => "adv:overfit-literal",
            Self::BoundaryNotPinned => "adv:boundary-pinned",
        }
    }
}

/// A generated check's outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Status {
    /// Nothing adversarial found.
    Pass,
    /// A finding: the candidate shows the failure class.
    Fail,
    /// The check could not be made meaningful (the candidate's own suite is
    /// not green, so a mutant could not be told from the existing failure).
    Skip,
    /// The check could not run (the copy, a write or a restore failed): the
    /// gate takes this as an outcome it could not establish, never a pass.
    Unknown,
}

/// One derived check and what it found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedCheck {
    /// Class.
    pub class: Class,
    /// Outcome.
    pub status: Status,
    /// Findings, in file order; one line each, never a hidden-test detail.
    pub findings: Vec<String>,
}

impl GeneratedCheck {
    /// The gate-facing evidence.
    #[must_use]
    pub fn to_evidence(&self) -> CheckEvidence {
        CheckEvidence {
            check_id: self.class.check_id().to_owned(),
            kind: CHECK_KIND.to_owned(),
            status: match self.status {
                Status::Pass => "PASS",
                Status::Fail => "FAIL",
                Status::Skip => "SKIP",
                Status::Unknown => "UNKNOWN",
            }
            .to_owned(),
        }
    }

    /// sha256 of the findings: what a retained report cites.
    #[must_use]
    pub fn digest(&self) -> String {
        let bytes = serde_json::to_vec(self).unwrap_or_default();
        hex::encode(sha2::Sha256::digest(bytes))
    }

    fn from_findings(class: Class, findings: Vec<String>) -> Self {
        Self {
            class,
            status: if findings.is_empty() {
                Status::Pass
            } else {
                Status::Fail
            },
            findings,
        }
    }
}

// ---- Languages and test functions ----------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    Python,
    Script,
}

fn lang_of(path: &str) -> Option<Lang> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    match ext.as_str() {
        "rs" => Some(Lang::Rust),
        "py" => Some(Lang::Python),
        "js" | "mjs" | "cjs" | "jsx" | "ts" | "mts" | "cts" | "tsx" => Some(Lang::Script),
        _ => None,
    }
}

fn is_comment(lang: Lang, trimmed: &str) -> bool {
    match lang {
        Lang::Python => trimmed.starts_with('#'),
        Lang::Rust | Lang::Script => {
            trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*')
        }
    }
}

/// The line without its string contents, so braces and keywords inside
/// strings do not count.
fn strip_strings(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in line.chars() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                    out.push(c);
                }
            }
            None => {
                if c == '"' || (c == '`') {
                    quote = Some(c);
                }
                out.push(c);
            }
        }
    }
    out
}

fn collapse(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Clone, Debug)]
struct TestFn {
    name: String,
    /// `(depth, trimmed line)` of the body; depth 0 is directly in the body.
    body: Vec<(usize, String)>,
}

impl TestFn {
    fn assertions(&self, lang: Lang) -> Vec<(usize, String)> {
        self.body
            .iter()
            .enumerate()
            .filter(|(_, (_, l))| is_assertion(lang, l))
            .map(|(i, (_, l))| (i, collapse(l)))
            .collect()
    }

    /// Index of the first unconditional return at the top of the body.
    fn early_return(&self) -> Option<usize> {
        self.body.iter().position(|(d, l)| {
            *d == 0
                && (l == "return"
                    || l == "return;"
                    || l.starts_with("return ")
                    || l.starts_with("return;"))
        })
    }

    fn text(&self) -> String {
        self.body
            .iter()
            .map(|(d, l)| format!("{d}:{}", collapse(l)))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn is_assertion(lang: Lang, trimmed: &str) -> bool {
    if is_comment(lang, trimmed) {
        return false;
    }
    match lang {
        Lang::Rust => [
            "assert!(",
            "assert_eq!(",
            "assert_ne!(",
            "assert_matches!(",
            "debug_assert",
        ]
        .iter()
        .any(|m| trimmed.contains(m)),
        Lang::Python => {
            trimmed.starts_with("assert ")
                || trimmed.starts_with("assert(")
                || trimmed.contains("self.assert")
                || trimmed.contains("pytest.raises(")
        }
        Lang::Script => {
            trimmed.contains("assert.")
                || trimmed.contains("assert(")
                || trimmed.contains("expect(")
                || trimmed.contains("t.assert")
        }
    }
}

/// Split `a, b, c` at top-level commas.
fn top_level_args(inner: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in inner.chars() {
        if let Some(q) = quote {
            cur.push(c);
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => {
                quote = Some(c);
                cur.push(c);
            }
            '(' | '[' | '{' => {
                depth += 1;
                cur.push(c);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => {
                args.push(cur.trim().to_owned());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        args.push(cur.trim().to_owned());
    }
    args
}

fn call_args(line: &str, prefix: &str) -> Option<Vec<String>> {
    let start = line.find(prefix)? + prefix.len();
    let rest = &line[start..];
    let end = rest.rfind(')')?;
    Some(top_level_args(&rest[..end]))
}

fn is_truthy_literal(s: &str) -> bool {
    matches!(s.trim(), "true" | "True" | "1" | "!false" | "not False")
}

/// An assertion that cannot fail.
fn is_tautology(lang: Lang, line: &str) -> bool {
    let l = collapse(line);
    let one = |prefixes: &[&str]| {
        prefixes.iter().any(|p| {
            call_args(&l, p)
                .and_then(|a| a.first().cloned())
                .is_some_and(|a| is_truthy_literal(&a))
        })
    };
    let two = |prefixes: &[&str]| {
        prefixes.iter().any(|p| {
            call_args(&l, p).is_some_and(|a| a.len() >= 2 && !a[0].is_empty() && a[0] == a[1])
        })
    };
    match lang {
        Lang::Rust => one(&["assert!("]) || two(&["assert_eq!("]),
        Lang::Python => {
            if let Some(rest) = l.strip_prefix("assert ") {
                let cond = rest.split(',').next().unwrap_or("").trim();
                if is_truthy_literal(cond) {
                    return true;
                }
                if let Some((a, b)) = cond.split_once(" == ") {
                    return a.trim() == b.trim();
                }
                return false;
            }
            one(&["assert("]) || two(&["assertEqual("])
        }
        Lang::Script => {
            one(&["assert.ok(", "assert("])
                || two(&[
                    "assert.equal(",
                    "assert.strictEqual(",
                    "assert.deepEqual(",
                    "assert.deepStrictEqual(",
                ])
                || ["toBe(", "toEqual(", "toStrictEqual("].iter().any(|m| {
                    l.find("expect(").is_some_and(|i| {
                        let subject = call_args(&l[i..], "expect(");
                        let ret = l.find(m).and_then(|_| call_args(&l, m));
                        match (subject, ret) {
                            (Some(s), Some(r)) => s.len() == 1 && r.len() == 1 && s[0] == r[0],
                            _ => false,
                        }
                    })
                })
        }
    }
}

fn brace_delta(line: &str) -> (usize, usize) {
    let s = strip_strings(line);
    (s.matches('{').count(), s.matches('}').count())
}

/// The quoted name inside the first call parentheses: `test("name", ...)`.
fn first_quoted(line: &str) -> Option<String> {
    let q = line.find(['"', '\'', '`'])?;
    let quote = line[q..].chars().next()?;
    let rest = &line[q + quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_owned())
}

fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ' || *c == '\t').count()
}

fn test_fns(lang: Lang, text: &str) -> Vec<TestFn> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    match lang {
        Lang::Rust => {
            let mut i = 0;
            while i < lines.len() {
                let t = lines[i].trim();
                if t.starts_with("#[test") || t.starts_with("#[tokio::test") {
                    let mut j = i + 1;
                    while j < lines.len() && !lines[j].contains("fn ") {
                        j += 1;
                    }
                    if j >= lines.len() {
                        break;
                    }
                    let name = lines[j]
                        .split("fn ")
                        .nth(1)
                        .and_then(|s| s.split('(').next())
                        .unwrap_or("")
                        .trim()
                        .to_owned();
                    out.push(brace_body(&lines, j, name));
                    i = j + 1;
                    continue;
                }
                i += 1;
            }
        }
        Lang::Script => {
            for (i, l) in lines.iter().enumerate() {
                let t = l.trim();
                let is_decl = ["test(", "it(", "test.only(", "it.only("]
                    .iter()
                    .any(|p| t.starts_with(p));
                if is_decl && let Some(name) = first_quoted(t) {
                    out.push(brace_body(&lines, i, name));
                }
            }
        }
        Lang::Python => {
            for (i, l) in lines.iter().enumerate() {
                let t = l.trim();
                let decl = t
                    .strip_prefix("async def ")
                    .or_else(|| t.strip_prefix("def "));
                let Some(decl) = decl else { continue };
                if !decl.starts_with("test") {
                    continue;
                }
                let name = decl.split('(').next().unwrap_or("").trim().to_owned();
                let base = indent_of(l);
                let mut body = Vec::new();
                let mut body_indent = None;
                for b in &lines[i + 1..] {
                    if b.trim().is_empty() {
                        continue;
                    }
                    let ind = indent_of(b);
                    if ind <= base {
                        break;
                    }
                    let bi = *body_indent.get_or_insert(ind);
                    body.push((usize::from(ind > bi), b.trim().to_owned()));
                }
                out.push(TestFn { name, body });
            }
        }
    }
    out
}

/// Body of the function opening on line `start`: braces matched from there.
fn brace_body(lines: &[&str], start: usize, name: String) -> TestFn {
    let mut depth = 0usize;
    let mut opened = false;
    let mut body = Vec::new();
    for (k, l) in lines[start..].iter().enumerate() {
        let (open, close) = brace_delta(l);
        let before = depth;
        depth += open;
        if open > 0 {
            opened = true;
        }
        if k > 0 || opened {
            let t = l.trim();
            if k > 0 && !t.is_empty() {
                // depth 1 is directly inside the function's braces.
                body.push((before.saturating_sub(1), t.to_owned()));
            }
        }
        depth = depth.saturating_sub(close);
        if opened && depth == 0 {
            // The closing line is not body.
            body.pop();
            break;
        }
    }
    TestFn { name, body }
}

fn fns_by_name(lang: Lang, text: Option<&str>) -> BTreeMap<String, TestFn> {
    text.map(|t| test_fns(lang, t))
        .unwrap_or_default()
        .into_iter()
        .map(|f| (f.name.clone(), f))
        .collect()
}

// ---- Static checks ----------------------------------------------------------

/// A removed or weakened test.
fn weakening(files: &[ChangedFile], declared: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for f in files {
        if !is_test_path(&f.path) {
            continue;
        }
        let Some(lang) = lang_of(&f.path) else {
            continue;
        };
        let old = fns_by_name(lang, f.old.as_deref());
        let new = fns_by_name(lang, f.new.as_deref());
        for (name, o) in &old {
            if declared
                .iter()
                .any(|d| name.contains(d.as_str()) || d.contains(name.as_str()))
            {
                continue;
            }
            let Some(n) = new.get(name) else {
                out.push(format!(
                    "{}: test `{name}` was removed{}",
                    f.path,
                    if f.new.is_none() {
                        " with its file"
                    } else {
                        ""
                    }
                ));
                continue;
            };
            let old_a: Vec<String> = o.assertions(lang).into_iter().map(|(_, l)| l).collect();
            let new_a: BTreeSet<String> = n.assertions(lang).into_iter().map(|(_, l)| l).collect();
            if n.assertions(lang).len() < old_a.len() {
                out.push(format!(
                    "{}: test `{name}` has fewer assertions ({} -> {})",
                    f.path,
                    old_a.len(),
                    n.assertions(lang).len()
                ));
            } else if let Some(gone) = old_a.iter().find(|a| !new_a.contains(*a)) {
                out.push(format!(
                    "{}: test `{name}` no longer asserts `{gone}`",
                    f.path
                ));
            }
        }
        // A skip marker added anywhere in a test file.
        let old_lines: BTreeSet<&str> = f.old.as_deref().unwrap_or("").lines().collect();
        for l in f.new.as_deref().unwrap_or("").lines() {
            if !old_lines.contains(l) && TEST_MARKERS.iter().any(|m| l.contains(m)) {
                out.push(format!(
                    "{}: skip/retry marker added: `{}`",
                    f.path,
                    l.trim()
                ));
            }
        }
    }
    out
}

/// A new or edited test that cannot fail.
fn vacuous(files: &[ChangedFile]) -> Vec<String> {
    let mut out = Vec::new();
    for f in files {
        if !is_test_path(&f.path) {
            continue;
        }
        let (Some(lang), Some(new_text)) = (lang_of(&f.path), f.new.as_deref()) else {
            continue;
        };
        let old = fns_by_name(lang, f.old.as_deref());
        for t in test_fns(lang, new_text) {
            if old.get(&t.name).is_some_and(|o| o.text() == t.text()) {
                continue;
            }
            let asserts = t.assertions(lang);
            let cut = t.early_return().unwrap_or(usize::MAX);
            let reachable: Vec<&(usize, String)> =
                asserts.iter().filter(|(i, _)| *i < cut).collect();
            let live: Vec<&&(usize, String)> = reachable
                .iter()
                .filter(|(i, _)| !is_tautology(lang, &t.body[*i].1))
                .collect();
            let why = if asserts.is_empty() {
                Some("asserts nothing")
            } else if reachable.len() < asserts.len() && live.is_empty() {
                Some("returns before it asserts")
            } else if live.is_empty() {
                Some("asserts only tautologies")
            } else if reachable.len() < asserts.len() {
                Some("has assertions after an unconditional return")
            } else {
                None
            };
            if let Some(why) = why {
                out.push(format!("{}: test `{}` {why}", f.path, t.name));
            }
        }
    }
    out
}

/// Quoted strings and integers a text contains.
fn literals(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' || c == '\'' {
            let mut j = i + 1;
            let mut s = String::new();
            while j < chars.len() && chars[j] != c && chars[j] != '\n' {
                s.push(chars[j]);
                j += 1;
            }
            if j < chars.len()
                && chars[j] == c
                && s.chars().count() >= 2
                && s.chars().any(char::is_alphanumeric)
                && !s.contains("::")
            {
                out.insert(s);
            }
            i = j + 1;
            continue;
        }
        if c.is_ascii_digit() {
            let prev_ok = i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
            let mut j = i;
            let mut s = String::new();
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '_') {
                if chars[j] != '_' {
                    s.push(chars[j]);
                }
                j += 1;
            }
            let next_ok =
                j >= chars.len() || !chars[j].is_alphabetic() || is_int_suffix(&chars[j..]);
            if prev_ok && next_ok && s.len() >= 2 {
                out.insert(s);
            }
            i = j;
            continue;
        }
        i += 1;
    }
    out
}

fn is_int_suffix(rest: &[char]) -> bool {
    let word: String = rest
        .iter()
        .take_while(|c| c.is_alphanumeric())
        .collect::<String>();
    matches!(
        word.as_str(),
        "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize" | "n"
    )
}

const MEMBERSHIP_CONTEXT: &[&str] = &[" in ", ".includes(", ".has(", ".contains(", "matches!("];

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether `lit` stands on `line` as the operand of an equality test, a match
/// arm, a switch case, a dictionary key or a collection membership test.
/// Ordinary comparisons (`x <= 500`), ternaries and call arguments are not.
fn special_cases(line: &str, lit: &str) -> bool {
    if MEMBERSHIP_CONTEXT.iter().any(|k| line.contains(k)) && line.contains(lit) {
        return true;
    }
    let mut from = 0;
    while let Some(rel) = line[from..].find(lit) {
        let at = from + rel;
        from = at + lit.len().max(1);
        let mut before = &line[..at];
        let mut after = &line[at + lit.len()..];
        // The literal's quotes belong to it.
        for q in ['"', '\''] {
            if before.ends_with(q) && after.starts_with(q) {
                before = &before[..before.len() - 1];
                after = &after[1..];
                break;
            }
        }
        if before.chars().next_back().is_some_and(is_word_char)
            || after.chars().next().is_some_and(is_word_char)
        {
            continue;
        }
        let b = before.trim_end();
        let a = after.trim_start();
        let compared = b.ends_with(['<', '>', '!']) || b.ends_with("<=") || b.ends_with(">=");
        let equal_before = b.ends_with("==") || b.ends_with("===") || b.ends_with("case");
        let equal_after = a.starts_with("==") || a.starts_with("!=");
        let arm = !compared && (a.starts_with("=>") || a.starts_with('|'));
        let key =
            a.starts_with(':') && !line.contains('?') && (b.ends_with('{') || b.ends_with(','));
        if equal_before || equal_after || arm || key {
            return true;
        }
    }
    false
}

fn is_import(lang: Lang, t: &str) -> bool {
    match lang {
        Lang::Rust => t.starts_with("use ") || t.starts_with("mod ") || t.starts_with("extern "),
        Lang::Python => t.starts_with("import ") || t.starts_with("from "),
        Lang::Script => {
            t.starts_with("import ")
                || t.starts_with("export ")
                || t.contains("require(")
                || t.contains(" from \"")
                || t.contains(" from '")
        }
    }
}

/// Every test file under `tree`, as `(path, text)`.
fn tree_tests(tree: &Path) -> Vec<(String, String)> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, String)>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if p.is_dir() {
                if !matches!(name, "target" | "node_modules" | ".git" | "__pycache__") {
                    walk(&p, root, out);
                }
            } else if let Ok(rel) = p.strip_prefix(root) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                if lang_of(&rel).is_some()
                    && is_test_path(&rel)
                    && let Ok(t) = std::fs::read_to_string(&p)
                {
                    out.push((rel, t));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(tree, tree, &mut out);
    out
}

/// Source that special-cases a literal only the tests contain.
fn overfit(tree: &Path, files: &[ChangedFile]) -> Vec<String> {
    let tests = tree_tests(tree);
    let mut visible: BTreeSet<String> = BTreeSet::new();
    for (_, t) in &tests {
        visible.extend(literals(t));
    }
    let mut out = Vec::new();
    for f in files {
        if is_test_path(&f.path) {
            continue;
        }
        let (Some(lang), Some(new_text)) = (lang_of(&f.path), f.new.as_deref()) else {
            continue;
        };
        let old_text = f.old.as_deref().unwrap_or("");
        let old_lines: BTreeSet<&str> = old_text.lines().collect();
        for (n, line) in new_text.lines().enumerate() {
            let t = line.trim();
            if old_lines.contains(line) || t.is_empty() || is_comment(lang, t) || is_import(lang, t)
            {
                continue;
            }
            let plain = t.replace('_', "");
            for lit in literals(line) {
                let numeric = lit.chars().all(|c| c.is_ascii_digit());
                let context = if numeric { plain.as_str() } else { t };
                if visible.contains(&lit) && special_cases(context, &lit) {
                    out.push(format!(
                        "{}:{}: special-cases `{lit}`, a literal of the visible tests",
                        f.path,
                        n + 1
                    ));
                }
            }
        }
    }
    out
}

/// The static checks, always the three of them in this order.
#[must_use]
pub fn static_checks(
    tree: &Path,
    files: &[ChangedFile],
    declared_changes: &[String],
) -> Vec<GeneratedCheck> {
    vec![
        GeneratedCheck::from_findings(Class::TestWeakening, weakening(files, declared_changes)),
        GeneratedCheck::from_findings(Class::VacuousPass, vacuous(files)),
        GeneratedCheck::from_findings(Class::OverfitLiteral, overfit(tree, files)),
    ]
}

// ---- Boundary mutation -----------------------------------------------------------

/// One generated mutant of a candidate line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mutant {
    /// Path.
    pub path: String,
    /// 1-based line.
    pub line: usize,
    /// The mutated line (full).
    pub mutated: String,
    /// What changed.
    pub description: String,
}

/// For each byte of `line`, whether it lies inside a string or character
/// literal (an unterminated quote masks the rest of the line: conservative).
fn string_mask(line: &str) -> Vec<bool> {
    let mut mask = vec![false; line.len()];
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match quote {
            Some(q) => {
                for m in &mut mask[i..i + c.len_utf8()] {
                    *m = true;
                }
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
            }
            None if c == '"' || c == '\'' || c == '`' => {
                quote = Some(c);
                for m in &mut mask[i..i + c.len_utf8()] {
                    *m = true;
                }
            }
            None => {}
        }
    }
    mask
}

const COMPARATORS: [(&str, &str); 4] = [
    (" >= ", " > "),
    (" <= ", " < "),
    (" > ", " >= "),
    (" < ", " <= "),
];

fn int_token(s: &str) -> Option<(String, String)> {
    let tok: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '_')
        .collect();
    if tok.is_empty() || !tok.chars().next()?.is_ascii_digit() {
        return None;
    }
    let digits: String = tok.chars().filter(|c| *c != '_').collect();
    let after: String = s[tok.len()..]
        .chars()
        .take_while(|c| c.is_alphanumeric())
        .collect();
    let suffix = if after.is_empty() || is_int_suffix(&after.chars().collect::<Vec<_>>()) {
        after
    } else {
        return None;
    };
    Some((digits, suffix))
}

fn int_token_before(s: &str) -> Option<(usize, String, String)> {
    let trimmed = s.trim_end();
    let start = trimmed
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
        .map_or(0, |i| i + 1);
    let word = &trimmed[start..];
    let (digits, suffix) = int_token(word)?;
    if word.len() != digits.len() + suffix.len() && !word.contains('_') {
        return None;
    }
    Some((start, digits, suffix))
}

/// The mutants of a candidate's changed comparisons, in file and line order.
#[must_use]
pub fn boundary_mutants(files: &[ChangedFile], cap: usize) -> Vec<Mutant> {
    let mut out = Vec::new();
    for f in files {
        if is_test_path(&f.path) {
            continue;
        }
        let (Some(lang), Some(new_text)) = (lang_of(&f.path), f.new.as_deref()) else {
            continue;
        };
        let old_lines: BTreeSet<&str> = f.old.as_deref().unwrap_or("").lines().collect();
        for (n, line) in new_text.lines().enumerate() {
            let t = line.trim();
            if old_lines.contains(line) || t.is_empty() || is_comment(lang, t) {
                continue;
            }
            let in_string = string_mask(line);
            for (cmp, weaker) in COMPARATORS {
                let mut from = 0;
                while let Some(rel) = line[from..].find(cmp) {
                    let at = from + rel;
                    from = at + cmp.len();
                    // A comparator inside a string literal is text.
                    if in_string.get(at).copied().unwrap_or(false) {
                        continue;
                    }
                    let after = &line[at + cmp.len()..];
                    let before = &line[..at];
                    let number_after = int_token(after).map(|(d, s)| (at + cmp.len(), d, s));
                    let number_before = int_token_before(before);
                    if number_after.is_none() && number_before.is_none() {
                        continue;
                    }
                    let mk = |mutated: String, description: String| Mutant {
                        path: f.path.clone(),
                        line: n + 1,
                        mutated,
                        description,
                    };
                    out.push(mk(
                        format!("{}{}{}", before, weaker, after),
                        format!("`{}` -> `{}`", cmp.trim(), weaker.trim()),
                    ));
                    for delta in [1i64, -1] {
                        let (pos, digits, suffix, len) = if let Some((p, d, s)) = &number_after {
                            (*p, d.clone(), s.clone(), after_len(after))
                        } else if let Some((p, d, s)) = &number_before {
                            (*p, d.clone(), s.clone(), before[*p..].trim_end().len())
                        } else {
                            continue;
                        };
                        let Ok(value) = digits.parse::<i64>() else {
                            continue;
                        };
                        let moved = value + delta;
                        if moved < 0 {
                            continue;
                        }
                        let mut m = String::new();
                        m.push_str(&line[..pos]);
                        m.push_str(&format!("{moved}{suffix}"));
                        m.push_str(&line[pos + len..]);
                        out.push(mk(
                            m,
                            format!("boundary {digits} -> {moved} next to `{}`", cmp.trim()),
                        ));
                    }
                    if out.len() >= cap {
                        out.truncate(cap);
                        return out;
                    }
                }
            }
        }
    }
    out.truncate(cap);
    out
}

fn after_len(after: &str) -> usize {
    after
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .map(char::len_utf8)
        .sum()
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let name = e.file_name();
        if matches!(
            name.to_str(),
            Some("target" | "node_modules" | ".git" | "__pycache__" | ".pytest_cache")
        ) {
            continue;
        }
        let p = e.path();
        if p.is_dir() {
            copy_tree(&p, &dst.join(&name))?;
        } else {
            std::fs::copy(&p, dst.join(&name))?;
        }
    }
    Ok(())
}

fn remove_pycache(dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().and_then(|n| n.to_str()) == Some("__pycache__") {
                let _ = std::fs::remove_dir_all(&p);
            } else {
                remove_pycache(&p);
            }
        }
    }
}

/// Write `text` to `path` with a modification time later than anything built
/// before it, so a build cache keyed on mtime cannot keep the old bytes.
fn write_fresh(path: &Path, text: &str, tick: u64) -> std::io::Result<()> {
    std::fs::write(path, text)?;
    let file = std::fs::OpenOptions::new().write(true).open(path)?;
    file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(tick))
}

/// Options of the boundary check.
#[derive(Clone, Debug)]
pub struct MutationOptions {
    /// Most mutants.
    pub cap: usize,
    /// Per command timeout.
    pub timeout_ms: u64,
}

impl Default for MutationOptions {
    fn default() -> Self {
        Self {
            cap: MAX_MUTANTS,
            timeout_ms: 120_000,
        }
    }
}

async fn suite_green(
    runner: &dyn CommandRunner,
    commands: &[Vec<String>],
    cwd: &Path,
    env: &[(String, String)],
    timeout_ms: u64,
) -> bool {
    for argv in commands {
        let r = runner.run(argv, cwd, env, timeout_ms).await;
        if r.exit_code != Some(0) || r.timed_out || r.cancelled {
            return false;
        }
    }
    true
}

/// The off-by-one check: mutate each changed numeric comparison and demand
/// that the repository's own check commands notice. `tree` is the candidate
/// tree and is not modified; mutants run in a copy under `scratch`.
pub async fn boundary_check(
    runner: &dyn CommandRunner,
    commands: &[Vec<String>],
    tree: &Path,
    scratch: &Path,
    env: &[(String, String)],
    files: &[ChangedFile],
    opts: &MutationOptions,
) -> GeneratedCheck {
    let mutants = boundary_mutants(files, opts.cap);
    if mutants.is_empty() {
        return GeneratedCheck {
            class: Class::BoundaryNotPinned,
            status: Status::Pass,
            findings: vec![],
        };
    }
    let skip = |why: String| GeneratedCheck {
        class: Class::BoundaryNotPinned,
        status: Status::Unknown,
        findings: vec![why],
    };
    let _ = std::fs::remove_dir_all(scratch);
    if let Err(e) = copy_tree(tree, scratch) {
        return skip(format!("could not copy the tree: {e}"));
    }
    let mut env: Vec<(String, String)> = env.to_vec();
    env.retain(|(k, _)| k != "PYTHONDONTWRITEBYTECODE");
    env.push(("PYTHONDONTWRITEBYTECODE".into(), "1".into()));
    remove_pycache(scratch);
    if !suite_green(runner, commands, scratch, &env, opts.timeout_ms).await {
        return GeneratedCheck {
            class: Class::BoundaryNotPinned,
            status: Status::Skip,
            findings: vec![
                "the candidate's own checks are not green; mutants cannot be told apart".into(),
            ],
        };
    }
    // Per changed line: how many mutants were made and which survived. A line
    // is a finding only when every mutant of it survived, i.e. no test
    // exercises the boundary at all; a boundary some test does reach (a
    // mutant killed) is pinned on at least one side, which is what a fix
    // that adds a test for the case it fixes looks like.
    let mut tally: BTreeMap<(String, usize), (usize, Vec<String>)> = BTreeMap::new();
    let mut originals: BTreeMap<String, String> = BTreeMap::new();
    for (tick, m) in (1u64..).zip(&mutants) {
        let path = scratch.join(&m.path);
        let original = match originals.get(&m.path) {
            Some(o) => o.clone(),
            None => {
                let Ok(o) = std::fs::read_to_string(&path) else {
                    return skip(format!("{} is unreadable in the copy", m.path));
                };
                originals.insert(m.path.clone(), o.clone());
                o
            }
        };
        let mut lines: Vec<String> = original.lines().map(str::to_owned).collect();
        if m.line == 0 || m.line > lines.len() {
            continue;
        }
        lines[m.line - 1].clone_from(&m.mutated);
        let mut mutated = lines.join("\n");
        if original.ends_with('\n') {
            mutated.push('\n');
        }
        if write_fresh(&path, &mutated, tick).is_err() {
            return skip(format!("could not write the mutant of {}", m.path));
        }
        remove_pycache(scratch);
        let green = suite_green(runner, commands, scratch, &env, opts.timeout_ms).await;
        let restored = write_fresh(&path, &original, tick + 1_000);
        if restored.is_err() {
            return skip(format!("could not restore {}", m.path));
        }
        let e = tally.entry((m.path.clone(), m.line)).or_default();
        e.0 += 1;
        if green {
            e.1.push(m.description.clone());
        }
    }
    let findings = tally
        .into_iter()
        .filter(|(_, (total, survivors))| survivors.len() == *total)
        .flat_map(|((path, line), (_, survivors))| {
            survivors.into_iter().map(move |d| {
                format!(
                    "{path}:{line}: mutant {d} survived every check; no test reaches this boundary"
                )
            })
        })
        .collect();
    GeneratedCheck::from_findings(Class::BoundaryNotPinned, findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cf(path: &str, old: Option<&str>, new: Option<&str>) -> ChangedFile {
        ChangedFile {
            path: path.into(),
            old: old.map(str::to_owned),
            new: new.map(str::to_owned),
        }
    }

    #[test]
    fn rust_vacuous_tests_are_found_and_real_ones_are_not() {
        let src = "#[test]\nfn early() {\n    return;\n    assert_eq!(f(1), 2);\n}\n\n#[test]\nfn none() {\n    let _ = f(1);\n}\n\n#[test]\nfn taut() {\n    assert!(true);\n    assert_eq!(1, 1);\n}\n\n#[test]\nfn good() {\n    assert_eq!(f(1), 2);\n}\n";
        let v = vacuous(&[cf("tests/a.rs", None, Some(src))]);
        assert_eq!(v.len(), 3, "{v:?}");
        assert!(v.iter().any(|l| l.contains("`early` returns before")));
        assert!(v.iter().any(|l| l.contains("`none` asserts nothing")));
        assert!(v.iter().any(|l| l.contains("`taut` asserts only")));
    }

    #[test]
    fn python_and_script_vacuity() {
        let py = "def test_a():\n    return\n    assert f(1) == 2\n\ndef test_b():\n    assert f(1) == 2\n\ndef test_c():\n    assert True\n";
        let v = vacuous(&[cf("test_x.py", None, Some(py))]);
        assert_eq!(v.len(), 2, "{v:?}");
        let js = "test(\"a\", () => {\n  return;\n  assert.equal(f(1), 2);\n});\ntest(\"b\", () => {\n  assert.equal(f(1), 2);\n});\ntest(\"c\", () => {\n  f(1);\n});\n";
        let v = vacuous(&[cf("test/x.test.mjs", None, Some(js))]);
        assert_eq!(v.len(), 2, "{v:?}");
        assert!(v.iter().any(|l| l.contains("`a`")) && v.iter().any(|l| l.contains("`c`")));
    }

    #[test]
    fn an_unchanged_test_is_not_rescanned() {
        let src = "#[test]\nfn none() {\n    let _ = f(1);\n}\n";
        let v = vacuous(&[cf("tests/a.rs", Some(src), Some(src))]);
        assert!(v.is_empty());
    }

    #[test]
    fn weakening_finds_removed_edited_and_skipped_tests() {
        let old = "#[test]\nfn a() {\n    assert_eq!(f(1), 2);\n    assert_eq!(f(2), 3);\n}\n\n#[test]\nfn b() {\n    assert_eq!(g(1), 5);\n}\n";
        let new = "#[test]\nfn a() {\n    assert_eq!(f(1), 2);\n}\n\n#[test]\n#[ignore]\nfn c() {\n    assert_eq!(g(1), 5);\n}\n";
        let w = weakening(&[cf("tests/a.rs", Some(old), Some(new))], &[]);
        assert!(
            w.iter().any(|l| l.contains("`a` has fewer assertions")),
            "{w:?}"
        );
        assert!(w.iter().any(|l| l.contains("`b` was removed")), "{w:?}");
        assert!(w.iter().any(|l| l.contains("marker added")), "{w:?}");
        // The same edit, declared by the plan, is not a finding for that test.
        let w = weakening(
            &[cf("tests/a.rs", Some(old), Some(new))],
            &["a".into(), "b".into()],
        );
        assert!(
            w.iter().all(|l| !l.contains("`a`") && !l.contains("`b`")),
            "{w:?}"
        );
        // Appending tests is not weakening.
        let appended = format!("{old}\n#[test]\nfn n() {{\n    assert!(h());\n}}\n");
        assert!(weakening(&[cf("tests/a.rs", Some(old), Some(&appended))], &[]).is_empty());
        // An edited expected value is.
        let edited = old.replace("f(2), 3", "f(2), 4");
        let w = weakening(&[cf("tests/a.rs", Some(old), Some(&edited))], &[]);
        assert!(w.iter().any(|l| l.contains("no longer asserts")), "{w:?}");
        // A deleted file.
        let w = weakening(&[cf("tests/a.rs", Some(old), None)], &[]);
        assert_eq!(w.len(), 2, "{w:?}");
    }

    #[test]
    fn overfit_literals_need_a_visible_test_literal_in_an_equality_context() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("tests")).unwrap();
        std::fs::write(
            dir.path().join("tests/t.rs"),
            "#[test]\nfn t() { assert_eq!(norm(\" ab-1 \"), \"AB1\"); assert!(check(500).is_err()); }\n",
        )
        .unwrap();
        let src_old = "pub fn norm(s: &str) -> String { s.trim().to_owned() }\n";
        let hardcoded = "pub fn norm(s: &str) -> String {\n    if s == \" ab-1 \" { return \"AB1\".into(); }\n    s.trim().to_owned()\n}\npub fn check(q: i64) -> Result<(), ()> {\n    if q == 500 { return Err(()); }\n    Ok(())\n}\n";
        let f = overfit(
            dir.path(),
            &[cf("src/lib.rs", Some(src_old), Some(hardcoded))],
        );
        assert_eq!(f.len(), 2, "{f:?}");
        let honest = "pub fn norm(s: &str) -> String {\n    s.trim().replace('-', \"\").to_uppercase()\n}\npub fn check(q: i64) -> Result<(), ()> {\n    if q > 100 { return Err(()); }\n    Ok(())\n}\n";
        assert!(overfit(dir.path(), &[cf("src/lib.rs", Some(src_old), Some(honest))]).is_empty());
        // Rust digit separators do not hide a literal.
        let sep = "pub fn check(q: i64) -> Result<(), ()> {\n    if q == 5_00 { return Err(()); }\n    Ok(())\n}\n";
        assert_eq!(
            overfit(dir.path(), &[cf("src/lib.rs", None, Some(sep))]).len(),
            1
        );
        let table = "pub fn check(q: i64) -> Result<(), ()> {\n    if matches!(q, 500 | 101) { return Err(()); }\n    Ok(())\n}\n";
        assert_eq!(
            overfit(dir.path(), &[cf("src/lib.rs", None, Some(table))]).len(),
            1
        );
    }

    #[test]
    fn mutants_are_generated_for_changed_numeric_comparisons_only() {
        let old = "fn f(q: i64) -> bool {\n    q > 0\n}\n";
        let new = "fn f(q: i64) -> bool {\n    q > 0 && q <= 100\n}\n";
        let m = boundary_mutants(&[cf("src/lib.rs", Some(old), Some(new))], 20);
        // `>` and `<=` each: operator, +1, -1 (the 0 side has no -1).
        let descs: Vec<&str> = m.iter().map(|m| m.description.as_str()).collect();
        assert!(descs.iter().any(|d| d.contains("`<=` -> `<`")), "{descs:?}");
        assert!(descs.iter().any(|d| d.contains("100 -> 101")), "{descs:?}");
        assert!(descs.iter().any(|d| d.contains("100 -> 99")), "{descs:?}");
        assert!(m.iter().all(|m| m.line == 2));
        // An unchanged line, a test file and a non-number comparison make none.
        assert!(boundary_mutants(&[cf("src/lib.rs", Some(new), Some(new))], 20).is_empty());
        assert!(boundary_mutants(&[cf("tests/t.rs", None, Some(new))], 20).is_empty());
        assert!(
            boundary_mutants(
                &[cf(
                    "src/lib.rs",
                    None,
                    Some("fn g(a: i64, b: i64) -> bool { a > b }\n")
                )],
                20
            )
            .is_empty()
        );
        // A comparison on a line that also holds a string is still one; a
        // comparator inside the string is not.
        let js = "  if (q >= 100) throw new RangeError(`at most 100: ${q}`);\n";
        let m = boundary_mutants(&[cf("src/shop.mjs", None, Some(js))], 20);
        assert_eq!(m.len(), 3, "{m:?}");
        assert!(m.iter().all(|m| m.mutated.contains("`at most 100: ${q}`")));
        let text = "  log(\"a > 5 and b < 7\");\n";
        assert!(boundary_mutants(&[cf("src/shop.mjs", None, Some(text))], 20).is_empty());
        // Generic angle brackets and arrows are not comparisons.
        assert!(
            boundary_mutants(
                &[cf(
                    "src/lib.rs",
                    None,
                    Some("fn g() -> Vec<u8> { Vec::new() }\n")
                )],
                20
            )
            .is_empty()
        );
    }

    #[test]
    fn evidence_maps_to_the_gate_vocabulary() {
        let c = GeneratedCheck::from_findings(Class::VacuousPass, vec!["x".into()]);
        let e = c.to_evidence();
        assert_eq!(
            (e.check_id.as_str(), e.kind.as_str(), e.status.as_str()),
            ("adv:vacuous-pass", "adversarial", "FAIL")
        );
        assert_eq!(c.digest().len(), 64);
        let p = GeneratedCheck::from_findings(Class::VacuousPass, vec![]);
        assert_eq!(p.to_evidence().status, "PASS");
    }
}
