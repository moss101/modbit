//! `guest-image-tool` — the publisher's side of signed guest images (M8.4;
//! docs/21 "Sandbox substrate boundary": the guest image is immutable and
//! versioned). A gateway only verifies; this tool signs.
//!
//! - `keygen --out <signing key file> [--key-id <id>]` — writes the signing
//!   key (hex, mode 0600) and prints `<key id>:<verifying key hex>`, the
//!   entry a gateway trusts (`MODBIT_GUEST_IMAGE_KEYS`).
//! - `sign --key <signing key file> --key-id <id> --kind <microvm-rootfs|reference-guest>
//!   --image <file> --guest-version <v> --guest-protocol <M.m> [--kernel <file>]
//!   [--built-from <text>] --out <manifest.json>`
//! - `verify --manifest <manifest.json> --image <file> --keys "<id>:<hex>,..."`

use std::path::PathBuf;
use std::process::ExitCode;

use modbit_sandbox::image::{self, ImageManifest, SignedManifest};

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn run() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("keygen") => {
            let out = PathBuf::from(
                arg(&args, "--out").ok_or_else(|| anyhow::anyhow!("--out <signing key file>"))?,
            );
            let key_id = arg(&args, "--key-id").unwrap_or_else(|| "guest-publisher".into());
            let key = image::fresh_signing_key();
            std::fs::write(&out, hex::encode(key.to_bytes()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&out, std::fs::Permissions::from_mode(0o600))?;
            }
            println!("{key_id}:{}", hex::encode(key.verifying_key().to_bytes()));
            Ok(())
        }
        Some("sign") => {
            let key_file =
                arg(&args, "--key").ok_or_else(|| anyhow::anyhow!("--key <signing key file>"))?;
            let key_hex = std::fs::read_to_string(&key_file)?;
            let key_bytes: [u8; 32] = hex::decode(key_hex.trim())?
                .try_into()
                .map_err(|_| anyhow::anyhow!("the signing key is not 32 bytes"))?;
            let key = ed25519_dalek::SigningKey::from_bytes(&key_bytes);
            let key_id = arg(&args, "--key-id").unwrap_or_else(|| "guest-publisher".into());
            let kind = arg(&args, "--kind").ok_or_else(|| anyhow::anyhow!("--kind"))?;
            let image_path =
                PathBuf::from(arg(&args, "--image").ok_or_else(|| anyhow::anyhow!("--image"))?);
            let (sha256, size) = image::sha256_file(&image_path)?;
            let kernel_sha256 = match arg(&args, "--kernel") {
                Some(k) => image::sha256_file(&PathBuf::from(k))?.0,
                None => String::new(),
            };
            let manifest = ImageManifest {
                kind,
                sha256,
                size,
                guest_version: arg(&args, "--guest-version")
                    .ok_or_else(|| anyhow::anyhow!("--guest-version"))?,
                guest_protocol: arg(&args, "--guest-protocol").unwrap_or_else(|| {
                    format!(
                        "{}.{}",
                        modbit_sandbox::GUEST_PROTOCOL_MAJOR,
                        modbit_sandbox::GUEST_PROTOCOL_MINOR
                    )
                }),
                kernel_sha256,
                built_from: arg(&args, "--built-from").unwrap_or_default(),
                built_at_ms: now_ms(),
            };
            let signed = image::sign(&manifest, &key_id, &key);
            let out = PathBuf::from(arg(&args, "--out").ok_or_else(|| anyhow::anyhow!("--out"))?);
            std::fs::write(&out, serde_json::to_string_pretty(&signed)?)?;
            println!(
                "signed {} ({} bytes, sha256 {}) as guest {} protocol {} under key {key_id} → {}",
                image_path.display(),
                manifest.size,
                manifest.sha256,
                manifest.guest_version,
                manifest.guest_protocol,
                out.display()
            );
            Ok(())
        }
        Some("verify") => {
            let manifest_path =
                arg(&args, "--manifest").ok_or_else(|| anyhow::anyhow!("--manifest"))?;
            let signed: SignedManifest =
                serde_json::from_str(&std::fs::read_to_string(&manifest_path)?)?;
            let image_path =
                PathBuf::from(arg(&args, "--image").ok_or_else(|| anyhow::anyhow!("--image"))?);
            let keys = image::trusted_keys_from_env(&arg(&args, "--keys").unwrap_or_default());
            let v =
                image::verify(&signed, &image_path, &keys).map_err(|e| anyhow::anyhow!("{e}"))?;
            println!(
                "verified: {} guest {} protocol {} under key {}",
                v.manifest.kind, v.manifest.guest_version, v.manifest.guest_protocol, v.key_id
            );
            Ok(())
        }
        _ => anyhow::bail!("usage: guest-image-tool keygen|sign|verify ..."),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("guest-image-tool: {e:#}");
            ExitCode::FAILURE
        }
    }
}
