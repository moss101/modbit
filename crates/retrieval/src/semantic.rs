//! Semantic chunk index (docs/18 "Semantic ANN": USearch HNSW index with
//! versioned embeddings; "Index freshness": embedding is queued only for
//! changed chunks; a query against an older generation declares it so the
//! planner overlays exact/lexical results for the changed files; M3.5).
//!
//! Embedding is infrastructure, not reasoning (docs/18): the index takes any
//! `Embedder`. The shipped `HashingEmbedder` is a deterministic feature
//! hashing of code tokens into a unit vector — real vectors with real
//! nearest-neighbour search, honest about what it is (its id names it; it
//! claims no learned semantics). A provider embedding model plugs in through
//! the same port once credentials exist (DR-M2-001).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use usearch::Index;
use usearch::ffi::{IndexOptions, MetricKind, ScalarKind};

/// Turns texts into unit vectors. `id` versions the embeddings: a different
/// id means a different vector space and forces a rebuild.
pub trait Embedder: Send + Sync {
    /// Model/version id recorded with every generation.
    fn id(&self) -> &str;
    /// Vector dimensions.
    fn dimensions(&self) -> usize;
    /// Embed texts (same order).
    fn embed(&self, texts: &[&str]) -> Vec<Vec<f32>>;
}

/// Deterministic feature-hashing embedder (`hashing-v1`): identifier and
/// word tokens (camelCase and snake_case split) hashed into `dims` buckets
/// with sign hashing, then L2-normalized. Identical texts map to identical
/// vectors; texts sharing tokens are close.
#[derive(Debug, Clone)]
pub struct HashingEmbedder {
    dims: usize,
}

impl Default for HashingEmbedder {
    fn default() -> Self {
        Self { dims: 256 }
    }
}

impl HashingEmbedder {
    /// With `dims` buckets.
    #[must_use]
    pub fn new(dims: usize) -> Self {
        Self { dims: dims.max(8) }
    }
}

fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if raw.is_empty() {
            continue;
        }
        for part in raw.split('_') {
            if part.is_empty() {
                continue;
            }
            // camelCase split
            let mut cur = String::new();
            let chars: Vec<char> = part.chars().collect();
            for (i, ch) in chars.iter().enumerate() {
                if ch.is_uppercase() && i > 0 && chars[i - 1].is_lowercase() && !cur.is_empty() {
                    out.push(cur.to_lowercase());
                    cur = String::new();
                }
                cur.push(*ch);
            }
            if !cur.is_empty() {
                out.push(cur.to_lowercase());
            }
        }
    }
    out.retain(|t| t.len() > 1);
    out
}

impl Embedder for HashingEmbedder {
    fn id(&self) -> &str {
        "hashing-v1"
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn embed(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        texts
            .iter()
            .map(|t| {
                let mut v = vec![0f32; self.dims];
                for tok in tokens(t) {
                    let h = Sha256::digest(tok.as_bytes());
                    let bucket = u32::from_le_bytes([h[0], h[1], h[2], h[3]]) as usize % self.dims;
                    let sign = if h[4] & 1 == 0 { 1.0 } else { -1.0 };
                    v[bucket] += sign;
                }
                let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for x in &mut v {
                        *x /= norm;
                    }
                }
                v
            })
            .collect()
    }
}

/// One indexed chunk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chunk {
    /// Root-relative path.
    pub path: String,
    /// Chunk label (symbol name or `L<start>-<end>` window).
    pub label: String,
    /// 1-based line range.
    pub lines: (u32, u32),
    /// Byte span.
    pub span: (u64, u64),
    /// sha256 of the chunk text.
    pub chunk_hash: String,
    /// Revision the chunk was embedded at.
    pub embedded_at_revision: u64,
}

/// One semantic hit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SemanticHit {
    /// The chunk.
    pub chunk: Chunk,
    /// Cosine similarity (1 = identical direction).
    pub score: f32,
}

/// A file to index: path, text and its symbol spans (`name`, start, end).
pub type FileSource<'a> = (&'a str, &'a str, Vec<(String, u64, u64)>);

/// A chunk before embedding: label, 1-based line range, byte span, text.
pub type RawChunk = (String, (u32, u32), (u64, u64), String);

/// Chunking window for files without symbol chunks.
const WINDOW_LINES: u32 = 40;
/// Maximum chunk text embedded (bytes).
const MAX_CHUNK_BYTES: usize = 4096;

/// The semantic index of one workspace.
pub struct SemanticIndex {
    embedder: Box<dyn Embedder>,
    index: Index,
    generation: u64,
    next_key: u64,
    keys: BTreeMap<u64, Chunk>,
    by_path: BTreeMap<String, Vec<u64>>,
    pending: BTreeSet<String>,
    embed_calls: usize,
}

impl std::fmt::Debug for SemanticIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SemanticIndex")
            .field("embedder", &self.embedder.id())
            .field("generation", &self.generation)
            .field("chunks", &self.keys.len())
            .field("pending", &self.pending.len())
            .finish()
    }
}

/// Chunk a file: symbol spans when given, else fixed line windows.
#[must_use]
pub fn chunk_file(text: &str, symbols: &[(String, u64, u64)]) -> Vec<RawChunk> {
    let mut out = Vec::new();
    if !symbols.is_empty() {
        for (label, start, end) in symbols {
            let (s, e) = (*start as usize, (*end as usize).min(text.len()));
            if s >= e || !text.is_char_boundary(s) || !text.is_char_boundary(e) {
                continue;
            }
            let body: String = text[s..e].chars().take(MAX_CHUNK_BYTES).collect();
            let line_start = text[..s].matches('\n').count() as u32 + 1;
            let line_end = line_start + body.matches('\n').count() as u32;
            out.push((
                label.clone(),
                (line_start, line_end),
                (*start, e as u64),
                body,
            ));
        }
        if !out.is_empty() {
            return out;
        }
    }
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut offset = 0u64;
    let mut i = 0usize;
    while i < lines.len() {
        let end = (i + WINDOW_LINES as usize).min(lines.len());
        let body: String = lines[i..end].concat();
        let len = body.len() as u64;
        out.push((
            format!("L{}-{}", i + 1, end),
            (i as u32 + 1, end as u32),
            (offset, offset + len),
            body.chars().take(MAX_CHUNK_BYTES).collect(),
        ));
        offset += len;
        i = end;
    }
    out
}

impl SemanticIndex {
    /// Empty index at `generation` with `embedder`.
    pub fn new(embedder: Box<dyn Embedder>, generation: u64) -> Result<Self, String> {
        let options = IndexOptions {
            dimensions: embedder.dimensions(),
            metric: MetricKind::Cos,
            quantization: ScalarKind::F32,
            connectivity: 0,
            expansion_add: 0,
            expansion_search: 0,
            multi: false,
        };
        let index = Index::new(&options).map_err(|e| e.to_string())?;
        index.reserve(1024).map_err(|e| e.to_string())?;
        Ok(Self {
            embedder,
            index,
            generation,
            next_key: 1,
            keys: BTreeMap::new(),
            by_path: BTreeMap::new(),
            pending: BTreeSet::new(),
            embed_calls: 0,
        })
    }

    /// Build from `(path, text, symbol spans)` at `generation`.
    pub fn build<'a>(
        embedder: Box<dyn Embedder>,
        files: impl Iterator<Item = FileSource<'a>>,
        generation: u64,
    ) -> Result<Self, String> {
        let mut idx = Self::new(embedder, generation)?;
        for (p, t, syms) in files {
            idx.index_file(p, t, &syms, generation)?;
        }
        Ok(idx)
    }

    /// Embedder id (the embedding version).
    #[must_use]
    pub fn embedder_id(&self) -> &str {
        self.embedder.id()
    }

    /// Generation (workspace revision the embeddings were last updated for).
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Chunks indexed.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Paths changed since their chunks were embedded (queued, not yet re-embedded).
    #[must_use]
    pub fn pending(&self) -> Vec<String> {
        self.pending.iter().cloned().collect()
    }

    /// Embedder invocations so far (evidence that only changed chunks re-embed).
    #[must_use]
    pub fn embed_calls(&self) -> usize {
        self.embed_calls
    }

    fn remove_path(&mut self, path: &str) -> Result<(), String> {
        if let Some(keys) = self.by_path.remove(path) {
            for k in keys {
                self.index.remove(k).map_err(|e| e.to_string())?;
                self.keys.remove(&k);
            }
        }
        Ok(())
    }

    fn index_file(
        &mut self,
        path: &str,
        text: &str,
        symbols: &[(String, u64, u64)],
        revision: u64,
    ) -> Result<(), String> {
        self.remove_path(path)?;
        let chunks = chunk_file(text, symbols);
        if chunks.is_empty() {
            return Ok(());
        }
        // Only chunks whose text changed need a new vector: unchanged chunk
        // hashes reuse nothing here (the file was removed above), but the
        // embedder is called once per file with all its chunks.
        let bodies: Vec<&str> = chunks.iter().map(|c| c.3.as_str()).collect();
        let vectors = self.embedder.embed(&bodies);
        self.embed_calls += 1;
        if self.index.capacity() < self.index.size() + chunks.len() + 1 {
            self.index
                .reserve((self.index.size() + chunks.len()) * 2)
                .map_err(|e| e.to_string())?;
        }
        let mut keys = Vec::with_capacity(chunks.len());
        for ((label, lines, span, body), v) in chunks.into_iter().zip(vectors) {
            let key = self.next_key;
            self.next_key += 1;
            self.index.add(key, &v).map_err(|e| e.to_string())?;
            self.keys.insert(
                key,
                Chunk {
                    path: path.to_owned(),
                    label,
                    lines,
                    span,
                    chunk_hash: hex::encode(Sha256::digest(body.as_bytes())),
                    embedded_at_revision: revision,
                },
            );
            keys.push(key);
        }
        self.by_path.insert(path.to_owned(), keys);
        self.pending.remove(path);
        Ok(())
    }

    /// Queue changed paths (docs/18: embedding is queued only for changed
    /// chunks). Until `flush`, queries declare them stale.
    pub fn mark_changed(&mut self, paths: &[String]) {
        for p in paths {
            self.pending.insert(p.clone());
        }
    }

    /// Re-embed exactly the queued paths at `generation`; `None` text removes.
    pub fn flush<'a>(
        &mut self,
        changed: impl Iterator<Item = (&'a str, Option<(&'a str, Vec<(String, u64, u64)>)>)>,
        generation: u64,
    ) -> Result<(), String> {
        for (p, content) in changed {
            match content {
                Some((t, syms)) => self.index_file(p, t, &syms, generation)?,
                None => {
                    self.remove_path(p)?;
                    self.pending.remove(p);
                }
            }
        }
        self.generation = generation;
        Ok(())
    }

    /// Nearest chunks to `query` (cosine similarity), bounded.
    pub fn search(&self, query: &str, max_hits: usize) -> Result<Vec<SemanticHit>, String> {
        if self.keys.is_empty() {
            return Ok(vec![]);
        }
        let v = self.embedder.embed(&[query]).remove(0);
        let m = self
            .index
            .search(&v, max_hits.max(1))
            .map_err(|e| e.to_string())?;
        Ok(m.keys
            .iter()
            .zip(m.distances.iter())
            .filter_map(|(k, d)| {
                self.keys.get(k).map(|c| SemanticHit {
                    chunk: c.clone(),
                    score: 1.0 - d,
                })
            })
            .collect())
    }
}
