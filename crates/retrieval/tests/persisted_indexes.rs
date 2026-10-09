//! PX-111: the persisted index store, proved on a real corpus (a copy of five of this
//! workspace's crates): a warm start derives nothing and equals
//! a fresh build; an edit made while nothing was running is found by the
//! content-hash diff; damage — a flipped byte in the derived blob that still
//! parses, in the trigram blob, in the manifest, in the Tantivy files — is
//! detected, named and rebuilt around; and a process that dies inside a
//! checkpoint (really: the test binary re-runs itself as the writer and
//! aborts) leaves the previous snapshot whole or the new one whole, never
//! half of either.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use modbit_retrieval::indexset::{Derived, IndexSet, assemble_records, refresh_derived};
use modbit_retrieval::persist::{FileRecord, IndexStore};
use modbit_retrieval::{SearchOptions, SymbolIndex};

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let name = e.file_name();
        if name == "target" || name == "node_modules" || name == ".git" {
            continue;
        }
        let (p, dest) = (e.path(), to.join(&name));
        if p.is_dir() {
            copy_tree(&p, &dest);
        } else if e.metadata().is_ok_and(|m| m.len() < 400_000) {
            std::fs::copy(&p, &dest).unwrap();
        }
    }
}

/// A scratch workspace: a copy of `crates/`, and a store directory beside it.
struct Corpus {
    _dir: tempfile::TempDir,
    root: PathBuf,
    store_base: PathBuf,
}

fn corpus() -> Corpus {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("ws");
    // A real corpus of a size a debug build can cycle through many times.
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for krate in [
        "retrieval",
        "memory",
        "context",
        "compaction",
        "prompt-compiler",
    ] {
        copy_tree(&crates.join(krate), &root.join(krate));
    }
    let store_base = dir.path().join("indexes");
    Corpus {
        root: root.canonicalize().unwrap(),
        store_base,
        _dir: dir,
    }
}

impl Corpus {
    fn store(&self) -> IndexStore {
        IndexStore::open(&self.store_base, &self.root)
    }

    fn open(&self, revision: u64, with_store: bool) -> IndexSet {
        let store = self.store();
        IndexSet::open(
            &self.root,
            revision,
            with_store.then_some(&store),
            vec![],
            BTreeMap::new(),
        )
        .unwrap()
    }

    fn checkpoint(&self, set: &IndexSet, revision: u64) {
        let records = assemble_records(&set.exact, &set.symbols, &set.refs, &set.graph);
        self.store()
            .checkpoint(revision, &records, set.exact.trigram_index())
            .unwrap();
    }
}

fn state_of(set: &IndexSet, name: &str) -> String {
    set.status
        .components
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.state.clone())
        .unwrap_or_default()
}

/// Compare two answer documents and say where they first differ, briefly.
fn assert_same(got: &serde_json::Value, want: &serde_json::Value, label: &str) {
    if got == want {
        return;
    }
    let mut report = String::new();
    for section in ["symbols", "edges", "graph"] {
        let (g, w) = (&got[section], &want[section]);
        if let (Some(g), Some(w)) = (g.as_object(), w.as_object()) {
            for (path, wv) in w {
                if g.get(path) != Some(wv) {
                    report.push_str(&format!("{section} differs at {path}\n"));
                    if report.len() > 800 {
                        break;
                    }
                }
            }
            for path in g.keys().filter(|p| !w.contains_key(*p)) {
                report.push_str(&format!("{section} has extra {path}\n"));
            }
        }
    }
    for section in ["lexical", "exact", "unresolved"] {
        if got[section] != want[section] {
            report.push_str(&format!("{section} differs\n"));
        }
    }
    panic!("{label}: the answers differ\n{report}");
}

const QUERIES: &[&str] = &[
    "compute_total",
    "EvidenceGraph",
    "persisted index",
    "fn search_exact",
    "where is the symbol index refreshed",
    "RepositoryIndex build",
    "trigram",
];

/// Every answer the indexes give, as comparable data.
/// The exact index is usable before the derived ones exist (the start of a
/// run needs the file list and nothing else), and finishing the open gives
/// the indexes `IndexSet::open` gives.
#[test]
fn the_exact_index_opens_alone_and_the_rest_completes_to_the_same_set() {
    let c = corpus();
    let store = IndexStore::open(&c.store_base, &c.root);
    let whole = IndexSet::open(&c.root, 1, Some(&store), vec![], Default::default()).unwrap();
    let want = answers(&whole);
    let files = whole.exact.stats().files;
    drop(whole);
    // A fresh store beside it: the first phase touches no derived index and
    // writes nothing.
    let other = IndexStore::open(&c.store_base.join("two-phase"), &c.root);
    let started = std::time::Instant::now();
    let (exact, phase) = IndexSet::open_exact(&c.root, 1, Some(&other)).unwrap();
    let exact_ms = started.elapsed().as_millis();
    assert_eq!(exact.stats().files, files);
    assert!(
        !other.lexical_dir().exists(),
        "phase one must not build the lexical index"
    );
    let rest =
        IndexSet::open_rest(&exact, 1, Some(&other), vec![], Default::default(), phase).unwrap();
    for name in ["trigram", "symbols", "refs", "graph", "lexical"] {
        assert!(
            rest.status.components.iter().any(|x| x.name == name),
            "{name}: {:?}",
            rest.status.components
        );
    }
    let set = IndexSet {
        exact,
        lexical: rest.lexical,
        symbols: rest.symbols,
        graph: rest.graph,
        refs: rest.refs,
        status: rest.status,
    };
    assert_same(&answers(&set), &want, "two phases equal one");
    eprintln!(
        "PX-111 phase one over {files} files: {exact_ms} ms; the whole open {} ms",
        set.status.first_ready_ms
    );
}

/// A persisted lexical index that cannot be opened, refreshed or rebuilt in
/// place (here: a plain file sits where its directory should be; on Windows
/// the same happens when a file the index maps cannot be replaced) does not
/// refuse the open: the lexical index is built in memory from the same texts,
/// the reason is recorded, and the answers are the same.
#[test]
fn an_unusable_lexical_directory_falls_back_to_memory_and_says_why() {
    let c = corpus();
    let cold = c.open(1, true);
    let want = answers(&cold);
    c.checkpoint(&cold, 1);
    drop(cold);
    let store = c.store();
    let dir = store.lexical_dir();
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::write(&dir, b"not a directory").unwrap();
    let warm = c.open(1, true);
    assert_eq!(state_of(&warm, "lexical"), "rebuilt", "{:?}", warm.status);
    assert!(
        warm.status
            .rebuild_reasons
            .iter()
            .any(|r| r.starts_with("lexical: on-disk index unavailable")),
        "{:?}",
        warm.status.rebuild_reasons
    );
    assert_same(&answers(&warm), &want, "the in-memory lexical index");
}

fn answers(set: &IndexSet) -> serde_json::Value {
    let mut symbols: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    let mut edges: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    let mut graph: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for (path, ..) in set.exact.texts_with_hash() {
        symbols.insert(
            path.to_owned(),
            serde_json::to_value(
                set.symbols
                    .symbols_in(path)
                    .iter()
                    .map(|s| (&s.name, &s.kind, &s.container, s.span, s.line_start))
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
        );
        edges.insert(
            path.to_owned(),
            serde_json::to_value(
                set.refs
                    .edges_from(path)
                    .iter()
                    .map(|e| (e.kind, e.from_line, &e.to_path, &e.to_symbol, e.confidence))
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
        );
        let v = set.graph.query(&modbit_retrieval::GraphQuery {
            path: path.to_owned(),
            relation: "all".into(),
            depth: 2,
            max: 50,
        });
        graph.insert(
            path.to_owned(),
            serde_json::to_value((v.imports, v.importers, v.tests)).unwrap(),
        );
    }
    // BM25 scores depend on how the segments are laid out (a refreshed index
    // keeps deleted documents in its statistics until they merge away), so
    // the comparison is of what is found — the paths of the best hits — not of
    // the third decimal of a score.
    let lexical: Vec<serde_json::Value> = QUERIES
        .iter()
        .map(|q| {
            let mut paths: Vec<String> = set
                .lexical
                .search(q, 5)
                .unwrap()
                .iter()
                .map(|h| h.path.clone())
                .collect();
            paths.sort();
            serde_json::to_value(paths).unwrap()
        })
        .collect();
    // An exact hit carries the revision its file was indexed at, which is
    // history, not content.
    let exact: Vec<serde_json::Value> = QUERIES
        .iter()
        .map(|q| {
            serde_json::to_value(
                set.exact
                    .search_exact(q, &SearchOptions::default())
                    .iter()
                    .map(|h| {
                        (
                            &h.path,
                            h.line,
                            h.column,
                            h.span,
                            &h.line_text,
                            &h.content_hash,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap()
        })
        .collect();
    serde_json::json!({"symbols": symbols, "edges": edges, "graph": graph, "lexical": lexical, "exact": exact,
        "unresolved": set.refs.unresolved_count()})
}

#[test]
fn a_warm_start_derives_nothing_and_answers_exactly_as_a_fresh_build_does() {
    let c = corpus();
    let cold = c.open(1, true);
    for name in ["trigram", "symbols", "refs", "graph", "lexical"] {
        assert_eq!(state_of(&cold, name), "built", "{name}: {:?}", cold.status);
    }
    assert_eq!(cold.status.builds, 5);
    assert!(
        cold.status
            .components
            .iter()
            .all(|x| !x.reason.is_empty() || x.name == "exact")
    );
    c.checkpoint(&cold, 1);
    let want = answers(&cold);
    drop(cold);
    let warm = c.open(1, true);
    for name in ["trigram", "symbols", "refs", "graph", "lexical"] {
        assert_eq!(state_of(&warm, name), "loaded", "{name}: {:?}", warm.status);
    }
    assert_eq!(
        warm.status.builds, 0,
        "a warm start builds nothing: {:?}",
        warm.status
    );
    assert_eq!(warm.status.loads, 5);
    assert_eq!(
        warm.status.recomputed_files, 0,
        "{:?}",
        warm.status.recomputed_sample
    );
    assert!(
        warm.status.rebuild_reasons.is_empty(),
        "{:?}",
        warm.status.rebuild_reasons
    );
    assert_same(
        &answers(&warm),
        &want,
        "the warm indexes answer as the cold ones did",
    );
    // And as a build with no store at all.
    let plain = c.open(1, false);
    assert_same(&answers(&plain), &want, "no store");
    eprintln!(
        "PX-111 cold vs warm over {} files / {} bytes: first ready cold {} ms, warm {} ms; components cold {:?}, warm {:?}",
        warm.exact.stats().files,
        warm.exact.stats().bytes,
        plain.status.first_ready_ms,
        warm.status.first_ready_ms,
        plain
            .status
            .components
            .iter()
            .map(|c| (c.name.as_str(), c.ms))
            .collect::<Vec<_>>(),
        warm.status
            .components
            .iter()
            .map(|c| (c.name.as_str(), c.ms))
            .collect::<Vec<_>>(),
    );
}

#[test]
fn an_edit_made_while_nothing_was_running_is_found_by_its_hash_and_nothing_else_is_derived() {
    let c = corpus();
    let cold = c.open(1, true);
    c.checkpoint(&cold, 1);
    drop(cold);
    // While the Core is down: one file edited, one added, one deleted. The
    // name is built here so this file (which is part of the corpus) does not
    // contain it.
    let added = format!("a_function_added_{}_was_down", "while_the_core");
    let some_rs: Vec<PathBuf> = [
        "retrieval/src/lexical.rs",
        "retrieval/src/graph.rs",
        "memory/src/lib.rs",
    ]
    .iter()
    .map(|p| c.root.join(p))
    .collect();
    let mut text = std::fs::read_to_string(&some_rs[0]).unwrap();
    text.push_str(&format!("\npub fn {added}() {{}}\n"));
    std::fs::write(&some_rs[0], text).unwrap();
    std::fs::write(
        c.root.join("retrieval/src/brand_new.rs"),
        format!("pub fn brand_new_thing() {{ {added}(); }}\n"),
    )
    .unwrap();
    std::fs::remove_file(&some_rs[2]).unwrap();
    let warm = c.open(2, true);
    assert_eq!(warm.status.builds, 0, "{:?}", warm.status);
    assert_eq!(
        warm.status.recomputed_files, 2,
        "the edited and the new file, nothing else"
    );
    // The answers equal a build from scratch at the new state.
    let fresh = c.open(2, false);
    assert_same(
        &answers(&warm),
        &answers(&fresh),
        "warm vs fresh after an edit",
    );
    // And the edit is visible: no stale hit.
    let opts = SearchOptions::default();
    assert_eq!(warm.exact.search_exact(&added, &opts).len(), 2);
    assert!(
        warm.lexical
            .search("brand_new_thing", 3)
            .unwrap()
            .iter()
            .any(|h| h.path.ends_with("brand_new.rs"))
    );
    assert!(warm.symbols.symbols_in("memory/src/lib.rs").is_empty());
    let callers = warm.refs.symbol_edges(&added, None, "callers", 10);
    assert!(
        callers
            .edges
            .iter()
            .any(|e| e.from_path.ends_with("brand_new.rs")),
        "{callers:?}"
    );
}

/// Flip one byte so the file still parses as what it was: the damage only a
/// checksum can see.
fn flip_inside_a_json_string(path: &Path) {
    let mut bytes = std::fs::read(path).unwrap();
    // The first identifier-ish letter after `"name":"`.
    let needle = b"\"name\":\"";
    let at = bytes
        .windows(needle.len())
        .position(|w| w == needle)
        .unwrap()
        + needle.len();
    bytes[at] = if bytes[at] == b'z' {
        b'y'
    } else {
        bytes[at] + 1
    };
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn damage_is_detected_named_and_rebuilt_around_and_the_answers_stay_right() {
    let c = corpus();
    let cold = c.open(1, true);
    c.checkpoint(&cold, 1);
    let want = answers(&cold);
    drop(cold);
    let store = c.store();
    let generation = store.load().generation;
    // 1. The derived blob: a symbol's name changed in place. It still parses.
    let derived = store.dir().join(format!("derived.{generation}.json"));
    let original = std::fs::read(&derived).unwrap();
    flip_inside_a_json_string(&derived);
    let corrupted = std::fs::read(&derived).unwrap();
    let parsed: Vec<FileRecord> = serde_json::from_slice(&corrupted)
        .expect("the damage is invisible to the parser, so only the checksum can catch it");
    let clean: Vec<FileRecord> = serde_json::from_slice(&original).unwrap();
    assert_ne!(
        parsed, clean,
        "the damaged blob says something different from the real one"
    );
    let warm = c.open(1, true);
    assert!(
        warm.status
            .rebuild_reasons
            .iter()
            .any(|r| r.starts_with("derived: checksum mismatch")),
        "{:?}",
        warm.status.rebuild_reasons
    );
    assert_eq!(state_of(&warm, "symbols"), "rebuilt");
    assert_eq!(
        state_of(&warm, "trigram"),
        "loaded",
        "the other blob was fine"
    );
    assert_same(&answers(&warm), &want, "the damaged blob served nothing");
    assert!(
        warm.status
            .components
            .iter()
            .any(|x| x.name == "symbols" && x.reason.contains("checksum"))
    );
    c.checkpoint(&warm, 1);
    drop(warm);
    // 2. The trigram blob truncated; 3. the manifest: a flipped byte.
    let generation = store.load().generation;
    let tri = store.dir().join(format!("trigrams.{generation}.bin"));
    let b = std::fs::read(&tri).unwrap();
    std::fs::write(&tri, &b[..b.len() / 2]).unwrap();
    let warm = c.open(1, true);
    assert_eq!(state_of(&warm, "trigram"), "rebuilt");
    assert_eq!(state_of(&warm, "symbols"), "loaded");
    assert_same(&answers(&warm), &want, "after damage");
    c.checkpoint(&warm, 1);
    drop(warm);
    let manifest = store.dir().join("manifest.json");
    let mut m = std::fs::read(&manifest).unwrap();
    m[20] ^= 0x01;
    std::fs::write(&manifest, m).unwrap();
    let warm = c.open(1, true);
    for name in ["trigram", "symbols", "refs", "graph"] {
        assert_eq!(state_of(&warm, name), "rebuilt", "{name}");
    }
    assert!(
        warm.status
            .rebuild_reasons
            .iter()
            .any(|r| r.starts_with("manifest:")),
        "{:?}",
        warm.status.rebuild_reasons
    );
    assert_same(&answers(&warm), &want, "after damage");
    c.checkpoint(&warm, 1);
    drop(warm);
    // 4. The Tantivy index: a byte flipped in a data file, and meta.json gone.
    let lex = store.lexical_dir();
    let segment = std::fs::read_dir(&lex)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|x| x == "idx" || x == "pos" || x == "term" || x == "store")
        })
        .max_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        .expect("a segment file");
    let mut sb = std::fs::read(&segment).unwrap();
    let mid = sb.len() / 2;
    sb[mid] ^= 0x40;
    std::fs::write(&segment, sb).unwrap();
    let warm = c.open(1, true);
    assert_eq!(state_of(&warm, "lexical"), "rebuilt", "{:?}", warm.status);
    assert!(
        warm.status
            .rebuild_reasons
            .iter()
            .any(|r| r.starts_with("lexical:")),
        "{:?}",
        warm.status.rebuild_reasons
    );
    assert_same(&answers(&warm), &want, "after damage");
    drop(warm);
    std::fs::remove_file(lex.join("meta.json")).unwrap();
    let warm = c.open(1, true);
    assert_eq!(state_of(&warm, "lexical"), "rebuilt");
    assert_same(&answers(&warm), &want, "after damage");
}

/// A different format version is discarded, not read.
#[test]
fn a_snapshot_of_another_root_or_format_is_not_this_workspaces() {
    let c = corpus();
    let cold = c.open(1, true);
    c.checkpoint(&cold, 1);
    drop(cold);
    // The same store directory opened for another root finds a manifest written for this one.
    let other = IndexStore::open(&c.store_base, &c.root.join("retrieval"));
    assert!(other.load().first_run, "another root has its own directory");
    let manifest = c.store().dir().join("manifest.json");
    let text = std::fs::read_to_string(&manifest).unwrap();
    let (body, _) = text.split_once('\n').unwrap();
    let bumped = body.replacen("\"format\":1", "\"format\":99", 1);
    let sum = {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(bumped.as_bytes()))
    };
    std::fs::write(&manifest, format!("{bumped}\n{sum}\n")).unwrap();
    let warm = c.open(1, true);
    assert!(
        warm.status
            .rebuild_reasons
            .iter()
            .any(|r| r.contains("format")),
        "{:?}",
        warm.status.rebuild_reasons
    );
    assert_eq!(state_of(&warm, "symbols"), "rebuilt");
}

/// The incremental refresh equals a fresh build, and takes the changed set only.
#[test]
fn refresh_from_a_changed_set_equals_a_fresh_build() {
    let c = corpus();
    let mut set = c.open(1, true);
    let edited = "retrieval/src/impact.rs";
    let mut text = std::fs::read_to_string(c.root.join(edited)).unwrap();
    text.push_str("\npub fn refreshed_function() { select_impacted_stub(); }\npub fn select_impacted_stub() {}\n");
    std::fs::write(c.root.join(edited), text).unwrap();
    std::fs::remove_file(c.root.join("memory/src/lib.rs")).unwrap();
    let changed = vec![edited.to_owned(), "memory/src/lib.rs".to_owned()];
    set.exact.refresh(&changed, 2);
    let stats = refresh_derived(
        &set.exact,
        Derived {
            lexical: &mut set.lexical,
            symbols: &mut set.symbols,
            refs: &mut set.refs,
            graph: &mut set.graph,
        },
        &changed,
        2,
        BTreeMap::new(),
        None,
    )
    .unwrap();
    assert_eq!(stats.files, 2);
    let fresh = c.open(2, false);
    assert_same(&answers(&set), &answers(&fresh), "refresh vs fresh");
    let _unused: Option<SymbolIndex> = None;
}

// ---- A process that dies inside a checkpoint ------------------------------

/// Child half of the kill test: when `PX111_CHILD_DIR` names a corpus, open
/// it, write a checkpoint (the fault named by `MODBIT_FAULT_INDEX_ABORT`
/// makes it abort inside) and exit 0 if it somehow returns.
#[test]
fn child_writer() {
    let Ok(dir) = std::env::var("PX111_CHILD_DIR") else {
        return;
    };
    let root = PathBuf::from(&dir).join("ws");
    let store = IndexStore::open(&PathBuf::from(&dir).join("indexes"), &root);
    let set = IndexSet::open(&root, 2, Some(&store), vec![], BTreeMap::new()).unwrap();
    let records = assemble_records(&set.exact, &set.symbols, &set.refs, &set.graph);
    store
        .checkpoint(2, &records, set.exact.trigram_index())
        .unwrap();
}

#[test]
fn a_process_killed_inside_a_checkpoint_leaves_a_whole_snapshot_and_the_answers_stay_right() {
    for stage in ["blob", "before_manifest", "mid_manifest", "after_manifest"] {
        let c = corpus();
        let cold = c.open(1, true);
        c.checkpoint(&cold, 1);
        let want = answers(&cold);
        drop(cold);
        let before = c.store().load();
        assert_eq!(before.generation, 1);
        // The workspace moved on (so a second generation is worth writing).
        let dir = c.root.parent().unwrap().to_path_buf();
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "child_writer", "--nocapture"])
            .env("PX111_CHILD_DIR", &dir)
            .env("MODBIT_FAULT_INDEX_ABORT", stage)
            .output()
            .unwrap();
        assert!(
            !child.status.success(),
            "{stage}: the writer was supposed to die inside the write"
        );
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(
                child.status.signal(),
                Some(6),
                "{stage}: aborted (SIGABRT), not a clean exit"
            );
        }
        let after = c.store().load();
        match stage {
            "after_manifest" => {
                assert_eq!(after.generation, 2, "{stage}: committed before the death");
            }
            _ => {
                assert_eq!(
                    after.generation, 1,
                    "{stage}: the previous snapshot is still the current one"
                );
            }
        }
        assert!(
            after.reasons.is_empty(),
            "{stage}: a whole snapshot, nothing to discard: {:?}",
            after.reasons
        );
        assert!(after.records.is_some() && after.trigrams.is_some());
        // The next start uses it, cleans the garbage on its next checkpoint,
        // and answers as a fresh build does.
        let warm = c.open(1, true);
        assert_eq!(warm.status.builds, 0, "{stage}: {:?}", warm.status);
        assert_same(&answers(&warm), &want, stage);
        c.checkpoint(&warm, 1);
        let leftovers: Vec<String> = std::fs::read_dir(c.store().dir())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "{stage}: {leftovers:?}");
    }
}

/// The incremental-refresh numbers (PX-111 qualification; on demand, release
/// build): `cargo test --release -p modbit-retrieval --test persisted_indexes
/// px_111_incremental -- --ignored --nocapture`. Measures this workspace
/// itself: one changed file, then one hundred, through the same
/// `refresh_derived` the Core runs after a write.
#[test]
#[ignore = "benchmark: run on demand in a release build"]
fn px_111_incremental_refresh_numbers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let mut set = IndexSet::open(&root, 1, None, vec![], BTreeMap::new()).unwrap();
    let files = set.exact.stats().files;
    let rust: Vec<String> = set
        .exact
        .texts_with_hash()
        .filter(|(p, _, l, _)| *l == Some("rust") && p.starts_with("crates/"))
        .map(|(p, ..)| p.to_owned())
        .collect();
    let mut time = |paths: &[String], rev: u64| -> u128 {
        let started = std::time::Instant::now();
        set.exact.refresh(paths, rev);
        refresh_derived(
            &set.exact,
            Derived {
                lexical: &mut set.lexical,
                symbols: &mut set.symbols,
                refs: &mut set.refs,
                graph: &mut set.graph,
            },
            paths,
            rev,
            BTreeMap::new(),
            None,
        )
        .unwrap();
        started.elapsed().as_micros()
    };
    let one = vec![rust[rust.len() / 2].clone()];
    let mut singles: Vec<u128> = (0..20).map(|i| time(&one, 2 + i)).collect();
    singles.sort_unstable();
    let hundred: Vec<String> = rust.iter().take(100).cloned().collect();
    let mut hundreds: Vec<u128> = (0..5).map(|i| time(&hundred, 30 + i)).collect();
    hundreds.sort_unstable();
    println!(
        "PX-111 incremental refresh over {files} files: one file median {} us (min {}, max {}); {} files median {} us (min {}, max {})",
        singles[singles.len() / 2],
        singles[0],
        singles[singles.len() - 1],
        hundred.len(),
        hundreds[hundreds.len() / 2],
        hundreds[0],
        hundreds[hundreds.len() - 1],
    );
}

/// Where the one-file refresh spends its time (on demand, release build).
#[test]
#[ignore = "benchmark: run on demand in a release build"]
fn px_111_incremental_breakdown() {
    use modbit_retrieval::graph::import_specifiers;
    use modbit_retrieval::lexical::ChangedDoc;
    use modbit_retrieval::refs;
    use modbit_retrieval::symbols::extract_symbols;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let mut set = IndexSet::open(&root, 1, None, vec![], BTreeMap::new()).unwrap();
    let path = "crates/retrieval/src/impact.rs".to_owned();
    let ms = |t: std::time::Instant| t.elapsed().as_micros();
    let mut rows: Vec<(String, u128)> = Vec::new();
    for round in 0..5u64 {
        let rev = 2 + round;
        let t = std::time::Instant::now();
        set.exact.refresh(std::slice::from_ref(&path), rev);
        let exact = ms(t);
        let (text, lang, hash) = {
            let (t, l, h) = set.exact.entry_text(&path).unwrap();
            (t.to_owned(), l.map(str::to_owned), h.to_owned())
        };
        let t = std::time::Instant::now();
        let syms = extract_symbols(&path, &text, lang.as_deref(), &hash, rev);
        let sym_extract = ms(t);
        let t = std::time::Instant::now();
        let facts = refs::extract(&path, lang.as_deref().unwrap(), &text, &hash);
        let facts_extract = ms(t);
        let t = std::time::Instant::now();
        let specs = import_specifiers(lang.as_deref().unwrap(), &path, &text);
        let spec_extract = ms(t);
        let t = std::time::Instant::now();
        set.symbols.set_file(&path, syms);
        set.symbols.set_revision(rev);
        let sym_set = ms(t);
        let t = std::time::Instant::now();
        let docs: Vec<ChangedDoc> = vec![(path.clone(), Some((text.clone(), lang.clone())))];
        set.lexical.refresh(&docs, rev).unwrap();
        let lex = ms(t);
        let t = std::time::Instant::now();
        let redone = set
            .refs
            .refresh(vec![(path.clone(), Some(facts))], &set.symbols, rev);
        let refs_t = ms(t);
        let t = std::time::Instant::now();
        set.graph.refresh_with_specs(
            vec![(path.clone(), Some((lang.clone(), specs)))],
            BTreeMap::new(),
            None,
            rev,
        );
        let graph = ms(t);
        println!("PX-111 round {round}: lexical {lex} us, refs {refs_t} us");
        if round == 4 {
            rows.push(("exact.refresh (read+hash+trigrams)".into(), exact));
            rows.push(("extract symbols".into(), sym_extract));
            rows.push(("extract facts".into(), facts_extract));
            rows.push(("extract import specifiers".into(), spec_extract));
            rows.push(("symbols.set_file".into(), sym_set));
            rows.push(("lexical.refresh (commit+reload)".into(), lex));
            rows.push((
                format!("refs.refresh (re-resolved {} files)", redone.len()),
                refs_t,
            ));
            rows.push(("graph.refresh_with_specs".into(), graph));
        }
    }
    for (name, us) in rows {
        println!("PX-111 breakdown: {name}: {us} us");
    }
}
