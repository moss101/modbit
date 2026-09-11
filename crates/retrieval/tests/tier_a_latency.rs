//! PX-028 / QUAL-PX-028: incremental index latency on the Tier A fixtures.
//!
//! The budget is stated here because docs/76 asks for "incremental index
//! latency within budget" without a number: after one file changes, the
//! exact, lexical, symbol and semantic indexes of a fixture repository are
//! all current again within `BUDGET_MS`, measured as the p95 over repeated
//! single-file changes on each fixture, on the hardware the evidence names.
//! Forty refreshes per fixture: on a shared CI runner a scheduling hiccup
//! stretches one sample by hundreds of milliseconds, and a p95 over a dozen
//! samples is the maximum; over forty it tolerates two such outliers while
//! still failing on a slow refresh path.

use std::path::{Path, PathBuf};
use std::time::Instant;

use modbit_retrieval::{
    HashingEmbedder, LexicalIndex, RepositoryIndex, SemanticIndex, SymbolIndex,
    lexical::ChangedDoc, symbols::ChangedSymbols,
};

/// The budget for one incremental refresh across all four indexes.
const BUDGET_MS: u128 = 250;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/repos")
        .join(name)
        .canonicalize()
        .expect("fixture")
}

/// Copy a fixture into a fresh git repository, so the index sees a real
/// worktree and the fixture stays untouched.
fn checkout(name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let src = fixture(name);
    let mut stack = vec![src.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            let rel = p.strip_prefix(&src).unwrap();
            if rel.starts_with("target")
                || rel.starts_with("node_modules")
                || rel.starts_with(".git")
            {
                continue;
            }
            if p.is_dir() {
                std::fs::create_dir_all(dir.path().join(rel)).unwrap();
                stack.push(p);
            } else {
                std::fs::copy(&p, dir.path().join(rel)).unwrap();
            }
        }
    }
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-q",
            "-m",
            "fixture",
        ],
    ] {
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(&args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }
    dir
}

fn spans(sx: &SymbolIndex, p: &str) -> Vec<(String, u64, u64)> {
    sx.symbols_in(p)
        .iter()
        .map(|s| (s.name.clone(), s.span.0, s.span.1))
        .collect()
}

/// One incremental refresh of every index after `path` changed; returns the
/// wall time of the whole refresh.
fn refresh_all(
    root: &Path,
    idx: &mut RepositoryIndex,
    lx: &mut LexicalIndex,
    sx: &mut SymbolIndex,
    sem: &mut SemanticIndex,
    path: &str,
    revision: u64,
) -> u128 {
    let started = Instant::now();
    idx.refresh(&[path.to_owned()], revision);
    let (text, language, hash) = {
        let f = idx.file(path).expect("changed file is indexed");
        let text = idx
            .texts()
            .find(|(p, _, _)| *p == path)
            .map(|(_, t, _)| t.to_owned())
            .unwrap_or_default();
        (text, f.language.clone(), f.content_hash.clone())
    };
    let doc: ChangedDoc = (path.to_owned(), Some((text.clone(), language.clone())));
    lx.refresh(&[doc], revision).unwrap();
    let syms: ChangedSymbols = (path.to_owned(), Some((text.clone(), language, hash)));
    sx.refresh(&[syms], revision);
    sem.mark_changed(&[path.to_owned()]);
    let s = spans(sx, path);
    sem.flush(std::iter::once((path, Some((text.as_str(), s)))), revision)
        .unwrap();
    let _ = root;
    started.elapsed().as_millis()
}

#[test]
fn incremental_index_latency_is_within_budget_on_the_tier_a_fixtures() {
    let cases = [
        ("rust-cli", "src/lib.rs"),
        ("python-service", "service.py"),
        ("ts-webapp", "src/cart.ts"),
    ];
    let mut report = Vec::new();
    for (name, changed) in cases {
        let dir = checkout(name);
        let root = dir.path();
        let mut idx = RepositoryIndex::build(root, 1).unwrap();
        let mut lx = LexicalIndex::build(idx.texts(), 1).unwrap();
        let mut sx = SymbolIndex::build(idx.texts_with_hash(), 1);
        let files: Vec<modbit_retrieval::FileSource> =
            idx.texts().map(|(p, t, _)| (p, t, spans(&sx, p))).collect();
        let mut sem =
            SemanticIndex::build(Box::new(HashingEmbedder::default()), files.into_iter(), 1)
                .unwrap();
        let original = std::fs::read_to_string(root.join(changed)).unwrap();
        let mut samples = Vec::new();
        for i in 0..40u64 {
            // A real edit each time: a distinct comment line at the end.
            std::fs::write(
                root.join(changed),
                format!("{original}\n// incremental change {i}\n"),
            )
            .unwrap();
            samples.push(refresh_all(
                root,
                &mut idx,
                &mut lx,
                &mut sx,
                &mut sem,
                changed,
                2 + i,
            ));
            // Every index is current: the exact index sees the new line, the
            // lexical index finds it, the symbol index still has the file's
            // symbols, and the semantic index has nothing pending.
            assert!(
                idx.texts()
                    .find(|(p, _, _)| *p == changed)
                    .is_some_and(|(_, t, _)| t.contains(&format!("incremental change {i}"))),
                "{name}: exact index is stale after refresh {i}"
            );
            assert_eq!(idx.revision(), 2 + i);
            assert!(
                sem.pending().is_empty(),
                "{name}: semantic index has pending paths"
            );
            assert!(
                !sx.symbols_in(changed).is_empty(),
                "{name}: symbols vanished on refresh"
            );
        }
        samples.sort_unstable();
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        report.push(format!(
            "{name}: p50={p50}ms p95={p95}ms over {} refreshes",
            samples.len()
        ));
        assert!(
            p95 <= BUDGET_MS,
            "{name}: incremental refresh p95 {p95}ms exceeds the {BUDGET_MS}ms budget ({samples:?})"
        );
    }
    println!(
        "incremental index latency ({BUDGET_MS}ms budget): {}",
        report.join("; ")
    );
}
