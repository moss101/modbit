//! Trigram prefilter for exact and regex search (PX-111; docs/18
//! "ripgrep-compatible search path", "Index freshness"): a posting list per
//! three-byte window of the (ASCII-lowercased) file text, so a query that
//! must contain some literal run of three or more bytes only reads the files
//! that contain every trigram of it.
//!
//! The index changes the cost of a search and never its answer. It is a
//! prefilter: a candidate file is searched by exactly the code that searches
//! every file otherwise, so the results are equal by construction as long as
//! the candidate set is a superset of the files that match — which the
//! literal extraction guarantees by being conservative: a construct it does
//! not understand gives no requirement (the search scans), and so does a
//! literal shorter than three bytes. Case-insensitive search folds ASCII
//! only; the two non-ASCII characters whose Unicode simple case folding
//! reaches an ASCII letter (U+212A for `k`, U+017F for `s`) and every
//! non-ASCII letter are treated as wildcards in that mode.

use std::collections::{BTreeMap, HashMap};

/// A three-byte window.
pub type Trigram = [u8; 3];

/// One file's trigram set, with the content hash it was read at (the key
/// persistence validates it by).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileTrigrams {
    /// Root-relative path.
    pub path: String,
    /// sha256 (hex) of the bytes.
    pub hash: String,
    /// Sorted, deduplicated trigrams of the lowercased text.
    pub trigrams: Vec<Trigram>,
}

/// The trigram posting lists of one workspace.
#[derive(Debug, Default)]
pub struct TrigramIndex {
    files: Vec<Option<FileTrigrams>>,
    free: Vec<u32>,
    by_path: BTreeMap<String, u32>,
    postings: HashMap<Trigram, Vec<u32>>,
}

/// Sorted, deduplicated trigrams of `text`, ASCII-lowercased.
#[must_use]
pub fn trigrams_of(text: &[u8]) -> Vec<Trigram> {
    if text.len() < 3 {
        return vec![];
    }
    let lower: Vec<u8> = text.iter().map(u8::to_ascii_lowercase).collect();
    let mut out: Vec<Trigram> = lower.windows(3).map(|w| [w[0], w[1], w[2]]).collect();
    out.sort_unstable();
    out.dedup();
    out
}

impl TrigramIndex {
    /// An empty index.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Files in the index.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_path.len()
    }

    /// Whether the index holds no file.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_path.is_empty()
    }

    /// The content hash the index holds for `path`.
    #[must_use]
    pub fn hash_of(&self, path: &str) -> Option<&str> {
        self.by_path
            .get(path)
            .and_then(|id| self.files[*id as usize].as_ref())
            .map(|f| f.hash.as_str())
    }

    /// The trigram set the index holds for `path`.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&FileTrigrams> {
        self.by_path
            .get(path)
            .and_then(|id| self.files[*id as usize].as_ref())
    }

    /// Every file's set, in path order.
    pub fn entries(&self) -> impl Iterator<Item = &FileTrigrams> {
        self.by_path
            .values()
            .filter_map(|id| self.files[*id as usize].as_ref())
    }

    /// Add or replace a file.
    pub fn insert(&mut self, path: &str, hash: &str, trigrams: Vec<Trigram>) {
        self.remove(path);
        let id = match self.free.pop() {
            Some(id) => id,
            None => {
                self.files.push(None);
                u32::try_from(self.files.len() - 1).unwrap_or(u32::MAX)
            }
        };
        for t in &trigrams {
            let list = self.postings.entry(*t).or_default();
            // Ids are reused, so keep each list sorted.
            let at = list.binary_search(&id).unwrap_or_else(|e| e);
            list.insert(at, id);
        }
        self.by_path.insert(path.to_owned(), id);
        self.files[id as usize] = Some(FileTrigrams {
            path: path.to_owned(),
            hash: hash.to_owned(),
            trigrams,
        });
    }

    /// Remove a file.
    pub fn remove(&mut self, path: &str) {
        let Some(id) = self.by_path.remove(path) else {
            return;
        };
        if let Some(f) = self.files[id as usize].take() {
            for t in f.trigrams {
                if let Some(list) = self.postings.get_mut(&t) {
                    if let Ok(at) = list.binary_search(&id) {
                        list.remove(at);
                    }
                    if list.is_empty() {
                        self.postings.remove(&t);
                    }
                }
            }
        }
        self.free.push(id);
    }

    /// The paths of the files that contain every trigram in `required`, in
    /// path order. An empty requirement is every file.
    #[must_use]
    pub fn candidates(&self, required: &[Trigram]) -> Vec<&str> {
        let mut lists: Vec<&Vec<u32>> = Vec::with_capacity(required.len());
        for t in required {
            match self.postings.get(t) {
                Some(l) => lists.push(l),
                None => return vec![],
            }
        }
        if lists.is_empty() {
            return self.by_path.keys().map(String::as_str).collect();
        }
        lists.sort_by_key(|l| l.len());
        let mut acc: Vec<u32> = lists[0].clone();
        for l in &lists[1..] {
            acc = intersect(&acc, l);
            if acc.is_empty() {
                return vec![];
            }
        }
        let mut paths: Vec<&str> = acc
            .iter()
            .filter_map(|id| self.files[*id as usize].as_ref())
            .map(|f| f.path.as_str())
            .collect();
        paths.sort_unstable();
        paths
    }
}

fn intersect(a: &[u32], b: &[u32]) -> Vec<u32> {
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::with_capacity(a.len().min(b.len()));
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out
}

/// The trigrams every match of `pattern` (Rust `regex` syntax) must contain:
/// the trigrams of every maximal run of three or more literal bytes the
/// pattern requires. `None` when nothing can be required — an alternation,
/// a flag other than `(?i)`, no run of three bytes — and the caller then
/// scans. Conservative by design: a construct this does not model breaks the
/// run it sits in rather than guessing, and an optional atom (`x?`, `x*`,
/// `x{0,}`) is not required.
#[must_use]
pub fn required_trigrams(pattern: &str, case_insensitive: bool) -> Option<Vec<Trigram>> {
    let runs = required_runs(pattern, case_insensitive)?;
    let mut out: Vec<Trigram> = Vec::new();
    for run in runs {
        if run.len() >= 3 {
            let lower: Vec<u8> = run.iter().map(u8::to_ascii_lowercase).collect();
            out.extend(lower.windows(3).map(|w| [w[0], w[1], w[2]]));
        }
    }
    if out.is_empty() {
        return None;
    }
    out.sort_unstable();
    out.dedup();
    Some(out)
}

/// Literal bytes of a regex atom that is a plain character.
fn literal_bytes(c: char, ci: bool, out: &mut Vec<u8>) -> bool {
    if ci {
        // Case folding can leave ASCII (`k` ~ U+212A, `s` ~ U+017F) and
        // folds every non-ASCII letter: those are not literals.
        if !c.is_ascii() || c.eq_ignore_ascii_case(&'k') || c.eq_ignore_ascii_case(&'s') {
            return false;
        }
    }
    let mut buf = [0u8; 4];
    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
    true
}

fn required_runs(pattern: &str, ci_in: bool) -> Option<Vec<Vec<u8>>> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut ci = ci_in;
    // `(?i)` anywhere makes the whole pattern fold-sensitive for our purposes.
    let mut i = 0;
    while i + 3 < chars.len() {
        if chars[i] == '(' && chars[i + 1] == '?' && chars[i + 2] == 'i' && chars[i + 3] == ')' {
            ci = true;
        }
        i += 1;
    }
    let mut runs: Vec<Vec<u8>> = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    // The byte length the last literal atom added to `cur` (0 when the last
    // atom was not a literal), so a following `?`/`*` can take it back.
    let mut last_lit_len = 0usize;
    let mut i = 0;
    let n = chars.len();
    let flush = |cur: &mut Vec<u8>, runs: &mut Vec<Vec<u8>>| {
        if !cur.is_empty() {
            runs.push(std::mem::take(cur));
        }
    };
    while i < n {
        let c = chars[i];
        match c {
            '|' => return None,
            '\\' => {
                i += 1;
                let e = *chars.get(i)?;
                match e {
                    // Escaped metacharacters are literals.
                    '.' | '\\' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}'
                    | '^' | '$' | '#' | '&' | '-' | '~' => {
                        let before = cur.len();
                        if literal_bytes(e, ci, &mut cur) {
                            last_lit_len = cur.len() - before;
                        } else {
                            flush(&mut cur, &mut runs);
                            last_lit_len = 0;
                        }
                    }
                    'n' | 't' | 'r' => {
                        let lit = match e {
                            'n' => '\n',
                            't' => '\t',
                            _ => '\r',
                        };
                        let before = cur.len();
                        cur.push(lit as u8);
                        last_lit_len = cur.len() - before;
                    }
                    // Classes, anchors, hex/unicode/property escapes, `\Q`:
                    // not literals. Hex and unicode escapes carry arguments
                    // to skip.
                    'x' => {
                        flush(&mut cur, &mut runs);
                        last_lit_len = 0;
                        if chars.get(i + 1) == Some(&'{') {
                            while i < n && chars[i] != '}' {
                                i += 1;
                            }
                        } else {
                            i += 2;
                        }
                    }
                    'u' | 'U' | 'p' | 'P' => {
                        flush(&mut cur, &mut runs);
                        last_lit_len = 0;
                        if chars.get(i + 1) == Some(&'{') {
                            while i < n && chars[i] != '}' {
                                i += 1;
                            }
                        } else if e == 'p' || e == 'P' {
                            i += 1;
                        } else {
                            i += if e == 'u' { 4 } else { 8 };
                        }
                    }
                    'd'
                    | 'D'
                    | 'w'
                    | 'W'
                    | 's'
                    | 'S'
                    | 'b'
                    | 'B'
                    | 'A'
                    | 'z'
                    | 'Z'
                    | 'G'
                    | 'a'
                    | 'f'
                    | 'v'
                    | '0'..='9'
                    | 'Q'
                    | 'E' => {
                        flush(&mut cur, &mut runs);
                        last_lit_len = 0;
                    }
                    // Any other escaped character: not understood.
                    _ => return None,
                }
                i += 1;
            }
            '[' => {
                // A class, with nested classes and escapes.
                flush(&mut cur, &mut runs);
                last_lit_len = 0;
                let mut depth = 1;
                i += 1;
                // A leading `]` or `^]` is a literal inside the class.
                if chars.get(i) == Some(&'^') {
                    i += 1;
                }
                if chars.get(i) == Some(&']') {
                    i += 1;
                }
                while i < n && depth > 0 {
                    match chars[i] {
                        '\\' => i += 1,
                        '[' => depth += 1,
                        ']' => depth -= 1,
                        _ => {}
                    }
                    i += 1;
                }
                if depth > 0 {
                    return None;
                }
            }
            '(' => {
                flush(&mut cur, &mut runs);
                last_lit_len = 0;
                if chars.get(i + 1) == Some(&'?') {
                    // `(?i)` and `(?:` only.
                    match (chars.get(i + 2), chars.get(i + 3)) {
                        (Some('i'), Some(')')) => {
                            i += 4;
                            continue;
                        }
                        (Some(':'), _) => {}
                        _ => return None,
                    }
                }
                // Skip the group: its content may be optional or repeated.
                let mut depth = 1;
                i += 1;
                while i < n && depth > 0 {
                    match chars[i] {
                        '\\' => i += 1,
                        '[' => {
                            // A class inside a group.
                            let mut d = 1;
                            i += 1;
                            if chars.get(i) == Some(&'^') {
                                i += 1;
                            }
                            if chars.get(i) == Some(&']') {
                                i += 1;
                            }
                            while i < n && d > 0 {
                                match chars[i] {
                                    '\\' => i += 1,
                                    '[' => d += 1,
                                    ']' => d -= 1,
                                    _ => {}
                                }
                                i += 1;
                            }
                            i -= 1;
                        }
                        '(' => depth += 1,
                        ')' => depth -= 1,
                        '|' => return None,
                        _ => {}
                    }
                    i += 1;
                }
                if depth > 0 {
                    return None;
                }
            }
            ')' => return None,
            '.' | '^' | '$' => {
                flush(&mut cur, &mut runs);
                last_lit_len = 0;
                i += 1;
            }
            '*' | '?' => {
                // The previous atom is optional: take its literal back.
                let keep = cur.len().saturating_sub(last_lit_len);
                cur.truncate(keep);
                flush(&mut cur, &mut runs);
                last_lit_len = 0;
                i += 1;
                if chars.get(i) == Some(&'?') {
                    i += 1;
                }
            }
            '+' => {
                // Required once, repeatable: the run ends with it.
                flush(&mut cur, &mut runs);
                last_lit_len = 0;
                i += 1;
                if chars.get(i) == Some(&'?') {
                    i += 1;
                }
            }
            '{' => {
                // `{m}`, `{m,}`, `{m,n}`: anything else is not understood.
                let mut j = i + 1;
                let mut min_text = String::new();
                while j < n && chars[j].is_ascii_digit() {
                    min_text.push(chars[j]);
                    j += 1;
                }
                if min_text.is_empty() {
                    return None;
                }
                if chars.get(j) == Some(&',') {
                    j += 1;
                    while j < n && chars[j].is_ascii_digit() {
                        j += 1;
                    }
                }
                if chars.get(j) != Some(&'}') {
                    return None;
                }
                let min: u64 = min_text.parse().ok()?;
                if min == 0 {
                    let keep = cur.len().saturating_sub(last_lit_len);
                    cur.truncate(keep);
                }
                flush(&mut cur, &mut runs);
                last_lit_len = 0;
                i = j + 1;
                if chars.get(i) == Some(&'?') {
                    i += 1;
                }
            }
            other => {
                let before = cur.len();
                if literal_bytes(other, ci, &mut cur) {
                    last_lit_len = cur.len() - before;
                } else {
                    flush(&mut cur, &mut runs);
                    last_lit_len = 0;
                }
                i += 1;
            }
        }
    }
    flush(&mut cur, &mut runs);
    Some(runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(p: &str, ci: bool) -> Option<Vec<String>> {
        required_trigrams(p, ci).map(|v| {
            v.iter()
                .map(|t| String::from_utf8_lossy(t).into_owned())
                .collect()
        })
    }

    #[test]
    fn a_literal_requires_its_own_trigrams() {
        assert_eq!(
            req("total", false),
            Some(vec!["ota".to_owned(), "tal".to_owned(), "tot".to_owned()])
        );
        assert_eq!(req("ab", false), None, "two bytes give no trigram");
        assert_eq!(req("a.c", false), None);
    }

    #[test]
    fn optional_repeated_and_alternated_atoms_are_not_required() {
        // `x?` and `x*` take the atom back; `+` keeps it but ends the run.
        assert_eq!(
            req("colou?r", false),
            Some(vec!["col".to_owned(), "olo".to_owned()]),
            "the optional `u` is taken back"
        );
        // `fooo*bar`: the starred `o` is taken back, leaving `foo` and `bar`.
        assert_eq!(
            req("fooo*bar", false),
            Some(vec!["bar".to_owned(), "foo".to_owned()])
        );
        assert!(req("abc|def", false).is_none());
        assert!(req("(abc|def)ghi", false).is_none());
        assert_eq!(req("(?:abc)?xyz", false), Some(vec!["xyz".into()]));
        assert_eq!(req("a{0,3}bcd", false), Some(vec!["bcd".into()]));
        assert!(req("(?s)abc", false).is_none());
        assert!(req("(?x) a b c", false).is_none());
    }

    #[test]
    fn case_insensitive_search_does_not_trust_letters_that_fold_outside_ascii() {
        assert!(req("task", true).is_none(), "k and s are wildcards");
        assert_eq!(req("total", true).map(|v| v.len()), Some(3));
        assert_eq!(
            req("caf\u{e9}", true),
            Some(vec!["caf".to_owned()]),
            "the accent is a wildcard"
        );
        assert!(req("caf\u{e9}s", false).is_some());
    }

    #[test]
    fn the_index_answers_with_files_that_contain_every_trigram_and_updates() {
        let mut ix = TrigramIndex::new();
        ix.insert("a.rs", "h1", trigrams_of(b"fn compute_total() {}"));
        ix.insert("b.rs", "h2", trigrams_of(b"fn other() {}"));
        let want = required_trigrams("compute_total", false).unwrap();
        assert_eq!(ix.candidates(&want), vec!["a.rs"]);
        // Case: the index is lowercased, the requirement too.
        assert_eq!(
            ix.candidates(&required_trigrams("COMPUTE", true).unwrap()),
            vec!["a.rs"]
        );
        ix.insert("b.rs", "h3", trigrams_of(b"compute_total again"));
        assert_eq!(ix.candidates(&want), vec!["a.rs", "b.rs"]);
        ix.remove("a.rs");
        assert_eq!(ix.candidates(&want), vec!["b.rs"]);
        assert_eq!(ix.hash_of("b.rs"), Some("h3"));
        assert!(
            ix.candidates(&required_trigrams("zzz", false).unwrap())
                .is_empty()
        );
        // A freed id is reused without disturbing the lists.
        ix.insert("c.rs", "h4", trigrams_of(b"compute_total"));
        assert_eq!(ix.candidates(&want), vec!["b.rs", "c.rs"]);
    }
}
