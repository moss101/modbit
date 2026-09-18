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

#[cfg(test)]
mod tests {
    use super::*;

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
