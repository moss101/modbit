//! A thin client of a running Core for the commands `modbit-cli` has no verb
//! for (PX-136): admit an operator routing plan, materialize and read the
//! outcome statistics, and read the request's accounting record.
//!
//! The harness never starts a Core itself: the CLI's first call spawns one
//! for the profile (`core.ready` in the data directory), and this client
//! attaches to that same Core with the boot secret it published, exactly as
//! the CLI does. Everything it sends is a typed command of the public
//! surface protocol; nothing here reaches into the Core's stores.

use std::path::Path;

use modbit_protocol::client::Client;
use modbit_protocol::local::{ReadyLine, decode_hex};
use modbit_protocol::v1::{
    AcquireSessionLease, AdmitRoutingPlan, ClientKind, CommandEnvelope, GetModelRegistry,
    GetRequestOutcome, GetRoutingPlan, GetSessionSnapshot, GetTaskStatus, Id,
    MaterializeOutcomeStatistics, ModelRegistryView, OutcomeStatisticsView, RequestOutcomeView,
    RoutingAdmissionView, RoutingPlanView, SessionLeaseAcquired, SessionSnapshot, TaskStatus,
};
use prost::Message;

/// An attached connection.
pub struct CoreLink {
    client: Client,
}

fn fresh_id() -> Id {
    Id {
        value: rand::random::<[u8; 16]>().to_vec(),
    }
}

fn envelope(command_type: &str, payload: Vec<u8>, generation: Option<u64>) -> CommandEnvelope {
    CommandEnvelope {
        command_id: Some(fresh_id()),
        tenant_id: None,
        user_id: None,
        session_id: None,
        aggregate_id: None,
        expected_generation: generation,
        command_type: command_type.into(),
        schema_version: 1,
        payload,
        issued_at: None,
    }
}

/// A hex id (as the CLI prints it) as a wire id.
///
/// # Errors
/// The text is not 32 hex digits.
pub fn id_of(hex_id: &str) -> Result<Id, String> {
    let bytes = decode_hex(hex_id).filter(|b| b.len() == 16);
    bytes
        .map(|value| Id { value })
        .ok_or_else(|| format!("`{hex_id}` is not a 16-byte hex id"))
}

impl CoreLink {
    /// Attach to the Core serving `data_dir`.
    ///
    /// # Errors
    /// No ready file, or the Core refused the connection.
    pub async fn attach(data_dir: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(data_dir.join("core.ready"))
            .map_err(|e| format!("no core.ready in {}: {e}", data_dir.display()))?;
        let ready = ReadyLine::parse(text.trim()).ok_or("core.ready is not a ready line")?;
        let secret = decode_hex(&ready.boot_secret_hex).ok_or("bad secret in core.ready")?;
        let client = Client::connect(
            &ready.endpoint,
            &secret,
            ClientKind::Cli,
            "modbit-bench-paired",
        )
        .await
        .map_err(|e| e.to_string())?;
        Ok(Self { client })
    }

    async fn command<M: Message + Default>(
        &mut self,
        command_type: &str,
        payload: Vec<u8>,
        generation: Option<u64>,
    ) -> Result<M, String> {
        let ack = self
            .client
            .command(envelope(command_type, payload, generation))
            .await
            .map_err(|e| format!("{command_type}: {e}"))?;
        Client::result(&ack).map_err(|e| format!("{command_type}: {e}"))
    }

    /// The session's lease generation, acquired when there is none yet.
    ///
    /// # Errors
    /// The Core refused.
    pub async fn lease(&mut self, session: &Id) -> Result<u64, String> {
        let snapshot: SessionSnapshot = self
            .command(
                "GetSessionSnapshot",
                GetSessionSnapshot {
                    session_id: Some(session.clone()),
                }
                .encode_to_vec(),
                None,
            )
            .await?;
        if snapshot.lease_generation > 0 {
            return Ok(snapshot.lease_generation);
        }
        let acquired: SessionLeaseAcquired = self
            .command(
                "AcquireSessionLease",
                AcquireSessionLease {
                    session_id: Some(session.clone()),
                    owner: "modbit-bench-paired".into(),
                }
                .encode_to_vec(),
                None,
            )
            .await?;
        Ok(acquired.lease_generation)
    }

    /// Admit an operator plan for the task's run.
    ///
    /// # Errors
    /// The Core could not be asked; a refusal is in the returned view.
    pub async fn admit_plan(
        &mut self,
        task: &Id,
        lease: u64,
        plan_json: &str,
    ) -> Result<RoutingAdmissionView, String> {
        self.command(
            "AdmitRoutingPlan",
            AdmitRoutingPlan {
                task_id: Some(task.clone()),
                plan_json: plan_json.to_owned(),
            }
            .encode_to_vec(),
            Some(lease),
        )
        .await
    }

    /// The request's accounting record (the whole record as JSON).
    ///
    /// # Errors
    /// The Core could not be asked or the record is not JSON.
    pub async fn request_outcome(
        &mut self,
        task: &Id,
    ) -> Result<(RequestOutcomeView, serde_json::Value), String> {
        let view: RequestOutcomeView = self
            .command(
                "GetRequestOutcome",
                GetRequestOutcome {
                    task_id: Some(task.clone()),
                }
                .encode_to_vec(),
                None,
            )
            .await?;
        let record = if view.record_json.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(&view.record_json)
                .map_err(|e| format!("the accounting record is not JSON: {e}"))?
        };
        Ok((view, record))
    }

    /// Materialize the session's outcome statistics under `version`.
    ///
    /// # Errors
    /// The Core could not be asked; a refusal is in the returned view.
    pub async fn materialize_statistics(
        &mut self,
        session: &Id,
        lease: u64,
        version: &str,
    ) -> Result<OutcomeStatisticsView, String> {
        self.command(
            "MaterializeOutcomeStatistics",
            MaterializeOutcomeStatistics {
                session_id: Some(session.clone()),
                stats_version: version.to_owned(),
            }
            .encode_to_vec(),
            Some(lease),
        )
        .await
    }

    /// The task's status.
    ///
    /// # Errors
    /// The Core could not be asked.
    pub async fn task_status(&mut self, task: &Id) -> Result<TaskStatus, String> {
        self.command(
            "GetTaskStatus",
            GetTaskStatus {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
            None,
        )
        .await
    }

    /// The task's routing plan view.
    ///
    /// # Errors
    /// The Core could not be asked.
    pub async fn routing_plan(&mut self, task: &Id) -> Result<RoutingPlanView, String> {
        self.command(
            "GetRoutingPlan",
            GetRoutingPlan {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
            None,
        )
        .await
    }

    /// The active registry's view.
    ///
    /// # Errors
    /// The Core could not be asked.
    pub async fn registry(&mut self) -> Result<ModelRegistryView, String> {
        self.command(
            "GetModelRegistry",
            GetModelRegistry {}.encode_to_vec(),
            None,
        )
        .await
    }
}
