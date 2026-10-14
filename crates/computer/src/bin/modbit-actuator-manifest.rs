//! `modbit-actuator-manifest` - build, sign and audit the hash-pinned manifest
//! of the bundled actuator helper (PX-072; docs/66 CUC-E04).
//!
//! The release pipeline runs it after the helper is built and code-signed:
//!
//! ```text
//! modbit-actuator-manifest keygen  --private key.pem --public key.pub.pem      (once; keep key.pem secret)
//! modbit-actuator-manifest build   --spec spec.json --root Resources/actuator --out payload.json
//! modbit-actuator-manifest sign    --payload payload.json --key key.pem --out Resources/actuator/actuator-manifest.json
//! modbit-actuator-manifest audit   --manifest Resources/actuator/actuator-manifest.json --keys trusted-keys.json --core-version 1.2.3
//! ```
//!
//! `spec.json` is a manifest payload whose `sha256` and `size` fields are
//! left out; `build` fills them in from the files under `--root`. `sign`
//! writes the same `{payload, signature, key_id}` envelope as the M10.2
//! update manifest. `audit` is the release gate: it exits non-zero unless
//! the manifest is trusted and every helper for this platform verifies.
//! The private key is read from a file and never printed.

use std::path::PathBuf;
use std::process::ExitCode;

use base64::Engine as _;
use ed25519_dalek::SigningKey;
use modbit_computer::supply::{self, Manifest, SupplyConfig};
use sha2::{Digest, Sha256};

/// PKCS#8 prefix of an ed25519 private key (RFC 8410): followed by 32 seed bytes.
const PKCS8_PREFIX: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn need(args: &[String], name: &str) -> Result<String, String> {
    arg(args, name).ok_or_else(|| format!("missing {name}"))
}

fn pem(label: &str, der: &[u8]) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(der);
    let lines: Vec<String> = b64
        .as_bytes()
        .chunks(64)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect();
    format!("-----BEGIN {label}-----\n{}\n-----END {label}-----\n", lines.join("\n"))
}

fn read_signing_key(path: &str) -> Result<SigningKey, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let body: String = text.lines().filter(|l| !l.starts_with("-----")).collect();
    let der = base64::engine::general_purpose::STANDARD
        .decode(body.trim())
        .map_err(|e| format!("{path}: {e}"))?;
    if der.len() != 48 || der[..16] != PKCS8_PREFIX {
        return Err(format!("{path}: not an ed25519 PKCS#8 private key"));
    }
    let seed: [u8; 32] = der[16..].try_into().map_err(|_| "seed length".to_owned())?;
    Ok(SigningKey::from_bytes(&seed))
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("keygen") => {
            let seed: [u8; 32] = std::array::from_fn(|_| rand::random::<u8>());
            let sk = SigningKey::from_bytes(&seed);
            let mut der = PKCS8_PREFIX.to_vec();
            der.extend_from_slice(&seed);
            let private = need(args, "--private")?;
            std::fs::write(&private, pem("PRIVATE KEY", &der)).map_err(|e| format!("{private}: {e}"))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o600));
            }
            let public = need(args, "--public")?;
            let pinned = supply::pinned_for(&sk);
            std::fs::write(&public, &pinned.public_key_pem).map_err(|e| format!("{public}: {e}"))?;
            println!("key_id {}", pinned.key_id);
            Ok(())
        }
        Some("build") => {
            let spec = need(args, "--spec")?;
            let root = PathBuf::from(need(args, "--root")?);
            let text = std::fs::read_to_string(&spec).map_err(|e| format!("{spec}: {e}"))?;
            let mut v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{spec}: {e}"))?;
            let helpers = v
                .get_mut("helpers")
                .and_then(|h| h.as_array_mut())
                .ok_or("the spec has no helpers array")?;
            for h in helpers {
                let rel = h.get("path").and_then(|p| p.as_str()).ok_or("a helper has no path")?.to_owned();
                let file = root.join(&rel);
                let bytes = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
                h["sha256"] = serde_json::json!(hex::encode(Sha256::digest(&bytes)));
                h["size"] = serde_json::json!(bytes.len());
            }
            v["schema"] = serde_json::json!(supply::SCHEMA);
            v["kind"] = serde_json::json!(supply::KIND);
            let m: Manifest = serde_json::from_value(v).map_err(|e| format!("the spec is not a manifest: {e}"))?;
            let out = need(args, "--out")?;
            std::fs::write(&out, serde_json::to_string_pretty(&m).map_err(|e| e.to_string())?)
                .map_err(|e| format!("{out}: {e}"))
        }
        Some("sign") => {
            let payload = need(args, "--payload")?;
            let text = std::fs::read_to_string(&payload).map_err(|e| format!("{payload}: {e}"))?;
            let m: Manifest = serde_json::from_str(&text).map_err(|e| format!("{payload}: {e}"))?;
            let sk = read_signing_key(&need(args, "--key")?)?;
            let signed = supply::sign_manifest(&m, &sk);
            let out = need(args, "--out")?;
            std::fs::write(&out, serde_json::to_string_pretty(&signed).map_err(|e| e.to_string())?)
                .map_err(|e| format!("{out}: {e}"))
        }
        Some("audit") => {
            let cfg = SupplyConfig {
                manifest: PathBuf::from(need(args, "--manifest")?),
                keys_file: arg(args, "--keys").map(PathBuf::from),
                keys: vec![],
                allow_unsigned: false,
                core_version: need(args, "--core-version")?,
            };
            let results = supply::audit(&cfg).map_err(|e| e.to_string())?;
            if results.is_empty() {
                return Err("NO_ENTRY_FOR_PLATFORM: the manifest pins no helper for this platform".into());
            }
            let mut bad = 0;
            for (version, r) in results {
                match r {
                    Ok(v) => println!("ok      {version}  {}  {}", v.sha256, v.path.display()),
                    Err(e) => {
                        bad += 1;
                        println!("REFUSED {version}  {e}");
                    }
                }
            }
            if bad == 0 { Ok(()) } else { Err(format!("{bad} helper(s) did not verify")) }
        }
        _ => Err("usage: modbit-actuator-manifest <keygen|build|sign|audit> ... (see the module docs)".into()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("modbit-actuator-manifest: {e}");
            ExitCode::FAILURE
        }
    }
}
