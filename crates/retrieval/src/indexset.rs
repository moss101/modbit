//! A workspace's indexes opened from the persisted store, refreshed from a
//! changed set, and written back (PX-111; docs/18 "Index freshness").
//!
//! [`IndexSet::open`] is the whole warm start: it loads the store, reads the
//! workspace's files as they are on disk now (hydration is read-through: file
//! bytes never come from an index), and for every derived index uses a
//! persisted per-file record only where the file's content hash is the one the
//! record was derived from. A first run, a corrupt store, a format mismatch
//! and an edit made while nothing was running are the same case seen from
//! different sides: some or all records are not valid, those files are
//! derived again, and the reason is recorded in the [`IndexStatus`]. Nothing
//! stale can be served and nothing unchanged is parsed twice.
//!
//! [`refresh_derived`] is the incremental path: given the paths that changed
//! (a write's change set, or the diff `RepositoryIndex::resync` found), each
//! file is parsed once and every derived index takes the result.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::graph::{ChangedLines, CommitRecord, EvidenceGraph, SpecUpdate, import_specifiers};
use crate::index::{IndexError, RepositoryIndex};
use crate::lexical::{ChangedDoc, LexicalIndex};
use crate::persist::{FileRecord, IndexStore, Persisted};
use crate::refs::{self, FileFacts, RefGraph};
use crate::symbols::{Symbol, SymbolIndex, extract_symbols};

/// How one component of the set came to be.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentStatus {
    /// `exact` | `trigram` | `symbols` | `refs` | `graph` | `lexical`.
    pub name: String,
    /// `hydrated` (read from the workspace's files), `loaded` (from the
    /// store), `built` (derived from scratch: nothing usable was stored) or
    /// `rebuilt` (the store held something that failed its checks).
    pub state: String,
    /// Files it covers.
    pub files: u64,
    /// Milliseconds it took to load or build.
    pub ms: u64,
    /// Why it was derived instead of loaded; empty when it was loaded.
    pub reason: String,
}

/// What opening, refreshing and checkpointing have done for one workspace.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexStatus {
    /// The components and how each came to be.
    pub components: Vec<ComponentStatus>,
    /// Components derived from scratch since this status began.
    pub builds: u64,
    /// Components loaded from the store.
    pub loads: u64,
    /// Incremental refreshes.
    pub refreshes: u64,
    /// Milliseconds of the last refresh.
    pub last_refresh_ms: u64,
    /// Files of the last refresh.
    pub last_refresh_files: u64,
    /// Files whose persisted record did not match their bytes (or had none)
    /// and were derived again at open.
    pub recomputed_files: u64,
    /// The first few of them, by path.
    pub recomputed_sample: Vec<String>,
    /// Everything that was discarded, with the reason.
    pub rebuild_reasons: Vec<String>,
    /// Bytes the store holds for this workspace.
    pub persisted_bytes: u64,
    /// Generation of the last snapshot read or written.
    pub generation: u64,
    /// Milliseconds from the start of the open to every component usable.
    pub first_ready_ms: u64,
    /// Milliseconds of the last checkpoint.
    pub last_checkpoint_ms: u64,
    /// Where the store keeps this workspace.
    pub store_dir: String,
}

/// A workspace's indexes.
pub struct IndexSet {
    /// Exact / regex / path search, with the trigram prefilter.
    pub exact: RepositoryIndex,
    /// BM25.
    pub lexical: LexicalIndex,
    /// Definitions.
    pub symbols: SymbolIndex,
    /// Imports, history, tests.
    pub graph: EvidenceGraph,
    /// Reference, call and implementor edges.
    pub refs: RefGraph,
    /// How it came to be.
    pub status: IndexStatus,
}

fn lexical_err(e: &tantivy::TantivyError) -> String {
    format!("{e}")
}

/// One file's derived data, computed from its bytes.
fn derive_record(
    path: &str,
    text: &str,
    language: Option<&str>,
    hash: &str,
    revision: u64,
) -> FileRecord {
    let symbols = extract_symbols(path, text, language, hash, revision);
    let (facts, imports) = match language {
        Some(l) => (
            refs::extract(path, l, text, hash),
            import_specifiers(l, path, text),
        ),
        None => (FileFacts::default(), vec![]),
    };
    FileRecord {
        path: path.to_owned(),
        hash: hash.to_owned(),
        language: language.map(str::to_owned),
        symbols,
        facts,
        imports,
    }
}

/// The first phase of an open: the store read and the exact index hydrated
/// from the files on disk. It is what the request profiler and the path
/// searches need; the derived indexes follow with [`IndexSet::open_rest`].
pub struct ExactPhase {
    persisted: Persisted,
    status: IndexStatus,
    exact_ms: u64,
    started: Instant,
}

/// The derived indexes of an open, built over an already-open exact index.
pub struct RestSet {
    /// BM25.
    pub lexical: LexicalIndex,
    /// Definitions.
    pub symbols: SymbolIndex,
    /// Imports, history, tests.
    pub graph: EvidenceGraph,
    /// Reference, call and implementor edges.
    pub refs: RefGraph,
    /// How the whole open came to be.
    pub status: IndexStatus,
}

impl IndexSet {
    /// Open the indexes of `root` at `revision`: from `store` where it holds
    /// something valid, deriving the rest. `commits` and `changed_lines`
    /// come from the host's Git (they are not persisted: they are cheap and
    /// belong to the repository, not to the index).
    pub fn open(
        root: &Path,
        revision: u64,
        store: Option<&IndexStore>,
        commits: Vec<CommitRecord>,
        changed_lines: ChangedLines,
    ) -> Result<Self, IndexError> {
        let (exact, phase) = Self::open_exact(root, revision, store)?;
        let rest = Self::open_rest(&exact, revision, store, commits, changed_lines, phase)?;
        Ok(Self {
            exact,
            lexical: rest.lexical,
            symbols: rest.symbols,
            graph: rest.graph,
            refs: rest.refs,
            status: rest.status,
        })
    }

    /// Phase one: read the store and hydrate the exact index from the files as
    /// they are on disk now, with the trigram sets of every file whose bytes
    /// are unchanged taken from the store. Nothing is written.
    pub fn open_exact(
        root: &Path,
        revision: u64,
        store: Option<&IndexStore>,
    ) -> Result<(RepositoryIndex, ExactPhase), IndexError> {
        let started = Instant::now();
        let mut status = IndexStatus::default();
        let persisted = store.map(IndexStore::load).unwrap_or_default();
        status.generation = persisted.generation;
        status.persisted_bytes = persisted.bytes;
        status.store_dir = store
            .map(|s| s.dir().display().to_string())
            .unwrap_or_default();
        for r in &persisted.reasons {
            status.rebuild_reasons.push(r.clone());
        }
        let t = Instant::now();
        let exact = RepositoryIndex::build_cached(root, revision, persisted.trigrams.as_ref())?;
        let exact_ms = elapsed_ms(t);
        Ok((
            exact,
            ExactPhase {
                persisted,
                status,
                exact_ms,
                started,
            },
        ))
    }

    /// Phase two: the derived indexes over `exact` (the lexical index, the
    /// symbols, the reference graph, the import and history graph), using a
    /// persisted record only for a file whose bytes hash as it did.
    pub fn open_rest(
        exact: &RepositoryIndex,
        revision: u64,
        store: Option<&IndexStore>,
        commits: Vec<CommitRecord>,
        changed_lines: ChangedLines,
        phase: ExactPhase,
    ) -> Result<RestSet, IndexError> {
        let ExactPhase {
            persisted,
            mut status,
            exact_ms,
            started,
        } = phase;
        let first = persisted.first_run || store.is_none();
        let why_not = |component: &str| -> String {
            if store.is_none() {
                "no store: indexes are not persisted".to_owned()
            } else if persisted.first_run {
                "first build: nothing stored".to_owned()
            } else {
                persisted
                    .reasons
                    .iter()
                    .find(|r| r.starts_with(component) || r.starts_with("manifest"))
                    .cloned()
                    .unwrap_or_else(|| format!("{component}: not in the store"))
            }
        };
        let push = |status: &mut IndexStatus,
                    name: &str,
                    state: &str,
                    files: u64,
                    ms: u64,
                    reason: String| {
            match state {
                "loaded" => status.loads += 1,
                "built" | "rebuilt" => status.builds += 1,
                _ => {}
            }
            status.components.push(ComponentStatus {
                name: name.to_owned(),
                state: state.to_owned(),
                files,
                ms,
                reason,
            });
        };
        push(
            &mut status,
            "exact",
            "hydrated",
            exact.stats().files as u64,
            exact_ms,
            String::new(),
        );
        let tri_files = exact.trigram_index().len() as u64;
        let tri_reused = persisted.trigrams.is_some();
        push(
            &mut status,
            "trigram",
            if tri_reused {
                "loaded"
            } else if first {
                "built"
            } else {
                "rebuilt"
            },
            tri_files,
            0,
            if tri_reused {
                String::new()
            } else {
                why_not("trigrams")
            },
        );

        // Derived per-file data: a persisted record is used only for a file
        // whose content hash is the one it was derived from.
        let t = Instant::now();
        let mut by_path: BTreeMap<String, Vec<Symbol>> = BTreeMap::new();
        let mut facts: BTreeMap<String, FileFacts> = BTreeMap::new();
        let mut specs: Vec<(String, Option<String>, Vec<String>)> = Vec::new();
        let mut recomputed = 0u64;
        let stored = persisted.records.as_ref();
        for (path, text, language, hash) in exact.texts_with_hash() {
            let reusable = stored.and_then(|m| m.get(path)).filter(|r| {
                r.hash == hash
                    && r.language.as_deref() == language
                    && (language.is_none() || r.facts.content_hash == hash)
            });
            let rec = match reusable {
                Some(r) => {
                    let mut r = r.clone();
                    for s in &mut r.symbols {
                        s.index_revision = revision;
                    }
                    r
                }
                None => {
                    recomputed += 1;
                    if status.recomputed_sample.len() < 8 {
                        status.recomputed_sample.push(path.to_owned());
                    }
                    derive_record(path, text, language, hash, revision)
                }
            };
            by_path.insert(path.to_owned(), rec.symbols);
            facts.insert(path.to_owned(), rec.facts);
            specs.push((path.to_owned(), rec.language, rec.imports));
        }
        status.recomputed_files = recomputed;
        let derived_loaded = persisted.records.is_some();
        let state = if derived_loaded {
            "loaded"
        } else if first {
            "built"
        } else {
            "rebuilt"
        };
        let reason = if derived_loaded {
            String::new()
        } else {
            why_not("derived")
        };
        let all_paths: Vec<String> = exact
            .texts_with_hash()
            .map(|(p, ..)| p.to_owned())
            .collect();
        let symbols = SymbolIndex::from_parts(revision, by_path);
        let files_n = all_paths.len() as u64;
        let symbols_ms = elapsed_ms(t);
        push(
            &mut status,
            "symbols",
            state,
            files_n,
            symbols_ms,
            reason.clone(),
        );
        let t = Instant::now();
        let refs = RefGraph::from_facts(facts, &symbols, all_paths, revision);
        push(
            &mut status,
            "refs",
            state,
            files_n,
            elapsed_ms(t),
            reason.clone(),
        );
        let t = Instant::now();
        let graph = EvidenceGraph::build_from_specs(
            specs
                .iter()
                .map(|(p, l, s)| (p.as_str(), l.as_deref(), s.clone())),
            commits,
            changed_lines,
            revision,
        );
        push(&mut status, "graph", state, files_n, elapsed_ms(t), reason);

        // Lexical: a persisted Tantivy directory checked by its own
        // checksums, brought to the files as they are by the diff of content
        // hashes its own per-file records give; or built.
        let t = Instant::now();
        let docs = || exact.texts();
        let (lexical, lstate, lreason) = match store {
            None => (
                LexicalIndex::build(docs(), revision).map_err(|e| lexical_io(&e))?,
                "built",
                why_not("lexical"),
            ),
            Some(s) => {
                let dir = s.lexical_dir();
                let opened = if dir.join("meta.json").exists() {
                    LexicalIndex::open_in_dir(&dir, revision)
                        .map_err(|e| format!("lexical: {}", lexical_err(&e)))
                } else {
                    Err("lexical: not in the store".to_owned())
                };
                // The persisted index is a cache of the files: any failure to
                // open, bring up to date or rebuild it on disk (on Windows a
                // file the index still maps cannot be replaced or deleted,
                // and that is an error, not a reason to refuse the open)
                // falls back to an in-memory index built from the same
                // texts, with the reason recorded.
                let attempt = || -> Result<(LexicalIndex, &'static str), String> {
                    let mut lx = opened?;
                    let have = lx.doc_hashes().clone();
                    let mut changed: Vec<ChangedDoc> = Vec::new();
                    for (p, t, l) in exact.texts() {
                        let hash = exact.file(p).map(|f| f.content_hash.as_str());
                        if have.get(p).map(String::as_str) != hash {
                            changed
                                .push((p.to_owned(), Some((t.to_owned(), l.map(str::to_owned)))));
                        }
                    }
                    for p in have.keys() {
                        if exact.entry_text(p).is_none() {
                            changed.push((p.clone(), None));
                        }
                    }
                    lx.refresh(&changed, revision)
                        .map_err(|e| format!("lexical: refresh: {e}"))?;
                    Ok((lx, "loaded"))
                };
                match attempt() {
                    Ok((lx, state)) => (lx, state, String::new()),
                    Err(e) => {
                        let first_lexical = !dir.exists();
                        if !first_lexical {
                            status.rebuild_reasons.push(e.clone());
                        }
                        // Every handle on the old index is dropped by now
                        // (`attempt` owned it); discard retries a sharing
                        // violation itself.
                        s.discard_lexical();
                        let reason = if first_lexical {
                            why_not("lexical")
                        } else {
                            e.clone()
                        };
                        match LexicalIndex::build_in_dir(&dir, docs(), revision) {
                            Ok(lx) => (lx, if first_lexical { "built" } else { "rebuilt" }, reason),
                            Err(disk) => {
                                status.rebuild_reasons.push(format!(
                                    "lexical: on-disk index unavailable ({disk}); served from memory"
                                ));
                                let lx = LexicalIndex::build(docs(), revision)
                                    .map_err(|e| lexical_io(&e))?;
                                (lx, "rebuilt", reason)
                            }
                        }
                    }
                }
            }
        };
        push(
            &mut status,
            "lexical",
            lstate,
            lexical.len() as u64,
            elapsed_ms(t),
            lreason,
        );
        status.first_ready_ms = elapsed_ms(started);
        Ok(RestSet {
            lexical,
            symbols,
            graph,
            refs,
            status,
        })
    }
}

fn elapsed_ms(t: Instant) -> u64 {
    u64::try_from(t.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn lexical_io(e: &tantivy::TantivyError) -> IndexError {
    IndexError::Io {
        path: "lexical index".to_owned(),
        source: std::io::Error::other(e.to_string()),
    }
}

/// The records of every searchable file, assembled from the live indexes (for
/// a checkpoint): nothing is derived again to write them.
#[must_use]
pub fn assemble_records(
    exact: &RepositoryIndex,
    symbols: &SymbolIndex,
    refs: &RefGraph,
    graph: &EvidenceGraph,
) -> Vec<FileRecord> {
    exact
        .texts_with_hash()
        .map(|(path, _, language, hash)| FileRecord {
            path: path.to_owned(),
            hash: hash.to_owned(),
            language: language.map(str::to_owned),
            symbols: symbols.symbols_in(path).to_vec(),
            facts: refs.facts(path).cloned().unwrap_or_default(),
            imports: graph
                .specs_of(path)
                .map(<[String]>::to_vec)
                .unwrap_or_default(),
        })
        .collect()
}

/// The derived indexes a refresh updates.
pub struct Derived<'a> {
    /// BM25.
    pub lexical: &'a mut LexicalIndex,
    /// Definitions.
    pub symbols: &'a mut SymbolIndex,
    /// Reference edges.
    pub refs: &'a mut RefGraph,
    /// Imports, history, tests.
    pub graph: &'a mut EvidenceGraph,
}

/// What a refresh did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RefreshStats {
    /// Files refreshed.
    pub files: usize,
    /// Milliseconds it took.
    pub ms: u64,
}

/// Refresh the derived indexes from a changed set. `exact` is already at the
/// new state (`refresh` or `resync` ran on it); each changed file is parsed
/// once here and the symbol index, the lexical index, the reference graph and
/// the evidence graph take the result. A path that no longer has searchable
/// text leaves them all.
pub fn refresh_derived(
    exact: &RepositoryIndex,
    derived: Derived<'_>,
    changed: &[String],
    revision: u64,
    changed_lines: ChangedLines,
    commits: Option<Vec<CommitRecord>>,
) -> Result<RefreshStats, String> {
    let Derived {
        lexical,
        symbols,
        refs,
        graph,
    } = derived;
    let started = Instant::now();
    let mut docs: Vec<ChangedDoc> = Vec::new();
    let mut facts: Vec<(String, Option<FileFacts>)> = Vec::new();
    let mut specs: Vec<SpecUpdate> = Vec::new();
    for p in changed {
        match exact.entry_text(p) {
            Some((text, language, hash)) => {
                let rec = derive_record(p, text, language, hash, revision);
                symbols.set_file(p, rec.symbols);
                docs.push((
                    p.clone(),
                    Some((text.to_owned(), language.map(str::to_owned))),
                ));
                facts.push((p.clone(), Some(rec.facts)));
                specs.push((p.clone(), Some((rec.language, rec.imports))));
            }
            None => {
                symbols.set_file(p, vec![]);
                docs.push((p.clone(), None));
                facts.push((p.clone(), None));
                specs.push((p.clone(), None));
            }
        }
    }
    symbols.set_revision(revision);
    lexical
        .refresh(&docs, revision)
        .map_err(|e| e.to_string())?;
    refs.refresh(facts, symbols, revision);
    graph.refresh_with_specs(specs, changed_lines, commits, revision);
    Ok(RefreshStats {
        files: changed.len(),
        ms: elapsed_ms(started),
    })
}
