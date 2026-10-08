//! PX-085 (docs/68, DR-PX-2026-10-03-010): cloud automations — definitions
//! as rows of the tenant, signed generic webhooks with replay protection and
//! tenant mapping, forge events matched against event triggers, and the
//! kill switches. No scheduler, engine or approval system is added: a
//! trigger only ever produces the canonical task the forge intake produces
//! (`CloudStore::dispatch_firing`), and the worker's claim loop is the only
//! time evaluator.
//!
//! * A definition is validated by `modbit_automation::parse_and_validate`
//!   (unknown fields and triggers refused, every size bounded) and enabled
//!   only by an approval bound to the exact version hash that lists exactly
//!   the capabilities, paths and hosts it reaches. An edit is a new version
//!   and is disabled until approved again.
//! * `POST /v1/hooks/{endpoint}` authenticates by signature alone. The
//!   per-endpoint secret is `HMAC(master key, endpoint id || rotation)`, so
//!   no raw secret is stored; the signature is checked in constant time and
//!   before the window (a forged request learns nothing about the clock),
//!   the nonce is the primary key of a table (a replay is a unique
//!   violation), and the endpoint — never the body or a header — decides the
//!   tenant. An unknown endpoint answers exactly as a wrong signature does.
//!   Every refusal is on the audit under a typed reason.

use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use hmac::{Hmac, KeyInit, Mac};
use modbit_automation::definition::{
    Definition, Effects, Trigger, parse_and_validate, resolve_inputs,
};
use modbit_automation::filter::trigger_matches;
use modbit_automation::schedule::Spec;
use modbit_automation::webhook::{self, Refusal};
use modbit_event_store::cloud::Principal;
use modbit_event_store::cloud::automation::{
    AutomationRecord, Dispatched, EnableOutcome, FiringInput, Recorded, RunControls, ScheduleSeed,
    VersionRecord,
};
use serde_json::{Map, Value, json};
use sha2::Sha256;

use crate::AppState;
use crate::routes::{ApiError, ApiResult, Caller, caller, parse_id};

/// The execution profiles a cloud automation may run under (AUT-D07): a
/// read-only definition runs under `plan`, anything that writes or reaches
/// out under `cloud_isolated`, the cloud profile the sandbox confines.
pub const CLOUD_PROFILES: &[&str] = &["plan", "cloud_isolated"];

/// The largest webhook body the intake accepts.
const MAX_HOOK_BODY: usize = 256 * 1024;

/// The execution profile a definition's ceiling allows in the cloud, or
/// `None` (refused) for one outside the allow-list.
#[must_use]
pub fn profile_for(effects: Effects) -> &'static str {
    match effects {
        Effects::ReadOnly => "plan",
        _ => "cloud_isolated",
    }
}

/// What a version's run needs, from the validated definition.
#[must_use]
pub fn controls_of(d: &Definition) -> RunControls {
    use modbit_automation::definition::{ConcurrencyPolicy, MissedPolicy};
    RunControls {
        prompt: d.prompt.trim().to_owned(),
        effects: d.profile.effects.label().to_owned(),
        profile: profile_for(d.profile.effects).to_owned(),
        concurrency: match d.concurrency.policy {
            ConcurrencyPolicy::Skip => "skip",
            ConcurrencyPolicy::Queue => "queue",
            ConcurrencyPolicy::Replace => "replace",
        }
        .to_owned(),
        queue_max: d.concurrency.queue_max,
        max_runs_per_hour: d.budget.max_runs_per_hour,
        daily_budget_minor: d.budget.daily_budget_minor,
        max_turns: d.limits.max_turns,
        max_tool_calls: d.limits.max_tool_calls,
        max_cost_minor: d.limits.max_cost_minor,
        max_wall_ms: u64::from(d.limits.deadline_minutes) * 60_000,
        approval_wait_minutes: d.limits.approval_wait_minutes,
        missed_policy: match d.missed.policy {
            MissedPolicy::Skip => "skip",
            MissedPolicy::RunOnce => "run_once",
        }
        .to_owned(),
        catch_up_window_ms: i64::from(d.missed.catch_up_window_minutes) * 60_000,
    }
}

/// [`controls_of`], refused when the execution profile it derives is not on
/// the cloud allow-list (AUT-D07): a definition never names a profile.
fn checked_controls(d: &Definition) -> ApiResult<RunControls> {
    let c = controls_of(d);
    if CLOUD_PROFILES.contains(&c.profile.as_str()) {
        Ok(c)
    } else {
        Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "PROFILE_NOT_ALLOWED",
            format!(
                "`{}` is not an execution profile cloud automations may run under",
                c.profile
            ),
        ))
    }
}

fn sorted(v: &[String]) -> Vec<String> {
    let mut v = v.to_vec();
    v.sort();
    v
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The secret of an endpoint at a rotation: `HMAC(master, id || rotation)`.
/// Nothing but the master key and the counter is needed to recompute it, so
/// no raw secret is stored.
#[must_use]
pub fn endpoint_secret(master: &[u8], endpoint_id: &str, rotation: u32) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(master).expect("hmac accepts any key length");
    mac.update(b"modbit-webhook-endpoint-v1\0");
    mac.update(endpoint_id.as_bytes());
    mac.update(b"\0");
    mac.update(rotation.to_string().as_bytes());
    format!("whsec_{}", hex::encode(mac.finalize().into_bytes()))
}

fn definition_text(body: &Value) -> ApiResult<String> {
    match &body["definition"] {
        Value::String(s) => Ok(s.clone()),
        v @ Value::Object(_) => Ok(v.to_string()),
        _ => Err(ApiError::bad(
            "definition (a JSON document, or its text) is required",
        )),
    }
}

fn issues_error(issues: &[modbit_automation::Issue]) -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "DEFINITION_INVALID",
        serde_json::to_string(issues).unwrap_or_default(),
    )
}

fn view_of(a: &AutomationRecord, current: Option<&VersionRecord>) -> Value {
    let enabled_now = a
        .enabled
        .as_ref()
        .is_some_and(|e| e.version == a.current_version);
    let state = if enabled_now && a.paused {
        "PAUSED"
    } else if enabled_now {
        "ENABLED"
    } else if a.enabled.is_none() && a.current_version > 1 {
        "NEEDS_APPROVAL"
    } else {
        "DISABLED"
    };
    json!({
        "automation_id": a.automation_id, "name": a.name, "state": state,
        "paused": a.paused, "current_version": a.current_version,
        "definition_hash": current.map(|c| c.definition_hash.clone()),
        "execution_profile": current.map(|c| c.controls.profile.clone()),
        "workspace_root": a.workspace_root, "repository": a.repository,
        "service_principal_id": a.service_principal_id,
        "principal": format!("service:{}", a.service_principal_id),
        "enabled": a.enabled,
    })
}

/// A tenant administrator, or a refusal (the same gate as provisioning).
async fn admin(state: &AppState, ext: &axum::Extension<Caller>) -> ApiResult<Principal> {
    crate::provisioning::require_tenant_admin(state, ext).await
}

fn aid(text: &str) -> ApiResult<uuid::Uuid> {
    parse_id(text, |s| uuid::Uuid::parse_str(s).ok(), "automation_id")
}

async fn load(
    state: &AppState,
    p: &Principal,
    id: uuid::Uuid,
) -> ApiResult<(AutomationRecord, VersionRecord)> {
    let Some(a) = state.store.get_automation(p.tenant_id, id).await? else {
        // Another tenant's id is not found, and the attempt is audited.
        state
            .store
            .record_denial(
                Some(p.tenant_id),
                Some(p.principal_id),
                &format!("automation:{id}"),
                "not in tenant",
            )
            .await?;
        return Err(ApiError::not_found(format!("automation {id}")));
    };
    let Some(v) = state
        .store
        .automation_version(p.tenant_id, id, a.current_version)
        .await?
    else {
        return Err(ApiError::not_found(format!("automation {id} version")));
    };
    Ok((a, v))
}

/// `POST /v1/automations/validate {definition}`: the editor's live check.
/// Nothing is stored.
pub(crate) async fn validate(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    admin(&state, &ext).await?;
    let text = definition_text(&body)?;
    Ok(Json(match parse_and_validate(&text) {
        Ok(d) => json!({
            "ok": true, "issues": [], "name": d.name, "definition_hash": d.hash(),
            "execution_profile": profile_for(d.profile.effects),
            "needs_listed_approval": d.needs_listed_approval(),
            "effects": d.profile.effects.label(),
            "capabilities": d.profile.capabilities, "paths": d.profile.paths, "hosts": d.profile.hosts,
        }),
        Err(issues) => json!({"ok": false, "issues": issues}),
    }))
}

/// `POST /v1/automations {definition, workspace_root?, repository?}`.
pub(crate) async fn create(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
    Json(body): Json<Value>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let p = admin(&state, &ext).await?;
    let text = definition_text(&body)?;
    let d = parse_and_validate(&text).map_err(|i| issues_error(&i))?;
    let workspace_root = body["workspace_root"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_owned();
    let repository = body["repository"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if workspace_root.len() > 4096 || repository.len() > 256 {
        return Err(ApiError::bad("workspace_root or repository too long"));
    }
    if !repository.is_empty() && !repository.contains('/') {
        return Err(ApiError::bad("repository is owner/name"));
    }
    let controls = checked_controls(&d)?;
    let Some(rec) = state
        .store
        .create_automation(
            p.tenant_id,
            p.principal_id,
            &d.name,
            &workspace_root,
            &repository,
            &d.canonical(),
            &d.hash(),
            &controls,
        )
        .await?
    else {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "AUTOMATION_EXISTS",
            format!("a definition named `{}` exists in this tenant", d.name),
        ));
    };
    state
        .store
        .audit_provisioning(
            &format!("principal:{}", p.principal_id),
            Some(p.tenant_id),
            "automation.create",
            &rec.automation_id,
            json!({"name": d.name, "hash": d.hash(), "service_principal": rec.service_principal_id}),
        )
        .await?;
    let v = state
        .store
        .automation_version(p.tenant_id, aid(&rec.automation_id)?, 1)
        .await?;
    Ok((StatusCode::CREATED, Json(view_of(&rec, v.as_ref()))))
}

/// `GET /v1/automations`: the tenant's definitions and the pause switches.
pub(crate) async fn list(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
) -> ApiResult<Json<Value>> {
    let p = caller(&ext);
    let mut out = Vec::new();
    for a in state.store.list_automations(p.tenant_id).await? {
        let v = state
            .store
            .automation_version(p.tenant_id, aid(&a.automation_id)?, a.current_version)
            .await?;
        out.push(view_of(&a, v.as_ref()));
    }
    let (global, tenant) = state.store.automation_switches(p.tenant_id).await?;
    Ok(Json(
        json!({"automations": out, "global_paused": global, "tenant_paused": tenant}),
    ))
}

/// `GET /v1/automations/{id}`: one definition with every version.
pub(crate) async fn get(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let p = caller(&ext);
    let (a, cur) = load(&state, &p, aid(&id)?).await?;
    let versions = state
        .store
        .automation_versions(p.tenant_id, aid(&id)?)
        .await?;
    Ok(Json(json!({
        "automation": view_of(&a, Some(&cur)),
        "definition_json": cur.definition_json,
        "versions": versions.iter().map(|v| json!({
            "version": v.version, "definition_hash": v.definition_hash,
            "definition_json": v.definition_json, "created_at_ms": v.created_at_ms,
            "approved": a.enabled.as_ref().is_some_and(|e| e.version == v.version && e.definition_hash == v.definition_hash),
        })).collect::<Vec<_>>(),
    })))
}

/// `GET /v1/automations/{id}/runs?limit=`: the run history (typed reasons
/// for skips; a running firing reads its task's outcome).
pub(crate) async fn runs(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
    Path(id): Path<String>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let p = caller(&ext);
    let id = aid(&id)?;
    load(&state, &p, id).await?;
    let limit = q
        .get("limit")
        .and_then(|l| l.parse::<i64>().ok())
        .unwrap_or(100)
        .clamp(1, 500);
    Ok(Json(
        json!({"runs": state.store.automation_runs(p.tenant_id, Some(id), limit).await?}),
    ))
}

/// `GET /v1/automations/audit`: the tenant's refused deliveries and skips.
pub(crate) async fn audit(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let p = admin(&state, &ext).await?;
    let limit = q
        .get("limit")
        .and_then(|l| l.parse::<i64>().ok())
        .unwrap_or(200)
        .clamp(1, 1000);
    Ok(Json(
        json!({"audit": state.store.automation_audit(Some(p.tenant_id), limit).await?}),
    ))
}

/// `POST /v1/automations/pause {paused, note?}`: the tenant's pause switch.
pub(crate) async fn tenant_pause(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let p = admin(&state, &ext).await?;
    let paused = body["paused"].as_bool().unwrap_or(true);
    state
        .store
        .set_automation_switch(
            &p.tenant_id.to_string(),
            paused,
            body["note"].as_str().unwrap_or_default(),
            &format!("principal:{}", p.principal_id),
        )
        .await?;
    Ok(Json(json!({"tenant_paused": paused})))
}

/// `POST /v1/automations/kill {note?}`: pause the whole tenant, drop queued
/// firings and ask every running automation task to cancel.
pub(crate) async fn tenant_kill(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
) -> ApiResult<Json<Value>> {
    let p = admin(&state, &ext).await?;
    let report = state
        .store
        .kill_automations(p.tenant_id, None, &format!("principal:{}", p.principal_id))
        .await?;
    Ok(Json(json!(report)))
}

/// `POST /v1/admin/automations/pause {paused}`: the platform-wide switch
/// (the platform administrator's secret).
pub(crate) async fn global_pause(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let actor = crate::provisioning::admin_actor(&state, &headers)?;
    let paused = body["paused"].as_bool().unwrap_or(true);
    state
        .store
        .set_automation_switch(
            "global",
            paused,
            body["note"].as_str().unwrap_or_default(),
            actor,
        )
        .await?;
    state
        .store
        .audit_provisioning(
            actor,
            None,
            "automation.global_pause",
            "global",
            json!({"paused": paused}),
        )
        .await?;
    Ok(Json(json!({"global_paused": paused})))
}

/// `POST /v1/admin/automations/audit`: every refusal, including those that
/// never named a tenant (the platform's view).
pub(crate) async fn admin_audit(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    crate::provisioning::admin_actor(&state, &headers)?;
    Ok(Json(
        json!({"audit": state.store.automation_audit(None, 1000).await?}),
    ))
}

/// `POST /v1/automations/{id}:update|:enable|:disable|:pause|:kill`.
pub(crate) async fn action(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
    Path(seg): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let p = admin(&state, &ext).await?;
    let (id, action) = seg.split_once(':').ok_or_else(|| {
        ApiError::bad("expected /v1/automations/{id}:update|:enable|:disable|:pause|:kill")
    })?;
    let id = aid(id)?;
    let (a, cur) = load(&state, &p, id).await?;
    let who = format!("principal:{}", p.principal_id);
    match action {
        "update" => {
            let text = definition_text(&body)?;
            let d = parse_and_validate(&text).map_err(|i| issues_error(&i))?;
            if d.name != a.name {
                return Err(ApiError::bad(
                    "the name of a definition is its identity and does not change",
                ));
            }
            let version = state
                .store
                .add_automation_version(
                    p.tenant_id,
                    id,
                    p.principal_id,
                    &d.canonical(),
                    &d.hash(),
                    &checked_controls(&d)?,
                )
                .await?
                .ok_or_else(|| ApiError::not_found(format!("automation {id}")))?;
            state
                .store
                .audit_provisioning(
                    &who,
                    Some(p.tenant_id),
                    "automation.update",
                    &id.to_string(),
                    json!({"version": version, "hash": d.hash()}),
                )
                .await?;
            let (a, cur) = load(&state, &p, id).await?;
            Ok(Json(view_of(&a, Some(&cur))))
        }
        "enable" => enable(&state, &p, &a, &cur, &body, &who).await,
        "disable" => {
            state.store.disable_automation(p.tenant_id, id).await?;
            state
                .store
                .audit_provisioning(
                    &who,
                    Some(p.tenant_id),
                    "automation.disable",
                    &id.to_string(),
                    json!({}),
                )
                .await?;
            let (a, cur) = load(&state, &p, id).await?;
            Ok(Json(view_of(&a, Some(&cur))))
        }
        "pause" => {
            let paused = body["paused"].as_bool().unwrap_or(true);
            state
                .store
                .set_automation_paused(p.tenant_id, id, paused)
                .await?;
            let (a, cur) = load(&state, &p, id).await?;
            Ok(Json(view_of(&a, Some(&cur))))
        }
        "kill" => {
            let report = state
                .store
                .kill_automations(p.tenant_id, Some(id), &who)
                .await?;
            Ok(Json(json!(report)))
        }
        other => Err(ApiError::bad(format!(
            "unknown automation action `{other}`"
        ))),
    }
}

/// The owner's enable approval (AUT-D03, AUT-D02): bound to the exact
/// version and hash, listing exactly what the profile reaches.
async fn enable(
    state: &AppState,
    p: &Principal,
    a: &AutomationRecord,
    cur: &VersionRecord,
    body: &Value,
    who: &str,
) -> ApiResult<Json<Value>> {
    let version = body["version"].as_u64().unwrap_or(0) as u32;
    let hash = body["definition_hash"].as_str().unwrap_or_default();
    if version != cur.version || hash != cur.definition_hash {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "APPROVAL_MISMATCH",
            format!(
                "the approval names version {version} and hash {hash}; the current version is {} with hash {}. Review the current definition and approve that",
                cur.version, cur.definition_hash
            ),
        ));
    }
    let d = parse_and_validate(&cur.definition_json).map_err(|i| issues_error(&i))?;
    if d.hash() != cur.definition_hash {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "DEFINITION_TAMPERED",
            "the stored definition no longer hashes to its recorded hash",
        ));
    }
    let effects = d.profile.effects.label();
    let caps = strings(&body["capabilities"]);
    let paths = strings(&body["paths"]);
    let hosts = strings(&body["hosts"]);
    if body["effects"].as_str() != Some(effects)
        || sorted(&caps) != sorted(&d.profile.capabilities)
        || sorted(&paths) != sorted(&d.profile.paths)
        || sorted(&hosts) != sorted(&d.profile.hosts)
    {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "APPROVAL_LISTS_DIFFER",
            format!(
                "the approval must list exactly the effects ({effects}), capabilities {:?}, paths {:?} and hosts {:?} the definition asks for",
                d.profile.capabilities, d.profile.paths, d.profile.hosts
            ),
        ));
    }
    // A trigger that runs without a person cannot be missing an input.
    if d.triggers
        .iter()
        .any(|t| !matches!(t, Trigger::Manual { .. }))
        && let Err(issues) = resolve_inputs(&d, &Map::new())
    {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "INPUT_NEEDS_DEFAULT",
            format!(
                "a trigger that starts without a person needs a default for every required input: {}",
                issues
                    .iter()
                    .map(|i| i.path.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    let id = aid(&a.automation_id)?;
    let now = state.store.automation_now_ms().await?;
    let seeds: Vec<ScheduleSeed> = d
        .triggers
        .iter()
        .filter_map(|t| {
            let spec = Spec::of(t, now)?;
            Some(ScheduleSeed {
                trigger_id: t.id().to_owned(),
                anchor_ms: now,
                next_due_ms: spec.next_after(now),
            })
        })
        .collect();
    match state
        .store
        .enable_automation(
            p.tenant_id,
            id,
            p.principal_id,
            version,
            hash,
            effects,
            &caps,
            &paths,
            &hosts,
            &seeds,
        )
        .await?
    {
        EnableOutcome::Enabled => {}
        EnableOutcome::NotFound => {
            return Err(ApiError::not_found(format!("automation {id}")));
        }
        EnableOutcome::VersionNotCurrent { current } => {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "APPROVAL_MISMATCH",
                format!("the current version is {current}"),
            ));
        }
        EnableOutcome::HashMismatch => {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "APPROVAL_MISMATCH",
                "the hash is not the version's",
            ));
        }
    }
    state
        .store
        .audit_provisioning(
            who,
            Some(p.tenant_id),
            "automation.enable",
            &a.automation_id,
            json!({"version": version, "hash": hash, "effects": effects, "capabilities": caps, "paths": paths, "hosts": hosts}),
        )
        .await?;
    let (a, cur) = load(state, p, id).await?;
    Ok(Json(view_of(&a, Some(&cur))))
}

// ---- webhook endpoints ----------------------------------------------------------

fn master_key(state: &AppState) -> ApiResult<&[u8]> {
    state.extras.webhook_master_key.as_deref().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "WEBHOOK_DISABLED",
            "this API has no webhook master key configured (MODBIT_CLOUD_WEBHOOK_MASTER_KEY_HEX)",
        )
    })
}

fn endpoint_json(
    e: &modbit_event_store::cloud::automation::EndpointRecord,
    secret: Option<&str>,
) -> Value {
    let mut v = json!({
        "endpoint_id": e.endpoint_id, "automation_id": e.automation_id,
        "trigger_id": e.trigger_id, "rotation": e.rotation,
        "path": format!("/v1/hooks/{}", e.endpoint_id), "revoked": e.revoked_at_ms.is_some(),
    });
    if let Some(s) = secret {
        // Shown once: only the master key and the counter recompute it.
        v["secret"] = json!(s);
    }
    v
}

/// `POST /v1/automations/{id}/endpoints {trigger_id}`: map a webhook
/// trigger to a new public endpoint; its secret is returned this once.
pub(crate) async fn create_endpoint(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let p = admin(&state, &ext).await?;
    let master = master_key(&state)?.to_vec();
    let id = aid(&id)?;
    let (_a, cur) = load(&state, &p, id).await?;
    let d = parse_and_validate(&cur.definition_json).map_err(|i| issues_error(&i))?;
    let trigger_id = body["trigger_id"].as_str().unwrap_or_default();
    if !matches!(d.trigger(trigger_id), Some(Trigger::Webhook { .. })) {
        return Err(ApiError::bad(
            "trigger_id must name a webhook trigger of the current version",
        ));
    }
    let e = state
        .store
        .create_automation_endpoint(p.tenant_id, id, trigger_id, p.principal_id)
        .await?;
    state
        .store
        .audit_provisioning(
            &format!("principal:{}", p.principal_id),
            Some(p.tenant_id),
            "automation.endpoint.create",
            &e.endpoint_id,
            json!({"automation_id": id.to_string(), "trigger_id": trigger_id}),
        )
        .await?;
    let secret = endpoint_secret(&master, &e.endpoint_id, e.rotation);
    Ok((StatusCode::CREATED, Json(endpoint_json(&e, Some(&secret)))))
}

/// `GET /v1/automations/{id}/endpoints`.
pub(crate) async fn list_endpoints(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let p = admin(&state, &ext).await?;
    let id = aid(&id)?;
    load(&state, &p, id).await?;
    let eps = state.store.automation_endpoints(p.tenant_id, id).await?;
    Ok(Json(
        json!({"endpoints": eps.iter().map(|e| endpoint_json(e, None)).collect::<Vec<_>>()}),
    ))
}

/// `POST /v1/automation-endpoints/{endpoint_id}:rotate|:revoke`.
pub(crate) async fn endpoint_action(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
    Path(seg): Path<String>,
) -> ApiResult<Json<Value>> {
    let p = admin(&state, &ext).await?;
    let (id, action) = seg.split_once(':').ok_or_else(|| {
        ApiError::bad("expected /v1/automation-endpoints/{endpoint_id}:rotate|:revoke")
    })?;
    let who = format!("principal:{}", p.principal_id);
    match action {
        "rotate" => {
            let master = master_key(&state)?.to_vec();
            let Some(e) = state
                .store
                .rotate_automation_endpoint(p.tenant_id, id)
                .await?
            else {
                return Err(ApiError::not_found(format!("endpoint {id}")));
            };
            state
                .store
                .audit_provisioning(
                    &who,
                    Some(p.tenant_id),
                    "automation.endpoint.rotate",
                    id,
                    json!({"rotation": e.rotation}),
                )
                .await?;
            let secret = endpoint_secret(&master, &e.endpoint_id, e.rotation);
            Ok(Json(endpoint_json(&e, Some(&secret))))
        }
        "revoke" => {
            if !state
                .store
                .revoke_automation_endpoint(p.tenant_id, id)
                .await?
            {
                return Err(ApiError::not_found(format!("endpoint {id}")));
            }
            state
                .store
                .audit_provisioning(
                    &who,
                    Some(p.tenant_id),
                    "automation.endpoint.revoke",
                    id,
                    json!({}),
                )
                .await?;
            Ok(Json(json!({"endpoint_id": id, "revoked": true})))
        }
        other => Err(ApiError::bad(format!("unknown endpoint action `{other}`"))),
    }
}

// ---- the signed intake -------------------------------------------------------------

fn header<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .trim()
}

fn refusal(status: StatusCode, code: &str, message: &str) -> Response {
    (status, Json(json!({"code": code, "message": message}))).into_response()
}

fn skipped(reason: &str) -> Response {
    (
        StatusCode::ACCEPTED,
        Json(json!({"code": "SKIPPED", "reason": reason})),
    )
        .into_response()
}

/// A delivery or event id the sender chose: bounded and plain.
fn plain_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

/// `POST /v1/hooks/{endpoint}` — a signed generic webhook (AUT-B04). No
/// bearer token: the signature under the endpoint's derived secret is the
/// authentication, and the endpoint decides the tenant.
pub(crate) async fn hook(
    State(state): State<Arc<AppState>>,
    Path(endpoint): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Response> {
    let Some(master) = state.extras.webhook_master_key.as_deref() else {
        return Ok(refusal(
            StatusCode::SERVICE_UNAVAILABLE,
            "WEBHOOK_DISABLED",
            "this API accepts no automation webhook",
        ));
    };
    if body.len() > MAX_HOOK_BODY {
        return Ok(refusal(
            StatusCode::PAYLOAD_TOO_LARGE,
            "PAYLOAD_TOO_LARGE",
            "the body is over 256 KiB",
        ));
    }
    let known = if endpoint.len() <= 64 {
        state.store.automation_endpoint(&endpoint).await?
    } else {
        None
    };
    // Bounded work for strangers: one bucket for unknown endpoints.
    let bucket = match &known {
        Some(e) => format!("hook:{}", e.endpoint_id),
        None => "hook:unknown".to_owned(),
    };
    if !state.limiter.admit(&bucket) {
        return Ok(refusal(
            StatusCode::TOO_MANY_REQUESTS,
            "RATE_LIMITED",
            "too many deliveries; retry shortly",
        ));
    }
    let now_ms = state.store.automation_now_ms().await?;
    let live = known.as_ref().filter(|e| e.revoked_at_ms.is_none());
    // An unknown or revoked endpoint is verified against a secret nobody
    // holds, so it answers exactly as a wrong signature does.
    let secret = match live {
        Some(e) => endpoint_secret(master, &e.endpoint_id, e.rotation),
        None => endpoint_secret(master, "unknown", 0),
    };
    let sig = header(&headers, webhook::SIGNATURE_HEADER);
    let verified = webhook::verify(
        secret.as_bytes(),
        sig,
        &body,
        now_ms / 1000,
        webhook::DEFAULT_WINDOW_SECONDS,
    );
    let verified = match verified {
        Ok(v) => v,
        Err(r) => {
            let code = match r {
                Refusal::Malformed | Refusal::BadNonce => "SIGNATURE_MALFORMED",
                other => other.code(),
            };
            let (audit_code, tenant, detail) = match live {
                Some(e) => (
                    code,
                    modbit_domain::TenantId::parse(&e.tenant_id).ok(),
                    "the delivery was refused",
                ),
                None => ("UNKNOWN_ENDPOINT", None, "no such endpoint, or revoked"),
            };
            state
                .store
                .audit_automation(
                    audit_code,
                    tenant,
                    Some(&endpoint.chars().take(64).collect::<String>()),
                    None,
                    "",
                    &format!("{detail}: {code}"),
                )
                .await?;
            let status = StatusCode::UNAUTHORIZED;
            return Ok(match code {
                "SIGNATURE_STALE" | "SIGNATURE_FUTURE" | "SIGNATURE_MALFORMED" => {
                    refusal(status, code, "the signature header is not acceptable")
                }
                _ => refusal(status, "SIGNATURE_INVALID", "the signature does not verify"),
            });
        }
    };
    let Some(ep) = live else {
        // Unreachable (the dummy secret cannot be signed under), kept closed.
        return Ok(refusal(
            StatusCode::UNAUTHORIZED,
            "SIGNATURE_INVALID",
            "the signature does not verify",
        ));
    };
    let tenant = modbit_domain::TenantId::parse(&ep.tenant_id)
        .map_err(|_| ApiError::bad("endpoint tenant"))?;
    let automation =
        uuid::Uuid::parse_str(&ep.automation_id).map_err(|_| ApiError::bad("endpoint"))?;
    // The endpoint decides the tenant. A header that names another one is a
    // refusal, never a redirect; the body is payload and is not consulted.
    let named = header(&headers, "x-modbit-tenant");
    if !named.is_empty() && named != ep.tenant_id {
        state
            .store
            .audit_automation(
                "TENANT_MISMATCH",
                Some(tenant),
                Some(&ep.endpoint_id),
                Some(automation),
                &verified.nonce,
                "the request names a tenant the endpoint does not map to",
            )
            .await?;
        return Ok(refusal(
            StatusCode::FORBIDDEN,
            "TENANT_MISMATCH",
            "the endpoint maps to another tenant than the request names",
        ));
    }
    // Replay: the nonce is the primary key.
    if !state
        .store
        .claim_webhook_nonce(
            &ep.endpoint_id,
            &verified.nonce,
            verified.timestamp_s,
            now_ms,
            webhook::DEFAULT_WINDOW_SECONDS,
        )
        .await?
    {
        state
            .store
            .audit_automation(
                "WEBHOOK_REPLAYED",
                Some(tenant),
                Some(&ep.endpoint_id),
                Some(automation),
                &verified.nonce,
                "this signed request was received before",
            )
            .await?;
        return Ok(refusal(
            StatusCode::CONFLICT,
            "WEBHOOK_REPLAYED",
            "this signed request was received before",
        ));
    }
    let delivery = {
        let d = header(&headers, "x-modbit-delivery");
        if d.is_empty() {
            verified.nonce.clone()
        } else if plain_id(d) {
            d.to_owned()
        } else {
            return Err(ApiError::bad("x-modbit-delivery is 1-128 plain characters"));
        }
    };
    // The definition must be enabled at a version whose hash is the one
    // approved (and still the one the document hashes to).
    let Some(a) = state.store.get_automation(tenant, automation).await? else {
        state
            .store
            .audit_automation(
                "NOT_ENABLED",
                Some(tenant),
                Some(&ep.endpoint_id),
                Some(automation),
                &delivery,
                "the definition no longer exists",
            )
            .await?;
        return Ok(refusal(
            StatusCode::CONFLICT,
            "NOT_ENABLED",
            "the automation is not enabled",
        ));
    };
    let enabled = match a.enabled.as_ref() {
        Some(e) => state
            .store
            .automation_version(tenant, automation, e.version)
            .await?
            .filter(|v| v.definition_hash == e.definition_hash)
            .and_then(|v| {
                parse_and_validate(&v.definition_json)
                    .ok()
                    .map(|d| (e.version, v, d))
            })
            .filter(|(_, v, d)| d.hash() == v.definition_hash),
        None => None,
    };
    let Some((version, _v, def)) = enabled else {
        state
            .store
            .audit_automation(
                "NOT_ENABLED",
                Some(tenant),
                Some(&ep.endpoint_id),
                Some(automation),
                &delivery,
                "no enable approval names the current definition",
            )
            .await?;
        return Ok(refusal(
            StatusCode::CONFLICT,
            "NOT_ENABLED",
            "the automation is not enabled",
        ));
    };
    let Some(trigger) = def
        .trigger(&ep.trigger_id)
        .filter(|t| matches!(t, Trigger::Webhook { .. }))
    else {
        state
            .store
            .audit_automation(
                "NOT_ENABLED",
                Some(tenant),
                Some(&ep.endpoint_id),
                Some(automation),
                &delivery,
                "the endpoint's trigger is not in the enabled version",
            )
            .await?;
        return Ok(refusal(
            StatusCode::CONFLICT,
            "NOT_ENABLED",
            "the automation is not enabled",
        ));
    };
    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        state
            .store
            .audit_automation(
                "BAD_PAYLOAD",
                Some(tenant),
                Some(&ep.endpoint_id),
                Some(automation),
                &delivery,
                "the body is not JSON",
            )
            .await?;
        return Err(ApiError::bad("the webhook body must be JSON"));
    };
    let Trigger::Webhook { name, .. } = trigger else {
        unreachable!("filtered above");
    };
    match trigger_matches(trigger, "webhook", name, &payload) {
        Ok(true) => {}
        Ok(false) | Err(_) => {
            state
                .store
                .audit_automation(
                    "FILTER",
                    Some(tenant),
                    Some(&ep.endpoint_id),
                    Some(automation),
                    &delivery,
                    "the trigger's filters did not match this body",
                )
                .await?;
            return Ok(skipped("FILTER"));
        }
    }
    let key = modbit_automation::dispatch_key(&ep.automation_id, version, &delivery);
    let text = String::from_utf8_lossy(&body).into_owned();
    fire(
        &state,
        FiringInput {
            tenant,
            automation_id: automation,
            version,
            dispatch_key: key,
            trigger_id: ep.trigger_id.clone(),
            trigger_kind: "webhook".into(),
            event_id: delivery,
            source: "webhook".into(),
            payload: text,
            payload_label: "webhook".into(),
            slot_ms: None,
            catch_up: false,
            missed: 0,
            now_ms,
            force_skip: None,
        },
    )
    .await
}

/// Record a firing and, when it is to run, create its task; the response a
/// signed sender sees.
async fn fire(state: &AppState, input: FiringInput) -> ApiResult<Response> {
    let key = input.dispatch_key.clone();
    match state.store.record_firing(&input).await? {
        Recorded::Pending => {}
        Recorded::Queued => {
            return Ok((
                StatusCode::ACCEPTED,
                Json(json!({"code": "QUEUED", "dispatch_key": key})),
            )
                .into_response());
        }
        Recorded::Skipped(reason) => return Ok(skipped(&reason)),
        Recorded::Duplicate => {
            return Ok((
                StatusCode::OK,
                Json(json!({"code": "DUPLICATE", "dispatch_key": key, "message": "this event fired this version before; nothing was created"})),
            )
                .into_response());
        }
        Recorded::NotEnabled => {
            return Ok(refusal(
                StatusCode::CONFLICT,
                "NOT_ENABLED",
                "the automation is not enabled",
            ));
        }
    }
    match state.store.dispatch_firing(&key).await? {
        Dispatched::Started { task_id, session_id } => Ok((
            StatusCode::CREATED,
            Json(json!({"code": "FIRED", "dispatch_key": key, "task_id": task_id.to_string(), "session_id": session_id.to_string()})),
        )
            .into_response()),
        Dispatched::Refused(reason) => Ok(skipped(&reason)),
        Dispatched::NotPending => Ok(skipped("NOT_PENDING")),
    }
}

// ---- forge events ----------------------------------------------------------------------

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn label_names(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x["name"].as_str().map(str::to_owned))
                .take(50)
                .collect()
        })
        .unwrap_or_default()
}

/// A forge delivery as the normalised event the definition's filters read:
/// the trigger table of AUT-B01 (`event`, the payload label, the payload).
/// `None`: not an event any trigger takes.
#[must_use]
pub fn normalize_forge(
    event: &str,
    action: &str,
    p: &Value,
) -> Option<(&'static str, &'static str, Value)> {
    let pr = &p["pull_request"];
    let issue = &p["issue"];
    let login = |v: &Value| v["login"].as_str().unwrap_or_default().to_owned();
    match (event, action) {
        ("pull_request", "opened" | "synchronize" | "reopened") => Some((
            "pull_request",
            "forge_pr",
            json!({
                "action": if action == "opened" { "opened" } else { "updated" },
                "branch": pr["head"]["ref"], "base": pr["base"]["ref"],
                "title": clip(pr["title"].as_str().unwrap_or_default(), 500),
                "text": clip(pr["body"].as_str().unwrap_or_default(), 8192),
                "author": login(&pr["user"]), "labels": label_names(&pr["labels"]),
                "number": pr["number"], "url": pr["html_url"], "sha": pr["head"]["sha"],
            }),
        )),
        ("pull_request", "labeled") => Some((
            "label_added",
            "forge_pr",
            json!({
                "action": "labeled", "label": p["label"]["name"],
                "branch": pr["head"]["ref"], "title": clip(pr["title"].as_str().unwrap_or_default(), 500),
                "text": clip(pr["body"].as_str().unwrap_or_default(), 8192),
                "author": login(&pr["user"]), "labels": label_names(&pr["labels"]),
                "number": pr["number"], "url": pr["html_url"],
            }),
        )),
        ("issues", "labeled") => Some((
            "label_added",
            "forge_comment",
            json!({
                "action": "labeled", "label": p["label"]["name"],
                "title": clip(issue["title"].as_str().unwrap_or_default(), 500),
                "text": clip(issue["body"].as_str().unwrap_or_default(), 8192),
                "author": login(&issue["user"]), "labels": label_names(&issue["labels"]),
                "number": issue["number"], "url": issue["html_url"],
            }),
        )),
        ("push", _) => {
            let reference = p["ref"].as_str().unwrap_or_default();
            let branch = reference.strip_prefix("refs/heads/").unwrap_or(reference);
            let mut paths: Vec<String> = Vec::new();
            for c in p["commits"].as_array().into_iter().flatten().take(100) {
                for k in ["added", "modified", "removed"] {
                    for f in c[k].as_array().into_iter().flatten() {
                        if let Some(f) = f.as_str()
                            && paths.len() < 300
                            && !paths.iter().any(|x| x == f)
                        {
                            paths.push(clip(f, 300));
                        }
                    }
                }
            }
            let message = p["head_commit"]["message"].as_str().unwrap_or_default();
            Some((
                "push",
                "forge_pr",
                json!({
                    "action": "pushed", "branch": branch,
                    "title": clip(message.lines().next().unwrap_or_default(), 500),
                    "text": clip(message, 8192),
                    "author": p["sender"]["login"], "labels": [], "paths": paths,
                    "sha": p["after"], "before": p["before"],
                }),
            ))
        }
        ("check_suite" | "check_run", "completed") => {
            let c = if event == "check_suite" {
                &p["check_suite"]
            } else {
                &p["check_run"]
            };
            let branch = if event == "check_suite" {
                &c["head_branch"]
            } else {
                &c["check_suite"]["head_branch"]
            };
            Some((
                "check_completed",
                "forge_pr",
                json!({
                    "action": "completed", "branch": branch,
                    "title": clip(c["name"].as_str().unwrap_or("check suite"), 500),
                    "text": clip(c["conclusion"].as_str().unwrap_or_default(), 200),
                    "conclusion": c["conclusion"], "author": p["sender"]["login"],
                    "labels": [], "sha": c["head_sha"],
                }),
            ))
        }
        ("issue_comment", "created") => Some((
            "issue_comment",
            "forge_comment",
            json!({
                "action": "created",
                "title": clip(issue["title"].as_str().unwrap_or_default(), 500),
                "text": clip(p["comment"]["body"].as_str().unwrap_or_default(), 8192),
                "author": login(&p["comment"]["user"]), "labels": label_names(&issue["labels"]),
                "number": issue["number"], "url": p["comment"]["html_url"],
                "on_pull_request": issue["pull_request"].is_object(),
            }),
        )),
        ("pull_request_review_comment", "created") => Some((
            "review_comment",
            "forge_comment",
            json!({
                "action": "created", "branch": pr["head"]["ref"],
                "title": clip(pr["title"].as_str().unwrap_or_default(), 500),
                "text": clip(p["comment"]["body"].as_str().unwrap_or_default(), 8192),
                "author": login(&p["comment"]["user"]), "labels": label_names(&pr["labels"]),
                "paths": [p["comment"]["path"]], "number": pr["number"], "url": p["comment"]["html_url"],
            }),
        )),
        ("pull_request_review", "submitted") => Some((
            "review_submitted",
            "forge_comment",
            json!({
                "action": "submitted", "branch": pr["head"]["ref"],
                "title": clip(pr["title"].as_str().unwrap_or_default(), 500),
                "text": clip(p["review"]["body"].as_str().unwrap_or_default(), 8192),
                "state": p["review"]["state"],
                "author": login(&p["review"]["user"]), "labels": label_names(&pr["labels"]),
                "number": pr["number"], "url": p["review"]["html_url"],
            }),
        )),
        _ => None,
    }
}

/// A verified, mapped forge delivery matched against the tenant's enabled
/// event triggers (the tenant is the repository mapping's, never the
/// body's). Each match fires through the idempotent dispatch, keyed by the
/// delivery id; a failure here never changes what the forge intake answers.
pub(crate) async fn forge_event(
    state: &AppState,
    tenant: modbit_domain::TenantId,
    repository: &str,
    delivery: &str,
    event: &str,
    action: &str,
    payload: &Value,
) {
    let Some((name, label, normalized)) = normalize_forge(event, action, payload) else {
        return;
    };
    if let Err(e) = forge_event_inner(
        state,
        tenant,
        repository,
        delivery,
        name,
        label,
        &normalized,
    )
    .await
    {
        eprintln!("modbit-cloud-api: automation forge event {delivery}: {e}");
    }
}

async fn forge_event_inner(
    state: &AppState,
    tenant: modbit_domain::TenantId,
    repository: &str,
    delivery: &str,
    name: &str,
    label: &str,
    normalized: &Value,
) -> anyhow::Result<()> {
    let now_ms = state.store.automation_now_ms().await?;
    let text = normalized.to_string();
    for a in state.store.list_automations(tenant).await? {
        let Some(e) = a.enabled.as_ref() else {
            continue;
        };
        if !a.repository.is_empty() && a.repository != repository {
            continue;
        }
        let id = uuid::Uuid::parse_str(&a.automation_id)?;
        let Some(v) = state
            .store
            .automation_version(tenant, id, e.version)
            .await?
        else {
            continue;
        };
        if v.definition_hash != e.definition_hash {
            continue;
        }
        let Ok(def) = parse_and_validate(&v.definition_json) else {
            continue;
        };
        if def.hash() != v.definition_hash {
            continue;
        }
        for t in &def.triggers {
            let Trigger::Event { source, event, .. } = t else {
                continue;
            };
            if source != "forge" || event != name {
                continue;
            }
            let matched = trigger_matches(t, "forge", name, normalized).unwrap_or(false);
            if !matched {
                state
                    .store
                    .audit_automation(
                        "FILTER",
                        Some(tenant),
                        None,
                        Some(id),
                        delivery,
                        &format!("{name}: the trigger's filters did not match"),
                    )
                    .await?;
                continue;
            }
            let key = modbit_automation::dispatch_key(
                &a.automation_id,
                e.version,
                &format!("{delivery}:{}", t.id()),
            );
            let input = FiringInput {
                tenant,
                automation_id: id,
                version: e.version,
                dispatch_key: key.clone(),
                trigger_id: t.id().to_owned(),
                trigger_kind: "event".into(),
                event_id: format!("{delivery}:{}", t.id()),
                source: "forge".into(),
                payload: text.clone(),
                payload_label: label.to_owned(),
                slot_ms: None,
                catch_up: false,
                missed: 0,
                now_ms,
                force_skip: None,
            };
            if state.store.record_firing(&input).await? == Recorded::Pending {
                state.store.dispatch_firing(&key).await?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_endpoint_secret_derives_from_the_master_key_and_the_rotation_only() {
        let a = endpoint_secret(b"master-key-0123456789", "whk_a", 0);
        assert_eq!(a, endpoint_secret(b"master-key-0123456789", "whk_a", 0));
        assert_ne!(a, endpoint_secret(b"master-key-0123456789", "whk_b", 0));
        assert_ne!(a, endpoint_secret(b"master-key-0123456789", "whk_a", 1));
        assert_ne!(a, endpoint_secret(b"another-master-key-9", "whk_a", 0));
        assert!(a.starts_with("whsec_") && a.len() == 6 + 64);
    }

    #[test]
    fn a_read_only_definition_runs_under_plan_and_anything_else_under_cloud_isolated() {
        assert_eq!(profile_for(Effects::ReadOnly), "plan");
        for e in [
            Effects::ReversibleWrite,
            Effects::ProtectedWrite,
            Effects::ExternalSideEffect,
        ] {
            assert_eq!(profile_for(e), "cloud_isolated");
        }
        assert!(CLOUD_PROFILES.contains(&profile_for(Effects::ReadOnly)));
    }

    #[test]
    fn forge_events_normalise_to_what_the_filters_read() {
        let pr = json!({
            "action": "opened",
            "pull_request": {"number": 7, "title": "Fix it", "body": "details", "html_url": "u",
                "head": {"ref": "feature/x", "sha": "abc"}, "base": {"ref": "main"},
                "user": {"login": "octo"}, "labels": [{"name": "bug"}]},
        });
        let (name, label, n) = normalize_forge("pull_request", "opened", &pr).unwrap();
        assert_eq!((name, label), ("pull_request", "forge_pr"));
        assert_eq!(n["branch"], "feature/x");
        assert_eq!(n["labels"], json!(["bug"]));
        assert_eq!(n["action"], "opened");
        let (_, _, n) = normalize_forge("pull_request", "synchronize", &pr).unwrap();
        assert_eq!(n["action"], "updated");
        let push = json!({"ref": "refs/heads/main", "after": "d", "before": "c", "sender": {"login": "octo"},
            "head_commit": {"message": "title\n\nbody"},
            "commits": [{"added": ["a.rs"], "modified": ["b.rs", "a.rs"], "removed": []}]});
        let (name, _, n) = normalize_forge("push", "", &push).unwrap();
        assert_eq!(name, "push");
        assert_eq!(n["branch"], "main");
        assert_eq!(n["paths"], json!(["a.rs", "b.rs"]));
        assert_eq!(n["title"], "title");
        assert!(normalize_forge("pull_request", "closed", &pr).is_none());
        assert!(normalize_forge("star", "created", &pr).is_none());
        for (event, action, want) in [
            ("check_suite", "completed", "check_completed"),
            ("check_run", "completed", "check_completed"),
            ("issue_comment", "created", "issue_comment"),
            ("pull_request_review_comment", "created", "review_comment"),
            ("pull_request_review", "submitted", "review_submitted"),
            ("pull_request", "labeled", "label_added"),
            ("issues", "labeled", "label_added"),
        ] {
            assert_eq!(normalize_forge(event, action, &json!({})).unwrap().0, want);
        }
    }
}
