//! The Core's hold on a workspace's indexes (PX-110, PX-111; docs/18
//! "Index freshness"): they are opened from the profile's persisted store the
//! first time a workspace is touched (or at start, for the workspaces of tasks
//! that were unfinished when the Core went down), refreshed incrementally when
//! a tool writes or when the workspace moved without a recorded change set,
//! and written back on a schedule. The filesystem and the revision stay
//! canonical: a restarted Core reads the files as they are and uses a stored
//! record only for a file whose bytes still hash the same.
//!
//! The components keep their own maps in [`ToolHost`] (the tools lock them one
//! by one); this module fills them from one [`IndexSet`], owns the lifecycle
//! and keeps the status clients read (`GetIndexStatus`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use modbit_retrieval::indexset::{
    Derived, ExactPhase, IndexSet, IndexStatus, RestSet, assemble_records, refresh_derived,
};
use modbit_retrieval::persist::{self, IndexStore};
use modbit_retrieval::{
    EvidenceGraph, LexicalIndex, RefGraph, RepositoryIndex, SemanticIndex, SymbolIndex,
};
use tokio::sync::Mutex;

use crate::tools::{ToolHost, recent_commits, symbol_spans, worktree_changed_lines};

/// A snapshot is written after this many refreshes, or this long after the
/// last one, whichever comes first (and always after a load that had to
/// derive anything).
const CHECKPOINT_EVERY_REFRESHES: u64 = 25;
const CHECKPOINT_EVERY: Duration = Duration::from_secs(10);

/// `MODBIT_INDEX_CHECKPOINT_EVERY_REFRESHES` overrides the refresh count
/// (the qualification runs set it to 1 to put a snapshot under every write).
fn checkpoint_every_refreshes() -> u64 {
    std::env::var("MODBIT_INDEX_CHECKPOINT_EVERY_REFRESHES")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(CHECKPOINT_EVERY_REFRESHES)
}

/// A workspace whose exact index is open and whose derived indexes are not:
/// what the second phase of the open needs.
pub(crate) struct PendingOpen {
    store: IndexStore,
    phase: ExactPhase,
    revision: u64,
}

/// A changed path with its new text and symbol spans (`None` = removed).
type ChangedChunkSource = (String, Option<(String, Vec<(String, u64, u64)>)>);

/// What the Core knows about one workspace's indexes beyond the indexes.
pub(crate) struct IndexState {
    /// How they came to be and what refreshes did.
    pub status: std::sync::Mutex<IndexStatus>,
    /// Where they persist.
    pub store: IndexStore,
    /// Exact and regex searches answered through the trigram prefilter.
    pub searches_indexed: AtomicU64,
    /// Exact and regex searches that scanned every file.
    pub searches_scanned: AtomicU64,
    refreshes_since_checkpoint: AtomicU64,
    last_checkpoint: std::sync::Mutex<Instant>,
}

impl IndexState {
    fn new(store: IndexStore, status: IndexStatus) -> Self {
        Self {
            status: std::sync::Mutex::new(status),
            store,
            searches_indexed: AtomicU64::new(0),
            searches_scanned: AtomicU64::new(0),
            refreshes_since_checkpoint: AtomicU64::new(0),
            last_checkpoint: std::sync::Mutex::new(Instant::now()),
        }
    }

    /// A copy of the status as it is now.
    pub fn snapshot(&self) -> IndexStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

fn store_cap_bytes() -> u64 {
    std::env::var("MODBIT_INDEX_STORE_MAX_BYTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(persist::DEFAULT_STORE_CAP_BYTES)
}

/// Refresh every index of a workspace from `paths` (the paths a write
/// changed), or — `None` — from the diff between the files on disk and the
/// index (the workspace moved without a recorded change set). The semantic
/// index, when it has been built, re-embeds exactly those paths. Returns the
/// paths refreshed and the milliseconds it took.
#[allow(clippy::too_many_arguments)]
pub(crate) fn refresh_with_guards(
    idx: &mut RepositoryIndex,
    lexical: &mut LexicalIndex,
    symbols: &mut SymbolIndex,
    refs: &mut RefGraph,
    graph: &mut EvidenceGraph,
    semantic: &mut Option<SemanticIndex>,
    paths: Option<&[String]>,
    revision: u64,
) -> Result<(Vec<String>, u64), String> {
    let started = Instant::now();
    let changed: Vec<String> = match paths {
        Some(p) => {
            idx.refresh(p, revision);
            p.iter().map(|x| x.replace('\\', "/")).collect()
        }
        None => idx.resync(revision).map_err(|e| e.to_string())?,
    };
    let lines = worktree_changed_lines(idx.root());
    refresh_derived(
        idx,
        Derived {
            lexical,
            symbols,
            refs,
            graph,
        },
        &changed,
        revision,
        lines,
        None,
    )?;
    if let Some(sem) = semantic.as_mut() {
        // docs/18: embedding is queued for changed chunks, then flushed here.
        sem.mark_changed(&changed);
        let items: Vec<ChangedChunkSource> = changed
            .iter()
            .map(|p| {
                let t = idx
                    .entry_text(p)
                    .map(|(t, _, _)| (t.to_owned(), symbol_spans(symbols, p)));
                (p.clone(), t)
            })
            .collect();
        sem.flush(
            items
                .iter()
                .map(|(p, c)| (p.as_str(), c.as_ref().map(|(t, s)| (t.as_str(), s.clone())))),
            revision,
        )?;
    }
    Ok((
        changed,
        u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    ))
}

impl ToolHost {
    /// Open a workspace's exact index (phase one of an open: the store read,
    /// the files hydrated) the first time it is touched. What needs only the
    /// file list (the request profiler, path search) stops here; the derived
    /// indexes follow in [`Self::ensure_loaded`], on the first query that needs
    /// them, so the start of a run does not wait for their construction.
    /// Idempotent; concurrent callers wait for the one load.
    pub(crate) async fn ensure_exact(&self, canonical: &Path) -> Result<()> {
        if self.indexes.lock().await.contains_key(canonical) {
            return Ok(());
        }
        let _one_at_a_time = self.loading.lock().await;
        self.open_exact_locked(canonical).await
    }

    /// Phase one with the `loading` lock held by the caller.
    async fn open_exact_locked(&self, canonical: &Path) -> Result<()> {
        if self.indexes.lock().await.contains_key(canonical) {
            return Ok(());
        }
        let (ws, _) = self.workspace(&canonical.to_string_lossy()).await?;
        let revision = ws.lock().await.revision().number;
        let store = IndexStore::open(&self.data_dir_for_indexes(), canonical);
        let (root, st) = (canonical.to_path_buf(), store.clone());
        let (exact, phase) =
            tokio::task::spawn_blocking(move || IndexSet::open_exact(&root, revision, Some(&st)))
                .await
                .context("opening the exact index")?
                .map_err(|e| anyhow::anyhow!("opening the exact index: {e}"))?;
        let key = canonical.to_path_buf();
        self.pending_opens.lock().await.insert(
            key.clone(),
            PendingOpen {
                store,
                phase,
                revision,
            },
        );
        self.indexes
            .lock()
            .await
            .insert(key, Arc::new(Mutex::new(exact)));
        Ok(())
    }

    /// Open the rest of a workspace's indexes (the derived ones) the first
    /// time a query needs them. Idempotent; concurrent callers wait for the
    /// one load.
    pub(crate) async fn ensure_loaded(&self, canonical: &Path) -> Result<()> {
        if self.index_states.lock().await.contains_key(canonical) {
            return Ok(());
        }
        let _one_at_a_time = self.loading.lock().await;
        if self.index_states.lock().await.contains_key(canonical) {
            return Ok(());
        }
        self.open_exact_locked(canonical).await?;
        let key = canonical.to_path_buf();
        let PendingOpen {
            store,
            phase,
            revision,
        } = self
            .pending_opens
            .lock()
            .await
            .remove(&key)
            .context("the exact index is open but its open record is gone")?;
        let exact_arc = Arc::clone(
            self.indexes
                .lock()
                .await
                .get(&key)
                .context("the exact index")?,
        );
        let (root, st) = (key.clone(), store.clone());
        let guard = Arc::clone(&exact_arc).lock_owned().await;
        let rest = tokio::task::spawn_blocking(move || {
            let commits = recent_commits(&root);
            let lines = worktree_changed_lines(&root);
            IndexSet::open_rest(&guard, revision, Some(&st), commits, lines, phase)
        })
        .await
        .context("opening the derived indexes")?
        .map_err(|e| anyhow::anyhow!("opening the derived indexes: {e}"))?;
        let RestSet {
            lexical,
            symbols,
            graph,
            refs,
            status,
        } = rest;
        // A load that had to derive anything leaves the store whole again.
        let whole =
            status.builds == 0 && status.recomputed_files == 0 && status.rebuild_reasons.is_empty();
        let (symbols, graph, refs) = (
            Arc::new(Mutex::new(symbols)),
            Arc::new(Mutex::new(graph)),
            Arc::new(Mutex::new(refs)),
        );
        self.lexical
            .lock()
            .await
            .insert(key.clone(), Arc::new(Mutex::new(lexical)));
        self.symbols
            .lock()
            .await
            .insert(key.clone(), Arc::clone(&symbols));
        self.graphs
            .lock()
            .await
            .insert(key.clone(), Arc::clone(&graph));
        self.refs
            .lock()
            .await
            .insert(key.clone(), Arc::clone(&refs));
        self.semantic
            .lock()
            .await
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Mutex::new(None)));
        let state = Arc::new(IndexState::new(store, status));
        self.index_states
            .lock()
            .await
            .insert(key, Arc::clone(&state));
        if !whole {
            let handles = SnapshotHandles {
                state: Arc::clone(&state),
                index: exact_arc,
                symbols,
                refs,
                graph,
                base: self.data_dir_for_indexes(),
            };
            if let Err(e) = handles.write().await {
                eprintln!("modbit-core: index snapshot not written: {e:#}");
                if let Ok(mut s) = state.status.lock() {
                    s.rebuild_reasons.push(format!("checkpoint: {e:#}"));
                }
            }
        }
        Ok(())
    }

    fn data_dir_for_indexes(&self) -> PathBuf {
        self.data_dir.join("indexes")
    }

    /// The lifecycle state of a workspace's indexes.
    pub(crate) async fn index_state(&self, canonical: &Path) -> Result<Arc<IndexState>> {
        self.ensure_loaded(canonical).await?;
        self.index_states
            .lock()
            .await
            .get(canonical)
            .cloned()
            .context("index state")
    }

    /// The reference graph of a workspace root (PX-110).
    pub(crate) async fn refs(&self, canonical: &Path) -> Result<Arc<Mutex<RefGraph>>> {
        self.ensure_loaded(canonical).await?;
        self.refs
            .lock()
            .await
            .get(canonical)
            .cloned()
            .context("reference graph")
    }

    /// The semantic index's slot: empty until a query that needs it builds it
    /// (the other searches never pay for it).
    pub(crate) async fn semantic_slot(
        &self,
        canonical: &Path,
    ) -> Result<Arc<Mutex<Option<SemanticIndex>>>> {
        self.ensure_loaded(canonical).await?;
        self.semantic
            .lock()
            .await
            .get(canonical)
            .cloned()
            .context("semantic slot")
    }

    /// Refresh a workspace's indexes after a write, and write a snapshot when
    /// it is time.
    pub(crate) async fn refresh_workspace(
        &self,
        canonical: &Path,
        paths: &[String],
        revision: u64,
    ) -> Result<()> {
        let state = self.index_state(canonical).await?;
        let index = self.index(canonical).await?;
        let lexical = self.lexical(canonical).await?;
        let symbols = self.symbols(canonical).await?;
        let refs = self.refs(canonical).await?;
        let graph = self.graph(canonical).await?;
        let semantic = self.semantic_slot(canonical).await?;
        let (files, ms) = {
            let mut idx = index.lock().await;
            let mut lex = lexical.lock().await;
            let mut sym = symbols.lock().await;
            let mut rg = refs.lock().await;
            let mut gr = graph.lock().await;
            let mut sem = semantic.lock().await;
            refresh_with_guards(
                &mut idx,
                &mut lex,
                &mut sym,
                &mut rg,
                &mut gr,
                &mut sem,
                Some(paths),
                revision,
            )
            .map_err(|e| anyhow::anyhow!("refreshing the indexes: {e}"))?
        };
        self.note_refresh(canonical, &state, files.len() as u64, ms)
            .await;
        Ok(())
    }

    /// Record a refresh and write a snapshot if enough has happened since the
    /// last one.
    pub(crate) async fn note_refresh(
        &self,
        canonical: &Path,
        state: &Arc<IndexState>,
        files: u64,
        ms: u64,
    ) {
        if let Ok(mut s) = state.status.lock() {
            s.refreshes += 1;
            s.last_refresh_ms = ms;
            s.last_refresh_files = files;
        }
        let since = state
            .refreshes_since_checkpoint
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        let due = state
            .last_checkpoint
            .lock()
            .map(|t| t.elapsed() >= CHECKPOINT_EVERY)
            .unwrap_or(true);
        if (since >= checkpoint_every_refreshes() || due)
            && let Err(e) = self.checkpoint_indexes(canonical).await
        {
            eprintln!("modbit-core: index snapshot not written: {e:#}");
        }
    }

    /// Write the workspace's indexes to the store now.
    pub(crate) async fn checkpoint_indexes(&self, canonical: &Path) -> Result<()> {
        SnapshotHandles {
            state: self.index_state(canonical).await?,
            index: self.index(canonical).await?,
            symbols: self.symbols(canonical).await?,
            refs: self.refs(canonical).await?,
            graph: self.graph(canonical).await?,
            base: self.data_dir_for_indexes(),
        }
        .write()
        .await
    }
}

/// What a snapshot of one workspace's indexes needs: the live structures and
/// where they persist.
struct SnapshotHandles {
    state: Arc<IndexState>,
    index: Arc<Mutex<RepositoryIndex>>,
    symbols: Arc<Mutex<SymbolIndex>>,
    refs: Arc<Mutex<RefGraph>>,
    graph: Arc<Mutex<EvidenceGraph>>,
    base: PathBuf,
}

impl SnapshotHandles {
    async fn write(&self) -> Result<()> {
        let state = &self.state;
        let (revision, records, tri, tri_files) = {
            let idx = self.index.lock().await;
            let sym = self.symbols.lock().await;
            let rg = self.refs.lock().await;
            let gr = self.graph.lock().await;
            (
                idx.revision(),
                assemble_records(&idx, &sym, &rg, &gr),
                persist::encode_trigram_snapshot(idx.trigram_index()),
                idx.trigram_index().len(),
            )
        };
        let (store, began) = (state.store.clone(), Instant::now());
        let st2 = store.clone();
        let stats = tokio::task::spawn_blocking(move || {
            st2.checkpoint_encoded(revision, &records, &tri, tri_files)
        })
        .await
        .context("writing the index snapshot")?
        .context("writing the index snapshot")?;
        if let Ok(mut s) = state.status.lock() {
            s.generation = stats.generation;
            s.persisted_bytes = store.size_on_disk();
            s.last_checkpoint_ms = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
        }
        state.refreshes_since_checkpoint.store(0, Ordering::Relaxed);
        if let Ok(mut t) = state.last_checkpoint.lock() {
            *t = Instant::now();
        }
        persist::evict(&self.base, store.dir(), store_cap_bytes());
        Ok(())
    }
}

/// Open the indexes of every workspace an unfinished task works in, in the
/// background, so the first query after a restart finds them loaded.
pub(crate) fn warm_indexes(core: Arc<crate::server::Core>, roots: Vec<String>) {
    tokio::spawn(async move {
        let mut seen: HashMap<PathBuf, ()> = HashMap::new();
        for r in roots {
            let Ok(canonical) = Path::new(&r).canonicalize() else {
                continue;
            };
            if seen.insert(canonical.clone(), ()).is_some() {
                continue;
            }
            if let Err(e) = core.tools.ensure_loaded(&canonical).await {
                eprintln!(
                    "modbit-core: warming indexes of {}: {e:#}",
                    canonical.display()
                );
            }
        }
    });
}
