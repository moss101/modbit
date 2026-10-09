//! PX-011 (docs/24 "Forge webhook intake", docs/29 "Issue-to-task intake"):
//! a GitHub App's webhook to the Cloud API makes the same canonical task
//! the desktop makes from an issue — after the delivery's signature verifies
//! under the app's secret, once per delivery id, for the tenant whose
//! mapping names the repository, under that mapping's policy. The webhook
//! path adds no second task model: the events are `TaskCreated` (origin
//! `forge_webhook`), `TaskQueued`, the issue as an untrusted context
//! document and `TaskCreatedFromIssue` (provenance `forge_webhook`) —
//! appended here when no worker holds the session, made by the worker's
//! Core from the relayed command when one does. Unsigned, mis-signed,
//! replayed, unmapped and mis-installed deliveries are refused and on the
//! denial ledger; every delivery is on the delivery ledger with what came
//! of it.

use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use hmac::{Hmac, KeyInit, Mac};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{ForgeIssueIntake, TaskEvent, TaskOrigin};
use modbit_domain::{SessionId, TaskId, WorkspaceId};
use modbit_event_store::AppendRequest;
use modbit_event_store::cloud::new_event;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::AppState;
use crate::routes::{ApiError, ApiResult, Caller, caller, parse_id, relay_if_held};

const PROVIDER: &str = "github";
/// The command id (and so the task id) a delivery makes: a name derived
/// from the delivery id, so the same delivery can never make two tasks.
fn command_id_of(delivery: &str) -> uuid::Uuid {
    let digest = Sha256::digest(format!("modbit-webhook-1:{delivery}").as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    uuid::Uuid::from_bytes(bytes)
}

fn repository_name(v: &Value) -> Option<String> {
    v["repository"]["full_name"]
        .as_str()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty() && s.len() <= 256 && s.contains('/'))
}

/// `POST /v1/forge/repositories {repository, session_id, installation_id?, intake_label?, workspace_root?, execution_profile?}`:
/// the caller's tenant takes the repository's webhook deliveries into the
/// session. A repository another tenant holds is refused and audited.
pub(crate) async fn map_repository(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
    Json(body): Json<Value>,
) -> ApiResult<Response> {
    let p = caller(&ext);
    let repository = repository_name(&json!({"repository": {"full_name": body["repository"]}}))
        .ok_or_else(|| ApiError::bad("repository (owner/name) required"))?;
    let session_id = body["session_id"].as_str().unwrap_or_default();
    let sid = parse_id(session_id, |s| SessionId::parse(s).ok(), "session_id")?;
    if state.store.session(p.tenant_id, sid).await?.is_none() {
        state
            .store
            .record_denial(
                Some(p.tenant_id),
                Some(p.principal_id),
                &format!("session:{sid}"),
                "not in tenant",
            )
            .await?;
        return Err(ApiError::not_found(format!("session {sid}")));
    }
    let installation_id = body["installation_id"].as_i64().unwrap_or(0);
    let intake_label = body["intake_label"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_owned();
    let workspace_root = body["workspace_root"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_owned();
    let execution_profile = body["execution_profile"]
        .as_str()
        .unwrap_or("cloud_isolated")
        .to_owned();
    if intake_label.len() > 128 || workspace_root.len() > 4096 || execution_profile.len() > 64 {
        return Err(ApiError::bad(
            "intake_label, workspace_root or execution_profile too long",
        ));
    }
    let mapped = state
        .store
        .map_forge_repository(
            p.tenant_id,
            p.principal_id,
            PROVIDER,
            &repository,
            sid,
            installation_id,
            &intake_label,
            &workspace_root,
            &execution_profile,
        )
        .await?;
    let Some(m) = mapped else {
        // Another tenant's repository: denied and audited, the other tenant
        // never named.
        state
            .store
            .record_denial(
                Some(p.tenant_id),
                Some(p.principal_id),
                &format!("forge_repository:{PROVIDER}:{repository}"),
                "mapped by another tenant",
            )
            .await?;
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "REPOSITORY_MAPPED_ELSEWHERE",
            format!("{repository} is mapped by another tenant"),
        ));
    };
    Ok((StatusCode::CREATED, Json(json!(m))).into_response())
}

/// `GET /v1/forge/repositories`: the caller's tenant's mappings, and its
/// deliveries with what came of each.
pub(crate) async fn list_repositories(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
) -> ApiResult<Json<Value>> {
    let p = caller(&ext);
    let repositories = state.store.forge_repositories(p.tenant_id).await?;
    let deliveries = state.store.webhook_deliveries(p.tenant_id).await?;
    Ok(Json(
        json!({"repositories": repositories, "deliveries": deliveries}),
    ))
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .trim()
}

/// GitHub signs the raw body: `X-Hub-Signature-256: sha256=<hex hmac>`.
fn signature_verifies(secret: &[u8], signature: &str, body: &[u8]) -> bool {
    let Some(hex_sig) = signature.strip_prefix("sha256=") else {
        return false;
    };
    let Ok(sig) = hex::decode(hex_sig) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&sig).is_ok()
}

fn refused(
    status: StatusCode,
    code: &'static str,
    message: impl Into<String>,
    extra: Value,
) -> Response {
    let mut body = json!({"code": code, "message": message.into()});
    if let Some(o) = extra.as_object() {
        for (k, v) in o {
            body[k] = v.clone();
        }
    }
    (status, Json(body)).into_response()
}

/// `POST /v1/forge/github/webhook` — the GitHub App's deliveries. No bearer
/// token: the delivery authenticates by its signature under the app's
/// secret; the repository's mapping decides the tenant.
pub(crate) async fn github_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Response> {
    let delivery = header(&headers, "x-github-delivery").to_owned();
    let event = header(&headers, "x-github-event").to_owned();
    let audit_resource = format!(
        "webhook:{PROVIDER}:{}",
        if delivery.is_empty() { "-" } else { &delivery }
    );
    let Some(secret) = state.github_webhook_secret.as_deref() else {
        state
            .store
            .record_denial(
                None,
                None,
                &audit_resource,
                "webhook intake is not configured",
            )
            .await?;
        return Ok(refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "WEBHOOK_DISABLED",
            "this API accepts no forge webhook (MODBIT_CLOUD_GITHUB_WEBHOOK_SECRET unset)",
            Value::Null,
        ));
    };
    // 1. The signature, before anything of the body is read.
    let signature = header(&headers, "x-hub-signature-256");
    if signature.is_empty() {
        state
            .store
            .record_denial(None, None, &audit_resource, "unsigned")
            .await?;
        return Ok(refused(
            StatusCode::UNAUTHORIZED,
            "WEBHOOK_UNSIGNED",
            "X-Hub-Signature-256 is required",
            Value::Null,
        ));
    }
    if !signature_verifies(secret, signature, &body) {
        state
            .store
            .record_denial(None, None, &audit_resource, "signature does not verify")
            .await?;
        return Ok(refused(
            StatusCode::UNAUTHORIZED,
            "WEBHOOK_SIGNATURE",
            "the signature does not verify under the app's secret",
            Value::Null,
        ));
    }
    if delivery.is_empty() || delivery.len() > 128 || event.is_empty() || event.len() > 64 {
        return Err(ApiError::bad(
            "X-GitHub-Delivery and X-GitHub-Event are required",
        ));
    }
    // 2. Once per delivery id: a second delivery of the same id is a replay,
    //    whatever its body says now.
    if let Some(seen) = state
        .store
        .claim_webhook_delivery(PROVIDER, &delivery, &event)
        .await?
    {
        state
            .store
            .record_denial(seen.tenant_id, None, &audit_resource, "replayed delivery")
            .await?;
        return Ok(refused(
            StatusCode::CONFLICT,
            "WEBHOOK_REPLAYED",
            format!("delivery {delivery} was received before"),
            json!({"first_outcome": seen.outcome, "task_id": seen.task_id.map(|t| t.to_string())}),
        ));
    }
    let payload: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            state
                .store
                .finish_webhook_delivery(PROVIDER, &delivery, None, "malformed", None)
                .await?;
            return Err(ApiError::bad(format!("webhook body is not JSON: {e}")));
        }
    };
    // 3. The repository's mapping decides the tenant; a repository nobody
    //    mapped makes nothing anywhere.
    let Some(repository) = repository_name(&payload) else {
        state
            .store
            .finish_webhook_delivery(PROVIDER, &delivery, None, "no_repository", None)
            .await?;
        return Err(ApiError::bad("repository.full_name required"));
    };
    let Some(mapping) = state.store.forge_repository(PROVIDER, &repository).await? else {
        state
            .store
            .finish_webhook_delivery(PROVIDER, &delivery, None, "unmapped", None)
            .await?;
        state
            .store
            .record_denial(
                None,
                None,
                &audit_resource,
                &format!("{repository} is mapped to no tenant"),
            )
            .await?;
        return Ok(refused(
            StatusCode::NOT_FOUND,
            "REPOSITORY_UNMAPPED",
            format!("{repository} is mapped to no tenant"),
            Value::Null,
        ));
    };
    let tenant = mapping.tenant_id;
    // 4. Policy: the installation the mapping names, and the event the
    //    mapping takes.
    let installation = payload["installation"]["id"].as_i64().unwrap_or(0);
    if mapping.installation_id != 0 && installation != mapping.installation_id {
        state
            .store
            .finish_webhook_delivery(
                PROVIDER,
                &delivery,
                Some(tenant),
                "installation_mismatch",
                None,
            )
            .await?;
        state
            .store
            .record_denial(
                Some(tenant),
                None,
                &audit_resource,
                &format!(
                    "installation {installation} is not the mapping's {}",
                    mapping.installation_id
                ),
            )
            .await?;
        return Ok(refused(
            StatusCode::FORBIDDEN,
            "INSTALLATION_MISMATCH",
            "the delivery is not from the installation the mapping names",
            Value::Null,
        ));
    }
    let action = payload["action"].as_str().unwrap_or_default().to_owned();
    // PX-126: a check result or a pull-request comment is not a task; it is
    // a notice that the owning task should read the forge again.
    if let Some(r) = ingestion_delivery(
        &state,
        &Delivery {
            id: &delivery,
            event: &event,
            action: &action,
            audit_resource: &audit_resource,
        },
        &payload,
        &mapping,
        &repository,
    )
    .await?
    {
        return Ok(r);
    }
    let takes = event == "issues"
        && ((mapping.intake_label.is_empty() && action == "opened")
            || (!mapping.intake_label.is_empty()
                && action == "labeled"
                && payload["label"]["name"].as_str() == Some(mapping.intake_label.as_str())));
    if !takes {
        let outcome = format!("ignored:{event}.{action}");
        state
            .store
            .finish_webhook_delivery(PROVIDER, &delivery, Some(tenant), &outcome, None)
            .await?;
        return Ok((
            StatusCode::ACCEPTED,
            Json(json!({"code": "IGNORED", "delivery_id": delivery, "event": event, "action": action, "outcome": outcome})),
        )
            .into_response());
    }
    let issue = &payload["issue"];
    let intake = ForgeIssueIntake {
        url: issue["html_url"].as_str().unwrap_or_default().to_owned(),
        number: issue["number"].as_u64().unwrap_or(0),
        title: issue["title"].as_str().unwrap_or_default().to_owned(),
        author: issue["user"]["login"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        state: issue["state"].as_str().unwrap_or("open").to_owned(),
        labels: issue["labels"]
            .as_array()
            .map(|l| {
                l.iter()
                    .filter_map(|x| x["name"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        body: issue["body"].as_str().unwrap_or_default().to_owned(),
    };
    if intake.url.is_empty() || intake.number == 0 {
        state
            .store
            .finish_webhook_delivery(PROVIDER, &delivery, Some(tenant), "malformed_issue", None)
            .await?;
        return Err(ApiError::bad("issue.html_url and issue.number required"));
    }
    // The command (and so the task) is named by the delivery: the same
    // delivery can never make two tasks, on this path or the relayed one.
    let cid = command_id_of(&delivery);
    let sid = mapping.session_id;
    let goal = intake.goal();
    let workspace_root = if mapping.workspace_root.is_empty() {
        Value::Null
    } else {
        json!(mapping.workspace_root)
    };
    // 5. A worker holding the session is the log's only writer: the command
    //    is relayed with the issue and the worker's Core makes the task.
    let principal = modbit_event_store::cloud::Principal {
        principal_id: cid,
        tenant_id: tenant,
        kind: "forge".into(),
        label: format!("{PROVIDER}:{repository}"),
    };
    if let Some(r) = relay_if_held(
        &state,
        &principal,
        sid,
        cid,
        "CreateTask",
        json!({"goal_text": goal, "execution_profile": mapping.execution_profile, "workspace_root": workspace_root, "origin": "forge_webhook", "issue": intake}),
    )
    .await?
    {
        state
            .store
            .finish_webhook_delivery(PROVIDER, &delivery, Some(tenant), "relayed", Some(TaskId::from_bytes(*cid.as_bytes())))
            .await?;
        return Ok(r);
    }
    // 6. Otherwise the canonical events are appended here — the same four
    //    the Core makes from an issue.
    let task_id = TaskId::from_bytes(*cid.as_bytes());
    let actor = Actor::External(format!("{PROVIDER}:{repository}"));
    let text = intake.document_text();
    let document_id = hex::encode(Sha256::digest(text.as_bytes()));
    let content_ref = state
        .store
        .put_object(tenant, text.as_bytes(), "text/markdown")
        .await?;
    let events = vec![
        new_event(
            "TaskCreated",
            &TaskEvent::TaskCreated {
                session_id: sid,
                goal_text: goal.clone(),
                workspace_id: WorkspaceId::new(),
                workspace_root: workspace_root.as_str().map(str::to_owned),
                base_revision: None,
                execution_profile: mapping.execution_profile.clone(),
                policy_profile_id: None,
                origin: TaskOrigin::ForgeWebhook,
            },
            actor.clone(),
        )?,
        new_event("TaskQueued", &TaskEvent::TaskQueued, actor.clone())?,
        new_event(
            "ContextDocumentAttached",
            &TaskEvent::ContextDocumentAttached {
                document_id: document_id.clone(),
                source: format!("forge_webhook:{}", intake.url),
                title: intake.title.clone(),
                content_ref,
                byte_length: text.len() as u64,
                trust: "UNTRUSTED_EXTERNAL_CONTENT".into(),
            },
            actor.clone(),
        )?,
        new_event(
            "TaskCreatedFromIssue",
            &TaskEvent::TaskCreatedFromIssue {
                url: intake.url.clone(),
                number: intake.number,
                title: intake.title.clone(),
                provenance: "forge_webhook".into(),
                document_id,
            },
            actor,
        )?,
    ];
    let result = json!({"task_id": task_id.to_string(), "session_id": sid.to_string(), "state": "QUEUED", "execution_profile": mapping.execution_profile, "delivery_id": delivery, "issue": {"url": intake.url, "number": intake.number, "title": intake.title}});
    let appended = state
        .store
        .append(
            AppendRequest {
                tenant_id: tenant,
                session_id: sid,
                task_id: Some(task_id),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Task,
                aggregate_id: *task_id.as_bytes(),
                expected_sequence: Some(0),
                events,
            },
            Some((cid, "CreateTask", result.clone())),
        )
        .await?;
    state.store.mark_ready(tenant, sid).await?;
    state
        .store
        .finish_webhook_delivery(
            PROVIDER,
            &delivery,
            Some(tenant),
            "task_created",
            Some(task_id),
        )
        .await?;
    let mut out = result;
    out["session_offset"] = json!(appended.session_offset);
    Ok((StatusCode::CREATED, Json(out)).into_response())
}

/// A delivery being decided.
struct Delivery<'a> {
    id: &'a str,
    event: &'a str,
    action: &'a str,
    audit_resource: &'a str,
}

/// What an ingestion event asks of the owning task's Core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ingest {
    /// Read the commit's check runs (`IngestCiResults`).
    Ci,
    /// Read the pull request's comments (`IngestReviewComments`).
    ReviewComments,
}

impl Ingest {
    fn command(self) -> &'static str {
        match self {
            Self::Ci => "Ingest:ci",
            Self::ReviewComments => "Ingest:review_comments",
        }
    }
}

/// The pull request an event is about and when it happened.
struct Subject {
    kind: Ingest,
    number: Option<u64>,
    head_branch: Option<String>,
    at_ms: Option<i64>,
    /// The event was made by a bot (the app's own comments are not input).
    by_bot: bool,
}

/// `YYYY-MM-DDTHH:MM:SSZ` (GitHub's timestamp form) as milliseconds since
/// the epoch.
fn parse_iso_ms(text: &str) -> Option<i64> {
    let t = text.trim();
    let (date, time) = t.split_once('T')?;
    let time = time.strip_suffix('Z')?;
    let mut d = date.split('-');
    let (y, m, day): (i64, i64, i64) = (
        d.next()?.parse().ok()?,
        d.next()?.parse().ok()?,
        d.next()?.parse().ok()?,
    );
    let time = time.split('.').next()?;
    let mut hms = time.split(':');
    let (h, mi, s): (i64, i64, i64) = (
        hms.next()?.parse().ok()?,
        hms.next()?.parse().ok()?,
        hms.next()?.parse().ok()?,
    );
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) || h > 23 || mi > 59 || s > 60 {
        return None;
    }
    // days from civil (Howard Hinnant)
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 24 + h) * 60 + mi) * 60_000 + s * 1000)
}

/// The pull request and the moment of an event this endpoint turns into an
/// ingestion; `None` for every other event (and for actions that are not a
/// completed check or a created comment).
fn subject_of(event: &str, action: &str, p: &Value) -> Option<Subject> {
    let first_pr = |v: &Value| v["pull_requests"][0]["number"].as_u64();
    let by_bot = p["sender"]["type"] == "Bot";
    match (event, action) {
        ("check_suite", "completed") => Some(Subject {
            kind: Ingest::Ci,
            number: first_pr(&p["check_suite"]),
            head_branch: p["check_suite"]["head_branch"].as_str().map(str::to_owned),
            at_ms: p["check_suite"]["updated_at"]
                .as_str()
                .and_then(parse_iso_ms),
            by_bot: false,
        }),
        ("check_run", "completed") => Some(Subject {
            kind: Ingest::Ci,
            number: first_pr(&p["check_run"]),
            head_branch: p["check_run"]["check_suite"]["head_branch"]
                .as_str()
                .map(str::to_owned),
            at_ms: p["check_run"]["completed_at"]
                .as_str()
                .and_then(parse_iso_ms),
            by_bot: false,
        }),
        ("issue_comment", "created") if p["issue"]["pull_request"].is_object() => Some(Subject {
            kind: Ingest::ReviewComments,
            number: p["issue"]["number"].as_u64(),
            head_branch: None,
            at_ms: p["comment"]["created_at"].as_str().and_then(parse_iso_ms),
            by_bot,
        }),
        ("pull_request_review_comment", "created") => Some(Subject {
            kind: Ingest::ReviewComments,
            number: p["pull_request"]["number"].as_u64(),
            head_branch: None,
            at_ms: p["comment"]["created_at"].as_str().and_then(parse_iso_ms),
            by_bot,
        }),
        _ => None,
    }
}

/// PX-126 (docs/24 "Forge webhook intake"): a signed `check_suite`,
/// `check_run`, `issue_comment` or `pull_request_review_comment` delivery
/// reaches the Core that owns the pull request's task as a command —
/// `Ingest:ci` or `Ingest:review_comments`, run there as `IngestCiResults`
/// or `IngestReviewComments`. The delivery's body is never evidence: the
/// Core reads the forge itself, with its own token, and records what it
/// read with provenance. Signature, installation, delivery-id replay and
/// the repository's mapping are checked before this runs; here the event's
/// age, the owning task (in the mapped tenant only) and the durable queue.
async fn ingestion_delivery(
    state: &Arc<AppState>,
    d: &Delivery<'_>,
    payload: &Value,
    mapping: &modbit_event_store::cloud::ForgeRepository,
    repository: &str,
) -> ApiResult<Option<Response>> {
    let Some(subject) = subject_of(d.event, d.action, payload) else {
        return Ok(None);
    };
    let tenant = mapping.tenant_id;
    let finish = |outcome: String, task: Option<TaskId>| {
        let state = Arc::clone(state);
        let id = d.id.to_owned();
        async move {
            state
                .store
                .finish_webhook_delivery(PROVIDER, &id, Some(tenant), &outcome, task)
                .await
        }
    };
    // 1. The event's own time: an old capture replayed under a fresh
    //    delivery id is refused, and so is one from the future.
    let now = modbit_domain::Timestamp::now().millis();
    if let Some(at) = subject.at_ms
        && (now - at > state.extras.webhook_max_age_ms || at - now > state.extras.webhook_skew_ms)
    {
        finish("stale".into(), None).await?;
        state
            .store
            .record_denial(
                Some(tenant),
                None,
                d.audit_resource,
                &format!("event time {at} is outside the accepted window"),
            )
            .await?;
        return Ok(Some(refused(
            StatusCode::BAD_REQUEST,
            "WEBHOOK_STALE",
            "the event's time is outside the accepted window",
            Value::Null,
        )));
    }
    if subject.by_bot {
        let outcome = format!("ignored:{}.bot", d.event);
        finish(outcome.clone(), None).await?;
        return Ok(Some(
            (
                StatusCode::ACCEPTED,
                Json(json!({"code": "IGNORED", "delivery_id": d.id, "outcome": outcome})),
            )
                .into_response(),
        ));
    }
    // 2. The task that owns the pull request, in the mapped tenant.
    let found = state
        .store
        .task_for_pull_request(
            tenant,
            repository,
            subject.number,
            subject.head_branch.as_deref(),
        )
        .await?;
    let Some((task_id, session_id)) = found else {
        // A pull request another tenant's task opened is a mapping that
        // names the wrong tenant: refused and audited, and nothing of the
        // other tenant is named. One nobody's task opened is an ordinary
        // human pull request on a mapped repository.
        let elsewhere = match subject.number {
            Some(n) => state.store.pull_request_owner_tenant(repository, n).await?,
            None => None,
        };
        if elsewhere.is_some_and(|t| t != tenant) {
            finish("wrong_tenant".into(), None).await?;
            state
                .store
                .record_denial(
                    Some(tenant),
                    None,
                    d.audit_resource,
                    "the pull request belongs to a task of another tenant",
                )
                .await?;
            return Ok(Some(refused(
                StatusCode::NOT_FOUND,
                "PULL_REQUEST_UNKNOWN",
                "no task of this tenant owns that pull request",
                Value::Null,
            )));
        }
        finish("ignored:no_task".into(), None).await?;
        return Ok(Some(
            (
                StatusCode::ACCEPTED,
                Json(json!({"code": "IGNORED", "delivery_id": d.id, "outcome": "ignored:no_task"})),
            )
                .into_response(),
        ));
    };
    // 3. The command is named by the delivery, so a delivery is at most one
    //    ingestion; it is queued durably (the log's owner may be down or
    //    between workers) and the session is made ready for one.
    let cid = command_id_of(d.id);
    let queued = state
        .store
        .enqueue_command(
            tenant,
            session_id,
            cid,
            subject.kind.command(),
            json!({"task_id": task_id.to_string(), "delivery_id": d.id, "event": d.event, "action": d.action, "repository": repository}),
        )
        .await?;
    let held = state.store.held_by(tenant, session_id).await?;
    if held.is_none() {
        state.store.mark_ready(tenant, session_id).await?;
    }
    finish(
        format!(
            "{}:{}",
            if held.is_some() { "relayed" } else { "queued" },
            subject.kind.command()
        ),
        Some(task_id),
    )
    .await?;
    Ok(Some(
        (
            StatusCode::ACCEPTED,
            Json(json!({
                "code": "INGESTION_QUEUED", "command_id": cid.to_string(), "newly_queued": queued,
                "task_id": task_id.to_string(), "kind": subject.kind.command(),
                "relayed_to": held.map(|(w, g)| json!({"worker_id": w, "generation": g})),
            })),
        )
            .into_response(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_timestamps_parse_to_epoch_milliseconds() {
        assert_eq!(parse_iso_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_iso_ms("2026-10-07T20:20:26Z"),
            Some(1_791_404_426_000)
        );
        assert_eq!(parse_iso_ms("2000-02-29T23:59:59Z"), Some(951_868_799_000));
        assert_eq!(parse_iso_ms("not a time"), None);
        assert_eq!(parse_iso_ms("2026-13-01T00:00:00Z"), None);
    }

    #[test]
    fn only_completed_checks_and_created_pull_request_comments_are_ingestion_events() {
        let pr = json!({"check_suite": {"head_branch": "b", "pull_requests": [{"number": 9}], "updated_at": "2026-10-07T20:20:26Z"}});
        let s = subject_of("check_suite", "completed", &pr).unwrap();
        assert_eq!((s.kind, s.number), (Ingest::Ci, Some(9)));
        assert!(subject_of("check_suite", "requested", &pr).is_none());
        let issue =
            json!({"issue": {"number": 4}, "comment": {"created_at": "2026-10-07T20:20:26Z"}});
        assert!(
            subject_of("issue_comment", "created", &issue).is_none(),
            "a comment on a plain issue is not a pull-request comment"
        );
        let on_pr = json!({"issue": {"number": 4, "pull_request": {"url": "x"}}, "comment": {}, "sender": {"type": "Bot"}});
        let s = subject_of("issue_comment", "created", &on_pr).unwrap();
        assert!(s.by_bot && s.kind == Ingest::ReviewComments && s.number == Some(4));
        assert!(subject_of("issue_comment", "edited", &on_pr).is_none());
        assert!(subject_of("pull_request", "opened", &on_pr).is_none());
    }

    #[test]
    fn a_delivery_verifies_only_under_its_secret_and_exact_body() {
        let secret = b"s3cret";
        let body = br#"{"action":"opened"}"#;
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
        mac.update(body);
        let sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
        assert!(signature_verifies(secret, &sig, body));
        assert!(!signature_verifies(b"other", &sig, body));
        assert!(!signature_verifies(
            secret,
            &sig,
            br#"{"action":"opened"} "#
        ));
        assert!(!signature_verifies(secret, "sha1=abc", body));
        assert!(!signature_verifies(secret, "", body));
    }

    #[test]
    fn the_delivery_id_names_the_command() {
        let a = command_id_of("d-1");
        let b = command_id_of("d-1");
        let c = command_id_of("d-2");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
