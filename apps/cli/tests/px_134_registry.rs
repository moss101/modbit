//! PX-134 (QUAL-PX-134): the `registry` verbs of the headless CLI over a real
//! Core — keygen (a development key), sign, verify, activate, status,
//! revoke and rollback. The CLI holds no registry logic: the Core verifies
//! the signature, the schema and the freshness and decides the swap; these
//! assertions see what the Core answered.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Cli {
    data_dir: PathBuf,
    core: PathBuf,
    keys: String,
    secrets: std::cell::RefCell<Vec<String>>,
}

impl Cli {
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_modbit-cli"))
            .env("MODBIT_CORE_BIN", &self.core)
            .env("MODBIT_REGISTRY_KEYS", &self.keys)
            .env("OPENAI_API_KEY", "")
            .env("ANTHROPIC_API_KEY", "")
            .arg("--data-dir")
            .arg(&self.data_dir)
            .args(args)
            .output()
            .unwrap();
        let (stdout, stderr) = (
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        );
        // No verb ever prints a key: not on success, not on refusal.
        for s in self.secrets.borrow().iter() {
            assert!(
                !stdout.contains(s) && !stderr.contains(s),
                "{args:?} printed a secret key"
            );
        }
        (out.status.code().unwrap_or(-1), stdout, stderr)
    }

    fn ok(&self, args: &[&str]) -> String {
        let (code, out, err) = self.run(args);
        assert_eq!(code, 0, "{args:?}: {out}{err}");
        out
    }

    fn refused(&self, args: &[&str], code_text: &str) -> String {
        let (code, out, err) = self.run(args);
        assert_ne!(code, 0, "{args:?} must be refused: {out}{err}");
        assert!(
            format!("{out}{err}").contains(code_text),
            "{args:?}: expected {code_text} in {out}{err}"
        );
        format!("{out}{err}")
    }
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn entry(model: &str, roles: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "endpoint": "openai", "provider": "openai", "family": "gpt-5", "model": model,
        "roles": roles, "input_modalities": ["text"], "context_tokens": 400000,
        "max_output_tokens": 64000, "tools": true, "vision": false, "reasoning": true,
        "structured_output": true,
        "economics": {"input_per_mtok_minor": 125, "output_per_mtok_minor": 1000, "currency": "USD", "scale": 2},
        "latency": {"p50_ms": 900, "p95_ms": 4200},
        "governance": {"data_residency": "us", "retains_prompts": false, "allowed_profiles": []},
    })
}

fn document(generation: &str, issued_offset_ms: i64, ttl_ms: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    serde_json::json!({
        "schema_version": 1,
        "registry_generation": generation,
        "stats_version": "stats-1",
        "issued_at_ms": now + issued_offset_ms,
        "expires_at_ms": now + issued_offset_ms + ttl_ms,
        "quality_floors": [{"mode": "auto", "min_quality": 0.72, "max_cost_minor": 5000, "currency": "USD", "scale": 2}],
        "entries": [entry("gpt-5", &["solver", "reviewer"]), entry("gpt-5-mini", &["solver", "reviewer"])],
    })
    .to_string()
}

fn field(out: &str, name: &str) -> String {
    out.split_whitespace()
        .find_map(|w| w.strip_prefix(&format!("{name}=")))
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn registry_verbs_sign_verify_activate_status_revoke_and_roll_back_through_the_core() {
    let cli_exe = PathBuf::from(env!("CARGO_BIN_EXE_modbit-cli"));
    let core = cli_exe.parent().unwrap().join(if cfg!(windows) {
        "modbit-core.exe"
    } else {
        "modbit-core"
    });
    assert!(
        core.exists(),
        "{} (build modbit-core first)",
        core.display()
    );
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("profile");
    std::fs::create_dir_all(&data_dir).unwrap();
    let dir = tmp.path();

    // Two development keys; the Core trusts only `ops`.
    let cli0 = Cli {
        data_dir: data_dir.clone(),
        core: core.clone(),
        keys: String::new(),
        secrets: Default::default(),
    };
    let ops_key = dir.join("ops.key");
    let rogue_key = dir.join("rogue.key");
    let out = cli0.ok(&[
        "registry",
        "keygen",
        "--out",
        p(&ops_key),
        "--key-id",
        "ops",
    ]);
    assert!(out.contains("DEV KEY - NOT FOR PRODUCTION"), "{out}");
    let trusted = out
        .lines()
        .find_map(|l| l.trim().strip_prefix("MODBIT_REGISTRY_KEYS="))
        .unwrap()
        .to_owned();
    assert!(trusted.starts_with("ops:"), "{trusted}");
    cli0.ok(&[
        "registry",
        "keygen",
        "--out",
        p(&rogue_key),
        "--key-id",
        "ops",
    ]);
    let secret = |f: &Path| std::fs::read_to_string(f).unwrap().trim().to_owned();
    let (ops_secret, rogue_secret) = (secret(&ops_key), secret(&rogue_key));
    assert_eq!(ops_secret.len(), 64);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&ops_key).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    // A key file is never overwritten.
    cli0.refused(
        &["registry", "keygen", "--out", p(&ops_key)],
        "never overwritten",
    );
    assert_eq!(secret(&ops_key), ops_secret);

    let cli = Cli {
        data_dir,
        core,
        keys: trusted.clone(),
        secrets: std::cell::RefCell::new(vec![ops_secret, rogue_secret]),
    };
    let pubkey = trusted.strip_prefix("ops:").unwrap().to_owned();

    // 1. Sign, verify (offline), activate; the Core reports the digest the
    //    operator computed from the document.
    let doc1 = dir.join("gen1.json");
    std::fs::write(&doc1, document("reg-1", -60_000, 86_400_000)).unwrap();
    let signed1 = dir.join("gen1.signed.json");
    cli.ok(&[
        "registry",
        "sign",
        "--key",
        p(&ops_key),
        "--key-id",
        "ops",
        "--doc",
        p(&doc1),
        "--out",
        p(&signed1),
    ]);
    let verified = cli.ok(&["registry", "verify", p(&signed1), "--trusted", &trusted]);
    assert!(verified.contains("registry verify OK"), "{verified}");
    let digest1 = field(&verified, "digest");
    assert_eq!(digest1.len(), 64);
    // Nothing is active before an activation.
    assert!(
        cli.ok(&["registry", "status"])
            .contains("registry not active")
    );
    let activated = cli.ok(&["registry", "activate", p(&signed1)]);
    assert_eq!(field(&activated, "generation"), "reg-1", "{activated}");
    assert_eq!(field(&activated, "digest"), digest1, "{activated}");
    assert_eq!(field(&activated, "key"), "ops");
    let status = cli.ok(&["registry", "status"]);
    assert_eq!(field(&status, "generation"), "reg-1", "{status}");
    assert_eq!(field(&status, "digest"), digest1, "{status}");
    assert!(status.contains("binding openai/gpt-5 "), "{status}");

    // The model picker's honest line reads the typed field: a task now
    // states the registry the router reads.
    let session = cli.ok(&["session", "create"]);
    let sid = session.trim().strip_prefix("session ").unwrap().to_owned();
    let repo = dir.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let task = cli.ok(&[
        "task",
        "create",
        "--session",
        &sid,
        "--workspace",
        p(&repo),
        "look",
    ]);
    let tid = task.trim().strip_prefix("task ").unwrap().to_owned();
    let posture = cli.ok(&["task", "posture", "--task", &tid]);
    let routing = posture.lines().find(|l| l.starts_with("routing ")).unwrap();
    assert!(routing.contains("registry=reg-1"), "{routing}");
    assert!(!routing.contains("outcome=DIRECT"), "{routing}");

    // 2. Refusals leave the active bundle unchanged: a tampered bundle (the
    //    signature no longer covers the bytes), a bundle signed by another
    //    key under the same id, an expired document and an unknown key id.
    let tampered = dir.join("tampered.json");
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&signed1).unwrap()).unwrap();
    v["document_json"] = serde_json::Value::String(
        v["document_json"]
            .as_str()
            .unwrap()
            .replace("\"auto\"", "\"auto \""),
    );
    std::fs::write(&tampered, v.to_string()).unwrap();
    cli.refused(
        &["registry", "verify", p(&tampered), "--trusted", &trusted],
        "REGISTRY_BAD_SIGNATURE",
    );
    cli.refused(
        &["registry", "activate", p(&tampered)],
        "REGISTRY_BAD_SIGNATURE",
    );
    let doc_rogue = dir.join("rogue.json");
    std::fs::write(&doc_rogue, document("reg-rogue", -60_000, 86_400_000)).unwrap();
    let signed_rogue = dir.join("rogue.signed.json");
    cli.ok(&[
        "registry",
        "sign",
        "--key",
        p(&rogue_key),
        "--key-id",
        "ops",
        "--doc",
        p(&doc_rogue),
        "--out",
        p(&signed_rogue),
    ]);
    cli.refused(
        &["registry", "activate", p(&signed_rogue)],
        "REGISTRY_BAD_SIGNATURE",
    );
    let doc_unknown = dir.join("unknown.json");
    std::fs::write(&doc_unknown, document("reg-unknown", -60_000, 86_400_000)).unwrap();
    let signed_unknown = dir.join("unknown.signed.json");
    cli.ok(&[
        "registry",
        "sign",
        "--key",
        p(&ops_key),
        "--key-id",
        "stranger",
        "--doc",
        p(&doc_unknown),
        "--out",
        p(&signed_unknown),
    ]);
    cli.refused(
        &["registry", "activate", p(&signed_unknown)],
        "REGISTRY_UNKNOWN_KEY",
    );
    let doc_old = dir.join("expired.json");
    std::fs::write(&doc_old, document("reg-expired", -120_000, 60_000)).unwrap();
    let signed_old = dir.join("expired.signed.json");
    cli.ok(&[
        "registry",
        "sign",
        "--key",
        p(&ops_key),
        "--key-id",
        "ops",
        "--doc",
        p(&doc_old),
        "--out",
        p(&signed_old),
    ]);
    cli.refused(
        &["registry", "verify", p(&signed_old), "--pubkey", &pubkey],
        "REGISTRY_EXPIRED",
    );
    cli.refused(
        &["registry", "activate", p(&signed_old)],
        "REGISTRY_EXPIRED",
    );
    let status = cli.ok(&["registry", "status"]);
    assert_eq!(field(&status, "generation"), "reg-1", "{status}");
    assert_eq!(field(&status, "digest"), digest1, "{status}");

    // 3. A new generation is a compare-and-swap on the active one.
    let doc2 = dir.join("gen2.json");
    std::fs::write(&doc2, document("reg-2", -60_000, 86_400_000)).unwrap();
    let signed2 = dir.join("gen2.signed.json");
    cli.ok(&[
        "registry",
        "sign",
        "--key",
        p(&ops_key),
        "--key-id",
        "ops",
        "--doc",
        p(&doc2),
        "--out",
        p(&signed2),
    ]);
    cli.refused(
        &[
            "registry",
            "activate",
            p(&signed2),
            "--expected-generation",
            "reg-0",
        ],
        "REGISTRY_ACTIVATION_CONFLICT",
    );
    assert_eq!(
        field(&cli.ok(&["registry", "status"]), "generation"),
        "reg-1"
    );
    let a2 = cli.ok(&[
        "registry",
        "activate",
        p(&signed2),
        "--expected-generation",
        "reg-1",
    ]);
    assert_eq!(field(&a2, "generation"), "reg-2", "{a2}");
    assert_eq!(field(&a2, "previous_good"), "reg-1", "{a2}");

    // 4. A revocation is a new signed generation; the revoked binding stays
    //    revoked after a rollback to the generation that predates it.
    let signed3 = dir.join("gen3.signed.json");
    cli.ok(&[
        "registry",
        "revoke",
        "--key",
        p(&ops_key),
        "--key-id",
        "ops",
        "--doc",
        p(&doc2),
        "--generation",
        "reg-3",
        "--binding",
        "openai/gpt-5-mini",
        "--out",
        p(&signed3),
    ]);
    cli.refused(
        &[
            "registry",
            "revoke",
            "--key",
            p(&ops_key),
            "--key-id",
            "ops",
            "--doc",
            p(&doc2),
            "--generation",
            "reg-3",
            "--binding",
            "openai/none",
        ],
        "holds no binding",
    );
    let a3 = cli.ok(&[
        "registry",
        "activate",
        p(&signed3),
        "--expected-generation",
        "reg-2",
    ]);
    assert_eq!(field(&a3, "generation"), "reg-3", "{a3}");
    let status = cli.ok(&["registry", "status"]);
    let mini = status
        .lines()
        .find(|l| l.starts_with("binding openai/gpt-5-mini "))
        .unwrap();
    assert!(mini.contains("revoked=true"), "{mini}");
    let rolled = cli.ok(&["registry", "rollback", "--expected-generation", "reg-3"]);
    assert_eq!(field(&rolled, "generation"), "reg-2", "{rolled}");
    let mini = rolled
        .lines()
        .find(|l| l.starts_with("binding openai/gpt-5-mini "))
        .unwrap();
    assert!(
        mini.contains("revoked=true"),
        "a rollback never brings a revoked binding back: {mini}"
    );
    // A rollback names the generation it leaves.
    cli.refused(
        &["registry", "rollback", "--expected-generation", "reg-9"],
        "REGISTRY_ACTIVATION_CONFLICT",
    );
}
