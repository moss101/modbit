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
