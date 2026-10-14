//! QUAL-PX-072 on every operating system, with the real helper binary: the
//! hash-pinned, signed manifest; verification before every spawn; process
//! identity; rollback. Test keys and unsigned (or ad-hoc signed) binaries -
//! there is no Apple identity here; the code-signature layer is exercised
//! with `codesign` on macOS against ad-hoc signatures.
mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use ed25519_dalek::SigningKey;
use modbit_computer::actuator::{
    Actuator, ActuatorConfig, Call, LaunchError, ProcessActuator, Reply,
};
use modbit_computer::supply::{
    self, HelperEntry, KIND, Manifest, PinnedKey, Reject, SCHEMA, SupplyConfig,
};
use modbit_protocol::v1 as wire;
use sha2::{Digest, Sha256};

struct Bundle {
    dir: tempfile::TempDir,
    sk: SigningKey,
    manifest: Manifest,
}

fn helper() -> PathBuf {
    common::helper_bin()
}

fn entry(version: &str, rel: &str, file: &Path) -> HelperEntry {
    let bytes = std::fs::read(file).unwrap();
    HelperEntry {
        id: "modbit-actuator-macos".into(),
        version: version.into(),
        path: rel.into(),
        sha256: hex::encode(Sha256::digest(&bytes)),
        size: bytes.len() as u64,
        platform: supply::this_platform().into(),
        arch: "universal".into(),
        min_core_version: "0.0.0".into(),
        protocol_major: 1,
        require_signature: false,
        designated_requirement: None,
        rollback_to: None,
    }
}

impl Bundle {
    /// A bundle directory holding a copy of the real helper per version.
    fn new(versions: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("helpers")).unwrap();
        let mut helpers = vec![];
        for v in versions {
            let rel = format!(
                "helpers/modbit-actuator-macos-{v}{}",
                std::env::consts::EXE_SUFFIX
            );
            std::fs::copy(helper(), dir.path().join(&rel)).unwrap();
            helpers.push(entry(v, &rel, &dir.path().join(&rel)));
        }
        let mut b = Self {
            dir,
            sk: SigningKey::from_bytes(&[42; 32]),
            manifest: Manifest {
                schema: SCHEMA,
                kind: KIND.into(),
                helpers,
                sbom: vec![],
            },
        };
        b.write();
        b
    }

    fn write(&mut self) {
        let signed = supply::sign_manifest(&self.manifest, &self.sk);
        std::fs::write(
            self.manifest_path(),
            serde_json::to_string(&signed).unwrap(),
        )
        .unwrap();
    }

    fn manifest_path(&self) -> PathBuf {
        self.dir.path().join("actuator-manifest.json")
    }

    fn file(&self, v: &str) -> PathBuf {
        self.dir
            .path()
            .join(format!("helpers/modbit-actuator-macos-{v}"))
    }

    fn supply(&self) -> SupplyConfig {
        SupplyConfig {
            manifest: self.manifest_path(),
            keys_file: None,
            keys: vec![supply::pinned_for(&self.sk)],
            allow_unsigned: false,
            core_version: "1.0.0".into(),
        }
    }

    fn config(&self) -> ActuatorConfig {
        ActuatorConfig {
            executable: PathBuf::from("/definitely/not/used"),
            args: vec![],
            env: vec![],
            log_path: None,
            supply: Some(self.supply()),
        }
    }

    async fn launch(&self) -> Result<ProcessActuator, LaunchError> {
        ProcessActuator::launch(&self.config(), Duration::from_secs(10)).await
    }

    /// Flip one byte in the middle of a helper (same size, different digest).
    fn tamper(&self, v: &str) {
        let f = self.file(v);
        let mut b = std::fs::read(&f).unwrap();
        let at = b.len() / 4;
        b[at] ^= 0xFF;
        std::fs::write(&f, b).unwrap();
    }
}

fn unverified(e: LaunchError) -> String {
    match e {
        LaunchError::Unverified(s) => s,
        other => panic!("expected Unverified, got {other:?}"),
    }
}

#[tokio::test]
async fn a_pinned_helper_is_verified_launched_and_its_process_identity_matches_the_pin() {
    let b = Bundle::new(&["0.2.0"]);
    let a = b.launch().await.expect("launched");
    let rep = a.supply_report().expect("a supply report");
    assert_eq!(rep.used.version, "0.2.0");
    assert!(rep.rejected.is_empty());
    assert_eq!(
        rep.used.path,
        std::fs::canonicalize(b.file("0.2.0")).unwrap()
    );
    assert_eq!(rep.used.sha256, b.manifest.helpers[0].sha256);
    // The running pid IS that file, by the operating system's account.
    if let Some(exe) = supply::process_exe(a.pid()) {
        assert_eq!(std::fs::canonicalize(exe).unwrap(), rep.used.path);
    }
    assert!(matches!(
        a.call(
            Call::Ping(wire::PingRequest::default()),
            Duration::from_secs(5)
        )
        .await,
        Ok(Reply::Ping(_))
    ));
}

#[tokio::test]
async fn a_modified_helper_is_refused_before_it_is_executed() {
    let b = Bundle::new(&["0.2.0"]);
    b.tamper("0.2.0");
    let msg = unverified(b.launch().await.err().expect("refused"));
    assert!(
        msg.contains("NO_VERIFIED_HELPER") && msg.contains("DIGEST_MISMATCH"),
        "{msg}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_swapped_executable_never_runs_not_even_once() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("ran");
    let rel = "helpers/h";
    std::fs::create_dir_all(dir.path().join("helpers")).unwrap();
    let f = dir.path().join(rel);
    // The pinned file and its replacement are the same size.
    let mk = |tag: &str| format!("#!/bin/sh\necho {tag} >> {}\n", marker.display());
    std::fs::write(&f, mk("A")).unwrap();
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut b = Bundle {
        dir,
        sk: SigningKey::from_bytes(&[42; 32]),
        manifest: Manifest {
            schema: SCHEMA,
            kind: KIND.into(),
            helpers: vec![],
            sbom: vec![],
        },
    };
    b.manifest.helpers = vec![entry("1.0.0", rel, &f)];
    b.write();
    std::fs::write(&f, mk("B")).unwrap();
    let msg = unverified(b.launch().await.err().expect("refused"));
    assert!(msg.contains("DIGEST_MISMATCH"), "{msg}");
    assert!(!marker.exists(), "the replacement was executed");
}

#[tokio::test]
async fn a_helper_that_fails_verification_rolls_back_to_the_previous_pinned_one() {
    let mut b = Bundle::new(&["0.1.0", "0.2.0"]);
    b.manifest.helpers[1].rollback_to = Some("0.1.0".into());
    b.write();
    b.tamper("0.2.0");
    let a = b.launch().await.expect("rolled back and launched");
    let rep = a.supply_report().unwrap();
    assert_eq!(rep.used.version, "0.1.0");
    assert_eq!(rep.rejected.len(), 1);
    assert_eq!(rep.rejected[0].version, "0.2.0");
    assert!(
        matches!(rep.rejected[0].reason, Reject::DigestMismatch { .. }),
        "{:?}",
        rep.rejected
    );
    assert_eq!(rep.rejected[0].reason.code(), "DIGEST_MISMATCH");
}

#[tokio::test]
async fn when_nothing_verifies_nothing_is_launched_and_every_reason_is_reported() {
    let mut b = Bundle::new(&["0.1.0", "0.2.0"]);
    b.manifest.helpers[1].rollback_to = Some("0.1.0".into());
    b.write();
    b.tamper("0.1.0");
    std::fs::remove_file(b.file("0.2.0")).unwrap();
    let msg = unverified(b.launch().await.err().expect("refused"));
    assert!(
        msg.contains("0.2.0 -> HELPER_MISSING") && msg.contains("0.1.0 -> DIGEST_MISMATCH"),
        "{msg}"
    );
}

#[tokio::test]
async fn a_helper_for_a_newer_core_or_another_protocol_is_skipped_with_the_typed_reason() {
    let mut b = Bundle::new(&["0.1.0", "0.2.0", "0.3.0"]);
    b.manifest.helpers[2].rollback_to = Some("0.2.0".into());
    b.manifest.helpers[2].min_core_version = "9.0.0".into();
    b.manifest.helpers[1].rollback_to = Some("0.1.0".into());
    b.manifest.helpers[1].protocol_major = 2;
    b.write();
    let a = b.launch().await.expect("fell back to 0.1.0");
    let rep = a.supply_report().unwrap();
    assert_eq!(rep.used.version, "0.1.0");
    let codes: Vec<(&str, &str)> = rep
        .rejected
        .iter()
        .map(|r| (r.version.as_str(), r.reason.code()))
        .collect();
    assert_eq!(
        codes,
        [("0.3.0", "CORE_TOO_OLD"), ("0.2.0", "PROTOCOL_MISMATCH")]
    );
}

#[tokio::test]
async fn an_untrusted_manifest_launches_nothing() {
    // payload edited after signing
    let b = Bundle::new(&["0.2.0"]);
    let mut env: supply::SignedManifest =
        serde_json::from_str(&std::fs::read_to_string(b.manifest_path()).unwrap()).unwrap();
    env.payload = env.payload.replace("0.2.0", "0.9.9");
    std::fs::write(b.manifest_path(), serde_json::to_string(&env).unwrap()).unwrap();
    assert!(unverified(b.launch().await.err().unwrap()).contains("SIGNATURE_INVALID"));
    // signed by a key this build does not pin
    let mut b = Bundle::new(&["0.2.0"]);
    b.sk = SigningKey::from_bytes(&[43; 32]);
    b.write();
    let mut cfg = b.config();
    cfg.supply.as_mut().unwrap().keys =
        vec![supply::pinned_for(&SigningKey::from_bytes(&[42; 32]))];
    let e = ProcessActuator::launch(&cfg, Duration::from_secs(5))
        .await
        .err()
        .unwrap();
    assert!(unverified(e).contains("SIGNATURE_UNKNOWN_KEY"));
    // a revoked key
    let b = Bundle::new(&["0.2.0"]);
    let mut cfg = b.config();
    cfg.supply.as_mut().unwrap().keys = vec![PinnedKey {
        revoked: true,
        ..supply::pinned_for(&b.sk)
    }];
    assert!(
        unverified(
            ProcessActuator::launch(&cfg, Duration::from_secs(5))
                .await
                .err()
                .unwrap()
        )
        .contains("SIGNATURE_KEY_REVOKED")
    );
    // no signature at all
    let b = Bundle::new(&["0.2.0"]);
    std::fs::write(
        b.manifest_path(),
        serde_json::to_string(&b.manifest).unwrap(),
    )
    .unwrap();
    assert!(unverified(b.launch().await.err().unwrap()).contains("MANIFEST_UNSIGNED"));
    // ... which only an explicit developer opt-in accepts
    let mut cfg = b.config();
    cfg.supply.as_mut().unwrap().allow_unsigned = true;
    assert!(
        ProcessActuator::launch(&cfg, Duration::from_secs(10))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_path_that_leaves_the_bundle_is_refused() {
    let mut b = Bundle::new(&["0.2.0"]);
    b.manifest.helpers[0].path = "../../../../../../../bin/sh".into();
    b.write();
    let msg = unverified(b.launch().await.err().unwrap());
    assert!(msg.contains("PATH_ESCAPES_BUNDLE"), "{msg}");
    b.manifest.helpers[0].path = helper().to_string_lossy().into_owned();
    b.write();
    assert!(
        unverified(b.launch().await.err().unwrap()).contains("PATH_ESCAPES_BUNDLE"),
        "an absolute path is never followed"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_process_that_is_not_the_pinned_file_is_not_trusted_and_not_signalled() {
    use modbit_computer::supply::Verified;
    let sleeper_dir = tempfile::tempdir().unwrap();
    let sleep = sleeper_dir.path().join("sleeper");
    // A long-lived process of our own (the helper waits on its stdin);
    // a copied system binary can be killed by the platform-binary check.
    std::fs::copy(helper(), &sleep).unwrap();
    let mut child = std::process::Command::new(&sleep)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let pid = child.id();
    let digest = hex::encode(Sha256::digest(std::fs::read(&sleep).unwrap()));
    let canon = std::fs::canonicalize(&sleep).unwrap();
    let ok = Verified {
        version: "1".into(),
        path: canon.clone(),
        sha256: digest.clone(),
        protocol_major: 1,
    };
    if supply::process_exe(pid).is_none() {
        let _ = child.kill();
        let _ = child.wait();
        return; // a platform that cannot name a process's executable
    }
    assert_eq!(supply::check_process(pid, &ok), Ok(()));
    // The pinned digest is not what is on disk: not the pinned file.
    let wrong_digest = Verified {
        sha256: "00".repeat(32),
        ..ok.clone()
    };
    assert!(matches!(
        supply::check_process(pid, &wrong_digest),
        Err(Reject::ProcessIdentity(_))
    ));
    // The pinned path is another file.
    let other = Verified {
        path: std::fs::canonicalize("/bin/sh").unwrap(),
        ..ok.clone()
    };
    assert!(matches!(
        supply::check_process(pid, &other),
        Err(Reject::ProcessIdentity(_))
    ));
    // A signal to a pid whose executable differs is not delivered.
    assert!(supply::terminate_verified(pid, Path::new("/bin/sh")).is_err());
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        child.try_wait().unwrap().is_none(),
        "the process was signalled although its path differs"
    );
    // With the right path it is.
    supply::terminate_verified(pid, &canon).expect("signalled");
    let t0 = std::time::Instant::now();
    while child.try_wait().unwrap().is_none() {
        assert!(
            t0.elapsed() < Duration::from_secs(5),
            "SIGTERM did not end it"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(target_os = "macos")]
mod codesign {
    use super::*;
    use std::process::Command;

    fn ad_hoc_signed(b: &Bundle, v: &str) {
        let st = Command::new("/usr/bin/codesign")
            .args(["--force", "--sign", "-"])
            .arg(b.file(v))
            .status()
            .unwrap();
        assert!(st.success());
    }

    fn repin(b: &mut Bundle, v: &str, require: bool, requirement: Option<&str>) {
        let rel = format!("helpers/modbit-actuator-macos-{v}");
        let mut e = entry(v, &rel, &b.file(v));
        e.require_signature = require;
        e.designated_requirement = requirement.map(str::to_owned);
        b.manifest.helpers = vec![e];
        b.write();
    }

    #[tokio::test]
    async fn a_valid_code_signature_is_required_and_checked_when_the_manifest_says_so() {
        let mut b = Bundle::new(&["0.2.0"]);
        ad_hoc_signed(&b, "0.2.0");
        repin(&mut b, "0.2.0", true, None);
        assert!(
            b.launch().await.is_ok(),
            "an intact ad-hoc signature verifies"
        );
    }

    #[tokio::test]
    async fn the_designated_requirement_is_enforced() {
        let mut b = Bundle::new(&["0.2.0"]);
        ad_hoc_signed(&b, "0.2.0");
        // An ad-hoc signature does not satisfy an Apple-anchored requirement.
        repin(
            &mut b,
            "0.2.0",
            true,
            Some("anchor apple generic and certificate leaf[subject.OU] = \"TEAMID1234\""),
        );
        let msg = unverified(b.launch().await.err().unwrap());
        assert!(msg.contains("SIGNATURE_CHECK_FAILED"), "{msg}");
    }

    #[tokio::test]
    async fn code_altered_after_signing_is_caught_by_codesign_even_when_the_hash_was_re_pinned() {
        let mut b = Bundle::new(&["0.2.0"]);
        ad_hoc_signed(&b, "0.2.0");
        b.tamper("0.2.0");
        // The attacker also rewrites the pin: only the code signature is left.
        repin(&mut b, "0.2.0", true, None);
        let msg = unverified(b.launch().await.err().unwrap());
        assert!(msg.contains("SIGNATURE_CHECK_FAILED"), "{msg}");
    }

    #[tokio::test]
    async fn an_unsigned_binary_is_refused_where_a_signature_is_required() {
        let mut b = Bundle::new(&["0.2.0"]);
        let st = Command::new("/usr/bin/codesign")
            .args(["--remove-signature"])
            .arg(b.file("0.2.0"))
            .status()
            .unwrap();
        assert!(st.success());
        repin(&mut b, "0.2.0", true, None);
        let msg = unverified(b.launch().await.err().unwrap());
        assert!(msg.contains("SIGNATURE_CHECK_FAILED"), "{msg}");
    }

    #[test]
    fn the_signing_script_fails_clearly_without_an_identity_and_signs_with_one() {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/sign-actuator-macos.sh");
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("h");
        std::fs::copy(helper(), &bin).unwrap();
        let out = Command::new("/bin/bash")
            .arg(&script)
            .arg(&bin)
            .env_remove("MODBIT_SIGNING_IDENTITY")
            .output()
            .unwrap();
        assert!(!out.status.success());
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("MODBIT_SIGNING_IDENTITY"), "{err}");
        // An ad-hoc identity ("-") is accepted for a developer build and says so.
        let out = Command::new("/bin/bash")
            .arg(&script)
            .arg(&bin)
            .env("MODBIT_SIGNING_IDENTITY", "-")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(String::from_utf8_lossy(&out.stdout).contains("ad-hoc"));
        let ok = Command::new("/usr/bin/codesign")
            .args(["--verify", "--strict"])
            .arg(&bin)
            .status()
            .unwrap();
        assert!(ok.success());
    }
}

#[test]
fn the_manifest_tool_builds_signs_and_audits_a_bundle_and_the_audit_is_a_release_gate() {
    use std::process::Command;
    let tool = {
        // The tool is a binary of modbit-computer; build it next to the helper.
        let helper_dir = helper().parent().unwrap().to_path_buf();
        let exe = helper_dir.join(if cfg!(windows) {
            "modbit-actuator-manifest.exe"
        } else {
            "modbit-actuator-manifest"
        });
        if !exe.exists() {
            let st = Command::new(env!("CARGO"))
                .args([
                    "build",
                    "-p",
                    "modbit-computer",
                    "--bin",
                    "modbit-actuator-manifest",
                    "--target-dir",
                ])
                .arg(helper_dir.parent().unwrap())
                .status()
                .unwrap();
            assert!(st.success());
        }
        exe
    };
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("actuator");
    std::fs::create_dir_all(root.join("helpers")).unwrap();
    std::fs::copy(helper(), root.join("helpers/h-1.0.0")).unwrap();
    std::fs::write(
        dir.path().join("spec.json"),
        serde_json::json!({"helpers": [{
            "id": "modbit-actuator-macos", "version": "1.0.0", "path": "helpers/h-1.0.0",
            "platform": supply::this_platform(), "arch": "universal",
            "min_core_version": "0.0.0", "protocol_major": 1
        }]})
        .to_string(),
    )
    .unwrap();
    let run = |args: &[&str]| Command::new(&tool).args(args).output().unwrap();
    let s = |p: &Path| p.to_string_lossy().into_owned();
    let (key, public) = (dir.path().join("k.pem"), dir.path().join("k.pub.pem"));
    assert!(
        run(&["keygen", "--private", &s(&key), "--public", &s(&public)])
            .status
            .success()
    );
    let payload = dir.path().join("payload.json");
    let manifest = root.join("actuator-manifest.json");
    assert!(
        run(&[
            "build",
            "--spec",
            &s(&dir.path().join("spec.json")),
            "--root",
            &s(&root),
            "--out",
            &s(&payload)
        ])
        .status
        .success()
    );
    assert!(
        run(&[
            "sign",
            "--payload",
            &s(&payload),
            "--key",
            &s(&key),
            "--out",
            &s(&manifest)
        ])
        .status
        .success()
    );
    // trusted-keys.json in the M10.2 PinnedKey shape
    let pem = std::fs::read_to_string(&public).unwrap();
    let id = {
        let envelope: supply::SignedManifest =
            serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
        envelope.key_id
    };
    let keys = dir.path().join("trusted-keys.json");
    std::fs::write(
        &keys,
        serde_json::json!([{"key_id": id, "public_key_pem": pem}]).to_string(),
    )
    .unwrap();
    let audit = |core: &str| {
        run(&[
            "audit",
            "--manifest",
            &s(&manifest),
            "--keys",
            &s(&keys),
            "--core-version",
            core,
        ])
    };
    let ok = audit("1.0.0");
    assert!(
        ok.status.success(),
        "{}{}",
        String::from_utf8_lossy(&ok.stdout),
        String::from_utf8_lossy(&ok.stderr)
    );
    assert!(String::from_utf8_lossy(&ok.stdout).contains("ok      1.0.0"));
    // Tamper after signing: the gate refuses the bundle.
    let mut b = std::fs::read(root.join("helpers/h-1.0.0")).unwrap();
    let n = b.len() / 3;
    b[n] ^= 1;
    std::fs::write(root.join("helpers/h-1.0.0"), b).unwrap();
    let bad = audit("1.0.0");
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stdout).contains("DIGEST_MISMATCH"));
}
