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
use std::sync::{Arc, Mutex};
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

/// Two host-side HTTP servers for the egress steps (M8.6): one the policy
/// admits (`hello from allowed`), one it does not; and the credentialed
/// target that answers `authorized: true` only with the real secret.
struct EgressStack {
    allowed: std::net::SocketAddr,
    denied: std::net::SocketAddr,
    /// A server a rule of the spec admits and the organisation's allow-list
    /// does not name (PX-085), and how many requests it ever received.
    org_denied: std::net::SocketAddr,
    org_denied_hits: Arc<std::sync::atomic::AtomicUsize>,
    target: std::net::SocketAddr,
    secret: String,
    /// What the target saw in `Authorization` (never printed to the guest).
    seen: Arc<Mutex<Vec<String>>>,
}

async fn egress_stack() -> EgressStack {
    use axum::{Router, routing::get};
    async fn bind(app: Router) -> std::net::SocketAddr {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let a = l.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(l, app).await;
        });
        a
    }
    let secret = format!("real-secret-{}", uuid::Uuid::now_v7().simple());
    let seen: Arc<Mutex<Vec<String>>> = Default::default();
    let allowed =
        bind(Router::new().route("/hello", get(|| async { "hello from allowed\n" }))).await;
    let denied = bind(Router::new().route("/hello", get(|| async { "hello from denied\n" }))).await;
    let org_denied_hits: Arc<std::sync::atomic::AtomicUsize> = Default::default();
    let hits = Arc::clone(&org_denied_hits);
    let org_denied = bind(Router::new().route(
        "/hello",
        get(move || {
            let hits = Arc::clone(&hits);
            async move {
                hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                "hello from org-denied\n"
            }
        }),
    ))
    .await;
    let expected = format!("Bearer {secret}");
    let seen2 = Arc::clone(&seen);
    let target = bind(Router::new().route(
        "/user",
        get(move |headers: axum::http::HeaderMap| {
            let expected = expected.clone();
            let seen = Arc::clone(&seen2);
            async move {
                let got = headers
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default()
                    .to_owned();
                seen.lock().unwrap().push(got.clone());
                if got == expected {
                    (
                        axum::http::StatusCode::OK,
                        "{\"login\":\"ada\",\"authorized\":true}",
                    )
                } else {
                    (
                        axum::http::StatusCode::UNAUTHORIZED,
                        "{\"authorized\":false}",
                    )
                }
            }
        }),
    ))
    .await;
    EgressStack {
        allowed,
        denied,
        org_denied,
        org_denied_hits,
        target,
        secret,
        seen,
    }
}

fn spec(ws: &Path) -> SandboxSpec {
    spec_with(ws, None)
}

fn spec_with(ws: &Path, eg: Option<&EgressStack>) -> SandboxSpec {
    let network = match eg {
        Some(e) => NetworkPolicy {
            egress: vec![
                modbit_sandbox::policy::EgressRule {
                    host: "127.0.0.1".into(),
                    port: e.allowed.port(),
                    capability: "network.egress".into(),
                },
                // A rule the organisation's list does not name (PX-085).
                modbit_sandbox::policy::EgressRule {
                    host: "127.0.0.1".into(),
                    port: e.org_denied.port(),
                    capability: "network.egress".into(),
                },
            ],
            credentials: vec![modbit_sandbox::policy::CredentialGrant {
                handle: "forge-token".into(),
                virtual_host: "forge.modbit.internal".into(),
                target_url: format!("http://{}", e.target),
                header: "Authorization".into(),
                value_prefix: "Bearer ".into(),
                capability: "secret.use".into(),
                // REQ-EV-0288: short-lived by construction. The contract
                // expires and renews it explicitly rather than waiting.
                expires_at_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
                    .unwrap_or_default()
                    + 5 * 60 * 1000,
            }],
            // The organisation's list names the allowed server and the
            // credentialed target, and not the org-denied one.
            org_allow: Some(modbit_sandbox::policy::OrgAllow {
                entries: vec![
                    format!("127.0.0.1:{}", e.allowed.port()),
                    format!("127.0.0.1:{}", e.target.port()),
                ],
                note: "the test organisation's policy".into(),
            }),
        },
        None => NetworkPolicy::default(),
    };
    SandboxSpec {
        tenant_id: TenantId::new(),
        session_id: SessionId::new(),
        task_id: TaskId::new(),
        workspace_source: ws.to_path_buf(),
        protected_paths: vec![".git/hooks".into()],
        network,
        resources: Resources {
            max_output_bytes: 64 * 1024,
            exec_timeout_ms: 30_000,
            workspace_mib: 64,
            ..Resources::default()
        },
        // M8.8: a browser when this host (or the image) has one.
        browser: modbit_sandbox::backend::reference::detect_chromium().is_some()
            || std::env::var("MODBIT_GUEST_ROOTFS").is_ok(),
    }
}

/// Probes for a POSIX guest (busybox in the MicroVM image, the host's shell
/// on the reference backend) and for a Windows host.
fn fixture(ws: &Path) -> Fixture {
    fixture_with(ws, None, false)
}

/// `busybox`: the guest's userland is BusyBox (the MicroVM image) rather
/// than the host's (`curl` on macOS and Windows, `wget`/`curl` on Linux).
fn fixture_with(ws: &Path, eg: Option<&EgressStack>, busybox: bool) -> Fixture {
    let sh = |cmd: &str| -> Vec<String> {
        if cfg!(windows) {
            vec!["cmd".into(), "/C".into(), cmd.into()]
        } else {
            vec!["/bin/sh".into(), "-c".into(), cmd.into()]
        }
    };
    let fetch: Vec<String> = if busybox {
        vec!["/usr/bin/wget".into(), "-qO-".into()]
    } else if cfg!(windows) {
        vec!["curl.exe".into(), "-s".into()]
    } else {
        vec!["/usr/bin/curl".into(), "-s".into()]
    };
    let egress = eg.map(|e| modbit_sandbox::conformance::EgressFixture {
        allowed_url: format!("http://{}/hello", e.allowed),
        denied_url: format!("http://{}/hello", e.denied),
        denied_tunnel_url: format!("https://{}/hello", e.denied),
        org_denied_url: Some(format!("http://{}/hello", e.org_denied)),
        credentialed_url: "http://forge.modbit.internal/user".into(),
        secret: e.secret.clone(),
        secrets: [("forge-token".to_owned(), e.secret.clone())]
            .into_iter()
            .collect(),
        fetch,
        // BusyBox wget has no TLS and sends `https://` as a proxied GET; the
        // tunnel is asked for directly.
        tunnel: busybox.then(|| {
            vec![
                "/bin/sh".into(),
                "-c".into(),
                // The proxy's port is in the environment the guest sets.
                "p=${http_proxy#http://}; printf 'CONNECT %s HTTP/1.1\r\nHost: %s\r\n\r\n' \"$0\" \"$0\" | /usr/bin/nc ${p%:*} ${p#*:}".into(),
            ]
        }),
    });
    Fixture {
        spec: spec_with(ws, eg),
        env_probe: if cfg!(windows) { sh("set") } else { sh("env") },
        egress,
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
    assert_conformance_with(backend, ws, false).await
}

async fn assert_conformance_with(
    backend: &dyn SandboxBackend,
    ws: &Path,
    busybox: bool,
) -> modbit_sandbox::conformance::Report {
    // The egress servers live for the suite; the broker's audit and the
    // target's view of the secret are checked by the suite's own steps and
    // here (the target saw the real secret exactly as injected).
    let eg = egress_stack().await;
    let fx = fixture_with(ws, Some(&eg), busybox);
    // A hang anywhere in the substrate fails the suite, never the job.
    let outcome = tokio::time::timeout(Duration::from_secs(600), run(backend, &fx))
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
    let seen = eg.seen.lock().unwrap().clone();
    assert!(
        seen.iter().any(|a| a == &format!("Bearer {}", eg.secret)),
        "the credentialed target saw the injected secret: {seen:?}"
    );
    // REQ-EV-0288: the short-lived-handle steps are part of every backend's
    // contract, not an optional extra — a report without them is a report
    // that did not prove the handle expires.
    assert_eq!(
        eg.org_denied_hits.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "the server the organisation's list does not name never saw a request"
    );
    for name in [
        "egress_org_list_caps_a_rule",
        "egress_credentialed",
        "egress_secret_never_in_guest",
        "credential_handle_is_short_lived_and_absent_from_the_policy",
        "credential_handle_expires",
        "credential_handle_renews",
    ] {
        assert!(
            report.steps.iter().any(|s| s.name == name),
            "the contract ran `{name}`: {:?}",
            report.steps.iter().map(|s| s.name).collect::<Vec<_>>()
        );
    }
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
            .expect("verified guest binary")
            .with_chromium(modbit_sandbox::backend::reference::detect_chromium());
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
    let report = assert_conformance_with(&backend, &ws, true).await;
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
            chromium: modbit_sandbox::backend::reference::detect_chromium(),
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

/// PX-085 (AUT-D07): the organisation's egress allow-list is the tenant's
/// signed policy bundle's `network_allow`, and the gateway reads and verifies
/// it itself when it provisions a sandbox - a request cannot name one, a
/// bundle that cannot be verified or has expired closes egress, and a tenant
/// with no list is unrestricted by an organisation.
#[tokio::test]
async fn qual_px_085_the_gateway_reads_the_tenants_signed_organisation_list_itself_and_fails_closed()
 {
    use ed25519_dalek::SigningKey;
    use modbit_domain::policy_bundle::{self, BundleDocument, KIND, SCHEMA_VERSION};
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
            chromium: modbit_sandbox::backend::reference::detect_chromium(),
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
    let token = gw
        .served
        .state
        .worker_key
        .issue(&modbit_sandbox::auth::WorkerClaims {
            worker_id: "w1".into(),
            exp_ms: i64::MAX,
        });
    let org_key = SigningKey::from_bytes(&[85u8; 32]);
    let stranger = SigningKey::from_bytes(&[86u8; 32]);
    let doc = |tenant: TenantId, generation: u64, ttl_ms: i64, admin: Value| BundleDocument {
        kind: KIND.into(),
        schema_version: SCHEMA_VERSION,
        tenant_id: tenant.to_string(),
        generation,
        issued_at_ms: modbit_domain::Timestamp::now().millis() - 10_000,
        expires_at_ms: modbit_domain::Timestamp::now().millis() + ttl_ms,
        min_protocol_major: 1,
        admin_config: admin,
    };
    // One tenant per case, each with a ready session its worker holds.
    let provision = |tenant: TenantId, extra_spec: Value| {
        let (gw, token, ws) = (&gw, token.clone(), ws.clone());
        async move {
            let session = SessionId::new();
            let task = TaskId::new();
            let store = &gw.served.state.store;
            store
                .append(
                    modbit_event_store::AppendRequest {
                        tenant_id: tenant,
                        session_id: session,
                        task_id: None,
                        run_id: None,
                        turn_id: None,
                        step_id: None,
                        aggregate_type: modbit_domain::event::AggregateType::Session,
                        aggregate_id: *session.as_bytes(),
                        expected_sequence: Some(0),
                        events: vec![
                            modbit_event_store::cloud::new_event(
                                "SessionCreated",
                                &modbit_domain::session::SessionEvent::SessionCreated {
                                    tenant_id: tenant,
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
            store.mark_ready(tenant, session).await.unwrap();
            let lease = store
                .claim_session(session, "w1", 60_000)
                .await
                .unwrap()
                .expect("claimed");
            let mut spec = json!({"workspace_source": ws.to_string_lossy(), "protected_paths": [".git/hooks"], "egress": [{"host": "reports.example.com", "port": 443, "capability": "network.egress"}]});
            for (k, v) in extra_spec.as_object().cloned().unwrap_or_default() {
                spec[k] = v;
            }
            let (s, created) = gw
                .post(&token, "/v1/sandboxes", json!({"tenant_id": tenant.to_string(), "session_id": session.to_string(), "task_id": task.to_string(), "lease_generation": lease.generation, "spec": spec}))
                .await;
            assert_eq!(s, 201, "{created}");
            created["policy"]["org_allow"].clone()
        }
    };

    // No bundle ever: no organisation restriction. A request that names a list
    // of its own is not believed.
    let t0 = store.create_tenant("no-bundle").await.unwrap();
    assert_eq!(provision(t0, json!({})).await, Value::Null);
    assert_eq!(
        provision(
            t0,
            json!({"org_allow": {"entries": ["*"], "note": "from the request"}})
        )
        .await,
        Value::Null,
        "the request cannot set the organisation's list"
    );

    // A verified bundle with `network_allow`: that list.
    let t1 = store.create_tenant("with-list").await.unwrap();
    store
        .put_org_key(
            t1,
            "org-1",
            &hex::encode(org_key.verifying_key().to_bytes()),
        )
        .await
        .unwrap();
    let d = doc(
        t1,
        1,
        3_600_000,
        json!({"network_allow": ["reports.example.com", "*.corp.example", "api.example.com:8443"]}),
    );
    let published =
        serde_json::to_value(policy_bundle::sign(&d, "org-1", &org_key).unwrap()).unwrap();
    assert!(
        store
            .publish_policy_bundle(t1, 1, "org-1", &published, uuid::Uuid::nil())
            .await
            .unwrap()
    );
    let got = provision(t1, json!({})).await;
    assert_eq!(
        got["entries"],
        json!([
            "reports.example.com",
            "*.corp.example",
            "api.example.com:8443"
        ]),
        "{got}"
    );
    assert!(
        got["note"].as_str().unwrap().contains("generation 1"),
        "{got}"
    );

    // A verified bundle with no `network_allow`: the organisation restricts nothing.
    let t2 = store.create_tenant("no-network-key").await.unwrap();
    store
        .put_org_key(
            t2,
            "org-1",
            &hex::encode(org_key.verifying_key().to_bytes()),
        )
        .await
        .unwrap();
    let d = doc(t2, 1, 3_600_000, json!({"models_allow": ["gpt-5"]}));
    let published =
        serde_json::to_value(policy_bundle::sign(&d, "org-1", &org_key).unwrap()).unwrap();
    store
        .publish_policy_bundle(t2, 1, "org-1", &published, uuid::Uuid::nil())
        .await
        .unwrap();
    assert_eq!(provision(t2, json!({})).await, Value::Null);

    // A bundle signed by a key the organisation never registered, written
    // straight into the table past the API: egress is closed, and the note
    // says why.
    let t3 = store.create_tenant("forged").await.unwrap();
    store
        .put_org_key(
            t3,
            "org-1",
            &hex::encode(org_key.verifying_key().to_bytes()),
        )
        .await
        .unwrap();
    let d = doc(t3, 1, 3_600_000, json!({"network_allow": ["*"]}));
    let forged =
        serde_json::to_value(policy_bundle::sign(&d, "org-1", &stranger).unwrap()).unwrap();
    store
        .publish_policy_bundle(t3, 1, "org-1", &forged, uuid::Uuid::nil())
        .await
        .unwrap();
    let got = provision(t3, json!({})).await;
    assert_eq!(
        got["entries"],
        json!([]),
        "a forged list admits nothing: {got}"
    );
    assert!(
        got["note"]
            .as_str()
            .unwrap()
            .contains("BUNDLE_BAD_SIGNATURE"),
        "{got}"
    );

    // A bundle past its expiry: closed, not reverted to unrestricted.
    let t4 = store.create_tenant("expired").await.unwrap();
    store
        .put_org_key(
            t4,
            "org-1",
            &hex::encode(org_key.verifying_key().to_bytes()),
        )
        .await
        .unwrap();
    let d = doc(
        t4,
        1,
        -1_000,
        json!({"network_allow": ["reports.example.com"]}),
    );
    let old = serde_json::to_value(policy_bundle::sign(&d, "org-1", &org_key).unwrap()).unwrap();
    store
        .publish_policy_bundle(t4, 1, "org-1", &old, uuid::Uuid::nil())
        .await
        .unwrap();
    let got = provision(t4, json!({})).await;
    assert_eq!(got["entries"], json!([]), "{got}");
    assert!(
        got["note"].as_str().unwrap().contains("BUNDLE_EXPIRED"),
        "{got}"
    );

    // The list is the tenant's: tenant 1's list is not tenant 0's.
    assert_eq!(provision(t0, json!({})).await, Value::Null);
    gw.served.stop();
    tokio::time::sleep(Duration::from_millis(50)).await;
}

/// The organisation list's matching: host, `host:port` and `*.suffix`
/// entries; case does not matter; nothing else matches.
#[test]
fn the_organisations_list_names_hosts_host_ports_and_suffixes_and_nothing_else() {
    use modbit_sandbox::policy::OrgAllow;
    let list = OrgAllow {
        entries: vec![
            "Reports.Example.com".into(),
            "*.corp.example".into(),
            "api.example.com:8443".into(),
        ],
        note: String::new(),
    };
    assert!(list.admits("reports.example.com", 443));
    assert!(
        list.admits("REPORTS.example.com", 80),
        "any port for a bare host"
    );
    assert!(list.admits("git.corp.example", 443));
    assert!(list.admits("corp.example", 443), "the suffix itself");
    assert!(list.admits("api.example.com", 8443));
    assert!(
        !list.admits("api.example.com", 443),
        "a host:port entry names its port"
    );
    assert!(
        !list.admits("evil-corp.example", 443),
        "a suffix is a label boundary"
    );
    assert!(!list.admits("reports.example.com.evil.test", 443));
    assert!(!list.admits("127.0.0.1", 443));
    assert!(
        !OrgAllow::default().admits("reports.example.com", 443),
        "an empty list admits nothing"
    );
}
