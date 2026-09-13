//! The forge the Core lets `forge.*` reach (PX-006; docs/23, docs/29): one
//! configuration in memory — API host, web host, token — from the
//! environment at boot (`MODBIT_GITHUB_TOKEN`, `MODBIT_GITHUB_API_BASE_URL`,
//! `MODBIT_GITHUB_WEB_HOST`) or from `ConfigureForge` (the token crosses
//! the wire once and is held here; it is never journaled, logged, written
//! to any file, echoed in a view or returned to a client). The ledger the
//! tools consult for idempotency is the task's log: `ForgePullRequestOpened`
//! / `ForgePullRequestUpdated` by key.

use std::sync::{Arc, Mutex};

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::TaskEvent;
use modbit_domain::{SessionId, TaskId, TenantId};
use modbit_event_store::{AppendRequest, EventStore};
use modbit_tools::forge::{ForgeConfig, ForgeLedger};
use modbit_tools::registry::BoxFuture;
use serde_json::Value;

use crate::runtime::typed;

/// The configuration in force, replaced whole by `ConfigureForge`.
#[derive(Default)]
pub struct ForgeCustody {
    current: Mutex<Option<Arc<ForgeConfig>>>,
}

impl ForgeCustody {
    /// From the environment at boot.
    pub fn from_env() -> Self {
        let token = std::env::var("MODBIT_GITHUB_TOKEN")
            .ok()
            .filter(|t| !t.trim().is_empty());
        let api_base = std::env::var("MODBIT_GITHUB_API_BASE_URL").ok();
        let custody = Self::default();
        if token.is_some() || api_base.is_some() {
            custody.set(ForgeConfig {
                kind: "github".into(),
                api_base: api_base.unwrap_or_else(|| "https://api.github.com".into()),
                web_host: std::env::var("MODBIT_GITHUB_WEB_HOST")
                    .ok()
                    .filter(|h| !h.is_empty())
                    .unwrap_or_else(|| "github.com".into()),
                token,
            });
        }
        custody
    }

    /// Replace the configuration.
    pub fn set(&self, cfg: ForgeConfig) {
        *self.current.lock().expect("forge custody") = Some(Arc::new(cfg));
    }

    /// The configuration in force.
    pub fn get(&self) -> Option<Arc<ForgeConfig>> {
        self.current.lock().expect("forge custody").clone()
    }
}

/// The task's log as the idempotency ledger.
pub struct LogLedger {
    pub store: Arc<tokio::sync::Mutex<EventStore>>,
    pub tenant_id: TenantId,
    pub session_id: SessionId,
    pub task_id: TaskId,
    pub actor: Actor,
}

impl ForgeLedger for LogLedger {
    fn lookup<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Option<Value>> {
        Box::pin(async move {
            let store = self.store.lock().await;
            let events = store
                .read_aggregate(self.task_id.as_bytes(), 0, usize::MAX)
                .unwrap_or_default();
            let mut found = None;
            for e in &events {
                if !matches!(
                    e.envelope.event_type.as_str(),
                    "ForgePullRequestOpened" | "ForgePullRequestUpdated"
                ) {
                    continue;
                }
                let Ok(p) = store.payload(&e.envelope) else {
                    continue;
                };
                if p["idempotency_key"].as_str() == Some(key) {
                    found = Some(p["result"].clone());
                }
            }
            found
        })
    }

    fn record<'a>(&'a self, tool: &'a str, key: &'a str, value: &'a Value) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let (event_type, ev) = match tool {
                "forge.pr.create" => (
                    "ForgePullRequestOpened",
                    TaskEvent::ForgePullRequestOpened {
                        idempotency_key: key.to_owned(),
                        owner: value["owner"].as_str().unwrap_or_default().to_owned(),
                        repo: value["repo"].as_str().unwrap_or_default().to_owned(),
                        number: value["number"].as_u64().unwrap_or(0),
                        url: value["url"].as_str().unwrap_or_default().to_owned(),
                        head: value["head"]["ref"].as_str().unwrap_or_default().to_owned(),
                        head_sha: value["head"]["sha"].as_str().unwrap_or_default().to_owned(),
                        base: value["base"]["ref"].as_str().unwrap_or_default().to_owned(),
                        result: value.clone(),
                    },
                ),
                _ => (
                    "ForgePullRequestUpdated",
                    TaskEvent::ForgePullRequestUpdated {
                        idempotency_key: key.to_owned(),
                        owner: value["owner"].as_str().unwrap_or_default().to_owned(),
                        repo: value["repo"].as_str().unwrap_or_default().to_owned(),
                        number: value["number"].as_u64().unwrap_or(0),
                        url: value["url"].as_str().unwrap_or_default().to_owned(),
                        state: value["state"].as_str().unwrap_or_default().to_owned(),
                        result: value.clone(),
                    },
                ),
            };
            let mut store = self.store.lock().await;
            let _ = store.append(AppendRequest {
                tenant_id: self.tenant_id,
                session_id: self.session_id,
                task_id: Some(self.task_id),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Task,
                aggregate_id: *self.task_id.as_bytes(),
                expected_sequence: None,
                events: vec![typed(event_type, &ev, self.actor.clone())],
            });
        })
    }
}
