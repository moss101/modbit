//! Unified-diff parsing and selective hunk application (docs/20 "Trusted
//! Code Surface", REQ-EV-0036 per-hunk review): the review surface shows
//! revision-bound hunks and rejects some by rebuilding the file from the base
//! text plus the accepted hunks only.

use serde::{Deserialize, Serialize};

/// One hunk of a unified diff.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    /// 0-based index within its file.
    pub index: u32,
    /// `@@ -a,b +c,d @@ …` header line.
    pub header: String,
    /// Old start line (1-based).
    pub old_start: u32,
    /// Old line count.
    pub old_lines: u32,
    /// New start line (1-based).
    pub new_start: u32,
    /// New line count.
    pub new_lines: u32,
    /// Body lines with their prefix (` `, `+`, `-`), without the newline.
    pub lines: Vec<String>,
}

/// One file of a unified diff.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDiff {
    /// New path (old path for deletions).
    pub path: String,
    /// Old path when renamed.
    pub old_path: Option<String>,
    /// `A` added, `M` modified, `D` deleted, `R` renamed.
    pub status: char,
    /// Binary file (no hunks).
    pub binary: bool,
    /// Hunks.
    pub hunks: Vec<Hunk>,
}

fn parse_range(s: &str) -> (u32, u32) {
    let s = s.trim_start_matches(['-', '+']);
    match s.split_once(',') {
        Some((a, b)) => (a.parse().unwrap_or(0), b.parse().unwrap_or(0)),
        None => (s.parse().unwrap_or(0), 1),
    }
}

/// Parse `git diff` output.
#[must_use]
pub fn parse_unified(text: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut cur: Option<FileDiff> = None;
    let mut hunk: Option<Hunk> = None;
    let flush_hunk = |cur: &mut Option<FileDiff>, hunk: &mut Option<Hunk>| {
        if let (Some(f), Some(h)) = (cur.as_mut(), hunk.take()) {
            f.hunks.push(h);
        }
    };
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            flush_hunk(&mut cur, &mut hunk);
            if let Some(f) = cur.take() {
                files.push(f);
            }
            // `a/<old> b/<new>`
            let b = rest.rfind(" b/").map(|i| rest[i + 3..].to_owned());
            let a = rest
                .strip_prefix("a/")
                .and_then(|s| s.find(" b/").map(|i| s[..i].to_owned()));
            let path = b.clone().or(a.clone()).unwrap_or_default();
            cur = Some(FileDiff {
                path,
                old_path: match (&a, &b) {
                    (Some(a), Some(b)) if a != b => Some(a.clone()),
                    _ => None,
                },
                status: 'M',
                binary: false,
                hunks: vec![],
            });
            continue;
        }
        let Some(f) = cur.as_mut() else {
            continue;
        };
        if line.starts_with("new file mode") {
            f.status = 'A';
        } else if line.starts_with("deleted file mode") {
            f.status = 'D';
        } else if line.starts_with("rename from") {
            f.status = 'R';
        } else if line.starts_with("Binary files") {
            f.binary = true;
        } else if let Some(rest) = line.strip_prefix("@@ ") {
            flush_hunk(&mut cur, &mut hunk);
            let f = cur.as_mut().expect("file");
            let mut parts = rest.splitn(3, ' ');
            let (old_start, old_lines) = parse_range(parts.next().unwrap_or("-0,0"));
            let (new_start, new_lines) = parse_range(parts.next().unwrap_or("+0,0"));
            hunk = Some(Hunk {
                index: f.hunks.len() as u32,
                header: line.to_owned(),
                old_start,
                old_lines,
                new_start,
                new_lines,
                lines: vec![],
            });
        } else if let Some(h) = hunk.as_mut() {
            if line.starts_with('\\') {
                continue; // "\ No newline at end of file"
            }
            if line.starts_with([' ', '+', '-']) {
                h.lines.push(line.to_owned());
            }
        }
    }
    flush_hunk(&mut cur, &mut hunk);
    if let Some(f) = cur.take() {
        files.push(f);
    }
    files
}

/// Rebuild a file from its base text applying only `accepted` hunk indexes.
/// Hunks are applied in order against the base line numbering; the result is
/// exactly the candidate text when every hunk is accepted and exactly the base
/// text when none is.
#[must_use]
pub fn apply_selected(base: &str, hunks: &[Hunk], accepted: &[u32]) -> String {
    let base_lines: Vec<&str> = base.lines().collect();
    let base_ends_nl = base.ends_with('\n');
    let mut out: Vec<String> = Vec::new();
    let mut cursor = 0usize; // next base line index to copy
    for h in hunks {
        let start = (h.old_start.max(1) - 1) as usize;
        // Copy untouched base lines before the hunk.
        while cursor < start && cursor < base_lines.len() {
            out.push(base_lines[cursor].to_owned());
            cursor += 1;
        }
        if accepted.contains(&h.index) {
            for l in &h.lines {
                match l.as_bytes().first() {
                    Some(b' ') => {
                        out.push(l[1..].to_owned());
                        cursor += 1;
                    }
                    Some(b'-') => {
                        cursor += 1;
                    }
                    Some(b'+') => out.push(l[1..].to_owned()),
                    _ => {}
                }
            }
        } else {
            // Rejected: keep the base side.
            for l in &h.lines {
                match l.as_bytes().first() {
                    Some(b' ') | Some(b'-') => {
                        if cursor < base_lines.len() {
                            out.push(base_lines[cursor].to_owned());
                        }
                        cursor += 1;
                    }
                    _ => {}
                }
            }
        }
    }
    while cursor < base_lines.len() {
        out.push(base_lines[cursor].to_owned());
        cursor += 1;
    }
    let mut s = out.join("\n");
    if base_ends_nl || !out.is_empty() {
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\nindex 1..2 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,4 @@\n fn a() {}\n-fn b() {}\n+fn b() { 1 }\n+fn b2() {}\n fn c() {}\n@@ -10,2 +11,2 @@\n fn y() {}\n-fn z() {}\n+fn z() { 2 }\ndiff --git a/new.txt b/new.txt\nnew file mode 100644\n--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1 @@\n+hello\n";

    #[test]
    fn parses_files_and_hunks() {
        let files = parse_unified(DIFF);
        assert_eq!(files.len(), 2);
        assert_eq!(
            (
                files[0].path.as_str(),
                files[0].status,
                files[0].hunks.len()
            ),
            ("src/lib.rs", 'M', 2)
        );
        assert_eq!(
            (
                files[1].path.as_str(),
                files[1].status,
                files[1].hunks.len()
            ),
            ("new.txt", 'A', 1)
        );
        assert_eq!(files[0].hunks[1].old_start, 10);
    }

    #[test]
    fn selective_application_matches_user_choices_exactly() {
        let base =
            "fn a() {}\nfn b() {}\nfn c() {}\nl4\nl5\nl6\nl7\nl8\nl9\nfn y() {}\nfn z() {}\n";
        let files = parse_unified(DIFF);
        let all = apply_selected(base, &files[0].hunks, &[0, 1]);
        assert_eq!(
            all,
            "fn a() {}\nfn b() { 1 }\nfn b2() {}\nfn c() {}\nl4\nl5\nl6\nl7\nl8\nl9\nfn y() {}\nfn z() { 2 }\n"
        );
        let none = apply_selected(base, &files[0].hunks, &[]);
        assert_eq!(none, base);
        let only_second = apply_selected(base, &files[0].hunks, &[1]);
        assert_eq!(
            only_second,
            "fn a() {}\nfn b() {}\nfn c() {}\nl4\nl5\nl6\nl7\nl8\nl9\nfn y() {}\nfn z() { 2 }\n"
        );
        let created = apply_selected("", &files[1].hunks, &[0]);
        assert_eq!(created, "hello\n");
    }
}
