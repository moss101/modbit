//! M8.3 qualification (docs/21 "Sandbox substrate boundary", docs/24
//! "Sandbox Gateway", docs/33 "Sandbox Gateway"; QUAL-EV-0285, 0286, 0287,
//! 0289, 0290, 0291).
//!
//! - The backend contract suite on the reference backend (everywhere the
//!   built `modbit-guest` is beside this test binary) and on the MicroVM
//!   backend (where `MODBIT_FIRECRACKER_BIN`, `MODBIT_GUEST_KERNEL` and
//!   `MODBIT_GUEST_ROOTFS` name a real Firecracker, kernel and guest image
//!   and `/dev/kvm` is there — the hosted `cloud` job).
//! - The gateway over a real Postgres: sandboxes bound to the tenant and to
//!   the worker's session lease, cross-tenant use denied and audited.

use std::path::{Path, PathBuf};
use std::time::Duration;

use modbit_domain::{SessionId, TaskId, TenantId};
use modbit_sandbox::SandboxError;
use modbit_sandbox::backend::SandboxBackend;
use modbit_sandbox::backend::reference::ReferenceBackend;
use modbit_sandbox::conformance::{Fixture, run};
use modbit_sandbox::image::{self, ImageManifest, SignedManifest};
use modbit_sandbox::policy::{NetworkPolicy, Resources, SandboxSpec, compile};
use serde_json::{Value, json};

fn guest_bin() -> PathBuf {
    if let Ok(p) = std::env::var("MODBIT_GUEST_BIN") {
        return PathBuf::from(p);
    }
    let exe = std::env::current_exe().expect("test exe");
    let dir = exe.parent().and_then(|p| p.parent()).expect("target/debug");
    dir.join(if cfg!(windows) {
        "modbit-guest.exe"
    } else {
        "modbit-guest"
    })
}

fn workspace(dir: &Path) -> PathBuf {
    let ws = dir.join("ws");
    std::fs::create_dir_all(ws.join(".git/hooks")).unwrap();
    std::fs::write(ws.join("NOTES.md"), "# notes\n").unwrap();
    std::fs::write(ws.join(".git/hooks/pre-commit"), "#!/bin/sh\n").unwrap();
    ws
}

fn spec(ws: &Path) -> SandboxSpec {
    SandboxSpec {
        tenant_id: TenantId::new(),
        session_id: SessionId::new(),
        task_id: TaskId::new(),
        workspace_source: ws.to_path_buf(),
        protected_paths: vec![".git/hooks".into()],
        network: NetworkPolicy::default(),
        resources: Resources {
            max_output_bytes: 64 * 1024,
            exec_timeout_ms: 30_000,
            workspace_mib: 64,
            ..Resources::default()
        },
    }
}

/// Probes for a POSIX guest (busybox in the MicroVM image, the host's shell
/// on the reference backend) and for a Windows host.
fn fixture(ws: &Path) -> Fixture {
    let sh = |cmd: &str| -> Vec<String> {
        if cfg!(windows) {
            vec!["cmd".into(), "/C".into(), cmd.into()]
        } else {
            vec!["/bin/sh".into(), "-c".into(), cmd.into()]
        }
    };
    Fixture {
        spec: spec(ws),
        exec_probe: if cfg!(windows) {
            sh("echo hello& exit 3")
        } else {
            sh("echo hello; exit 3")
        },
        cat_probe: if cfg!(windows) {
            sh("findstr .*")
        } else {
            vec!["/bin/cat".into()]
        },
        sleep_probe: if cfg!(windows) {
            sh("ping -n 4 127.0.0.1 > nul")
        } else {
            sh("sleep 5")
        },
        flood_probe: if cfg!(windows) {
            sh("for /L %i in (1,1,8000) do @echo xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx")
        } else {
            sh("yes xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx | head -c 300000")
        },
        exec_env: if cfg!(windows) {
            let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
            // `cmd` resolves external commands (`findstr`, `ping`) by PATH.
            vec![
                format!("SystemRoot={root}"),
                format!(r"PATH={root}\System32"),
            ]
        } else {
            vec![]
        },
        // The cloud metadata endpoint: the classic control-plane address a
        // guest must not reach (docs/21: deny-by-default sandbox-to-internal).
        control_endpoint: ("169.254.169.254".into(), 80),
        lines_probe: if cfg!(windows) {
            sh("for %i in (1 2 3 4 5) do @(echo line %i& ping -n 2 127.0.0.1 >nul)")
        } else {
            sh("for i in 1 2 3 4 5; do echo line $i; sleep 1; done")
        },
        echo_stdin_probe: if cfg!(windows) {
            vec![
                "cmd".into(),
                "/V:ON".into(),
                "/C".into(),
                "set /p l=& echo got:!l!".into(),
            ]
        } else {
            sh("read l; echo got:$l")
        },
        pty: !cfg!(windows),
        write_hook_probe: if cfg!(windows) {
            sh("echo x > .git\\hooks\\pre-commit")
        } else {
            sh("echo x > .git/hooks/pre-commit")
        },
    }
}

/// A publisher key of the test's own and a manifest it signs for `image`.
fn signed_by_test(
    kind: &str,
    image_path: &Path,
    guest_version: &str,
) -> (SignedManifest, Vec<(String, [u8; 32])>) {
    let key = image::fresh_signing_key();
    let (sha256, size) = image::sha256_file(image_path).unwrap();
    let manifest = ImageManifest {
        kind: kind.into(),
        sha256,
        size,
        guest_version: guest_version.into(),
        guest_protocol: format!(
            "{}.{}",
            modbit_sandbox::GUEST_PROTOCOL_MAJOR,
            modbit_sandbox::GUEST_PROTOCOL_MINOR
        ),
        kernel_sha256: String::new(),
        built_from: "gateway.rs".into(),
        built_at_ms: 1,
    };
    (
        image::sign(&manifest, "test-publisher", &key),
        vec![("test-publisher".to_owned(), key.verifying_key().to_bytes())],
    )
}

/// The version the built guest reports (its crate version).
fn guest_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Print every MicroVM console log under `dir` (the guest's own words when
/// a step failed inside the substrate).
fn dump_consoles(dir: &Path) {
    fn walk(d: &Path, out: &mut Vec<PathBuf>) {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p
                    .file_name()
                    .is_some_and(|n| n == "console.log" || n == "guest.log")
                {
                    out.push(p);
                }
            }
        }
    }
    let mut logs = Vec::new();
    walk(dir, &mut logs);
    for p in logs {
        let text = std::fs::read_to_string(&p).unwrap_or_default();
        let tail: Vec<&str> = text.lines().rev().take(60).collect();
        eprintln!("---- {} (last {} lines) ----", p.display(), tail.len());
        for l in tail.into_iter().rev() {
            eprintln!("{l}");
        }
    }
}

async fn assert_conformance(
    backend: &dyn SandboxBackend,
    ws: &Path,
) -> modbit_sandbox::conformance::Report {
    // A hang anywhere in the substrate fails the suite, never the job.
    let outcome = tokio::time::timeout(Duration::from_secs(600), run(backend, &fixture(ws)))
        .await
        .expect("the suite finished within ten minutes");
    let report = match outcome {
        Ok(r) => r,
        Err(e) => {
            // The substrate's own record of what happened, before the verdict.
            dump_consoles(ws.parent().unwrap_or(ws));
            panic!("suite ran: {e}");
        }
    };
    eprintln!("{}", serde_json::to_string_pretty(&report).unwrap());
    if !report.passed() {
        dump_consoles(ws.parent().unwrap_or(ws));
    }
    assert!(
        report.passed(),
        "failed steps on {}: {:?}",
        report.backend,
        report.failures()
    );
    report
}

/// QUAL-EV-0291 (the reference half), QUAL-EV-0289, QUAL-EV-0290.
#[tokio::test]
async fn qual_m8_3_the_reference_backend_passes_the_backend_contract() {
    let bin = guest_bin();
    assert!(
        bin.is_file(),
        "modbit-guest at {} (build it, or set MODBIT_GUEST_BIN)",
        bin.display()
    );
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(dir.path());
    // M8.4: the guest binary is what a publisher signed; the guest that
    // comes up reports the version the manifest names.
    let (signed, trusted) = signed_by_test("reference-guest", &bin, &guest_version());
    let backend =
        ReferenceBackend::verified(bin.clone(), dir.path().join("sandboxes"), &signed, &trusted)
            .expect("verified guest binary");
    let report = assert_conformance(&backend, &ws).await;
    assert_eq!(
        (report.backend, report.isolated),
        ("reference", false),
        "the reference backend claims no isolation"
    );
    assert_eq!(report.guest_version, guest_version());
    // A manifest under another publisher's key, or naming a tampered
    // binary, is refused before anything runs.
    let (other_signed, _) = signed_by_test("reference-guest", &bin, &guest_version());
    let e = ReferenceBackend::verified(bin.clone(), dir.path().join("x"), &other_signed, &trusted)
        .err()
        .expect("refused");
    assert!(
        matches!(&e, SandboxError::Refused { code, .. } if code == "IMAGE_UNVERIFIED"),
        "{e}"
    );
    let tampered = dir.path().join("guest-tampered");
    let mut bytes = std::fs::read(&bin).unwrap();
    bytes.push(0);
    std::fs::write(&tampered, &bytes).unwrap();
    let e = ReferenceBackend::verified(tampered, dir.path().join("y"), &signed, &trusted)
        .err()
        .expect("refused");
    assert!(
        matches!(&e, SandboxError::Refused { code, .. } if code == "IMAGE_UNVERIFIED"),
        "{e}"
    );
    // A properly signed manifest that names another guest version: the
    // binary comes up, says who it is, and is refused at admission.
    let (stale, trusted2) = signed_by_test("reference-guest", &bin, "0.0.0-stale");
    let backend = ReferenceBackend::verified(bin, dir.path().join("z"), &stale, &trusted2)
        .expect("hash matches");
    let policy = compile(&spec(&ws)).unwrap();
    let provisioned = backend.provision("stale-1", &policy).await.expect("boots");
    let e = modbit_sandbox::link::GuestLink::admit_image(
        provisioned.channel,
        "stale-1",
        &policy,
        Duration::from_secs(30),
        backend.image(),
    )
    .await
    .err()
    .expect("refused");
    assert!(
        matches!(&e, SandboxError::Refused { code, .. } if code == "IMAGE_VERSION_MISMATCH"),
        "{e}"
    );
    backend.destroy("stale-1").await.unwrap();
}

/// QUAL-EV-0285, QUAL-EV-0287, QUAL-EV-0291 (the substrate half): a real
/// Firecracker MicroVM boots the guest image, the guest answers over
/// vsock, and the control plane is unreachable from inside.
#[cfg(unix)]
#[tokio::test]
async fn qual_m8_3_a_firecracker_microvm_boots_the_guest_and_passes_the_backend_contract() {
    use modbit_sandbox::backend::microvm::{MicrovmBackend, MicrovmConfig};
    let Some(cfg) = MicrovmConfig::from_env() else {
        eprintln!(
            "SKIPPED: MODBIT_FIRECRACKER_BIN / MODBIT_GUEST_KERNEL / MODBIT_GUEST_ROOTFS unset (the hosted cloud job runs this on KVM)"
        );
        return;
    };
    if let Some(why) = cfg.unavailable_reason() {
        panic!("the MicroVM substrate is configured but cannot run here: {why}");
    }
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(dir.path());
    // The image the job's publisher signed, under the job's trusted key.
    let backend = MicrovmBackend::new(MicrovmConfig {
        work_dir: dir.path().join("vms"),
        ..cfg.clone()
    })
    .expect("the signed root image verifies");
    let report = assert_conformance(&backend, &ws).await;
    assert_eq!((report.backend, report.isolated), ("microvm", true));
    assert_eq!(
        report.guest_version,
        backend.image().unwrap().guest_version,
        "the guest that booted is the one the manifest names"
    );
    // M8.4: a manifest under another key, a tampered image, and a manifest
    // naming another guest version are refused — the last one only after
    // the guest said who it is.
    let (other_signed, _) = signed_by_test("microvm-rootfs", &cfg.rootfs, &guest_version());
    let other_path = dir.path().join("other.manifest.json");
    std::fs::write(&other_path, serde_json::to_string(&other_signed).unwrap()).unwrap();
    let e = MicrovmBackend::new(MicrovmConfig {
        manifest: other_path,
        work_dir: dir.path().join("v2"),
        ..cfg.clone()
    })
    .err()
    .expect("refused");
    assert!(
        matches!(&e, SandboxError::Refused { code, .. } if code == "IMAGE_UNVERIFIED"),
        "{e}"
    );
    let tampered = dir.path().join("rootfs-tampered.ext4");
    std::fs::copy(&cfg.rootfs, &tampered).unwrap();
    {
        use std::io::{Seek, Write};
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(&tampered)
            .unwrap();
        f.seek(std::io::SeekFrom::Start(4096 * 64)).unwrap();
        f.write_all(b"tampered").unwrap();
    }
    let e = MicrovmBackend::new(MicrovmConfig {
        rootfs: tampered,
        work_dir: dir.path().join("v3"),
        ..cfg.clone()
    })
    .err()
    .expect("refused");
    assert!(
        matches!(&e, SandboxError::Refused { code, .. } if code == "IMAGE_UNVERIFIED"),
        "{e}"
    );
    let (stale, trusted) = signed_by_test("microvm-rootfs", &cfg.rootfs, "0.0.0-stale");
    let stale_path = dir.path().join("stale.manifest.json");
    std::fs::write(&stale_path, serde_json::to_string(&stale).unwrap()).unwrap();
    let stale_backend = MicrovmBackend::new(MicrovmConfig {
        manifest: stale_path,
        trusted_keys: trusted,
        work_dir: dir.path().join("v4"),
        ..cfg.clone()
    })
    .expect("hash matches");
    let policy = compile(&spec(&ws)).unwrap();
    let provisioned = stale_backend
        .provision("stale-vm", &policy)
        .await
        .expect("boots");
    let e = modbit_sandbox::link::GuestLink::admit_image(
        provisioned.channel,
        "stale-vm",
        &policy,
        Duration::from_secs(30),
        stale_backend.image(),
    )
    .await
    .err()
    .expect("refused");
    assert!(
        matches!(&e, SandboxError::Refused { code, .. } if code == "IMAGE_VERSION_MISMATCH"),
        "{e}"
    );
    stale_backend.destroy("stale-vm").await.unwrap();
    let net = report
        .steps
        .iter()
        .find(|s| s.name == "net_control_plane_unreachable")
        .unwrap();
    assert!(net.detail.contains("reachable: false"), "{}", net.detail);
    let health = report.steps.iter().find(|s| s.name == "health").unwrap();
    assert!(
        health.detail.contains("kernel: \"") && !health.detail.contains("kernel: \"\""),
        "the guest reports the MicroVM's kernel: {}",
        health.detail
    );
}

// ---- the gateway over a real Postgres ----

async fn fresh_database(url: &str) -> String {
    let cfg: tokio_postgres::Config = url.parse().expect("database url");
    let (client, conn) = cfg.connect(tokio_postgres::NoTls).await.expect("connect");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let name = format!("modbit_gateway_{}", uuid::Uuid::now_v7().simple());
    client
        .execute(&format!("CREATE DATABASE {name}"), &[])
        .await
        .expect("create database");
    let (head, _) = url.rsplit_once('/').expect("url path");
    format!("{head}/{name}")
}

struct Gw {
    base: String,
    http: reqwest::Client,
    served: modbit_sandbox_gateway::Served,
}

impl Gw {
    async fn post(&self, token: &str, path: &str, body: Value) -> (u16, Value) {
        let r = self
            .http
            .post(format!("{}{path}", self.base))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap();
        (r.status().as_u16(), r.json().await.unwrap_or(Value::Null))
    }
    async fn get(&self, token: &str, path: &str) -> (u16, Value) {
        let r = self
            .http
            .get(format!("{}{path}", self.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        (r.status().as_u16(), r.json().await.unwrap_or(Value::Null))
    }
    async fn delete(&self, token: &str, path: &str) -> (u16, Value) {
        let r = self
            .http
            .delete(format!("{}{path}", self.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        (r.status().as_u16(), r.json().await.unwrap_or(Value::Null))
    }
}

/// QUAL-EV-0286: every lifecycle and RPC request authenticates the worker
/// and binds to the tenant, session lease and task; cross-tenant handle use
/// is denied and audited.
#[tokio::test]
async fn qual_m8_3_the_gateway_binds_sandboxes_to_the_tenant_and_the_workers_session_lease() {
    let Ok(url) = std::env::var("MODBIT_CLOUD_TEST_DATABASE_URL") else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres)"
        );
        return;
    };
    let bin = guest_bin();
    assert!(bin.is_file(), "modbit-guest at {}", bin.display());
    let database_url = fresh_database(&url).await;
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(dir.path());
    let (signed, trusted_keys) = signed_by_test("reference-guest", &bin, &guest_version());
    let manifest_path = dir.path().join("guest.manifest.json");
    std::fs::write(&manifest_path, serde_json::to_string(&signed).unwrap()).unwrap();
    let served = modbit_sandbox_gateway::serve(modbit_sandbox_gateway::Config {
        store: modbit_event_store::cloud::CloudStoreConfig {
            database_url,
            s3: None,
        },
        worker_key: None,
        bind: "127.0.0.1:0".into(),
        backend: modbit_sandbox_gateway::BackendChoice::Reference {
            guest_bin: bin,
            work_dir: dir.path().join("sandboxes"),
            manifest: Some(manifest_path),
            trusted_keys,
        },
    })
    .await
    .expect("gateway");
    let gw = Gw {
        base: format!("http://{}", served.addr),
        http: reqwest::Client::new(),
        served,
    };
    let store = &gw.served.state.store;
    let key = &gw.served.state.worker_key;
    let token = |w: &str| {
        key.issue(&modbit_sandbox::auth::WorkerClaims {
            worker_id: w.into(),
            exp_ms: i64::MAX,
        })
    };
    let ta = store.create_tenant("a").await.unwrap();
    let tb = store.create_tenant("b").await.unwrap();
    // Tenant A's session, ready; worker w1 claims it at generation 1.
    let sa = SessionId::new();
    let task_a = TaskId::new();
    store
        .append(
            modbit_event_store::AppendRequest {
                tenant_id: ta,
                session_id: sa,
                task_id: None,
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: modbit_domain::event::AggregateType::Session,
                aggregate_id: *sa.as_bytes(),
                expected_sequence: Some(0),
                events: vec![
                    modbit_event_store::cloud::new_event(
                        "SessionCreated",
                        &modbit_domain::session::SessionEvent::SessionCreated {
                            tenant_id: ta,
                            user_id: modbit_domain::UserId::new(),
                            space_id: modbit_domain::SpaceId::new(),
                        },
                        modbit_domain::event::Actor::External("test".into()),
                    )
                    .unwrap(),
                ],
            },
            None,
        )
        .await
        .unwrap();
    store.mark_ready(ta, sa).await.unwrap();
    let lease = store
        .claim_session(sa, "w1", 60_000)
        .await
        .unwrap()
        .expect("claimed");
    assert_eq!(lease.generation, 1);
    let spec = json!({"workspace_source": ws.to_string_lossy(), "protected_paths": [".git/hooks"], "resources": {"max_output_bytes": 65536}});
    let body = |tenant: TenantId, generation: u64| json!({"tenant_id": tenant.to_string(), "session_id": sa.to_string(), "task_id": task_a.to_string(), "lease_generation": generation, "spec": spec});
    // No token, a bad token: refused before anything else.
    let r = gw
        .http
        .post(format!("{}/v1/sandboxes", gw.base))
        .json(&body(ta, 1))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 401);
    let (s, e) = gw.post("mbw_00.00", "/v1/sandboxes", body(ta, 1)).await;
    assert_eq!((s, e["code"].as_str()), (401, Some("UNAUTHENTICATED")));
    // A worker that does not hold the lease; the holder at the wrong generation.
    let (s, e) = gw.post(&token("w2"), "/v1/sandboxes", body(ta, 1)).await;
    assert_eq!(
        (s, e["code"].as_str()),
        (403, Some("LEASE_NOT_HELD")),
        "{e}"
    );
    let (s, e) = gw.post(&token("w1"), "/v1/sandboxes", body(ta, 2)).await;
    assert_eq!((s, e["code"].as_str()), (409, Some("STALE_LEASE")), "{e}");
    // Tenant B naming A's session: no lease for B on it — refused and audited on B.
    let (s, e) = gw.post(&token("w1"), "/v1/sandboxes", body(tb, 1)).await;
    assert_eq!(
        (s, e["code"].as_str()),
        (403, Some("LEASE_NOT_HELD")),
        "{e}"
    );
    // The holder provisions.
    let (s, created) = gw.post(&token("w1"), "/v1/sandboxes", body(ta, 1)).await;
    assert_eq!(s, 201, "{created}");
    let sid = created["sandbox_id"].as_str().unwrap().to_owned();
    assert_eq!(
        (created["backend"].as_str(), created["isolated"].as_bool()),
        (Some("reference"), Some(false))
    );
    assert_eq!(created["guest"]["protocol"], "1.1");
    assert_eq!(
        created["image"]["guest_version"], created["guest"]["version"],
        "the sandbox reports the verified image the guest came from: {created}"
    );
    let (_, h) = gw.get("", "/v1/health").await;
    assert_eq!(h["image"]["kind"], "reference-guest", "{h}");
    assert!(
        created["policy"]["protected_paths"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p == "/workspace/.git/hooks")
    );
    // Calls: a real exec through the gateway; a protected write refused by the guest.
    let (s, out) = gw
        .post(&token("w1"), &format!("/v1/sandboxes/{sid}/calls"), json!({"tenant_id": ta.to_string(), "task_id": task_a.to_string(), "effect_id": "eff-1", "call": {"kind": "exec", "argv": fixture(&ws).exec_probe, "env": fixture(&ws).exec_env, "timeout_ms": 20000}}))
        .await;
    assert_eq!(
        (
            s,
            out["exit_code"].as_i64(),
            out["stdout"].as_str().map(str::trim)
        ),
        (200, Some(3), Some("hello")),
        "{out}"
    );
    let (s, out) = gw
        .post(&token("w1"), &format!("/v1/sandboxes/{sid}/calls"), json!({"tenant_id": ta.to_string(), "call": {"kind": "fs.write", "path": "/workspace/.git/hooks/pre-commit", "content": "x"}}))
        .await;
    assert_eq!(
        (s, out["code"].as_str()),
        (409, Some("GUEST_REFUSED")),
        "{out}"
    );
    assert!(
        out["message"]
            .as_str()
            .unwrap()
            .starts_with("PROTECTED_PATH"),
        "{out}"
    );
    let (s, out) = gw
        .post(&token("w1"), &format!("/v1/sandboxes/{sid}/calls"), json!({"tenant_id": ta.to_string(), "call": {"kind": "fs.read", "path": "/workspace/NOTES.md"}}))
        .await;
    assert_eq!(
        (s, out["content"].as_str()),
        (200, Some("# notes\n")),
        "{out}"
    );
    // Cross-tenant handle use: B finds nothing, and the attempt is on B's audit.
    let (s, e) = gw
        .post(
            &token("w1"),
            &format!("/v1/sandboxes/{sid}/calls"),
            json!({"tenant_id": tb.to_string(), "call": {"kind": "health"}}),
        )
        .await;
    assert_eq!((s, e["code"].as_str()), (404, Some("NOT_FOUND")), "{e}");
    let (s, _) = gw
        .get(&token("w1"), &format!("/v1/sandboxes/{sid}?tenant_id={tb}"))
        .await;
    assert_eq!(s, 404);
    let (s, _) = gw
        .delete(&token("w1"), &format!("/v1/sandboxes/{sid}?tenant_id={tb}"))
        .await;
    assert_eq!(s, 404);
    let denials_b = store.denials(tb).await.unwrap();
    assert!(
        denials_b.len() >= 4,
        "B's audit carries the session and the three sandbox attempts: {denials_b:?}"
    );
    assert!(
        denials_b
            .iter()
            .filter(|(r, _)| r == &format!("sandbox:{sid}"))
            .count()
            >= 3
    );
    // Another worker of the same tenant: not the holder.
    let (s, e) = gw
        .post(
            &token("w2"),
            &format!("/v1/sandboxes/{sid}/calls"),
            json!({"tenant_id": ta.to_string(), "call": {"kind": "health"}}),
        )
        .await;
    assert_eq!((s, e["code"].as_str()), (403, Some("NOT_HOLDER")), "{e}");
    // The record, then destroy; afterwards the handle is gone.
    let (s, rec) = gw
        .get(&token("w1"), &format!("/v1/sandboxes/{sid}?tenant_id={ta}"))
        .await;
    assert_eq!(
        (s, rec["state"].as_str(), rec["lease_generation"].as_u64()),
        (200, Some("READY"), Some(1)),
        "{rec}"
    );
    let (s, d) = gw
        .delete(&token("w1"), &format!("/v1/sandboxes/{sid}?tenant_id={ta}"))
        .await;
    assert_eq!((s, d["state"].as_str()), (200, Some("DESTROYED")), "{d}");
    let (s, e) = gw
        .post(
            &token("w1"),
            &format!("/v1/sandboxes/{sid}/calls"),
            json!({"tenant_id": ta.to_string(), "call": {"kind": "health"}}),
        )
        .await;
    assert_eq!((s, e["code"].as_str()), (410, Some("SANDBOX_GONE")), "{e}");
    let (s, rec) = gw
        .get(&token("w1"), &format!("/v1/sandboxes/{sid}?tenant_id={ta}"))
        .await;
    assert_eq!((s, rec["state"].as_str()), (200, Some("DESTROYED")));
    // A's audit has nothing: every attempt on A's sandbox by A's holder was served.
    let denials_a = store.denials(ta).await.unwrap();
    assert!(
        denials_a
            .iter()
            .all(|(r, _)| r != &format!("sandbox:{sid}") || r.is_empty())
            || denials_a.iter().any(|(_, why)| why.contains("w2")),
        "{denials_a:?}"
    );
    gw.served.stop();
    tokio::time::sleep(Duration::from_millis(50)).await;
}
