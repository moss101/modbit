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
mod automations;
mod browser_view;
mod forge;
pub mod oidc;
mod provisioning;
pub mod rate;
mod routes;
mod stream;

use std::sync::Arc;

use axum::Router;
use modbit_event_store::cloud::{CloudStore, CloudStoreConfig};

pub use auth::TokenKey;
pub use oidc::OidcConfig;

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
    /// The key worker tokens verify under (M8.8: the worker's outbound link).
    pub worker_key: modbit_sandbox::auth::WorkerKey,
    /// The workers linked to this API and the view frames they send.
    pub workers: browser_view::WorkerLinks,
    /// PX-011: the forge app's webhook secret, in memory only — every
    /// delivery verifies under it; `None`: no webhook is accepted.
    pub github_webhook_secret: Option<Vec<u8>>,
    /// Settings beyond [`Config`] (PX-126, PX-129).
    pub extras: Extras,
    /// PX-129: the identity provider's metadata and keys, cached.
    pub oidc_cache: tokio::sync::Mutex<oidc::OidcCache>,
}

/// Settings of the optional parts of the API, kept apart from [`Config`] so
/// that adding one never changes how a `Config` is built.
#[derive(Clone)]
pub struct Extras {
    /// PX-126: the oldest event time (a check run's completion, a comment's
    /// creation) a webhook delivery may carry; an older one is refused
    /// `WEBHOOK_STALE` and audited.
    pub webhook_max_age_ms: i64,
    /// PX-126: how far in the future an event time may be (clock skew).
    pub webhook_skew_ms: i64,
    /// PX-129: the SHA-256 of the platform administrator's secret (never the
    /// secret itself; `None`: no `/v1/admin` route answers).
    pub admin_secret_hash: Option<[u8; 32]>,
    /// PX-129: the identity provider the API signs people in through
    /// (`None`: the OIDC routes answer `OIDC_DISABLED`).
    pub oidc: Option<OidcConfig>,
    /// PX-085: the server master key per-endpoint webhook secrets derive
    /// from (`HMAC(master, endpoint id || rotation)`); never stored in the
    /// database. `None`: no endpoint can be created and `/v1/hooks` answers
    /// `WEBHOOK_DISABLED` (fail closed).
    pub webhook_master_key: Option<Vec<u8>>,
}

impl std::fmt::Debug for Extras {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Extras")
            .field("webhook_max_age_ms", &self.webhook_max_age_ms)
            .field("admin", &self.admin_secret_hash.is_some())
            .field("oidc", &self.oidc)
            .field("webhook_master_key", &self.webhook_master_key.is_some())
            .finish_non_exhaustive()
    }
}

impl Default for Extras {
    fn default() -> Self {
        Self {
            webhook_max_age_ms: 60 * 60 * 1000,
            webhook_skew_ms: 5 * 60 * 1000,
            admin_secret_hash: None,
            oidc: None,
            webhook_master_key: None,
        }
    }
}

impl Extras {
    /// The platform administrator's secret, kept as its digest.
    #[must_use]
    pub fn with_admin_secret(mut self, secret: &str) -> Self {
        use sha2::Digest;
        self.admin_secret_hash = Some(sha2::Sha256::digest(secret.as_bytes()).into());
        self
    }

    /// PX-085: the master key per-endpoint webhook secrets derive from
    /// (32 bytes or more).
    #[must_use]
    pub fn with_webhook_master_key(mut self, key: &[u8]) -> Self {
        self.webhook_master_key = Some(key.to_vec());
        self
    }

    /// From the environment (`MODBIT_CLOUD_WEBHOOK_MASTER_KEY_HEX`,
    /// `MODBIT_CLOUD_WEBHOOK_MAX_AGE_SECS`,
    /// `MODBIT_CLOUD_ADMIN_SECRET`, `MODBIT_CLOUD_OIDC_*`).
    #[must_use]
    pub fn from_env() -> Self {
        let mut e = Self::default();
        if let Ok(secret) = std::env::var("MODBIT_CLOUD_ADMIN_SECRET")
            && !secret.is_empty()
        {
            e = e.with_admin_secret(&secret);
        }
        e.oidc = OidcConfig::from_env();
        if let Ok(h) = std::env::var("MODBIT_CLOUD_WEBHOOK_MASTER_KEY_HEX")
            && !h.is_empty()
        {
            match hex::decode(&h) {
                Ok(k) if k.len() >= 32 => e.webhook_master_key = Some(k),
                _ => eprintln!(
                    "modbit-cloud-api: MODBIT_CLOUD_WEBHOOK_MASTER_KEY_HEX must be 32 or more bytes of hex; automation webhooks stay off"
                ),
            }
        }
        if let Some(secs) = std::env::var("MODBIT_CLOUD_WEBHOOK_MAX_AGE_SECS")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|s| *s > 0)
        {
            e.webhook_max_age_ms = secs * 1000;
        }
        e
    }
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
    /// HMAC key bytes worker tokens verify under (`None`: a random key —
    /// no worker can link to this process).
    pub worker_key: Option<Vec<u8>>,
    /// PX-011: the GitHub App's webhook secret (`None`: webhook intake off).
    pub github_webhook_secret: Option<Vec<u8>>,
}

impl Config {
    /// From the environment (`MODBIT_CLOUD_DATABASE_URL`, `MODBIT_CLOUD_S3_ENDPOINT`,
    /// `MODBIT_CLOUD_GITHUB_WEBHOOK_SECRET`,
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
        let worker_key = match std::env::var("MODBIT_CLOUD_WORKER_KEY_HEX") {
            Ok(h) if !h.is_empty() => Some(hex::decode(h)?),
            _ => None,
        };
        // The webhook secret is read once and kept in memory; it is never
        // logged, stored or echoed (the same custody as every other secret).
        let github_webhook_secret = match std::env::var("MODBIT_CLOUD_GITHUB_WEBHOOK_SECRET") {
            Ok(s) if !s.is_empty() => Some(s.into_bytes()),
            _ => None,
        };
        Ok(Self {
            store: CloudStoreConfig { database_url, s3 },
            token_key,
            bind: std::env::var("MODBIT_CLOUD_BIND").unwrap_or_else(|_| "127.0.0.1:8787".into()),
            rate_capacity: 120,
            rate_per_second: 30.0,
            worker_key,
            github_webhook_secret,
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

/// Connect the store, apply migrations, start the event listener and serve
/// (the optional settings from the environment).
pub async fn serve(cfg: Config) -> anyhow::Result<Served> {
    serve_with(cfg, Extras::from_env()).await
}

/// [`serve`] with explicit optional settings.
pub async fn serve_with(cfg: Config, extras: Extras) -> anyhow::Result<Served> {
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
        worker_key: match cfg.worker_key {
            Some(k) => modbit_sandbox::auth::WorkerKey::new(k),
            None => {
                eprintln!(
                    "modbit-cloud-api: MODBIT_CLOUD_WORKER_KEY_HEX unset; no worker can link to this process"
                );
                modbit_sandbox::auth::WorkerKey::random()
            }
        },
        workers: browser_view::WorkerLinks::default(),
        github_webhook_secret: cfg.github_webhook_secret,
        extras,
        oidc_cache: tokio::sync::Mutex::new(oidc::OidcCache::default()),
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
