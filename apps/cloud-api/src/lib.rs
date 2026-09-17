//! `modbit-cloud-api` — the Cloud API (M8.1; docs/24 "Cloud API", docs/30
//! "Cloud HTTP control API", docs/33 "Cloud API implementation"): an
//! authenticated HTTPS control surface over the cloud store — sessions,
//! tasks and their control, approvals bound to intent, cursor replay of
//! committed events, ranged output reads, artifact grants — and a WSS event
//! stream with a resume cursor. Request handlers write commands and events
//! to Postgres and stream what committed; no model or tool code runs here
//! (a Cloud Core Worker does that under a session lease).
//!
//! Every mutating request carries a `command_id`; a retry with the same id
//! answers with the recorded outcome and appends nothing (docs/33
//! "Idempotency"). Every resource is dereferenced inside the caller's
//! tenant; another tenant's id is not found, and the attempt is audited
//! (docs/24 "Multi-tenancy").

#![forbid(unsafe_code)]

pub mod auth;
pub mod rate;
mod routes;
mod stream;

use std::sync::Arc;

use axum::Router;
use modbit_event_store::cloud::{CloudStore, CloudStoreConfig};

pub use auth::TokenKey;

/// Shared state.
pub struct AppState {
    /// The store.
    pub store: CloudStore,
    /// Token signing key.
    pub key: TokenKey,
    /// Rate limiter.
    pub limiter: rate::RateLimiter,
    /// Committed-event notifications (session, session_offset), fanned out to streams.
    pub notify: tokio::sync::broadcast::Sender<(modbit_domain::SessionId, u64)>,
    /// Access token lifetime.
    pub access_ttl_ms: i64,
    /// Refresh token lifetime.
    pub refresh_ttl_ms: i64,
}

/// Service configuration.
#[derive(Clone, Debug)]
pub struct Config {
    /// The store.
    pub store: CloudStoreConfig,
    /// HMAC key bytes for access tokens (`None`: a random key for this process).
    pub token_key: Option<Vec<u8>>,
    /// Bind address.
    pub bind: String,
    /// Requests a principal may burst.
    pub rate_capacity: u32,
    /// Sustained requests per second per principal.
    pub rate_per_second: f64,
}

impl Config {
    /// From the environment (`MODBIT_CLOUD_DATABASE_URL`, `MODBIT_CLOUD_S3_ENDPOINT`,
    /// `MODBIT_CLOUD_S3_BUCKET`, `MODBIT_CLOUD_S3_REGION`, `MODBIT_CLOUD_S3_ACCESS_KEY_ID`,
    /// `MODBIT_CLOUD_S3_SECRET_ACCESS_KEY`, `MODBIT_CLOUD_S3_ALLOW_HTTP`, `MODBIT_CLOUD_TOKEN_KEY_HEX`,
    /// `MODBIT_CLOUD_BIND`).
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
        let token_key = match std::env::var("MODBIT_CLOUD_TOKEN_KEY_HEX") {
            Ok(h) if !h.is_empty() => Some(hex::decode(h)?),
            _ => None,
        };
        Ok(Self {
            store: CloudStoreConfig { database_url, s3 },
            token_key,
            bind: std::env::var("MODBIT_CLOUD_BIND").unwrap_or_else(|_| "127.0.0.1:8787".into()),
            rate_capacity: 120,
            rate_per_second: 30.0,
        })
    }
}

/// Build the router over a connected store.
pub fn router(state: Arc<AppState>) -> Router {
    routes::router(state)
}

/// A running service.
pub struct Served {
    /// Bound address.
    pub addr: std::net::SocketAddr,
    /// State (for tests).
    pub state: Arc<AppState>,
    handle: tokio::task::JoinHandle<()>,
}

impl Served {
    /// Stop serving.
    pub fn stop(self) {
        self.handle.abort();
    }
}

/// Connect the store, apply migrations, start the event listener and serve.
pub async fn serve(cfg: Config) -> anyhow::Result<Served> {
    let store = CloudStore::connect(&cfg.store).await?;
    let (notify, _) = tokio::sync::broadcast::channel(4096);
    let mut rx = store.listen(&cfg.store.database_url).await?;
    let fan = notify.clone();
    tokio::spawn(async move {
        while let Some(n) = rx.recv().await {
            let _ = fan.send(n);
        }
    });
    let state = Arc::new(AppState {
        store,
        key: match cfg.token_key {
            Some(k) => TokenKey::new(k),
            None => {
                eprintln!(
                    "modbit-cloud-api: MODBIT_CLOUD_TOKEN_KEY_HEX unset; access tokens are valid for this process only"
                );
                TokenKey::random()
            }
        },
        limiter: rate::RateLimiter::new(cfg.rate_capacity, cfg.rate_per_second),
        notify,
        access_ttl_ms: 15 * 60 * 1000,
        refresh_ttl_ms: 30 * 24 * 60 * 60 * 1000,
    });
    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    let addr = listener.local_addr()?;
    let app = router(Arc::clone(&state));
    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("modbit-cloud-api: serve: {e}");
        }
    });
    Ok(Served {
        addr,
        state,
        handle,
    })
}
