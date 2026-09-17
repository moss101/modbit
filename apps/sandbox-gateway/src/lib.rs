//! The Sandbox Gateway (M8.3; docs/24 "Sandbox Gateway", docs/33 "Sandbox
//! Gateway", docs/21 "Sandbox substrate boundary"; REQ-EV-0286): the
//! tenant-authenticated boundary between Cloud Core Workers and the sandbox
//! substrate. A worker authenticates with its bearer token; every request
//! names the tenant, session and task it acts for, and the gateway checks
//! against the cloud store that this worker holds that session's lease at
//! the generation it presents before it provisions anything. A sandbox is
//! bound to its tenant: a request from another tenant finds nothing and is
//! audited. The gateway compiles the task's entitlement into the policy
//! the backend configures and the guest enforces, boots the guest through
//! the configured [`modbit_sandbox::backend::SandboxBackend`], admits it
//! (protocol negotiation, the ephemeral credential) and thereafter relays
//! typed calls to it over the private channel — it never runs model or
//! tool code itself.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use modbit_event_store::cloud::{CloudStore, CloudStoreConfig};
use modbit_sandbox::auth::WorkerKey;
use modbit_sandbox::backend::reference::ReferenceBackend;
use modbit_sandbox::backend::{Channel, SandboxBackend};
use modbit_sandbox::link::GuestLink;

pub mod routes;

/// Which backend a gateway serves.
#[derive(Clone, Debug)]
pub enum BackendChoice {
    /// Firecracker MicroVMs (the cloud substrate).
    #[cfg(unix)]
    Microvm(modbit_sandbox::backend::microvm::MicrovmConfig),
    /// The reference backend: the guest as a host process, no isolation.
    /// A gateway serving it says so on every sandbox it issues.
    Reference {
        /// The `modbit-guest` binary.
        guest_bin: PathBuf,
        /// Per-sandbox workspaces.
        work_dir: PathBuf,
        /// The binary's signed manifest (M8.4), verified under `trusted_keys`;
        /// `None` runs whatever is at the path (development only).
        manifest: Option<PathBuf>,
        /// Trusted publisher keys.
        trusted_keys: Vec<(String, [u8; 32])>,
    },
}

/// Gateway configuration.
#[derive(Clone)]
pub struct Config {
    /// The cloud store (leases, sandboxes, denials).
    pub store: CloudStoreConfig,
    /// The worker-token key (`None`: random, this process only).
    pub worker_key: Option<WorkerKey>,
    /// Bind address.
    pub bind: String,
    /// The backend.
    pub backend: BackendChoice,
}

impl Config {
    /// From the environment: `MODBIT_CLOUD_DATABASE_URL`, `MODBIT_CLOUD_S3_*`,
    /// `MODBIT_GATEWAY_WORKER_KEY_HEX`, `MODBIT_GATEWAY_BIND`,
    /// `MODBIT_SANDBOX_BACKEND` (`microvm` | `reference`), the MicroVM
    /// variables (`MODBIT_FIRECRACKER_BIN`, `MODBIT_GUEST_KERNEL`,
    /// `MODBIT_GUEST_ROOTFS`, `MODBIT_GUEST_IMAGE_MANIFEST`,
    /// `MODBIT_SANDBOX_WORK_DIR`) or `MODBIT_GUEST_BIN` (with
    /// `MODBIT_GUEST_BIN_MANIFEST`), and the trusted publisher keys
    /// `MODBIT_GUEST_IMAGE_KEYS` (`"id:hex,..."`).
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("MODBIT_CLOUD_DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("MODBIT_CLOUD_DATABASE_URL is required"))?;
        let s3 = match std::env::var("MODBIT_CLOUD_S3_ENDPOINT") {
            Ok(endpoint) if !endpoint.is_empty() => Some(modbit_event_store::cloud::S3Config {
                endpoint,
                bucket: std::env::var("MODBIT_CLOUD_S3_BUCKET").unwrap_or_else(|_| "modbit".into()),
                region: std::env::var("MODBIT_CLOUD_S3_REGION")
                    .unwrap_or_else(|_| "us-east-1".into()),
                access_key_id: std::env::var("MODBIT_CLOUD_S3_ACCESS_KEY_ID").unwrap_or_default(),
                secret_access_key: std::env::var("MODBIT_CLOUD_S3_SECRET_ACCESS_KEY")
                    .unwrap_or_default(),
                allow_http: std::env::var("MODBIT_CLOUD_S3_ALLOW_HTTP").is_ok_and(|v| v == "1"),
            }),
            _ => None,
        };
        let worker_key = match std::env::var("MODBIT_GATEWAY_WORKER_KEY_HEX") {
            Ok(h) if !h.is_empty() => {
                Some(WorkerKey::new(hex::decode(h).map_err(|e| {
                    anyhow::anyhow!("MODBIT_GATEWAY_WORKER_KEY_HEX: {e}")
                })?))
            }
            _ => None,
        };
        let backend = match std::env::var("MODBIT_SANDBOX_BACKEND").unwrap_or_else(|_| "microvm".into()).as_str() {
            "reference" => BackendChoice::Reference {
                guest_bin: PathBuf::from(std::env::var("MODBIT_GUEST_BIN").map_err(|_| anyhow::anyhow!("MODBIT_GUEST_BIN is required for the reference backend"))?),
                work_dir: PathBuf::from(std::env::var("MODBIT_SANDBOX_WORK_DIR").unwrap_or_else(|_| std::env::temp_dir().join("modbit-sandboxes").to_string_lossy().into_owned())),
                manifest: std::env::var("MODBIT_GUEST_BIN_MANIFEST").ok().filter(|v| !v.is_empty()).map(PathBuf::from),
                trusted_keys: modbit_sandbox::image::trusted_keys_from_env(&std::env::var("MODBIT_GUEST_IMAGE_KEYS").unwrap_or_default()),
            },
            #[cfg(unix)]
            "microvm" => BackendChoice::Microvm(
                modbit_sandbox::backend::microvm::MicrovmConfig::from_env()
                    .ok_or_else(|| anyhow::anyhow!("the microvm backend needs MODBIT_FIRECRACKER_BIN, MODBIT_GUEST_KERNEL, MODBIT_GUEST_ROOTFS and MODBIT_GUEST_IMAGE_MANIFEST"))?,
            ),
            other => anyhow::bail!("MODBIT_SANDBOX_BACKEND `{other}` is not a backend of this build"),
        };
        Ok(Self {
            store: CloudStoreConfig { database_url, s3 },
            worker_key,
            bind: std::env::var("MODBIT_GATEWAY_BIND").unwrap_or_else(|_| "127.0.0.1:8090".into()),
            backend,
        })
    }
}

/// A live sandbox: its link, serialized.
pub struct Live {
    /// The admitted link.
    pub link: tokio::sync::Mutex<GuestLink<Channel>>,
    /// Tenant.
    pub tenant_id: modbit_domain::TenantId,
    /// Worker.
    pub worker_id: String,
    /// The guest's hello, for the record.
    pub hello: modbit_protocol::v1::GuestHello,
}

/// Shared state.
pub struct AppState {
    /// The store.
    pub store: CloudStore,
    /// Worker tokens.
    pub worker_key: WorkerKey,
    /// The backend.
    pub backend: Arc<dyn SandboxBackend>,
    /// Live sandboxes by id.
    pub live: tokio::sync::Mutex<HashMap<uuid::Uuid, Arc<Live>>>,
}

/// A running gateway.
pub struct Served {
    /// Bound address.
    pub addr: SocketAddr,
    /// State (tests reach the store and the key through it).
    pub state: Arc<AppState>,
    stop: tokio::sync::watch::Sender<bool>,
}

impl Served {
    /// Stop serving.
    pub fn stop(&self) {
        let _ = self.stop.send(true);
    }
}

/// Bind and serve.
pub async fn serve(cfg: Config) -> anyhow::Result<Served> {
    let store = CloudStore::connect(&cfg.store).await?;
    let worker_key = cfg.worker_key.clone().unwrap_or_else(|| {
        eprintln!("modbit-sandbox-gateway: MODBIT_GATEWAY_WORKER_KEY_HEX unset; worker tokens are valid for this process only");
        WorkerKey::random()
    });
    let backend: Arc<dyn SandboxBackend> = match &cfg.backend {
        #[cfg(unix)]
        BackendChoice::Microvm(mc) => {
            if let Some(why) = mc.unavailable_reason() {
                anyhow::bail!("the microvm backend cannot run here: {why}");
            }
            Arc::new(
                modbit_sandbox::backend::microvm::MicrovmBackend::new(mc.clone())
                    .map_err(|e| anyhow::anyhow!("{e}"))?,
            )
        }
        BackendChoice::Reference {
            guest_bin,
            work_dir,
            manifest,
            trusted_keys,
        } => {
            eprintln!(
                "modbit-sandbox-gateway: serving the REFERENCE backend — guests are host processes with no isolation; not for tenants"
            );
            match manifest {
                Some(m) => {
                    let signed: modbit_sandbox::image::SignedManifest =
                        serde_json::from_str(&std::fs::read_to_string(m)?)?;
                    Arc::new(
                        ReferenceBackend::verified(
                            guest_bin.clone(),
                            work_dir.clone(),
                            &signed,
                            trusted_keys,
                        )
                        .map_err(|e| anyhow::anyhow!("{e}"))?,
                    )
                }
                None => {
                    eprintln!(
                        "modbit-sandbox-gateway: the reference guest binary is UNVERIFIED (no MODBIT_GUEST_BIN_MANIFEST)"
                    );
                    Arc::new(ReferenceBackend::new(guest_bin.clone(), work_dir.clone()))
                }
            }
        }
    };
    let state = Arc::new(AppState {
        store,
        worker_key,
        backend,
        live: tokio::sync::Mutex::new(HashMap::new()),
    });
    let app = routes::router(Arc::clone(&state));
    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    let addr = listener.local_addr()?;
    let (stop, mut stop_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = stop_rx.changed().await;
            })
            .await;
    });
    Ok(Served { addr, state, stop })
}
