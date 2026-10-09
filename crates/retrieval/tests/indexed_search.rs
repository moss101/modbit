//! PX-111: the indexed search path returns exactly what the scan returns.
//!
//! The trigram prefilter may only change what a search costs. Two proofs: a
//! differential run over a real corpus (this workspace's own `crates/`
//! directory) — two hundred queries, exact and regex, case-sensitive and not,
//! with globs and tight bounds, every one answered through the index and by
//! the full scan and compared hit for hit — and a soundness property over
//! generated patterns and texts: whenever the regex matches a text, the text
//! contains every trigram the extraction says the pattern requires.

use std::path::Path;
use std::time::Instant;

use modbit_retrieval::trigram::{required_trigrams, trigrams_of};
use modbit_retrieval::{RepositoryIndex, SearchOptions};

/// A deterministic generator (no dependency, reproducible failures).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }
}

fn corpus() -> RepositoryIndex {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    RepositoryIndex::build(&crates.canonicalize().unwrap(), 1).unwrap()
}

/// Identifier-like words that occur in the corpus.
fn words(idx: &RepositoryIndex, rng: &mut Lcg, n: usize) -> Vec<String> {
    let texts: Vec<&str> = idx.texts().map(|(_, t, _)| t).collect();
    let mut out = Vec::new();
    while out.len() < n {
        let t = texts[rng.below(texts.len())];
        let toks: Vec<&str> = t
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|w| w.len() >= 3 && w.len() <= 16)
            .collect();
        if toks.is_empty() {
            continue;
        }
        out.push((*rng.pick(&toks)).to_owned());
    }
    out
}

#[test]
fn two_hundred_queries_return_identical_results_through_the_index_and_the_scan() {
    let idx = corpus();
    let stats = idx.stats();
    assert!(stats.searchable > 100, "{stats:?}");
    let mut rng = Lcg(0x5eed);
    let w = words(&idx, &mut rng, 400);
    let mut queries: Vec<(String, bool, bool)> = Vec::new(); // (pattern, is_regex, ci)
    for i in 0..200 {
        let a = &w[i];
        let b = &w[200 + i];
        let q = match i % 10 {
            0 | 1 => (a.clone(), false, false),
            2 => (a.clone(), false, true),
            3 => (format!("{a}.*{b}"), true, false),
            4 => (format!("fn\\s+{a}"), true, false),
            5 => (format!("\\b{a}\\b"), true, false),
            6 => (format!("{a}[0-9]+"), true, true),
            7 => (format!("(?:{a})?{b}"), true, false),
            8 => (format!("{a}|{b}"), true, false),
            _ => (format!("{a}_{b}"), false, false),
        };
        queries.push(q);
    }
    // Some shapes that deserve a place of their own.
    for (p, re, ci) in [
        ("fn ", false, false),
        ("::", false, false),
        ("TODO", false, true),
        ("pub fn \\w+\\(", true, false),
        ("é", false, false),
        ("use std::collections", false, false),
        ("\\.rs\"", true, false),
        ("impl\\s+\\w+\\s+for\\s+\\w+", true, false),
    ] {
        queries.push((p.to_owned(), re, ci));
    }
    let mut indexed = 0usize;
    let mut with_hits = 0usize;
    let (mut scanned_idx, mut scanned_all) = (0usize, 0usize);
    let (mut t_idx, mut t_scan) = (0u128, 0u128);
    for (n, (pattern, is_regex, ci)) in queries.iter().enumerate() {
        let opts = SearchOptions {
            case_insensitive: *ci,
            path_glob: match n % 7 {
                0 => Some("**/*.rs".into()),
                1 => Some("retrieval/**".into()),
                _ => None,
            },
            max_hits: if n % 5 == 0 { 7 } else { 200 },
            max_hits_per_file: if n % 3 == 0 { 2 } else { 20 },
            max_line_bytes: 400,
        };
        let run = |use_index: bool| {
            let started = Instant::now();
            let r = if *is_regex {
                idx.search_regex_with(pattern, &opts, use_index)
            } else {
                Ok(idx.search_exact_with(pattern, &opts, use_index))
            };
            (r, started.elapsed().as_micros())
        };
        let (fast, d1) = run(true);
        let (slow, d2) = run(false);
        let (fast, plan) = fast.unwrap_or_else(|e| panic!("{pattern}: {e}"));
        let (slow, scan_plan) = slow.unwrap();
        assert_eq!(
            fast, slow,
            "query {n} {pattern:?} (regex={is_regex}, ci={ci}): the indexed hits differ from the scan's"
        );
        assert_eq!(scan_plan.path, "scan");
        if plan.path == "indexed" {
            indexed += 1;
            scanned_idx += plan.files_scanned;
            scanned_all += scan_plan.files_scanned;
            t_idx += d1;
            t_scan += d2;
            assert!(plan.files_scanned <= scan_plan.files_scanned);
        }
        if !fast.is_empty() {
            with_hits += 1;
        }
    }
    assert!(
        indexed >= 100,
        "only {indexed} of {} queries took the index",
        queries.len()
    );
    assert!(with_hits >= 100, "{with_hits} queries had hits");
    // The index reads a fraction of what the scan reads, and that shows in
    // the wall clock.
    let ratio = scanned_idx as f64 / scanned_all.max(1) as f64;
    eprintln!(
        "PX-111 indexed search: {} queries, {indexed} through the index; files read {scanned_idx} vs {scanned_all} ({:.1}% of the scan); wall {t_idx} us vs {t_scan} us ({:.1}x faster); {} files, {} bytes",
        queries.len(),
        ratio * 100.0,
        t_scan as f64 / t_idx.max(1) as f64,
        stats.searchable,
        stats.bytes,
    );
    assert!(
        ratio < 0.25,
        "the prefilter reads {:.0}% of the files a scan reads",
        ratio * 100.0
    );
    // The margin is stated: at least 1.3x on this 4 MB corpus (the scan is
    // already fast here and every query pays its own setup; the gap grows
    // with the corpus, see the benchmark in the Core's index suite).
    assert!(
        t_idx * 13 < t_scan * 10,
        "indexed {t_idx} us is not at least 1.3x faster than the scan's {t_scan} us"
    );
}

/// A pattern the extraction understands only partly must still be sound.
#[test]
fn the_required_trigrams_of_any_generated_pattern_are_in_every_text_it_matches() {
    let mut rng = Lcg(42);
    let atoms = [
        "a", "b", "c", "ab", "abc", "k", "s", "K", "é", "x", "\\.", "\\d", "\\w", "\\s", "\\b",
        ".", "[a-c]", "[^a]", "(ab)", "(?:abc)", "(a|b)", "foo", "bar", "\\n", "\\x41", "\\u{e9}",
        "^", "$", "(?i)", "q",
    ];
    let quants = [
        "", "", "", "?", "*", "+", "{2}", "{0,1}", "{1,3}", "??", "*?", "+?",
    ];
    let alphabet: Vec<char> = "abcAB kKsSéx.\n \t123foobarq".chars().collect();
    let mut checked = 0usize;
    let mut matched_texts = 0usize;
    let mut with_requirement = 0usize;
    for _ in 0..4000 {
        let mut p = String::new();
        for _ in 0..(1 + rng.below(5)) {
            p.push_str(rng.pick(&atoms));
            p.push_str(rng.pick(&quants));
        }
        if rng.below(6) == 0 {
            p = format!("{p}|{}", rng.pick(&atoms));
        }
        let ci = rng.below(3) == 0;
        let Ok(re) = regex::RegexBuilder::new(&p).case_insensitive(ci).build() else {
            continue;
        };
        let required = required_trigrams(&p, ci);
        if required.is_some() {
            with_requirement += 1;
        }
        for _ in 0..12 {
            let len = 3 + rng.below(30);
            let text: String = (0..len).map(|_| *rng.pick(&alphabet)).collect();
            checked += 1;
            if !re.is_match(&text) {
                continue;
            }
            matched_texts += 1;
            if let Some(req) = &required {
                let have = trigrams_of(text.as_bytes());
                for t in req {
                    assert!(
                        have.binary_search(t).is_ok(),
                        "pattern {p:?} (ci={ci}) matches {text:?} but the required trigram {:?} is not in it",
                        String::from_utf8_lossy(t)
                    );
                }
            }
        }
    }
    eprintln!(
        "PX-111 trigram soundness: {checked} (pattern, text) pairs, {matched_texts} matches, {with_requirement} patterns with a requirement"
    );
    assert!(matched_texts > 500 && with_requirement > 300);
}

/// An edit is seen at once: the index never serves a file from before it.
#[test]
fn a_refreshed_file_is_found_at_its_new_content_and_not_at_its_old() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("a.txt"), "the quick brown fox\n").unwrap();
    std::fs::write(root.join("b.txt"), "lazy dogs sleep\n").unwrap();
    let mut idx = RepositoryIndex::build(root, 1).unwrap();
    let opts = SearchOptions::default();
    let (h, plan) = idx.search_exact_with("quick brown", &opts, true);
    assert_eq!((h.len(), plan.path.as_str()), (1, "indexed"), "{plan:?}");
    std::fs::write(root.join("a.txt"), "the slow green turtle\n").unwrap();
    idx.refresh(&["a.txt".to_owned()], 2);
    assert!(
        idx.search_exact("quick brown", &opts).is_empty(),
        "a stale hit after an edit"
    );
    assert_eq!(idx.search_exact("slow green", &opts).len(), 1);
    // Removal and addition through the revision diff.
    std::fs::remove_file(root.join("b.txt")).unwrap();
    std::fs::write(root.join("c.txt"), "brand new quick brown\n").unwrap();
    let changed = idx.resync(3).unwrap();
    assert_eq!(changed, ["b.txt", "c.txt"]);
    assert!(idx.search_exact("lazy dogs", &opts).is_empty());
    assert_eq!(idx.search_exact("quick brown", &opts)[0].path, "c.txt");
    // A resync with nothing changed touches nothing.
    assert!(idx.resync(4).unwrap().is_empty());
}
