//! FIX-12 (audit N4, N5, N6): natural-language queries find files, lexical
//! hits name the region that matched, and the methods that found one region
//! reinforce each other instead of surfacing as separate hits. The corpus is
//! a real Git repository on disk, searched through the real indexes and the
//! real planner.
use std::collections::BTreeMap;

use modbit_retrieval::planner::retrieve;
use modbit_retrieval::{
    EvidenceGraph, HashingEmbedder, LexicalIndex, PlanRequest, RepositoryIndex, SemanticIndex,
    Sources, SymbolIndex,
};

fn git(dir: &std::path::Path, args: &[&str]) {
    assert!(
        std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .status()
            .unwrap()
            .success()
    );
}

/// A pricing module long enough that its text spans several lexical chunks;
/// the refund ceiling is enforced near line 200.
fn pricing_source() -> String {
    let mut s = String::from("//! Pricing rules.\n\n");
    for i in 0..200 {
        s.push_str(&format!(
            "/// Filler rule {i}.\npub fn filler_rule_{i}() -> u32 {{ {i} }}\n"
        ));
    }
    s.push_str("/// Clamp a refund to the ceiling set for returned orders.\n");
    s.push_str("pub fn clamp_refund(requested: u32, ceiling: u32) -> u32 {\n");
    s.push_str("    requested.min(ceiling)\n}\n");
    for i in 0..60 {
        s.push_str(&format!(
            "/// Trailing rule {i}.\npub fn trailing_rule_{i}() -> u32 {{ {i} }}\n"
        ));
    }
    s
}

fn repo() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    std::fs::create_dir_all(r.join("src")).unwrap();
    std::fs::write(
        r.join("src/cart.rs"),
        "//! The shopping cart.\npub struct Cart {\n    items: Vec<u32>,\n}\n\nimpl Cart {\n    /// Compute the total of the cart in cents.\n    pub fn compute_total(&self) -> u32 {\n        self.items.iter().sum()\n    }\n}\n",
    )
    .unwrap();
    std::fs::write(
        r.join("src/orders.rs"),
        "//! Orders.\n/// The order total is calculated from the quantity and the unit price.\npub fn order_total(quantity: u32, unit_cents: u32) -> u32 {\n    quantity * unit_cents\n}\n",
    )
    .unwrap();
    std::fs::write(r.join("src/pricing.rs"), pricing_source()).unwrap();
    std::fs::write(
        r.join("src/logging.rs"),
        "//! Logging setup for the service.\npub fn init_logging() {}\n",
    )
    .unwrap();
    std::fs::write(
        r.join("README.md"),
        "# shop\nA small shop service with a cart, orders and pricing rules.\n",
    )
    .unwrap();
    git(r, &["init", "-q", "-b", "main"]);
    git(r, &["add", "-A"]);
    git(
        r,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e",
            "commit",
            "-q",
            "-m",
            "base",
        ],
    );
    d
}

struct Built {
    idx: RepositoryIndex,
    lx: LexicalIndex,
    sx: SymbolIndex,
    sem: SemanticIndex,
    graph: EvidenceGraph,
}

fn build(dir: &std::path::Path) -> Built {
    let idx = RepositoryIndex::build(dir, 1).unwrap();
    let lx = LexicalIndex::build(idx.texts(), 1).unwrap();
    let sx = SymbolIndex::build(idx.texts_with_hash(), 1);
    let spans = |p: &str| {
        sx.symbols_in(p)
            .iter()
            .filter(|s| s.container.is_none())
            .map(|s| (s.name.clone(), s.span.0, s.span.1))
            .collect::<Vec<_>>()
    };
    let files: Vec<modbit_retrieval::FileSource> =
        idx.texts().map(|(p, t, _)| (p, t, spans(p))).collect();
    let sem =
        SemanticIndex::build(Box::new(HashingEmbedder::default()), files.into_iter(), 1).unwrap();
    let graph = EvidenceGraph::build(idx.texts(), vec![], BTreeMap::new(), 1);
    Built {
        idx,
        lx,
        sx,
        sem,
        graph,
    }
}

fn plan(b: &Built, q: &str) -> modbit_retrieval::PlanResult {
    retrieve(
        &Sources {
            index: &b.idx,
            lexical: &b.lx,
            symbols: &b.sx,
            semantic: &b.sem,
            graph: &b.graph,
        },
        &PlanRequest {
            query: q.into(),
            max_hits: 20,
            ..PlanRequest::default()
        },
    )
}

/// N4: a question is not an AND over every word in it.
#[test]
fn natural_language_questions_return_hits_instead_of_nothing() {
    let d = repo();
    let b = build(d.path());
    for (q, want) in [
        ("where do we compute the total of the cart", "src/cart.rs"),
        (
            "how is the order total calculated from quantity",
            "src/orders.rs",
        ),
        ("cart total quantity", "src/cart.rs"),
        (
            "where is the refund ceiling enforced for returned orders",
            "src/pricing.rs",
        ),
    ] {
        let hits = b.lx.search(q, 10).unwrap();
        assert!(!hits.is_empty(), "no lexical hit for {q:?}");
        assert!(
            hits.iter().any(|h| h.path == want),
            "{q:?} did not reach {want}: {hits:?}"
        );
    }
    // The planner answers it too (L1 runs BM25 over the same words).
    let r = plan(&b, "where do we compute the total of the cart");
    assert!(
        r.hits.iter().any(|h| h.path == "src/cart.rs"),
        "{:?}",
        r.hits
    );
}

/// The chunk with the matching region is what a lexical hit names (N6): the
/// refund ceiling sits near line 405 of a ~520 line file, so the head of the
/// file is not the answer.
#[test]
fn a_lexical_hit_names_the_region_that_matched() {
    let d = repo();
    let b = build(d.path());
    let text = std::fs::read_to_string(d.path().join("src/pricing.rs")).unwrap();
    let line = text
        .lines()
        .position(|l| l.contains("pub fn clamp_refund"))
        .unwrap()
        + 1;
    assert!(line > 100, "the fixture must put the match deep: {line}");
    let r = plan(
        &b,
        "where is the refund ceiling enforced for returned orders",
    );
    let hit = r
        .hits
        .iter()
        .find(|h| h.path == "src/pricing.rs")
        .unwrap_or_else(|| panic!("{:?}", r.hits));
    let (a, z) = hit
        .lines
        .unwrap_or_else(|| panic!("no line range: {hit:?}"));
    assert!(
        a as usize <= line && line <= z as usize,
        "{hit:?} does not cover line {line}"
    );
    assert!(z - a < 120, "the region is a chunk, not the file: {hit:?}");
}

/// N5: lexical, exact and semantic evidence for the same region are one hit
/// whose score is their sum, and each method's evidence is kept.
#[test]
fn methods_that_found_one_region_reinforce_one_hit() {
    let d = repo();
    let b = build(d.path());
    let r = plan(
        &b,
        "where do we compute the total of the cart with compute_total",
    );
    let for_cart: Vec<&modbit_retrieval::FusedHit> =
        r.hits.iter().filter(|h| h.path == "src/cart.rs").collect();
    assert_eq!(
        for_cart.len(),
        1,
        "one file, one region, one hit: {for_cart:?}"
    );
    let hit = for_cart[0];
    for source in ["lexical", "semantic", "exact"] {
        assert!(
            hit.sources.iter().any(|s| s == source),
            "{source} missing: {hit:?}"
        );
    }
    // Per-method evidence stays for provenance.
    for source in ["lexical", "semantic", "exact"] {
        let e = hit
            .evidence
            .iter()
            .find(|e| e.source == source)
            .unwrap_or_else(|| panic!("no {source} evidence: {hit:?}"));
        assert!(e.rrf > 0.0, "{e:?}");
    }
    let best_single = hit.evidence.iter().map(|e| e.rrf).fold(0.0_f32, f32::max);
    assert!(
        hit.score > best_single,
        "reinforcement must beat any single method: {hit:?}"
    );
}

/// Regions that are far apart in one file are different evidence: they stay
/// separate hits, each with its own lines.
#[test]
fn distant_regions_of_one_file_stay_separate_hits() {
    let d = repo();
    let b = build(d.path());
    let r = plan(&b, "filler_rule_3 and trailing_rule_5");
    let regions: Vec<&modbit_retrieval::FusedHit> = r
        .hits
        .iter()
        .filter(|h| h.path == "src/pricing.rs" && h.sources.iter().any(|s| s == "exact"))
        .collect();
    assert!(regions.len() >= 2, "{:?}", r.hits);
    for pair in regions.windows(2) {
        let (a, z) = (pair[0].lines.unwrap(), pair[1].lines.unwrap());
        assert!(a.1 < z.0 || z.1 < a.0, "regions overlap: {a:?} {z:?}");
    }
}

#[test]
fn files_are_cut_into_overlapping_line_windows_that_cover_every_line() {
    use modbit_retrieval::lexical::{CHUNK_LINES, CHUNK_STRIDE, line_chunks};
    let small = "one\ntwo\nthree\n";
    let c = line_chunks(small);
    assert_eq!(c.len(), 1);
    assert_eq!((c[0].lines, c[0].span), ((1, 3), (0, small.len() as u64)));
    assert_eq!(line_chunks("")[0].lines, (1, 1));
    let text: String = (1..=100).map(|i| format!("line {i}\n")).collect();
    let chunks = line_chunks(&text);
    assert_eq!(chunks[0].lines, (1, CHUNK_LINES as u32));
    assert_eq!(chunks[1].lines.0, CHUNK_STRIDE as u32 + 1);
    assert_eq!(chunks.last().unwrap().lines.1, 100);
    for c in &chunks {
        let (a, z) = (c.span.0 as usize, c.span.1 as usize);
        assert_eq!(&text[a..z], c.text);
        assert_eq!(c.text.lines().count() as u32, c.lines.1 - c.lines.0 + 1);
    }
    for line in 1..=100u32 {
        assert!(
            chunks
                .iter()
                .any(|c| c.lines.0 <= line && line <= c.lines.1),
            "line {line} is in no chunk"
        );
    }
}

#[test]
fn a_question_ranks_the_chunk_holding_every_term_first_and_keywords_stay_precise() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    std::fs::write(
        r.join("all.txt"),
        "the invoice rounding policy applies banker rounding to each invoice line\n",
    )
    .unwrap();
    std::fs::write(r.join("some.txt"), "invoice rounding is sequential\n").unwrap();
    std::fs::write(r.join("one.txt"), "policy documents live elsewhere\n").unwrap();
    std::fs::write(r.join("none.txt"), "unrelated gardening notes\n").unwrap();
    let idx = RepositoryIndex::build(r, 1).unwrap();
    let lx = LexicalIndex::build(idx.texts(), 1).unwrap();
    // A question: the file with every content term is first, partial matches follow.
    let hits = lx
        .search("which rounding policy applies to the invoice", 10)
        .unwrap();
    let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(paths[0], "all.txt", "{hits:?}");
    assert!(paths.contains(&"some.txt"), "two of four terms: {paths:?}");
    assert!(
        !paths.contains(&"one.txt") && !paths.contains(&"none.txt"),
        "one term of four is below the floor: {paths:?}"
    );
    // A short keyword query is conjunctive when something satisfies it.
    let hits = lx.search("invoice policy", 10).unwrap();
    assert_eq!(
        hits.iter().map(|h| h.path.as_str()).collect::<Vec<_>>(),
        vec!["all.txt"],
        "{hits:?}"
    );
    // ... and falls back to the disjunction when nothing does.
    let hits = lx.search("sequential policy", 10).unwrap();
    assert!(hits.len() >= 2, "{hits:?}");
    // Explicit syntax is the caller's: a quoted phrase is a phrase.
    let hits = lx.search("\"rounding policy\"", 10).unwrap();
    assert_eq!(hits.len(), 1, "{hits:?}");
    // Only stopwords still asks for something, and never panics.
    assert!(lx.search("the of and", 10).is_ok());
    assert!(lx.search("   ", 10).unwrap().is_empty());
}

#[test]
fn refresh_replaces_every_chunk_of_a_path() {
    let long: String = (1..=120).map(|i| format!("zebra marker {i}\n")).collect();
    let mut lx = LexicalIndex::build(std::iter::once(("big.txt", long.as_str(), None)), 1).unwrap();
    assert_eq!(lx.search("zebra marker", 5).unwrap().len(), 1);
    lx.refresh(&[("big.txt".into(), Some(("giraffe\n".into(), None)))], 2)
        .unwrap();
    assert!(
        lx.search("zebra marker", 5).unwrap().is_empty(),
        "no stale chunk"
    );
    let hits = lx.search("giraffe", 5).unwrap();
    assert_eq!(
        (hits.len(), hits[0].lines, hits[0].index_revision),
        (1, None, 2)
    );
}

/// N9: a definition larger than one chunk is embedded in full — a large
/// `impl` method by method, a large member-less function in line-bounded
/// pieces — so a match past the first 4 KiB is found.
#[test]
fn large_definitions_are_semantically_indexed_in_full() {
    use modbit_retrieval::semantic::MAX_CHUNK_BYTES;
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    let mut big_impl = String::from("pub struct Big;\n\nimpl Big {\n");
    for i in 0..30 {
        big_impl.push_str(&format!(
            "    /// Method {i}.\n    pub fn method_{i}(&self) -> u32 {{\n"
        ));
        for j in 0..8 {
            big_impl.push_str(&format!(
                "        let value_{j} = {i} + {j}; // padding padding padding\n"
            ));
        }
        if i == 25 {
            big_impl.push_str("        let quarantine_zeppelin_ledger = 1;\n");
        }
        big_impl.push_str("        value_0\n    }\n");
    }
    big_impl.push_str("}\n");
    assert!(big_impl.len() > 3 * MAX_CHUNK_BYTES);
    let mut long_fn = String::from("pub fn long_function() {\n");
    for j in 0..200 {
        long_fn.push_str(&format!("    let step_{j} = {j}; // ordinary statement\n"));
    }
    long_fn.push_str("    let obsidian_telescope_manifest = 2;\n}\n");
    assert!(long_fn.len() > 2 * MAX_CHUNK_BYTES);
    std::fs::write(r.join("big.rs"), &big_impl).unwrap();
    std::fs::write(r.join("long.rs"), &long_fn).unwrap();
    let idx = RepositoryIndex::build(r, 1).unwrap();
    let sx = SymbolIndex::build(idx.texts_with_hash(), 1);
    let spans = sx.chunk_spans("big.rs", MAX_CHUNK_BYTES);
    let labels: Vec<&str> = spans.iter().map(|s| s.0.as_str()).collect();
    assert!(
        labels.contains(&"Big"),
        "the struct stays one chunk: {labels:?}"
    );
    assert!(labels.contains(&"Big::method_25"), "{labels:?}");
    assert_eq!(labels.iter().filter(|l| l.starts_with("Big::")).count(), 30);
    let files: Vec<modbit_retrieval::FileSource> = idx
        .texts()
        .map(|(p, t, _)| (p, t, sx.chunk_spans(p, MAX_CHUNK_BYTES)))
        .collect();
    let sem =
        SemanticIndex::build(Box::new(HashingEmbedder::default()), files.into_iter(), 1).unwrap();
    let line_of = |text: &str, needle: &str| {
        text.lines().position(|l| l.contains(needle)).unwrap() as u32 + 1
    };
    let hits = sem.search("quarantine zeppelin ledger", 3).unwrap();
    let top = &hits[0].chunk;
    let line = line_of(&big_impl, "quarantine_zeppelin_ledger");
    assert_eq!(top.path, "big.rs", "{hits:?}");
    assert!(
        top.lines.0 <= line && line <= top.lines.1,
        "the method holding the match is the chunk: {top:?} vs line {line}"
    );
    assert!(top.label.ends_with("method_25"), "{top:?}");
    // A function with no members is cut into pieces and the last one is searchable.
    // (The hashing embedder is coarse, so the piece must be among the top few.)
    let hits = sem.search("obsidian telescope manifest", 5).unwrap();
    let line = line_of(&long_fn, "obsidian_telescope_manifest");
    let found = hits
        .iter()
        .map(|h| &h.chunk)
        .find(|c| c.path == "long.rs" && c.lines.0 <= line && line <= c.lines.1);
    let found = found.unwrap_or_else(|| panic!("the last piece is not indexed: {hits:?}"));
    let (a, z) = (found.span.0 as usize, found.span.1 as usize);
    assert!(long_fn[a..z].contains("obsidian_telescope_manifest"));
}
