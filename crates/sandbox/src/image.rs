//! Signed, versioned guest images (M8.4; docs/21 "Sandbox substrate
//! boundary": the guest image is immutable/versioned and contains no
//! tenant secrets). A publisher signs an [`ImageManifest`] — the image's
//! SHA-256, the guest version and protocol it carries, the kernel it was
//! built for — with an Ed25519 key; a gateway trusts the publisher keys it
//! is configured with and boots nothing else: the manifest must verify
//! under a trusted key, the image on disk must hash to what the manifest
//! says (checked again at every boot), and the guest that comes up must
//! report the version the manifest names — a guest from another image is
//! refused at admission. The same manifest form covers the reference
//! backend's guest binary.

use std::path::Path;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::{Result, SandboxError};

/// What a publisher attests about an image.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImageManifest {
    /// `microvm-rootfs` | `reference-guest`.
    pub kind: String,
    /// SHA-256 of the image file (the ext4 root, or the guest binary).
    pub sha256: String,
    /// Size in bytes.
    pub size: u64,
    /// The `modbit-guest` version inside (what its hello reports).
    pub guest_version: String,
    /// The guest protocol it speaks, `major.minor`.
    pub guest_protocol: String,
    /// The kernel this image is built for (SHA-256), empty for the reference guest.
    pub kernel_sha256: String,
    /// Build provenance (a commit, a job id).
    pub built_from: String,
    /// Build time, ms since the epoch.
    pub built_at_ms: i64,
}

/// A manifest with its signature.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SignedManifest {
    /// The publisher key id.
    pub key_id: String,
    /// Signature over `manifest_json`, hex.
    pub signature_hex: String,
    /// The manifest exactly as signed.
    pub manifest_json: String,
}

/// A verified image the gateway may boot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedImage {
    /// The manifest.
    pub manifest: ImageManifest,
    /// The publisher key id that vouched for it.
    pub key_id: String,
    /// The image file.
    pub path: std::path::PathBuf,
}

/// SHA-256 of a file, hex.
pub fn sha256_file(path: &Path) -> std::io::Result<(String, u64)> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    let mut size = 0u64;
    loop {
        let n = std::io::Read::read(&mut f, &mut buf)?;
        if n == 0 {
            break;
        }
        size += n as u64;
        h.update(&buf[..n]);
    }
    Ok((hex::encode(h.finalize()), size))
}

/// Sign a manifest (the publisher's tooling; a gateway only verifies).
#[must_use]
pub fn sign(manifest: &ImageManifest, key_id: &str, key: &SigningKey) -> SignedManifest {
    let manifest_json = serde_json::to_string(manifest).expect("manifest serializes");
    let signature = key.sign(manifest_json.as_bytes());
    SignedManifest {
        key_id: key_id.to_owned(),
        signature_hex: hex::encode(signature.to_bytes()),
        manifest_json,
    }
}

/// Verify a signed manifest under the trusted publisher keys
/// (`key id → 32-byte verifying key`), then the image on disk against it.
pub fn verify(
    signed: &SignedManifest,
    image: &Path,
    trusted: &[(String, [u8; 32])],
) -> Result<VerifiedImage> {
    let Some((key_id, key_bytes)) = trusted.iter().find(|(id, _)| *id == signed.key_id) else {
        return Err(SandboxError::Refused {
            code: "IMAGE_UNVERIFIED".into(),
            message: format!("publisher key `{}` is not trusted here", signed.key_id),
        });
    };
    let key = VerifyingKey::from_bytes(key_bytes).map_err(|e| SandboxError::Refused {
        code: "IMAGE_UNVERIFIED".into(),
        message: format!("trusted key `{key_id}` is malformed: {e}"),
    })?;
    let raw = hex::decode(&signed.signature_hex).map_err(|_| SandboxError::Refused {
        code: "IMAGE_UNVERIFIED".into(),
        message: "signature is not hex".into(),
    })?;
    let sig = Signature::from_slice(&raw).map_err(|_| SandboxError::Refused {
        code: "IMAGE_UNVERIFIED".into(),
        message: "signature has the wrong length".into(),
    })?;
    if key.verify(signed.manifest_json.as_bytes(), &sig).is_err() {
        return Err(SandboxError::Refused {
            code: "IMAGE_UNVERIFIED".into(),
            message: format!("the manifest does not verify under publisher key `{key_id}`"),
        });
    }
    let manifest: ImageManifest =
        serde_json::from_str(&signed.manifest_json).map_err(|e| SandboxError::Refused {
            code: "IMAGE_UNVERIFIED".into(),
            message: format!("the signed manifest is malformed: {e}"),
        })?;
    check_image(&manifest, image)?;
    Ok(VerifiedImage {
        manifest,
        key_id: key_id.clone(),
        path: image.to_path_buf(),
    })
}

/// The image on disk hashes to what the manifest says.
pub fn check_image(manifest: &ImageManifest, image: &Path) -> Result<()> {
    let (sha, size) = sha256_file(image)?;
    if sha != manifest.sha256 || size != manifest.size {
        return Err(SandboxError::Refused {
            code: "IMAGE_UNVERIFIED".into(),
            message: format!(
                "`{}` is not the image the manifest names (sha256 {} / {} bytes, manifest {} / {})",
                image.display(),
                sha,
                size,
                manifest.sha256,
                manifest.size
            ),
        });
    }
    Ok(())
}

/// The guest that came up is the one the manifest names.
pub fn check_guest(
    manifest: &ImageManifest,
    hello: &modbit_protocol::v1::GuestHello,
) -> Result<()> {
    let protocol = format!("{}.{}", hello.protocol_major, hello.protocol_minor);
    if hello.guest_version != manifest.guest_version || protocol != manifest.guest_protocol {
        return Err(SandboxError::Refused {
            code: "IMAGE_VERSION_MISMATCH".into(),
            message: format!(
                "the guest reports version {} protocol {protocol}; the verified image carries {} protocol {}",
                hello.guest_version, manifest.guest_version, manifest.guest_protocol
            ),
        });
    }
    Ok(())
}

/// Trusted publisher keys from `"<key id>:<64 hex>,..."` (the registry's
/// convention); a malformed entry is skipped, never trusted.
#[must_use]
pub fn trusted_keys_from_env(raw: &str) -> Vec<(String, [u8; 32])> {
    raw.split(',')
        .filter_map(|entry| {
            let (id, hex_key) = entry.trim().split_once(':')?;
            let bytes = hex::decode(hex_key.trim()).ok()?;
            let key: [u8; 32] = bytes.try_into().ok()?;
            VerifyingKey::from_bytes(&key).ok()?;
            Some((id.trim().to_owned(), key))
        })
        .collect()
}

/// A fresh signing key (tooling).
#[must_use]
pub fn fresh_signing_key() -> SigningKey {
    let bytes: [u8; 32] = rand::random();
    SigningKey::from_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_for(path: &Path) -> ImageManifest {
        let (sha256, size) = sha256_file(path).unwrap();
        ImageManifest {
            kind: "reference-guest".into(),
            sha256,
            size,
            guest_version: "0.1.0".into(),
            guest_protocol: "1.0".into(),
            kernel_sha256: String::new(),
            built_from: "test".into(),
            built_at_ms: 1,
        }
    }

    #[test]
    fn a_manifest_verifies_under_its_publisher_key_only_and_binds_the_image_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let img = dir.path().join("guest.bin");
        std::fs::write(&img, b"guest bytes").unwrap();
        let key = fresh_signing_key();
        let trusted = vec![("pub-1".to_owned(), key.verifying_key().to_bytes())];
        let signed = sign(&manifest_for(&img), "pub-1", &key);
        let v = verify(&signed, &img, &trusted).unwrap();
        assert_eq!(v.key_id, "pub-1");
        // Another key, an unknown key id, a tampered manifest, a tampered image.
        let other = fresh_signing_key();
        let forged = sign(&manifest_for(&img), "pub-1", &other);
        assert!(
            matches!(verify(&forged, &img, &trusted), Err(SandboxError::Refused { code, .. }) if code == "IMAGE_UNVERIFIED")
        );
        let unknown = sign(&manifest_for(&img), "pub-9", &key);
        assert!(
            matches!(verify(&unknown, &img, &trusted), Err(SandboxError::Refused { code, .. }) if code == "IMAGE_UNVERIFIED")
        );
        let mut edited = signed.clone();
        edited.manifest_json = edited.manifest_json.replace("0.1.0", "9.9.9");
        assert!(
            matches!(verify(&edited, &img, &trusted), Err(SandboxError::Refused { code, .. }) if code == "IMAGE_UNVERIFIED")
        );
        std::fs::write(&img, b"guest bytes!").unwrap();
        let e = verify(&signed, &img, &trusted).unwrap_err();
        assert!(
            matches!(&e, SandboxError::Refused { code, .. } if code == "IMAGE_UNVERIFIED"),
            "{e}"
        );
        assert!(e.to_string().contains("not the image the manifest names"));
    }

    #[test]
    fn the_guest_that_comes_up_must_be_the_one_the_manifest_names() {
        let m = ImageManifest {
            kind: "microvm-rootfs".into(),
            sha256: String::new(),
            size: 0,
            guest_version: "0.1.0".into(),
            guest_protocol: "1.0".into(),
            kernel_sha256: String::new(),
            built_from: String::new(),
            built_at_ms: 0,
        };
        let hello = |v: &str, major: u32| modbit_protocol::v1::GuestHello {
            protocol_major: major,
            protocol_minor: 0,
            guest_version: v.into(),
            methods: vec![],
            boot_id: String::new(),
        };
        assert!(check_guest(&m, &hello("0.1.0", 1)).is_ok());
        assert!(
            matches!(check_guest(&m, &hello("0.2.0", 1)), Err(SandboxError::Refused { code, .. }) if code == "IMAGE_VERSION_MISMATCH")
        );
        assert!(
            matches!(check_guest(&m, &hello("0.1.0", 2)), Err(SandboxError::Refused { code, .. }) if code == "IMAGE_VERSION_MISMATCH")
        );
        let keys = trusted_keys_from_env(
            "a:00,b:zz, c:4f8a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8",
        );
        assert!(keys.len() <= 1, "malformed entries are skipped: {keys:?}");
    }
}
