//! A Sandbox Gateway for the cloud tests, and the helper binaries' paths.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use modbit_cloud_worker::Config;
use modbit_event_store::cloud::CloudStoreConfig;

use super::worker_config;

/// The `modbit-guest` binary next to the test executables.
pub fn guest_bin() -> PathBuf {
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

/// The `modbit-cli` binary next to the test executables. The hosted cloud job
/// builds only core, execd and guest, so a missing CLI is built here, once per
/// test process, into the target directory the test executable lives in.
/// `MODBIT_CLI_BIN` overrides the path (and is never built).
pub fn cli_bin() -> PathBuf {
    if let Ok(p) = std::env::var("MODBIT_CLI_BIN") {
        return PathBuf::from(p);
    }
    let exe = std::env::current_exe().expect("test exe");
    let dir = exe.parent().and_then(|p| p.parent()).expect("target/debug");
    let bin = dir.join(if cfg!(windows) {
        "modbit-cli.exe"
    } else {
        "modbit-cli"
    });
    static BUILT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    BUILT.get_or_init(|| {
        if bin.exists() {
            return;
        }
        let target = dir.parent().expect("target dir");
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let out = std::process::Command::new(cargo)
            .args(["build", "--locked", "-p", "modbit-cli", "--target-dir"])
            .arg(target)
            .output()
            .expect("run cargo to build modbit-cli");
        assert!(
            out.status.success(),
            "building modbit-cli on demand failed (set MODBIT_CLI_BIN to a prebuilt binary):\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            bin.exists(),
            "modbit-cli not at {} after build",
            bin.display()
        );
    });
    bin
}

/// A Sandbox Gateway: on the reference backend (the same guest as a child
/// process; it isolates nothing and says so) its guest signed by a key of the
/// test's own; or, with [`Gateway::start_preferring_microvm`] where the job
/// provides Firecracker, a kernel and the signed image, on the MicroVM
/// backend.
pub struct Gateway {
    pub base_url: String,
    pub served: modbit_sandbox_gateway::Served,
    /// `microvm` | `reference`.
    pub backend: &'static str,
}

impl Gateway {
    pub async fn start(store: &CloudStoreConfig, dir: &Path) -> Self {
        Self::start_with(store, dir, false).await
    }

    /// The MicroVM backend when the environment names a usable Firecracker
    /// (`MODBIT_FIRECRACKER_BIN`, `MODBIT_GUEST_KERNEL`, `MODBIT_GUEST_ROOTFS`
    /// and the signed manifest), else the reference backend.
    pub async fn start_preferring_microvm(store: &CloudStoreConfig, dir: &Path) -> Self {
        Self::start_with(store, dir, true).await
    }

    async fn start_with(store: &CloudStoreConfig, dir: &Path, prefer_microvm: bool) -> Self {
        std::fs::create_dir_all(dir).unwrap();
        #[cfg(unix)]
        let microvm = if prefer_microvm {
            modbit_sandbox::backend::microvm::MicrovmConfig::from_env()
                .filter(|c| c.unavailable_reason().is_none())
                .map(|c| modbit_sandbox::backend::microvm::MicrovmConfig {
                    work_dir: dir.join("vms"),
                    ..c
                })
        } else {
            None
        };
        #[cfg(not(unix))]
        let microvm: Option<()> = {
            let _ = prefer_microvm;
            None
        };
        let (backend, kind) = match microvm {
            #[cfg(unix)]
            Some(c) => (modbit_sandbox_gateway::BackendChoice::Microvm(c), "microvm"),
            _ => (reference_backend(dir), "reference"),
        };
        let worker_key: Vec<u8> = uuid::Uuid::now_v7()
            .as_bytes()
            .iter()
            .chain(uuid::Uuid::new_v4().as_bytes())
            .copied()
            .collect();
        let served = modbit_sandbox_gateway::serve(modbit_sandbox_gateway::Config {
            store: store.clone(),
            worker_key: Some(modbit_sandbox::auth::WorkerKey::new(worker_key)),
            bind: "127.0.0.1:0".into(),
            backend,
        })
        .await
        .expect("gateway");
        eprintln!("test gateway: {kind} backend at {}", served.addr);
        Self {
            base_url: format!("http://{}", served.addr),
            served,
            backend: kind,
        }
    }

    pub fn token_for(&self, worker_id: &str) -> String {
        self.served
            .state
            .worker_key
            .issue(&modbit_sandbox::auth::WorkerClaims {
                worker_id: worker_id.into(),
                exp_ms: i64::MAX,
            })
    }
}

/// The reference backend over the guest binary, its image signed by a key of
/// the test's own.
fn reference_backend(dir: &Path) -> modbit_sandbox_gateway::BackendChoice {
    let bin = guest_bin();
    assert!(bin.is_file(), "modbit-guest at {}", bin.display());
    let key = modbit_sandbox::image::fresh_signing_key();
    let (sha256, size) = modbit_sandbox::image::sha256_file(&bin).unwrap();
    let manifest = modbit_sandbox::image::ImageManifest {
        kind: "reference-guest".into(),
        sha256,
        size,
        guest_version: env!("CARGO_PKG_VERSION").into(),
        guest_protocol: format!(
            "{}.{}",
            modbit_sandbox::GUEST_PROTOCOL_MAJOR,
            modbit_sandbox::GUEST_PROTOCOL_MINOR
        ),
        kernel_sha256: String::new(),
        built_from: "px_cloud_common".into(),
        built_at_ms: 1,
    };
    let signed = modbit_sandbox::image::sign(&manifest, "test-publisher", &key);
    let path = dir.join("guest.manifest.json");
    std::fs::write(&path, serde_json::to_string(&signed).unwrap()).unwrap();
    modbit_sandbox_gateway::BackendChoice::Reference {
        guest_bin: bin,
        work_dir: dir.join("sandboxes"),
        manifest: Some(path),
        trusted_keys: vec![("test-publisher".to_owned(), key.verifying_key().to_bytes())],
        chromium: modbit_sandbox::backend::reference::detect_chromium(),
    }
}

/// [`worker_config`] with the gateway a `cloud_isolated` task provisions from.
pub fn worker_config_gateway(
    store: &CloudStoreConfig,
    id: &str,
    data_dir: &Path,
    model_base: &str,
    gateway: &Gateway,
) -> Config {
    Config {
        sandbox_gateway: Some(modbit_cloud_worker::SandboxGatewayConfig {
            base_url: gateway.base_url.clone(),
            worker_token: gateway.token_for(id),
        }),
        ..worker_config(store, id, data_dir, model_base, None)
    }
}

/// [`worker_config_gateway`] with the forge the tests point at.
pub fn worker_config_gateway_forge(
    store: &CloudStoreConfig,
    id: &str,
    data_dir: &Path,
    model_base: &str,
    gateway: &Gateway,
    forge: &FakeForge,
) -> Config {
    Config {
        forge: Some(modbit_cloud_worker::ForgeConfig {
            api_base_url: forge.base_url.clone(),
            token: forge.token.clone(),
        }),
        ..worker_config_gateway(store, id, data_dir, model_base, gateway)
    }
}

/// A fake forge on loopback: answers `/user` with `authorized: true` only when
/// the `Authorization` header carries the real token, and keeps what it saw.
pub struct FakeForge {
    pub base_url: String,
    pub token: String,
    pub seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

pub async fn fake_forge() -> FakeForge {
    use axum::{Router, routing::get};
    let token = format!("ghp-real-{}", uuid::Uuid::now_v7().simple());
    let expected = format!("Bearer {token}");
    let seen: std::sync::Arc<std::sync::Mutex<Vec<String>>> = Default::default();
    let seen2 = std::sync::Arc::clone(&seen);
    let app = Router::new().route(
        "/user",
        get(move |headers: axum::http::HeaderMap| {
            let expected = expected.clone();
            let seen = std::sync::Arc::clone(&seen2);
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
    );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(l, app).await;
    });
    FakeForge {
        base_url: format!("http://{addr}"),
        token,
        seen,
    }
}

/// A GET against the gateway as `worker` (the egress audit, a sandbox record).
pub async fn gw_get(gateway: &Gateway, worker: &str, path: &str) -> (u16, serde_json::Value) {
    let r = reqwest::Client::new()
        .get(format!("{}{path}", gateway.base_url))
        .bearer_auth(gateway.token_for(worker))
        .send()
        .await
        .unwrap();
    (
        r.status().as_u16(),
        r.json().await.unwrap_or(serde_json::Value::Null),
    )
}
