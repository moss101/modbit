//! The persisted index store (PX-111; docs/18 "Concrete local indexing
//! stack", "Index freshness"): what the derived indexes know about a workspace,
//! kept on disk under the profile so a restarted Core serves its first query
//! without deriving it again.
//!
//! The filesystem and the revision stay canonical and an index never becomes
//! the source of file bytes. The store keeps *derived* data only, and keys all
//! of it by what it was derived from: every per-file record carries the content
//! hash of the bytes it was computed from, and a load uses a record only for a
//! file whose bytes hash the same today — so a stale record can never be served,
//! whatever the age of the snapshot, and an edit made while the Core was down
//! is found by the same diff that finds a clean restart.
//!
//! Layout of one workspace's directory (`<store>/<sha256(root)[..16]>/`):
//!
//! ```text
//! manifest.json            one JSON line, then the sha256 of that line; names the blobs
//! derived.<gen>.json       per-file records: symbols, reference facts, import specifiers
//! trigrams.<gen>.bin       per-file trigram sets (the exact-search prefilter)
//! lexical/                 the Tantivy index (its own checksummed files; each file's
//!                          record of path and content hash is in the index itself)
//! ```
//!
//! Writes are crash-safe by construction. A blob is written to a temporary
//! file, synced and renamed to a name that carries the generation, so it never
//! replaces what the current manifest points at; the manifest is written last,
//! the same way, and its rename is the commit. A process that dies anywhere
//! before that rename leaves the previous manifest and the blobs it names
//! intact (and some garbage that the next write removes); one that dies after
//! it leaves the new generation whole. Every blob is checked against the
//! sha256 in the manifest on load, the manifest against its own, and a format
//! or fact-version mismatch discards the lot: damage is detected, discarded
//! with a recorded reason and rebuilt, never trusted. The Tantivy directory is
//! validated with Tantivy's own file checksums.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::refs::{FACTS_VERSION, FileFacts};
use crate::symbols::Symbol;
use crate::trigram::{Trigram, TrigramIndex};

/// Version of the store's formats; a manifest of another version is discarded.
pub const FORMAT_VERSION: u32 = 1;
const MANIFEST: &str = "manifest.json";
const TRIGRAM_MAGIC: &[u8; 6] = b"MBTG\x01\x00";
/// Default size cap of the whole store (all workspaces), bytes.
pub const DEFAULT_STORE_CAP_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// What the derived indexes hold about one file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileRecord {
    /// Root-relative path.
    pub path: String,
    /// sha256 (hex) of the bytes the record was derived from.
    pub hash: String,
    /// Language label.
    pub language: Option<String>,
    /// Definitions.
    pub symbols: Vec<Symbol>,
    /// Reference facts.
    pub facts: FileFacts,
    /// Raw import specifiers.
    pub imports: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BlobRef {
    /// `derived` | `trigrams`.
    name: String,
    file: String,
    sha256: String,
    bytes: u64,
    files: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Manifest {
    format: u32,
    facts_version: u32,
    root: String,
    generation: u64,
    written_at_ms: u64,
    workspace_revision: u64,
    blobs: Vec<BlobRef>,
}

/// What a load found.
#[derive(Debug, Default)]
pub struct Persisted {
    /// Per-file records, when the derived blob loaded.
    pub records: Option<BTreeMap<String, FileRecord>>,
    /// Trigram sets, when that blob loaded.
    pub trigrams: Option<TrigramIndex>,
    /// Generation of the manifest that was read (0 when none was usable).
    pub generation: u64,
    /// Workspace revision the snapshot was written at.
    pub revision: u64,
    /// Why something was discarded, as `component: reason` (empty when the
    /// store loaded whole or was simply empty).
    pub reasons: Vec<String>,
    /// There was no manifest at all (a first run, not damage).
    pub first_run: bool,
    /// Bytes of the blobs read.
    pub bytes: u64,
}

/// What a checkpoint wrote.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckpointStats {
    /// The generation written.
    pub generation: u64,
    /// Bytes of blobs written.
    pub bytes: u64,
    /// Files in the snapshot.
    pub files: u64,
    /// Stale blobs and temporary files removed.
    pub removed: usize,
}

/// One workspace's directory in the store.
#[derive(Clone, Debug)]
pub struct IndexStore {
    dir: PathBuf,
    root: String,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn sha_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Where in a write the process dies under fault injection
/// (`MODBIT_FAULT_INDEX_ABORT`): `blob` (half a blob written, not renamed),
/// `before_manifest` (every blob in place, manifest not yet written),
/// `mid_manifest` (half the manifest written, not renamed) or
/// `after_manifest` (committed, nothing cleaned). Unset in production.
fn fault(stage: &str) -> bool {
    std::env::var("MODBIT_FAULT_INDEX_ABORT").is_ok_and(|v| v == stage)
}

fn die() -> ! {
    eprintln!("modbit-retrieval: fault injection: aborting inside an index write");
    std::process::abort();
}

#[cfg(unix)]
fn sync_dir(dir: &Path) {
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_dir(_dir: &Path) {}

impl IndexStore {
    /// The store directory of `root` under `base`.
    #[must_use]
    pub fn open(base: &Path, root: &Path) -> Self {
        let root_text = root.to_string_lossy().into_owned();
        let key = sha_hex(root_text.as_bytes());
        Self {
            dir: base.join(&key[..16]),
            root: root_text,
        }
    }

    /// The workspace's directory.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Where the Tantivy index lives.
    #[must_use]
    pub fn lexical_dir(&self) -> PathBuf {
        self.dir.join("lexical")
    }

    /// Remove everything of this workspace's directory (a discarded index).
    pub fn discard_lexical(&self) {
        let _ = std::fs::remove_dir_all(self.lexical_dir());
    }

    /// Bytes on disk under this workspace's directory.
    #[must_use]
    pub fn size_on_disk(&self) -> u64 {
        dir_size(&self.dir)
    }

    fn write_atomic(&self, name: &str, bytes: &[u8], blob_fault: bool) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let tmp = self.dir.join(format!("{name}.tmp-{}", std::process::id()));
        let mut f = std::fs::File::create(&tmp)?;
        if blob_fault && fault("blob") {
            f.write_all(&bytes[..bytes.len() / 2])?;
            f.sync_all()?;
            die();
        }
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, self.dir.join(name))?;
        sync_dir(&self.dir);
        Ok(())
    }

    /// Write a snapshot: the per-file records and the trigram sets, at
    /// `revision`. Returns what was written. Blobs first, manifest last.
    pub fn checkpoint(
        &self,
        revision: u64,
        records: &[FileRecord],
        trigrams: &TrigramIndex,
    ) -> std::io::Result<CheckpointStats> {
        self.checkpoint_encoded(
            revision,
            records,
            &encode_trigrams(trigrams),
            trigrams.len(),
        )
    }

    /// [`Self::checkpoint`] with the trigram sets already encoded
    /// ([`encode_trigram_snapshot`]), so a caller that must not hold the live
    /// index while it writes can encode under its lock and write after it.
    pub fn checkpoint_encoded(
        &self,
        revision: u64,
        records: &[FileRecord],
        tri: &[u8],
        trigram_files: usize,
    ) -> std::io::Result<CheckpointStats> {
        let previous = self.read_manifest().ok().map_or(0, |m| m.generation);
        let generation = previous + 1;
        let derived = serde_json::to_vec(records)
            .map_err(|e| std::io::Error::other(format!("encoding records: {e}")))?;
        let derived_name = format!("derived.{generation}.json");
        let tri_name = format!("trigrams.{generation}.bin");
        self.write_atomic(&derived_name, &derived, true)?;
        self.write_atomic(&tri_name, tri, true)?;
        if fault("before_manifest") {
            die();
        }
        let manifest = Manifest {
            format: FORMAT_VERSION,
            facts_version: FACTS_VERSION,
            root: self.root.clone(),
            generation,
            written_at_ms: now_ms(),
            workspace_revision: revision,
            blobs: vec![
                BlobRef {
                    name: "derived".into(),
                    file: derived_name.clone(),
                    sha256: sha_hex(&derived),
                    bytes: derived.len() as u64,
                    files: records.len() as u64,
                },
                BlobRef {
                    name: "trigrams".into(),
                    file: tri_name.clone(),
                    sha256: sha_hex(tri),
                    bytes: tri.len() as u64,
                    files: trigram_files as u64,
                },
            ],
        };
        let body = serde_json::to_string(&manifest)
            .map_err(|e| std::io::Error::other(format!("encoding manifest: {e}")))?;
        let text = format!("{body}\n{}\n", sha_hex(body.as_bytes()));
        if fault("mid_manifest") {
            std::fs::create_dir_all(&self.dir)?;
            let tmp = self
                .dir
                .join(format!("{MANIFEST}.tmp-{}", std::process::id()));
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&text.as_bytes()[..text.len() / 2])?;
            f.sync_all()?;
            die();
        }
        self.write_atomic(MANIFEST, text.as_bytes(), false)?;
        if fault("after_manifest") {
            die();
        }
        let removed = self.collect_garbage(&[derived_name, tri_name]);
        Ok(CheckpointStats {
            generation,
            bytes: (derived.len() + tri.len()) as u64,
            files: records.len() as u64,
            removed,
        })
    }

    /// Remove blobs and temporary files the current manifest does not name.
    fn collect_garbage(&self, keep: &[String]) -> usize {
        let mut removed = 0;
        let Ok(rd) = std::fs::read_dir(&self.dir) else {
            return 0;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let garbage = (name.starts_with("derived.") || name.starts_with("trigrams."))
                && !keep.contains(&name)
                || name.contains(".tmp-");
            if garbage && std::fs::remove_file(e.path()).is_ok() {
                removed += 1;
            }
        }
        removed
    }

    fn read_manifest(&self) -> Result<Manifest, String> {
        let text = std::fs::read_to_string(self.dir.join(MANIFEST)).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "missing".to_owned()
            } else {
                format!("unreadable: {e}")
            }
        })?;
        let mut lines = text.lines();
        let body = lines.next().ok_or("empty manifest")?;
        let sum = lines.next().ok_or("manifest has no checksum line")?;
        if sha_hex(body.as_bytes()) != sum.trim() {
            return Err("manifest checksum mismatch".into());
        }
        serde_json::from_str(body).map_err(|e| format!("manifest does not parse: {e}"))
    }

    /// Load what the store holds. Never fails: whatever cannot be trusted is
    /// reported in `reasons` and left out, and the caller derives it.
    #[must_use]
    pub fn load(&self) -> Persisted {
        let mut out = Persisted::default();
        let manifest = match self.read_manifest() {
            Ok(m) => m,
            Err(e) if e == "missing" => {
                out.first_run = true;
                return out;
            }
            Err(e) => {
                out.reasons.push(format!("manifest: {e}"));
                return out;
            }
        };
        if manifest.format != FORMAT_VERSION || manifest.facts_version != FACTS_VERSION {
            out.reasons.push(format!(
                "manifest: format {}.{} is not this build's {FORMAT_VERSION}.{FACTS_VERSION}",
                manifest.format, manifest.facts_version
            ));
            return out;
        }
        if manifest.root != self.root {
            out.reasons.push(format!(
                "manifest: written for another root ({})",
                manifest.root
            ));
            return out;
        }
        out.generation = manifest.generation;
        out.revision = manifest.workspace_revision;
        for blob in &manifest.blobs {
            let bytes = match std::fs::read(self.dir.join(&blob.file)) {
                Ok(b) => b,
                Err(e) => {
                    out.reasons
                        .push(format!("{}: {} is unreadable: {e}", blob.name, blob.file));
                    continue;
                }
            };
            if bytes.len() as u64 != blob.bytes || sha_hex(&bytes) != blob.sha256 {
                out.reasons
                    .push(format!("{}: checksum mismatch in {}", blob.name, blob.file));
                continue;
            }
            out.bytes += bytes.len() as u64;
            match blob.name.as_str() {
                "derived" => match serde_json::from_slice::<Vec<FileRecord>>(&bytes) {
                    Ok(v) => {
                        out.records = Some(v.into_iter().map(|r| (r.path.clone(), r)).collect());
                    }
                    Err(e) => out.reasons.push(format!("derived: does not parse: {e}")),
                },
                "trigrams" => match decode_trigrams(&bytes) {
                    Some(t) => out.trigrams = Some(t),
                    None => out.reasons.push("trigrams: does not decode".into()),
                },
                other => out
                    .reasons
                    .push(format!("manifest: unknown blob `{other}`")),
            }
        }
        out
    }
}

fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    for e in rd.flatten() {
        match e.metadata() {
            Ok(m) if m.is_dir() => total += dir_size(&e.path()),
            Ok(m) => total += m.len(),
            Err(_) => {}
        }
    }
    total
}

/// Keep the whole store under `max_bytes` by removing the workspace
/// directories used least recently (by manifest age), never `keep`. Returns
/// the directories removed.
pub fn evict(base: &Path, keep: &Path, max_bytes: u64) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(base) else {
        return vec![];
    };
    let mut dirs: Vec<(std::time::SystemTime, u64, PathBuf)> = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let used = std::fs::metadata(p.join(MANIFEST))
            .and_then(|m| m.modified())
            .or_else(|_| std::fs::metadata(&p).and_then(|m| m.modified()))
            .unwrap_or(std::time::UNIX_EPOCH);
        dirs.push((used, dir_size(&p), p));
    }
    let mut total: u64 = dirs.iter().map(|(_, s, _)| *s).sum();
    dirs.sort_by_key(|(t, _, _)| *t);
    let mut removed = Vec::new();
    for (_, size, p) in dirs {
        if total <= max_bytes {
            break;
        }
        if p == keep {
            continue;
        }
        if std::fs::remove_dir_all(&p).is_ok() {
            total = total.saturating_sub(size);
            removed.push(p);
        }
    }
    removed
}

/// The trigram sets as the bytes a snapshot stores.
#[must_use]
pub fn encode_trigram_snapshot(t: &TrigramIndex) -> Vec<u8> {
    encode_trigrams(t)
}

fn encode_trigrams(t: &TrigramIndex) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(TRIGRAM_MAGIC);
    out.extend_from_slice(&(t.len() as u32).to_le_bytes());
    for f in t.entries() {
        let path = f.path.as_bytes();
        out.extend_from_slice(&(path.len() as u32).to_le_bytes());
        out.extend_from_slice(path);
        let hash = hex::decode(&f.hash).unwrap_or_default();
        out.push(u8::try_from(hash.len()).unwrap_or(0));
        out.extend_from_slice(&hash);
        out.extend_from_slice(&(f.trigrams.len() as u32).to_le_bytes());
        for tri in &f.trigrams {
            out.extend_from_slice(tri);
        }
    }
    out
}

fn decode_trigrams(b: &[u8]) -> Option<TrigramIndex> {
    let mut at = 0usize;
    let take = |at: &mut usize, n: usize| -> Option<&[u8]> {
        let s = b.get(*at..*at + n)?;
        *at += n;
        Some(s)
    };
    if take(&mut at, TRIGRAM_MAGIC.len())? != TRIGRAM_MAGIC {
        return None;
    }
    let n = u32::from_le_bytes(take(&mut at, 4)?.try_into().ok()?);
    let mut out = TrigramIndex::new();
    for _ in 0..n {
        let plen = u32::from_le_bytes(take(&mut at, 4)?.try_into().ok()?) as usize;
        let path = std::str::from_utf8(take(&mut at, plen)?).ok()?.to_owned();
        let hlen = usize::from(*take(&mut at, 1)?.first()?);
        let hash = hex::encode(take(&mut at, hlen)?);
        let count = u32::from_le_bytes(take(&mut at, 4)?.try_into().ok()?) as usize;
        let raw = take(&mut at, count.checked_mul(3)?)?;
        let tris: Vec<Trigram> = raw.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
        out.insert(&path, &hash, tris);
    }
    (at == b.len()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigram::trigrams_of;

    fn record(path: &str, hash: &str) -> FileRecord {
        FileRecord {
            path: path.into(),
            hash: hash.into(),
            language: Some("rust".into()),
            symbols: vec![],
            facts: FileFacts::default(),
            imports: vec!["crate::a".into()],
        }
    }

    fn sample() -> (Vec<FileRecord>, TrigramIndex) {
        let mut t = TrigramIndex::new();
        let h = "ab".repeat(32);
        t.insert("a.rs", &h, trigrams_of(b"fn compute_total() {}"));
        t.insert("b.rs", &h, trigrams_of(b"fn other() {}"));
        (vec![record("a.rs", &h), record("b.rs", &h)], t)
    }

    #[test]
    fn a_snapshot_round_trips_and_a_second_generation_replaces_the_first() {
        let base = tempfile::tempdir().unwrap();
        let store = IndexStore::open(base.path(), Path::new("/ws"));
        assert!(store.load().first_run);
        let (records, tri) = sample();
        let s1 = store.checkpoint(5, &records, &tri).unwrap();
        assert_eq!(s1.generation, 1);
        let p = store.load();
        assert!(p.reasons.is_empty(), "{:?}", p.reasons);
        assert_eq!(p.generation, 1);
        assert_eq!(p.revision, 5);
        assert_eq!(p.records.as_ref().unwrap().len(), 2);
        let t = p.trigrams.unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(
            t.candidates(&crate::trigram::required_trigrams("compute", false).unwrap()),
            vec!["a.rs"]
        );
        let s2 = store.checkpoint(6, &records, &tri).unwrap();
        assert_eq!(s2.generation, 2);
        assert!(s2.removed >= 2, "the first generation's blobs are removed");
        let names: Vec<String> = std::fs::read_dir(store.dir())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().all(|n| !n.contains(".1.")), "{names:?}");
        assert_eq!(store.load().generation, 2);
    }

    #[test]
    fn damage_is_detected_named_and_never_trusted() {
        let base = tempfile::tempdir().unwrap();
        let store = IndexStore::open(base.path(), Path::new("/ws"));
        let (records, tri) = sample();
        store.checkpoint(1, &records, &tri).unwrap();
        // A flipped byte in the derived blob: caught by its checksum, the
        // trigram blob is still good.
        let derived = store.dir().join("derived.1.json");
        let mut bytes = std::fs::read(&derived).unwrap();
        let at = bytes.iter().position(|b| *b == b'a').unwrap();
        bytes[at] = b'b';
        std::fs::write(&derived, &bytes).unwrap();
        let p = store.load();
        assert!(p.records.is_none() && p.trigrams.is_some());
        assert!(
            p.reasons
                .iter()
                .any(|r| r.starts_with("derived: checksum mismatch")),
            "{:?}",
            p.reasons
        );
        // A flipped byte in the manifest: everything is discarded.
        let m = store.dir().join(MANIFEST);
        let mut mb = std::fs::read(&m).unwrap();
        mb[10] ^= 1;
        std::fs::write(&m, &mb).unwrap();
        let p = store.load();
        assert!(p.records.is_none() && p.trigrams.is_none());
        assert!(
            p.reasons.iter().any(|r| r.starts_with("manifest:")),
            "{:?}",
            p.reasons
        );
        // A truncated trigram blob.
        let (records, tri) = sample();
        store.checkpoint(2, &records, &tri).unwrap();
        let g = store.load().generation;
        let tri_file = store.dir().join(format!("trigrams.{g}.bin"));
        let b = std::fs::read(&tri_file).unwrap();
        std::fs::write(&tri_file, &b[..b.len() - 5]).unwrap();
        let p = store.load();
        assert!(p.trigrams.is_none() && p.records.is_some());
        // Another root's manifest is not this root's.
        let other = IndexStore {
            dir: store.dir().to_path_buf(),
            root: "/elsewhere".into(),
        };
        assert!(
            other
                .load()
                .reasons
                .iter()
                .any(|r| r.contains("another root"))
        );
    }

    #[test]
    fn eviction_keeps_the_store_under_its_cap_and_spares_the_workspace_in_use() {
        let base = tempfile::tempdir().unwrap();
        let (records, tri) = sample();
        let mut stores = Vec::new();
        for n in 0..3 {
            let s = IndexStore::open(base.path(), Path::new(&format!("/ws{n}")));
            s.checkpoint(1, &records, &tri).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
            stores.push(s);
        }
        let each = stores[0].size_on_disk();
        let removed = evict(base.path(), stores[0].dir(), each * 2);
        assert_eq!(removed.len(), 1, "{removed:?}");
        assert_eq!(
            removed[0],
            stores[1].dir(),
            "the least recently written other than the one in use"
        );
        assert!(stores[0].dir().exists() && stores[2].dir().exists());
    }
}
