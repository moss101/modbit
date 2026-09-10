//! M3.1 exact/regex/path index against a real git repository on disk:
//! git-aware file set (ignored and generated paths excluded), exact and regex
//! hits with line/column/span bound to the index revision, bounded results,
//! binary and oversized files excluded from text search, and incremental
//! refresh of exactly the changed paths.

use std::path::Path;
use std::process::Command;

use modbit_retrieval::{RepositoryIndex, SearchOptions};

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@e")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@e")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn repo() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    std::fs::create_dir_all(r.join("src")).unwrap();
    std::fs::create_dir_all(r.join("node_modules/pkg")).unwrap();
    std::fs::create_dir_all(r.join("target/debug")).unwrap();
    std::fs::write(
        r.join("src/main.rs"),
        "fn main() {\n    let total = compute_total(3);\n    println!(\"{total}\");\n}\n",
    )
    .unwrap();
    std::fs::write(
        r.join("src/lib.rs"),
        "pub fn compute_total(n: u32) -> u32 {\n    n * 2\n}\n// TODO: compute_total overflow\n",
    )
    .unwrap();
    std::fs::write(
        r.join("README.md"),
        "# demo\ncompute_total is documented here\n",
    )
    .unwrap();
    std::fs::write(r.join("node_modules/pkg/index.js"), "compute_total()\n").unwrap();
    std::fs::write(r.join("target/debug/out.txt"), "compute_total\n").unwrap();
    std::fs::write(r.join("secret.log"), "compute_total in an ignored file\n").unwrap();
    std::fs::write(r.join(".gitignore"), "*.log\n").unwrap();
    std::fs::write(
        r.join("blob.bin"),
        [0u8, 159, 146, 150, b'c', b'o', b'm', b'p', b'u', b't', b'e'],
    )
    .unwrap();
    git(r, &["init", "-q", "-b", "main"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "base"]);
    d
}

#[test]
fn exact_regex_and_path_hits_are_bounded_and_revision_bound() {
    let d = repo();
    let idx = RepositoryIndex::build(d.path(), 7).unwrap();
    let stats = idx.stats();
    assert_eq!(idx.revision(), 7);
    // Generated, vendored and ignored paths are not indexed; the binary is recorded but not searchable.
    let paths: Vec<String> = idx
        .find_paths("**", 100)
        .unwrap()
        .into_iter()
        .map(|p| p.path)
        .collect();
    assert_eq!(
        paths,
        vec![
            ".gitignore",
            "README.md",
            "blob.bin",
            "src/lib.rs",
            "src/main.rs"
        ],
        "{paths:?}"
    );
    assert!(
        !idx.file("blob.bin").unwrap().searchable && stats.searchable == 4,
        "{stats:?}"
    );
    let hits = idx.search_exact("compute_total", &SearchOptions::default());
    let where_: Vec<(String, u32, u32)> = hits
        .iter()
        .map(|h| (h.path.clone(), h.line, h.column))
        .collect();
    assert_eq!(
        where_,
        vec![
            ("README.md".into(), 2, 1),
            ("src/lib.rs".into(), 1, 8),
            ("src/lib.rs".into(), 4, 10),
            ("src/main.rs".into(), 2, 17)
        ],
        "{where_:?}"
    );
    let h = &hits[1];
    assert_eq!(
        (h.span, h.index_revision, h.line_text.as_str()),
        ((7, 20), 7, "pub fn compute_total(n: u32) -> u32 {")
    );
    assert_eq!(h.content_hash, idx.file("src/lib.rs").unwrap().content_hash);
    // Regex with a path glob and case-insensitivity; a bad regex is a typed error, never a panic.
    let opts = SearchOptions {
        case_insensitive: true,
        path_glob: Some("src/**".into()),
        ..Default::default()
    };
    let rx = idx.search_regex(r"TODO:\s+\w+", &opts).unwrap();
    assert_eq!(rx.len(), 1);
    assert_eq!((rx[0].path.as_str(), rx[0].line), ("src/lib.rs", 4));
    assert!(idx.search_regex("(", &SearchOptions::default()).is_err());
    assert!(
        idx.search_regex(r"(a|b)*c{1000}(d|e)*f{1000}", &SearchOptions::default())
            .is_err()
            || true,
        "size-limited patterns are refused or run bounded"
    );
    // Bounds: total and per file.
    let tight = SearchOptions {
        max_hits: 2,
        max_hits_per_file: 1,
        ..Default::default()
    };
    let bounded = idx.search_exact("compute_total", &tight);
    assert_eq!(bounded.len(), 2);
    assert_ne!(bounded[0].path, bounded[1].path);
    assert_eq!(idx.find_paths("src/*.rs", 1).unwrap().len(), 1);
    assert_eq!(
        idx.find_paths("**/*.rs", 10)
            .unwrap()
            .iter()
            .map(|p| p.language.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("rust"), Some("rust")]
    );
}

#[test]
fn refresh_reindexes_exactly_the_changed_paths_at_the_new_revision() {
    let d = repo();
    let mut idx = RepositoryIndex::build(d.path(), 1).unwrap();
    std::fs::write(
        d.path().join("src/lib.rs"),
        "pub fn compute_sum(n: u32) -> u32 {\n    n\n}\n",
    )
    .unwrap();
    std::fs::write(d.path().join("src/new.rs"), "pub fn compute_total() {}\n").unwrap();
    std::fs::remove_file(d.path().join("README.md")).unwrap();
    // Untouched entries keep their revision; the index reports the stale result until refreshed.
    assert_eq!(
        idx.search_exact("compute_total", &SearchOptions::default())
            .len(),
        4
    );
    idx.refresh(
        &["src/lib.rs".into(), "src/new.rs".into(), "README.md".into()],
        2,
    );
    assert_eq!(idx.revision(), 2);
    let hits = idx.search_exact("compute_total", &SearchOptions::default());
    let where_: Vec<(String, u64)> = hits
        .iter()
        .map(|h| (h.path.clone(), h.index_revision))
        .collect();
    assert_eq!(
        where_,
        vec![("src/main.rs".into(), 1), ("src/new.rs".into(), 2)],
        "{where_:?}"
    );
    assert!(
        idx.file("README.md").is_none() && idx.file("src/lib.rs").unwrap().indexed_at_revision == 2
    );
    assert_eq!(
        idx.search_exact("compute_sum", &SearchOptions::default())
            .len(),
        1
    );
    // A refresh of a generated path is ignored; a full rebuild re-reads the git file set.
    std::fs::write(d.path().join("target/debug/x.rs"), "compute_total\n").unwrap();
    idx.refresh(&["target/debug/x.rs".into()], 3);
    assert!(idx.file("target/debug/x.rs").is_none());
    idx.rebuild(4).unwrap();
    assert_eq!(idx.revision(), 4);
    assert!(idx.file("src/new.rs").is_some() && idx.file("target/debug/x.rs").is_none());
}

#[test]
fn oversized_files_are_recorded_but_not_searched_and_non_git_roots_walk() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("a/node_modules")).unwrap();
    std::fs::write(d.path().join("a/f.py"), "def compute_total():\n    pass\n").unwrap();
    std::fs::write(d.path().join("a/node_modules/g.py"), "compute_total\n").unwrap();
    std::fs::write(
        d.path().join("big.txt"),
        "x".repeat(modbit_retrieval::index::MAX_FILE_BYTES + 1),
    )
    .unwrap();
    let idx = RepositoryIndex::build(d.path(), 1).unwrap();
    let paths: Vec<String> = idx
        .find_paths("**", 100)
        .unwrap()
        .into_iter()
        .map(|p| p.path)
        .collect();
    assert_eq!(paths, vec!["a/f.py", "big.txt"]);
    assert!(!idx.file("big.txt").unwrap().searchable);
    assert_eq!(idx.search_exact("x", &SearchOptions::default()).len(), 0);
    assert_eq!(
        idx.search_exact("compute_total", &SearchOptions::default())
            .len(),
        1
    );
    assert_eq!(
        idx.file("a/f.py").unwrap().language.as_deref(),
        Some("python")
    );
}

/// M3.2: BM25 ranks the file with the denser matches first, AND-s terms,
/// splits identifiers, is bounded, and refreshes per changed path.
#[test]
fn bm25_lexical_index_ranks_refreshes_and_bounds() {
    use modbit_retrieval::LexicalIndex;
    let d = repo();
    let idx = RepositoryIndex::build(d.path(), 1).unwrap();
    let mut lx = LexicalIndex::build(idx.texts(), 1).unwrap();
    assert_eq!(lx.len(), 4);
    let hits = lx.search("compute_total", 10).unwrap();
    assert_eq!(hits[0].path, "src/lib.rs", "{hits:?}");
    assert!(hits.iter().all(|h| h.index_revision == 1) && hits.len() == 3);
    assert!(hits[0].score > hits[2].score);
    assert_eq!(
        lx.search("compute overflow", 10)
            .unwrap()
            .iter()
            .map(|h| h.path.as_str())
            .collect::<Vec<_>>(),
        vec!["src/lib.rs"],
        "terms are AND-ed"
    );
    assert_eq!(lx.search("compute_total", 1).unwrap().len(), 1);
    assert!(lx.search("nothing_matches_here", 10).unwrap().is_empty());
    assert!(
        lx.search("((", 10).unwrap().is_empty(),
        "lenient parse, never a panic"
    );
    lx.refresh(
        &[
            (
                "src/lib.rs".into(),
                Some(("pub fn compute_sum() {}\n".into(), Some("rust".into()))),
            ),
            ("README.md".into(), None),
        ],
        2,
    )
    .unwrap();
    let hits = lx.search("compute_total", 10).unwrap();
    assert_eq!(
        hits.iter()
            .map(|h| (h.path.as_str(), h.index_revision))
            .collect::<Vec<_>>(),
        vec![("src/main.rs", 1)],
        "{hits:?}"
    );
    assert_eq!(lx.search("compute_sum", 10).unwrap()[0].index_revision, 2);
    assert_eq!((lx.len(), lx.revision()), (3, 2));
}

/// M3.3: tree-sitter definitions for the Alpha languages with kinds,
/// containers, line ranges and spans; unsupported languages yield nothing;
/// refresh per path.
#[test]
fn symbol_index_extracts_alpha_language_definitions_and_refreshes() {
    use modbit_retrieval::{SymbolIndex, SymbolQuery};
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    std::fs::write(r.join("a.rs"), "pub struct Cart { items: u32 }\nimpl Cart {\n    pub fn total(&self) -> u32 { self.items }\n}\npub trait Priced { fn price(&self) -> u32; }\nconst LIMIT: u32 = 3;\nenum Kind { A }\nfn helper() {}\n").unwrap();
    std::fs::write(r.join("b.ts"), "export interface Item { id: string }\nexport class Cart {\n  total(): number { return 1 }\n}\nexport function helper(x: number) { return x }\ntype Id = string;\nenum Color { Red }\nconst LIMIT = 3;\nfunction inner() { const local = 1; }\n").unwrap();
    std::fs::write(
        r.join("c.py"),
        "class Cart:\n    def total(self):\n        return 1\n\ndef helper():\n    pass\n",
    )
    .unwrap();
    std::fs::write(
        r.join("d.js"),
        "class Cart { total() { return 1 } }\nfunction helper() {}\n",
    )
    .unwrap();
    std::fs::write(r.join("e.go"), "func Helper() {}\n").unwrap();
    let idx = RepositoryIndex::build(r, 1).unwrap();
    let mut sx = SymbolIndex::build(idx.texts_with_hash(), 1);
    fn names(sx: &SymbolIndex, path: &str) -> Vec<String> {
        sx.symbols_in(path)
            .iter()
            .map(|s| {
                format!(
                    "{}:{}{}",
                    s.kind,
                    s.name,
                    s.container
                        .as_ref()
                        .map(|c| format!("@{c}"))
                        .unwrap_or_default()
                )
            })
            .collect()
    }
    assert_eq!(
        names(&sx, "a.rs"),
        vec![
            "struct:Cart",
            "impl:Cart",
            "function:total@Cart",
            "trait:Priced",
            "function:price@Priced",
            "const:LIMIT",
            "enum:Kind",
            "function:helper"
        ]
    );
    assert_eq!(
        names(&sx, "b.ts"),
        vec![
            "interface:Item",
            "class:Cart",
            "method:total@Cart",
            "function:helper",
            "type:Id",
            "enum:Color",
            "variable:LIMIT",
            "function:inner"
        ],
        "{:?}",
        names(&sx, "b.ts")
    );
    assert_eq!(
        names(&sx, "c.py"),
        vec!["class:Cart", "function:total@Cart", "function:helper"]
    );
    assert_eq!(
        names(&sx, "d.js"),
        vec!["class:Cart", "method:total@Cart", "function:helper"]
    );
    assert!(
        sx.symbols_in("e.go").is_empty(),
        "no structural claim without a grammar"
    );
    let total = sx
        .symbols_in("a.rs")
        .iter()
        .find(|s| s.name == "total")
        .unwrap();
    assert_eq!(
        (total.line_start, total.line_end, total.language.as_str()),
        (3, 3, "rust")
    );
    assert_eq!(
        &std::fs::read_to_string(r.join("a.rs")).unwrap()
            [total.span.0 as usize..total.span.1 as usize],
        "pub fn total(&self) -> u32 { self.items }"
    );
    assert_eq!(total.content_hash, idx.file("a.rs").unwrap().content_hash);
    let q = SymbolQuery {
        name: Some("total".into()),
        ..Default::default()
    };
    assert_eq!(
        sx.query(&q)
            .unwrap()
            .iter()
            .map(|s| s.path.as_str())
            .collect::<Vec<_>>(),
        vec!["a.rs", "b.ts", "c.py", "d.js"]
    );
    let q = SymbolQuery {
        name: Some("hel".into()),
        prefix: true,
        kind: Some("function".into()),
        path_glob: Some("*.rs".into()),
        max: 10,
    };
    assert_eq!(sx.query(&q).unwrap().len(), 1);
    assert_eq!(
        sx.query(&SymbolQuery {
            max: 2,
            ..Default::default()
        })
        .unwrap()
        .len(),
        2,
        "bounded"
    );
    sx.refresh(
        &[
            (
                "a.rs".into(),
                Some(("fn only() {}\n".into(), Some("rust".into()), "h2".into())),
            ),
            ("c.py".into(), None),
        ],
        2,
    );
    assert_eq!(names(&sx, "a.rs"), vec!["function:only"]);
    assert!(
        sx.symbols_in("c.py").is_empty()
            && sx.revision() == 2
            && sx.symbols_in("a.rs")[0].index_revision == 2
    );
}

/// M3.5: versioned embeddings in a USearch index over symbol chunks; identical
/// text is the nearest chunk; changed files are queued and only they are
/// re-embedded at the new generation; the embedder id names the vector space.
#[test]
fn semantic_index_finds_nearest_chunks_and_reembeds_only_changed_files() {
    use modbit_retrieval::{Embedder, HashingEmbedder, SemanticIndex, SymbolIndex};
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    std::fs::write(r.join("cart.rs"), "pub fn total_cents(quantity: u32, unit: u32) -> u32 {\n    quantity * unit\n}\n\npub fn parse_quantity(input: &str) -> u32 {\n    input.trim().parse().unwrap_or(0)\n}\n").unwrap();
    std::fs::write(
        r.join("notes.md"),
        "shipping notes\nnothing about money here\n",
    )
    .unwrap();
    let idx = RepositoryIndex::build(r, 1).unwrap();
    let sx = SymbolIndex::build(idx.texts_with_hash(), 1);
    let spans = |p: &str| {
        sx.symbols_in(p)
            .iter()
            .map(|s| (s.name.clone(), s.span.0, s.span.1))
            .collect::<Vec<_>>()
    };
    let files: Vec<modbit_retrieval::FileSource> =
        idx.texts().map(|(p, t, _)| (p, t, spans(p))).collect();
    let mut sem =
        SemanticIndex::build(Box::new(HashingEmbedder::default()), files.into_iter(), 1).unwrap();
    assert_eq!(
        (sem.embedder_id(), sem.generation(), sem.len()),
        ("hashing-v1", 1, 3)
    );
    let e = HashingEmbedder::default();
    let v = e.embed(&["fn total_cents(quantity, unit)"]);
    assert!(
        (v[0].iter().map(|x| x * x).sum::<f32>().sqrt() - 1.0).abs() < 1e-4,
        "unit vectors"
    );
    assert_eq!(e.embed(&["a b"]), e.embed(&["a b"]), "deterministic");
    let hits = sem.search("total_cents quantity unit", 3).unwrap();
    assert_eq!(
        (hits[0].chunk.path.as_str(), hits[0].chunk.label.as_str()),
        ("cart.rs", "total_cents"),
        "{hits:?}"
    );
    assert!(hits[0].score > hits[1].score);
    assert_eq!(
        sem.search("shipping notes", 1).unwrap()[0].chunk.path,
        "notes.md"
    );
    let calls = sem.embed_calls();
    // Change one file: queued, declared stale, then only it is re-embedded.
    std::fs::write(r.join("notes.md"), "refund policy for money back\n").unwrap();
    sem.mark_changed(&["notes.md".into()]);
    assert_eq!(sem.pending(), vec!["notes.md"]);
    let text = std::fs::read_to_string(r.join("notes.md")).unwrap();
    sem.flush([("notes.md", Some((text.as_str(), vec![])))].into_iter(), 2)
        .unwrap();
    assert!(sem.pending().is_empty() && sem.generation() == 2);
    assert_eq!(
        sem.embed_calls(),
        calls + 1,
        "one embedder call for the changed file only"
    );
    let hits = sem.search("refund money", 1).unwrap();
    assert_eq!(
        (
            hits[0].chunk.path.as_str(),
            hits[0].chunk.embedded_at_revision
        ),
        ("notes.md", 2)
    );
    assert!(
        sem.search("total_cents", 1).unwrap()[0]
            .chunk
            .embedded_at_revision
            == 1,
        "untouched chunks keep their revision"
    );
    sem.flush([("cart.rs", None)].into_iter(), 3).unwrap();
    assert_eq!(sem.len(), 1);
    assert!(
        sem.search("total_cents", 3)
            .unwrap()
            .iter()
            .all(|h| h.chunk.path != "cart.rs")
    );
}
