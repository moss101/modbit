//! `modbit-cloud-worker` — the Cloud Core Worker (M8.2; docs/24 "Cloud Core
//! Worker", docs/33 "Session kernel lease", "Cloud worker lifecycle").
//!
//! The worker claims a ready session's fenced lease from Postgres, then
//! becomes that session's execution owner: it spawns the same `modbit-core`
//! the desktop runs (one per session, in the session's tenant), materializes
//! the session's cloud log in it (`ImportMirroredEvents`), runs the queued
//! tasks through the Agent Runtime, executes the commands the Cloud API
//! relayed to it, and mirrors every event the Core records back to the
//! cloud log verbatim under its lease generation. It renews the lease on a
//! heartbeat; a lease it cannot renew fences it — the Core is stopped, the
//! session released — and another worker resumes with a higher generation
//! from the same log.

#![forbid(unsafe_code)]

mod core_process;
mod session;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use modbit_event_store::cloud::{CloudStore, CloudStoreConfig};

/// Worker configuration.
#[derive(Clone, Debug)]
pub struct Config {
    /// The cloud store.
    pub store: CloudStoreConfig,
    /// This worker's id (unique among workers; the lease names it).
    pub worker_id: String,
    /// Where the per-session Cores keep their data.
    pub data_dir: PathBuf,
    /// The `modbit-core` binary.
    pub core_bin: PathBuf,
    /// Lease lifetime; renewed every third of it.
    pub lease_ttl: Duration,
    /// How often an idle worker looks for ready sessions.
    pub poll: Duration,
    /// The provider endpoint and model tasks run with (`None`: the Core's defaults).
    pub endpoint: Option<String>,
    /// The model.
    pub model: Option<String>,
    /// Sessions this worker hosts at once.
    pub capacity: usize,
    /// The provider each hosted Core is configured with at boot (`None`:
    /// the Core's own environment). The key is held in this process's
    /// memory and crosses to the Core once; it is never logged or stored.
    pub provider: Option<ProviderConfig>,
    /// The Sandbox Gateway each hosted Core provisions from under
    /// `cloud_isolated` (M8.5); the worker token is held in memory and
    /// crosses to the Core once.
    pub sandbox_gateway: Option<SandboxGatewayConfig>,
}

/// The Sandbox Gateway a worker's Cores reach (M8.5).
#[derive(Clone)]
pub struct SandboxGatewayConfig {
    /// Base URL.
    pub base_url: String,
    /// The worker's bearer token.
    pub worker_token: String,
}

impl std::fmt::Debug for SandboxGatewayConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SandboxGatewayConfig")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

/// A provider endpoint for the hosted Cores (docs/24 "Cloud Core Worker").
#[derive(Clone)]
pub struct ProviderConfig {
    /// `openai` | `anthropic`.
    pub provider: String,
    /// The credential (empty: none — a loopback or keyless endpoint).
    pub api_key: String,
    /// A compatible endpoint (empty: the provider's own).
    pub base_url: String,
}

impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("provider", &self.provider)
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl Config {
    /// From the environment (`MODBIT_CLOUD_DATABASE_URL`, `MODBIT_CLOUD_S3_*`,
    /// `MODBIT_CLOUD_WORKER_ID`, `MODBIT_CLOUD_WORKER_DATA_DIR`,
    /// `MODBIT_CORE_BIN`, `MODBIT_CLOUD_WORKER_ENDPOINT`, `MODBIT_CLOUD_WORKER_MODEL`,
    /// `MODBIT_CLOUD_WORKER_PROVIDER` with `_API_KEY` and `_BASE_URL`,
    /// `MODBIT_SANDBOX_GATEWAY_URL` with `MODBIT_SANDBOX_WORKER_TOKEN`).
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
        Ok(Self {
            store: CloudStoreConfig { database_url, s3 },
            worker_id: std::env::var("MODBIT_CLOUD_WORKER_ID")
                .unwrap_or_else(|_| format!("worker-{}", uuid::Uuid::now_v7())),
            data_dir: PathBuf::from(
                std::env::var("MODBIT_CLOUD_WORKER_DATA_DIR")
                    .unwrap_or_else(|_| "./cloud-worker".into()),
            ),
            core_bin: PathBuf::from(
                std::env::var("MODBIT_CORE_BIN").unwrap_or_else(|_| "modbit-core".into()),
            ),
            lease_ttl: Duration::from_secs(30),
            poll: Duration::from_secs(2),
            endpoint: std::env::var("MODBIT_CLOUD_WORKER_ENDPOINT")
                .ok()
                .filter(|s| !s.is_empty()),
            model: std::env::var("MODBIT_CLOUD_WORKER_MODEL")
                .ok()
                .filter(|s| !s.is_empty()),
            capacity: 4,
            provider: std::env::var("MODBIT_CLOUD_WORKER_PROVIDER")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|provider| ProviderConfig {
                    provider,
                    api_key: std::env::var("MODBIT_CLOUD_WORKER_PROVIDER_API_KEY")
                        .unwrap_or_default(),
                    base_url: std::env::var("MODBIT_CLOUD_WORKER_PROVIDER_BASE_URL")
                        .unwrap_or_default(),
                }),
            sandbox_gateway: std::env::var("MODBIT_SANDBOX_GATEWAY_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|base_url| SandboxGatewayConfig {
                    base_url,
                    worker_token: std::env::var("MODBIT_SANDBOX_WORKER_TOKEN").unwrap_or_default(),
                }),
        })
    }
}

/// How a worker's hosting of a session stands (docs/33 "Cloud worker
/// lifecycle").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hosting {
    /// Claimed; the Core runs.
    Hosting {
        /// The lease generation claimed.
        generation: u64,
    },
    /// Fenced: the lease went elsewhere, the Core stopped.
    Fenced {
        /// The generation that was fenced.
        generation: u64,
    },
    /// Released (the worker stopped or could not host); the session is ready.
    Released {
        /// The generation released.
        generation: u64,
    },
}

pub(crate) type HostingMap =
    Arc<std::sync::Mutex<std::collections::BTreeMap<modbit_domain::SessionId, Hosting>>>;

/// A running worker.
pub struct Worker {
    stop: tokio::sync::watch::Sender<bool>,
    handle: tokio::task::JoinHandle<()>,
    hosting: HostingMap,
    /// The worker's id.
    pub worker_id: String,
}

impl Worker {
    /// Ask the worker to stop: it releases its leases (sessions stay ready)
    /// and stops its Cores; waits for it.
    pub async fn stop(self) {
        let _ = self.stop.send(true);
        let _ = self.handle.await;
    }

    /// How this worker stands on `session` (`None`: never claimed here).
    pub fn hosting(&self, session: modbit_domain::SessionId) -> Option<Hosting> {
        self.hosting
            .lock()
            .map(|m| m.get(&session).cloned())
            .unwrap_or(None)
    }
}

/// Start a worker: connect, then claim and host ready sessions until stopped.
pub async fn start(cfg: Config) -> anyhow::Result<Worker> {
    std::fs::create_dir_all(&cfg.data_dir)?;
    let store = Arc::new(CloudStore::connect(&cfg.store).await?);
    let (stop, stop_rx) = tokio::sync::watch::channel(false);
    let worker_id = cfg.worker_id.clone();
    let cfg = Arc::new(cfg);
    let hosting: HostingMap = Default::default();
    let handle = tokio::spawn(run_loop(cfg, store, stop_rx, Arc::clone(&hosting)));
    Ok(Worker {
        stop,
        handle,
        hosting,
        worker_id,
    })
}

async fn run_loop(
    cfg: Arc<Config>,
    store: Arc<CloudStore>,
    mut stop: tokio::sync::watch::Receiver<bool>,
    hosting: HostingMap,
) {
    let mut hosted: Vec<(modbit_domain::SessionId, tokio::task::JoinHandle<()>)> = Vec::new();
    loop {
        hosted.retain(|(_, h)| !h.is_finished());
        if hosted.len() < cfg.capacity {
            // Never a session a host here is still winding down (its Core
            // holds the session directory until it has stopped).
            let winding_down: Vec<modbit_domain::SessionId> =
                hosted.iter().map(|(s, _)| *s).collect();
            match store
                .claim_ready_session(
                    &cfg.worker_id,
                    cfg.lease_ttl.as_millis() as i64,
                    &winding_down,
                )
                .await
            {
                Ok(Some(lease)) => {
                    eprintln!(
                        "modbit-cloud-worker[{}]: claimed session {} (tenant {}, generation {})",
                        cfg.worker_id, lease.session_id, lease.tenant_id, lease.generation
                    );
                    let sid = lease.session_id;
                    if let Ok(mut m) = hosting.lock() {
                        m.insert(
                            sid,
                            Hosting::Hosting {
                                generation: lease.generation,
                            },
                        );
                    }
                    let task = tokio::spawn(session::host(
                        Arc::clone(&cfg),
                        Arc::clone(&store),
                        lease,
                        stop.clone(),
                        Arc::clone(&hosting),
                    ));
                    hosted.push((sid, task));
                    continue;
                }
                Ok(None) => {}
                Err(e) => eprintln!("modbit-cloud-worker[{}]: claim: {e}", cfg.worker_id),
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(cfg.poll) => {}
            _ = stop.changed() => {
                if *stop.borrow() { break; }
            }
        }
    }
    for (_, h) in hosted {
        let _ = h.await;
    }
}
