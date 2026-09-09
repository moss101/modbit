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
