//! The tenant's signed policy bundle on the worker (PX-129; docs/24
//! "Execution policy distribution"). The control plane publishes; the worker
//! is an enforcement point and trusts nothing it has not verified: it reads
//! the tenant's newest bundle from the store and accepts it only if the
//! organisation's registered key signed it, it is written for this tenant, it
//! is fresh and compatible, and its generation is no lower than the last one
//! this session accepted — so a tampered row, a bundle another key signed, or
//! a row put back to an older generation (a rollback) changes nothing. What
//! it accepts becomes the administrator layer of the session's Core
//! (`admin-config.json`, atomically replaced) and is remembered beside it
//! (`policy-bundle.json`) so the floor survives a restart.
//!
//! Fail closed: a tenant that has ever held a bundle starts no task while
//! the bundle it holds has expired; a fresh bundle ends that.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use modbit_domain::TenantId;
use modbit_domain::policy_bundle::{self, BundleDocument, Expect, SignedBundle};
use modbit_event_store::cloud::CloudStore;
use serde_json::Value;

/// How often a session looks for a newer bundle.
const POLL: Duration = Duration::from_secs(2);

/// What the session holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Standing {
    /// The tenant has never held a bundle: nothing to enforce.
    NoPolicy,
    /// A verified bundle is in force until `expires_at_ms`.
    InForce {
        /// Its generation.
        generation: u64,
        /// Its expiry.
        expires_at_ms: i64,
    },
    /// The tenant has held a bundle and none valid is in force: no task starts.
    Lapsed {
        /// The last generation held.
        generation: u64,
    },
}

/// One session's view of its tenant's bundle.
pub(crate) struct PolicySync {
    tenant: TenantId,
    dir: PathBuf,
    last_poll: Option<Instant>,
    held: Option<BundleDocument>,
    /// The generation last refused (logged once, not every poll).
    last_refused: Option<u64>,
}

fn saved_path(dir: &Path) -> PathBuf {
    dir.join("policy-bundle.json")
}

fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

impl PolicySync {
    pub(crate) fn new(tenant: TenantId, dir: &Path) -> Self {
        Self {
            tenant,
            dir: dir.to_path_buf(),
            last_poll: None,
            held: None,
            last_refused: None,
        }
    }

    /// The bundle remembered from before a restart, verified again (the
    /// file is the worker's own, but it is not trusted for being one).
    async fn recall(&mut self, store: &CloudStore, now_ms: i64) {
        let Ok(text) = std::fs::read_to_string(saved_path(&self.dir)) else {
            return;
        };
        let Ok(signed) = serde_json::from_str::<SignedBundle>(&text) else {
            return;
        };
        let Ok(keys) = store.org_keys(self.tenant).await else {
            return;
        };
        // Expired or not, what was held sets the floor; a lapse is judged below.
        let probe = Expect {
            keys: &keys,
            tenant_id: &self.tenant.to_string(),
            now_ms: signed_issue_time(&signed).unwrap_or(now_ms),
            min_generation: 0,
            protocol_major: modbit_protocol::PROTOCOL_VERSION.major,
        };
        if let Ok(doc) = policy_bundle::verify(&signed, &probe) {
            self.held = Some(doc);
        }
    }

    /// Look for a newer bundle (at most every few seconds) and say where the
    /// session stands.
    pub(crate) async fn standing(&mut self, store: &CloudStore) -> Standing {
        let now = modbit_domain::Timestamp::now().millis();
        if self.last_poll.is_none() {
            self.recall(store, now).await;
        }
        if self.last_poll.is_none_or(|t| t.elapsed() >= POLL) {
            self.last_poll = Some(Instant::now());
            if let Err(e) = self.poll(store, now).await {
                eprintln!("modbit-cloud-worker: policy bundle: {e}");
            }
        }
        match &self.held {
            None => Standing::NoPolicy,
            Some(d) if d.expires_at_ms > now => Standing::InForce {
                generation: d.generation,
                expires_at_ms: d.expires_at_ms,
            },
            Some(d) => Standing::Lapsed {
                generation: d.generation,
            },
        }
    }

    async fn poll(&mut self, store: &CloudStore, now: i64) -> anyhow::Result<()> {
        let Some((generation, value)) = store.current_policy_bundle(self.tenant).await? else {
            return Ok(());
        };
        let floor = self.held.as_ref().map_or(0, |d| d.generation);
        if self
            .held
            .as_ref()
            .is_some_and(|d| d.generation == generation)
            && self.applied_matches()
        {
            return Ok(());
        }
        let signed: SignedBundle = match serde_json::from_value(value) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "modbit-cloud-worker: policy bundle of generation {generation} is malformed ({e}); keeping generation {floor}"
                );
                return Ok(());
            }
        };
        let keys = store.org_keys(self.tenant).await?;
        let verdict = policy_bundle::verify(
            &signed,
            &Expect {
                keys: &keys,
                tenant_id: &self.tenant.to_string(),
                now_ms: now,
                min_generation: floor,
                protocol_major: modbit_protocol::PROTOCOL_VERSION.major,
            },
        );
        let doc = match verdict {
            Ok(d) => d,
            Err(r) => {
                // The last good bundle stays in force; the refusal is named,
                // the document is not logged.
                if self.last_refused != Some(generation) {
                    eprintln!(
                        "modbit-cloud-worker: policy bundle of generation {generation} refused ({}): {r}; keeping generation {floor}",
                        r.code()
                    );
                    self.last_refused = Some(generation);
                }
                return Ok(());
            }
        };
        std::fs::create_dir_all(&self.dir)?;
        write_atomically(
            &self.dir.join("admin-config.json"),
            serde_json::to_string_pretty(&doc.admin_config)?.as_bytes(),
        )?;
        write_atomically(
            &saved_path(&self.dir),
            serde_json::to_string(&signed)?.as_bytes(),
        )?;
        eprintln!(
            "modbit-cloud-worker: policy bundle generation {} in force until {}",
            doc.generation, doc.expires_at_ms
        );
        self.held = Some(doc);
        Ok(())
    }

    /// Whether the Core's administrator layer on disk is the held bundle's.
    fn applied_matches(&self) -> bool {
        let Some(d) = &self.held else { return false };
        std::fs::read_to_string(self.dir.join("admin-config.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .is_some_and(|v| v == d.admin_config)
    }
}

/// A bundle's own issue time, for judging a remembered bundle as of then.
fn signed_issue_time(signed: &SignedBundle) -> Option<i64> {
    serde_json::from_str::<Value>(&signed.document)
        .ok()?
        .get("issued_at_ms")?
        .as_i64()
}
