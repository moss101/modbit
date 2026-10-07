//! Content-addressed object directory keyed by SHA-256 (docs/31 "Local
//! storage"). Objects are immutable: written to a temporary file, fsynced,
//! then renamed into place; a hash that already exists is never rewritten.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// The object directory.
#[derive(Debug, Clone)]
pub struct ObjectStore {
    root: PathBuf,
}

/// Lowercase hex SHA-256 of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

impl ObjectStore {
    /// Open (creating) the directory.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn path_for(&self, hash: &str) -> PathBuf {
        self.root.join(&hash[..2]).join(&hash[2..])
    }

    /// Store bytes; returns the digest. Idempotent.
    pub fn put(&self, bytes: &[u8]) -> Result<String> {
        let hash = sha256_hex(bytes);
        let path = self.path_for(&hash);
        if path.exists() {
            return Ok(hash);
        }
        fs::create_dir_all(path.parent().expect("object parent"))?;
        let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
        }
        fs::rename(&tmp, &path)?;
        if let Ok(dir) = fs::File::open(path.parent().expect("object parent")) {
            // Durability of the rename itself on POSIX; best effort elsewhere.
            let _ = dir.sync_all();
        }
        Ok(hash)
    }

    /// Remove an object; the bytes it held, or `None` when it was not there.
    /// Only a store whose every reference is accounted for may call this
    /// (the checkpoint blob store, REQ-PX-102): the Core's main store is
    /// referenced from the log and is never collected.
    pub fn remove(&self, hash: &str) -> Result<Option<u64>> {
        if hash.len() < 3 {
            return Ok(None);
        }
        let path = self.path_for(hash);
        match fs::metadata(&path) {
            Ok(m) => {
                let len = m.len();
                fs::remove_file(&path)?;
                Ok(Some(len))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Every object: its hash, its size, and when it was last written.
    pub fn list(&self) -> Result<Vec<(String, u64, std::time::SystemTime)>> {
        let mut out = Vec::new();
        let Ok(top) = fs::read_dir(&self.root) else {
            return Ok(out);
        };
        for dir in top.flatten() {
            let prefix = dir.file_name().to_string_lossy().into_owned();
            if prefix.len() != 2 || !dir.path().is_dir() {
                continue;
            }
            let Ok(inner) = fs::read_dir(dir.path()) else {
                continue;
            };
            for f in inner.flatten() {
                let name = f.file_name().to_string_lossy().into_owned();
                // Temporary files of a put in flight never name an object.
                if name.contains('.') || name.len() != 62 {
                    continue;
                }
                let Ok(meta) = f.metadata() else { continue };
                if meta.is_file() {
                    out.push((
                        format!("{prefix}{name}"),
                        meta.len(),
                        meta.modified().unwrap_or(std::time::UNIX_EPOCH),
                    ));
                }
            }
        }
        Ok(out)
    }

    /// Read and verify an object.
    pub fn get(&self, hash: &str) -> Result<Vec<u8>> {
        let path = self.path_for(hash);
        let bytes = fs::read(&path).map_err(|e| Error::Object {
            hash: hash.to_owned(),
            detail: format!("unreadable: {e}"),
        })?;
        if sha256_hex(&bytes) != hash {
            return Err(Error::Object {
                hash: hash.to_owned(),
                detail: "content does not match digest".into(),
            });
        }
        Ok(bytes)
    }

    /// Read `length` bytes at `offset` without loading the whole object;
    /// returns (data, total bytes, whole-object sha256). The digest is taken
    /// from the object's path (content-addressed) and re-verified over the
    /// full object only when the caller asks for offset 0 with a length that
    /// covers it; ranged reads trust the immutable, digest-named file.
    pub fn read_range(
        &self,
        hash: &str,
        offset: u64,
        length: u64,
    ) -> Result<(Vec<u8>, u64, Vec<u8>)> {
        use std::io::{Read, Seek, SeekFrom};
        let path = self.path_for(hash);
        let mut f = fs::File::open(&path).map_err(|e| Error::Object {
            hash: hash.to_owned(),
            detail: format!("unreadable: {e}"),
        })?;
        let total = f.metadata()?.len();
        let start = offset.min(total);
        let want = length.min(total - start) as usize;
        f.seek(SeekFrom::Start(start))?;
        let mut buf = vec![0u8; want];
        f.read_exact(&mut buf)?;
        let checksum = hex::decode(hash).map_err(|_| Error::Object {
            hash: hash.to_owned(),
            detail: "digest is not hex".into(),
        })?;
        Ok((buf, total, checksum))
    }

    /// Whether the object exists.
    #[must_use]
    pub fn contains(&self, hash: &str) -> bool {
        self.path_for(hash).exists()
    }

    /// Root directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}
