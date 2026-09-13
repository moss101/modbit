//! Provenance-bound external diagnostics intake (PX-004; docs/29, docs/18):
//! an adapter hands the Core what its editor's language services see. The
//! batch is bound to the workspace revision it names and to the per-file
//! revisions each diagnostic was computed on; the Core normalizes it into
//! the canonical diagnostic record (`modbit_diagnostics::Diagnostic`) with
//! provenance `external_ide`, stores it as an object and records
//! `ExternalDiagnosticsRecorded` on the task's log. From there it is
//! retrieval's diagnostic linkage (the pack's provenance says
//! `external_ide`) and an input of the verification plan — never a
//! verification result: a mandatory check still runs in Modbit. A batch for
//! another workspace revision is discarded (`STALE_REVISION`), a diagnostic
//! whose file moved on is dropped, a malformed batch is refused before
//! anything of it is persisted (`MALFORMED`); both refusals land as
//! `ExternalDiagnosticsRejected`.

use std::collections::BTreeMap;

use modbit_domain::TaskId;
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::TaskEvent;
use modbit_event_store::{AppendRequest, CommandOutcome, CommandRecord, EventStore};
use modbit_protocol::v1 as wire;
use modbit_workspace::WorkspaceService;
use serde::{Deserialize, Serialize};

use crate::runtime::typed;
use crate::server::Core;

/// The provenance every record of this intake carries.
pub const PROVENANCE: &str = "external_ide";

const MAX_BATCH: usize = 5_000;
const SEVERITIES: [&str; 4] = ["error", "warning", "information", "hint"];

/// A normalized batch as stored (object by hash; `batch_ref`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Batch {
    /// Always `external_ide`.
    pub provenance: String,
    /// Language service / adapter identity.
    pub source: String,
    /// Its version.
    pub source_version: String,
    /// Workspace revision the batch is bound to.
    pub workspace_revision: u64,
    /// The content hash each path's diagnostics were computed on.
    pub file_revisions: BTreeMap<String, String>,
    /// Canonical records; `source` names the adapter with `external_ide:` in front.
    pub diagnostics: Vec<modbit_diagnostics::Diagnostic>,
}

/// One diagnostic location at the current revision, for retrieval.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    /// Root-relative path.
    pub path: String,
    /// 1-based line.
    pub line: u32,
    /// Adapter identity.
    pub source: String,
}

fn malformed(detail: impl Into<String>) -> (String, String) {
    ("MALFORMED".to_owned(), detail.into())
}

/// Validate and normalize a submission against the workspace it names.
/// Returns the batch and the number of diagnostics dropped for a file
/// revision that moved on.
fn normalize(
    p: &wire::SubmitExternalDiagnostics,
    ws: &WorkspaceService,
) -> Result<(Batch, u32), (String, String)> {
    let source = p.source.trim();
    if source.is_empty() || source.len() > 200 {
        return Err(malformed("source required (at most 200 characters)"));
    }
    if p.source_version.len() > 100 {
        return Err(malformed("source_version is at most 100 characters"));
    }
    if p.diagnostics.is_empty() {
        return Err(malformed("a batch names at least one diagnostic"));
    }
    if p.diagnostics.len() > MAX_BATCH {
        return Err(malformed(format!(
            "a batch is at most {MAX_BATCH} diagnostics"
        )));
    }
    let mut file_revisions: BTreeMap<String, String> = BTreeMap::new();
    let mut current: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut diagnostics = Vec::with_capacity(p.diagnostics.len());
    let mut discarded = 0u32;
    for (i, d) in p.diagnostics.iter().enumerate() {
        let at = |what: &str| malformed(format!("diagnostic {i}: {what}"));
        let path = d.path.trim();
        if path.is_empty()
            || path.starts_with('/')
            || path.contains("\\")
            || path.split('/').any(|c| c == "..")
        {
            return Err(at("path must be root-relative"));
        }
        if !SEVERITIES.contains(&d.severity.as_str()) {
            return Err(at("severity must be error | warning | information | hint"));
        }
        if d.message.trim().is_empty() || d.message.len() > 4_000 {
            return Err(at("message required (at most 4000 characters)"));
        }
        if (d.line_end, d.char_end) < (d.line_start, d.char_start) {
            return Err(at("range end precedes its start"));
        }
        if d.file_revision.len() != 64 || !d.file_revision.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(at("file_revision must be the sha256 of the file's content"));
        }
        // The file as it is now: a diagnostic computed on other bytes is dropped.
        let now = current
            .entry(path.to_owned())
            .or_insert_with(|| ws.read(path).ok().map(|r| r.content_hash))
            .clone();
        if now.as_deref() != Some(d.file_revision.as_str()) {
            discarded += 1;
            continue;
        }
        file_revisions.insert(path.to_owned(), d.file_revision.clone());
        diagnostics.push(modbit_diagnostics::Diagnostic {
            path: path.to_owned(),
            range: modbit_diagnostics::Range {
                start: modbit_diagnostics::Position {
                    line: d.line_start,
                    character: d.char_start,
                },
                end: modbit_diagnostics::Position {
                    line: d.line_end,
                    character: d.char_end,
                },
            },
            severity: d.severity.clone(),
            code: (!d.code.is_empty()).then(|| d.code.clone()),
            message: d.message.clone(),
            source: format!("{PROVENANCE}:{source}"),
        });
    }
    Ok((
        Batch {
            provenance: PROVENANCE.to_owned(),
            source: source.to_owned(),
            source_version: p.source_version.clone(),
            workspace_revision: p.workspace_revision,
            file_revisions,
            diagnostics,
        },
        discarded,
    ))
}

/// What a recorded command answers again on a retry: the ack of the batch
/// it recorded, or the refusal it recorded.
fn replay_of(
    events: &[modbit_event_store::StoredEvent],
    store: &EventStore,
) -> Result<wire::ExternalDiagnosticsAck, (String, String)> {
    for e in events {
        let Ok(p) = store.payload(&e.envelope) else {
            continue;
        };
        match e.envelope.event_type.as_str() {
            "ExternalDiagnosticsRecorded" => {
                return Ok(wire::ExternalDiagnosticsAck {
                    batch_ref: p["batch_ref"].as_str().unwrap_or_default().to_owned(),
                    recorded: p["recorded"].as_u64().unwrap_or(0) as u32,
                    discarded: p["discarded"].as_u64().unwrap_or(0) as u32,
                    offset: e.offset,
                    replayed: true,
                });
            }
            "ExternalDiagnosticsRejected" => {
                return Err((
                    p["code"].as_str().unwrap_or("MALFORMED").to_owned(),
                    p["detail"].as_str().unwrap_or_default().to_owned(),
                ));
            }
            _ => {}
        }
    }
    Err((
        "REPLAY".to_owned(),
        "the recorded command carries no batch".to_owned(),
    ))
}

/// `SubmitExternalDiagnostics`.
pub async fn submit(
    core: &Core,
    p: &wire::SubmitExternalDiagnostics,
    record: CommandRecord,
    actor: Actor,
) -> Result<wire::ExternalDiagnosticsAck, (String, String)> {
    let Some(task_id) = p
        .task_id
        .as_ref()
        .and_then(|i| <[u8; 16]>::try_from(i.value.as_slice()).ok())
        .map(TaskId::from_bytes)
    else {
        return Err(("BAD_PAYLOAD".to_owned(), "task_id required".to_owned()));
    };
    {
        let store = core.store.lock().await;
        match store.prior_command(&record) {
            Ok(Some(CommandOutcome::Replayed(events))) => return replay_of(&events, &store),
            Ok(_) => {}
            Err(e) => return Err((crate::server::error_code(&e).to_owned(), e.to_string())),
        }
    }
    let task = core
        .store
        .lock()
        .await
        .task(&task_id)
        .map_err(|e| ("STORE".to_owned(), e.to_string()))?
        .ok_or_else(|| ("UNKNOWN_TASK".to_owned(), task_id.to_string()))?;
    let root = task.workspace_root.clone().ok_or_else(|| {
        (
            "NO_WORKSPACE".to_owned(),
            "task has no workspace root".to_owned(),
        )
    })?;
    let (ws, _) = core
        .tools
        .workspace(&root)
        .await
        .map_err(|e| ("WORKSPACE".to_owned(), e.to_string()))?;
    let svc = ws.lock().await;
    let current_revision = svc.revision().number;
    let reject = |code: &str, detail: String| -> Vec<modbit_event_store::NewEvent> {
        vec![typed(
            "ExternalDiagnosticsRejected",
            &TaskEvent::ExternalDiagnosticsRejected {
                source: p.source.clone(),
                source_version: p.source_version.clone(),
                workspace_revision: p.workspace_revision,
                code: code.to_owned(),
                detail,
            },
            actor.clone(),
        )]
    };
    let outcome: Result<(Batch, u32), (String, String)> = if p.workspace_revision
        != current_revision
    {
        Err((
            "STALE_REVISION".to_owned(),
            format!(
                "the batch is bound to workspace revision {}, the workspace is at {current_revision}",
                p.workspace_revision
            ),
        ))
    } else {
        normalize(p, &svc)
    };
    drop(svc);
    let mut store = core.store.lock().await;
    let base = |events| AppendRequest {
        tenant_id: core.tenant_id,
        session_id: task.session_id,
        task_id: Some(task_id),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::Task,
        aggregate_id: *task_id.as_bytes(),
        expected_sequence: None,
        events,
    };
    match outcome {
        Err((code, detail)) => {
            // Refused before anything of the batch is persisted: only the
            // refusal is on the log, under the command record.
            let _ = store.execute_command(record, base(reject(&code, detail.clone())));
            core.last_offset
                .send_replace(store.last_offset().unwrap_or(0));
            Err((code, detail))
        }
        Ok((batch, discarded)) => {
            let batch_ref = store
                .objects()
                .put(serde_json::to_vec(&batch).unwrap_or_default().as_slice())
                .map_err(|e| ("STORE".to_owned(), e.to_string()))?;
            let paths: Vec<String> = batch.file_revisions.keys().cloned().collect();
            let recorded = batch.diagnostics.len() as u32;
            let events = vec![typed(
                "ExternalDiagnosticsRecorded",
                &TaskEvent::ExternalDiagnosticsRecorded {
                    source: batch.source.clone(),
                    source_version: batch.source_version.clone(),
                    workspace_revision: batch.workspace_revision,
                    batch_ref: batch_ref.clone(),
                    recorded,
                    discarded,
                    paths,
                    provenance: PROVENANCE.to_owned(),
                },
                actor,
            )];
            let out = store
                .execute_command(record, base(events))
                .map_err(|e| (crate::server::error_code(&e).to_owned(), e.to_string()))?;
            let events = match out {
                CommandOutcome::Applied(e) | CommandOutcome::Replayed(e) => e,
            };
            let offset = events.last().map_or(0, |e| e.offset);
            core.last_offset.send_replace(offset);
            Ok(wire::ExternalDiagnosticsAck {
                batch_ref,
                recorded,
                discarded,
                offset,
                replayed: false,
            })
        }
    }
}

/// The batches recorded for a task at `workspace_revision` with their
/// object hashes, latest per source, read from the log (record-only
/// events; the objects by hash).
pub(crate) fn batches_at(
    store: &EventStore,
    task_id: TaskId,
    workspace_revision: u64,
) -> Vec<(String, Batch)> {
    let events = store
        .read_aggregate(task_id.as_bytes(), 0, usize::MAX)
        .unwrap_or_default();
    let mut latest: BTreeMap<String, (String, Batch)> = BTreeMap::new();
    for e in &events {
        if e.envelope.event_type != "ExternalDiagnosticsRecorded" {
            continue;
        }
        let Ok(p) = store.payload(&e.envelope) else {
            continue;
        };
        if p["workspace_revision"].as_u64() != Some(workspace_revision) {
            continue;
        }
        let batch_ref = p["batch_ref"].as_str().unwrap_or_default().to_owned();
        let Some(batch) = store
            .objects()
            .get(&batch_ref)
            .ok()
            .and_then(|b| serde_json::from_slice::<Batch>(&b).ok())
        else {
            continue;
        };
        latest.insert(batch.source.clone(), (batch_ref, batch));
    }
    latest.into_values().collect()
}

/// Diagnostic locations for retrieval at the current revision: only from
/// batches bound to it, and only for files whose content is still what the
/// diagnostic was computed on.
pub(crate) fn locations(
    store: &EventStore,
    task_id: TaskId,
    ws: &WorkspaceService,
) -> Vec<Location> {
    let revision = ws.revision().number;
    let mut out = Vec::new();
    let mut hashes: BTreeMap<String, Option<String>> = BTreeMap::new();
    for (_, batch) in batches_at(store, task_id, revision) {
        for d in &batch.diagnostics {
            let now = hashes
                .entry(d.path.clone())
                .or_insert_with(|| ws.read(&d.path).ok().map(|r| r.content_hash))
                .clone();
            if now.as_deref() != batch.file_revisions.get(&d.path).map(String::as_str) {
                continue;
            }
            out.push(Location {
                path: d.path.clone(),
                line: d.range.start.line + 1,
                source: batch.source.clone(),
            });
        }
    }
    out
}

/// The verification plan's inputs from the batches at `workspace_revision`.
pub(crate) fn plan_inputs(
    store: &EventStore,
    task_id: TaskId,
    workspace_revision: u64,
) -> Vec<modbit_verification::plan::ExternalDiagnosticsInput> {
    batches_at(store, task_id, workspace_revision)
        .into_iter()
        .map(
            |(batch_ref, b)| modbit_verification::plan::ExternalDiagnosticsInput {
                source: b.source.clone(),
                source_version: b.source_version.clone(),
                workspace_revision: b.workspace_revision,
                batch_ref,
                count: b.diagnostics.len() as u32,
                paths: b.file_revisions.keys().cloned().collect(),
            },
        )
        .collect()
}
