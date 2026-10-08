//! Tenant and principal provisioning, and the signed policy bundle's
//! publisher route (PX-129; docs/24 "Identity", "Execution policy
//! distribution").
//!
//! * The platform administrator (a secret only the operator of the API
//!   holds, `MODBIT_CLOUD_ADMIN_SECRET`) creates tenants and registers each
//!   tenant's organisation signing keys — the trust roots a policy bundle
//!   verifies under. Nothing a tenant's own administrator can reach changes
//!   a trust root.
//! * A tenant's administrator (a principal whose role is `admin`) creates,
//!   disables and re-roles principals of its own tenant, and publishes the
//!   tenant's policy bundle.
//! * Every provisioning action is on the provisioning audit with who did it;
//!   a principal's secret is returned once and is in no record.
//! * A bundle is accepted only when the organisation's key signed it, it is
//!   written for the publisher's tenant, it is fresh and compatible, and its
//!   generation is greater than every one published (`modbit_domain::
//!   policy_bundle::verify`); it is then served, as published, to the
//!   tenant's principals. The workers and clients verify it again.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use modbit_domain::TenantId;
use modbit_domain::policy_bundle::{self, BundleRefusal, Expect, SignedBundle};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::AppState;
use crate::routes::{ApiError, ApiResult, Caller, caller, parse_id};

/// Whether the bearer is the platform administrator's secret.
pub(crate) fn admin_actor(state: &AppState, headers: &HeaderMap) -> ApiResult<&'static str> {
    let Some(want) = state.extras.admin_secret_hash.as_ref() else {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "ADMIN_DISABLED",
            "this API has no platform administrator configured",
        ));
    };
    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .unwrap_or_default();
    let got = Sha256::digest(presented.as_bytes());
    // Both are 32-byte digests: compared without an early exit.
    let diff = got
        .iter()
        .zip(want.iter())
        .fold(0u8, |d, (a, b)| d | (a ^ b));
    if diff != 0 {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "UNAUTHENTICATED",
            "the platform administrator's secret is required",
        ));
    }
    Ok("platform-admin")
}

fn tenant_of(text: &str) -> ApiResult<TenantId> {
    parse_id(text, |s| TenantId::parse(s).ok(), "tenant_id")
}

fn valid_role(role: &str) -> bool {
    matches!(role, "member" | "admin")
}

fn valid_kind(kind: &str) -> bool {
    matches!(kind, "user" | "service")
}

/// `POST /v1/admin/tenants {name}`.
pub(crate) async fn create_tenant(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let actor = admin_actor(&state, &headers)?;
    let name = body["name"].as_str().unwrap_or_default().trim();
    if name.is_empty() || name.len() > 128 {
        return Err(ApiError::bad("name required (at most 128 characters)"));
    }
    let tenant = state.store.create_tenant(name).await?;
    state
        .store
        .audit_provisioning(
            actor,
            Some(tenant),
            "tenant.create",
            &tenant.to_string(),
            json!({"name": name}),
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"tenant_id": tenant.to_string(), "name": name})),
    ))
}

/// Create a principal in `tenant` on behalf of `actor`.
async fn create_principal_in(
    state: &AppState,
    actor: &str,
    tenant: TenantId,
    body: &Value,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let kind = body["kind"].as_str().unwrap_or("user");
    let role = body["role"].as_str().unwrap_or("member");
    let label = body["label"].as_str().unwrap_or_default().trim();
    if !valid_kind(kind) || !valid_role(role) {
        return Err(ApiError::bad(
            "kind is user or service; role is member or admin",
        ));
    }
    if label.is_empty() || label.len() > 128 {
        return Err(ApiError::bad("label required (at most 128 characters)"));
    }
    let identity = match (
        body["oidc"]["issuer"].as_str(),
        body["oidc"]["subject"].as_str(),
    ) {
        (Some(i), Some(s)) if !i.is_empty() && !s.is_empty() && s.len() <= 255 => Some((i, s)),
        (None, None) => None,
        _ => return Err(ApiError::bad("oidc needs both issuer and subject")),
    };
    if let Some((issuer, subject)) = identity
        && state
            .store
            .principal_for_identity(issuer, subject)
            .await?
            .is_some()
    {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "IDENTITY_TAKEN",
            "that identity is provisioned for a principal already",
        ));
    }
    let (p, secret) = state
        .store
        .create_principal_with(tenant, kind, label, role, identity)
        .await?;
    state
        .store
        .audit_provisioning(
            actor,
            Some(tenant),
            "principal.create",
            &p.principal_id.to_string(),
            json!({"kind": kind, "role": role, "label": label, "oidc": identity.map(|(i, s)| json!({"issuer": i, "subject": s}))}),
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "principal_id": p.principal_id.to_string(), "tenant_id": tenant.to_string(),
            "kind": kind, "role": role, "label": label,
            // Shown once; stored hashed.
            "secret": secret,
        })),
    ))
}

/// `POST /v1/admin/tenants/{tenant_id}/principals {kind?, label, role?, oidc?}`.
pub(crate) async fn admin_create_principal(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let actor = admin_actor(&state, &headers)?;
    let tenant = tenant_of(&tenant_id)?;
    create_principal_in(&state, actor, tenant, &body).await
}

/// `PUT /v1/admin/tenants/{tenant_id}/org-keys/{key_id} {public_key_hex}` (or
/// `{revoke: true}`): the tenant's organisation signing keys.
pub(crate) async fn admin_org_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((tenant_id, key_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let actor = admin_actor(&state, &headers)?;
    let tenant = tenant_of(&tenant_id)?;
    if key_id.is_empty()
        || key_id.len() > 64
        || !key_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(ApiError::bad(
            "key_id is 1 to 64 of letters, digits, `-`, `_`, `.`",
        ));
    }
    if body["revoke"] == true {
        let revoked = state.store.revoke_org_key(tenant, &key_id).await?;
        state
            .store
            .audit_provisioning(
                actor,
                Some(tenant),
                "org_key.revoke",
                &key_id,
                json!({"revoked": revoked}),
            )
            .await?;
        return Ok(Json(json!({"key_id": key_id, "revoked": revoked})));
    }
    let hex_key = body["public_key_hex"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if hex::decode(&hex_key).ok().is_none_or(|b| b.len() != 32) {
        return Err(ApiError::bad(
            "public_key_hex must be a 32-byte Ed25519 public key",
        ));
    }
    state.store.put_org_key(tenant, &key_id, &hex_key).await?;
    state
        .store
        .audit_provisioning(
            actor,
            Some(tenant),
            "org_key.put",
            &key_id,
            json!({"public_key_sha256": hex::encode(Sha256::digest(hex_key.as_bytes()))}),
        )
        .await?;
    Ok(Json(json!({"key_id": key_id, "registered": true})))
}

/// `GET /v1/admin/audit?` the whole provisioning audit.
pub(crate) async fn admin_audit(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    admin_actor(&state, &headers)?;
    Ok(Json(
        json!({"audit": state.store.provisioning_audit(None).await?}),
    ))
}

/// A tenant administrator's caller, or a refusal.
pub(crate) async fn require_tenant_admin(
    state: &AppState,
    ext: &axum::Extension<Caller>,
) -> ApiResult<modbit_event_store::cloud::Principal> {
    let p = caller(ext);
    let rec = state
        .store
        .principal_record(p.tenant_id, p.principal_id)
        .await?;
    if rec.is_some_and(|r| r.role == "admin" && !r.disabled) {
        Ok(p)
    } else {
        state
            .store
            .record_denial(
                Some(p.tenant_id),
                Some(p.principal_id),
                "provisioning",
                "not a tenant administrator",
            )
            .await?;
        Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "ADMIN_REQUIRED",
            "this needs a principal whose role is admin",
        ))
    }
}

/// `POST /v1/principals {kind?, label, role?, oidc?}` — a tenant administrator provisions its own tenant.
pub(crate) async fn create_principal(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
    Json(body): Json<Value>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let p = require_tenant_admin(&state, &ext).await?;
    create_principal_in(
        &state,
        &format!("principal:{}", p.principal_id),
        p.tenant_id,
        &body,
    )
    .await
}

/// `GET /v1/principals` — the tenant's principals.
pub(crate) async fn list_principals(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
) -> ApiResult<Json<Value>> {
    let p = require_tenant_admin(&state, &ext).await?;
    Ok(Json(
        json!({"principals": state.store.principal_records(p.tenant_id).await?}),
    ))
}

/// `POST /v1/principals/{principal_id}:disable|:enable|:role {role}`.
pub(crate) async fn principal_action(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
    Path(seg): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let admin = require_tenant_admin(&state, &ext).await?;
    let (id, action) = seg.split_once(':').ok_or_else(|| {
        ApiError::bad("expected /v1/principals/{principal_id}:disable|:enable|:role")
    })?;
    let pid = parse_id(id, |s| uuid::Uuid::parse_str(s).ok(), "principal_id")?;
    let actor = format!("principal:{}", admin.principal_id);
    let done = match action {
        "disable" | "enable" => {
            if action == "disable" && pid == admin.principal_id {
                return Err(ApiError::new(
                    StatusCode::CONFLICT,
                    "LAST_ADMIN_GUARD",
                    "an administrator does not disable itself",
                ));
            }
            state
                .store
                .set_principal_disabled(admin.tenant_id, pid, action == "disable")
                .await?
        }
        "role" => {
            let role = body["role"].as_str().unwrap_or_default();
            if !valid_role(role) {
                return Err(ApiError::bad("role is member or admin"));
            }
            if pid == admin.principal_id && role != "admin" {
                return Err(ApiError::new(
                    StatusCode::CONFLICT,
                    "LAST_ADMIN_GUARD",
                    "an administrator does not demote itself",
                ));
            }
            state
                .store
                .set_principal_role(admin.tenant_id, pid, role)
                .await?
        }
        other => return Err(ApiError::bad(format!("unknown action `{other}`"))),
    };
    if !done {
        // Another tenant's principal is not found, and the attempt audited.
        state
            .store
            .record_denial(
                Some(admin.tenant_id),
                Some(admin.principal_id),
                &format!("principal:{pid}"),
                "not in tenant",
            )
            .await?;
        return Err(ApiError::not_found(format!("principal {pid}")));
    }
    state
        .store
        .audit_provisioning(
            &actor,
            Some(admin.tenant_id),
            &format!("principal.{action}"),
            &pid.to_string(),
            json!({"role": body["role"]}),
        )
        .await?;
    Ok(Json(
        json!({"principal_id": pid.to_string(), "action": action, "done": true}),
    ))
}

fn refusal_status(r: &BundleRefusal) -> StatusCode {
    match r {
        BundleRefusal::StaleGeneration { .. } => StatusCode::CONFLICT,
        BundleRefusal::UnknownKey(_) | BundleRefusal::BadSignature | BundleRefusal::WrongTenant => {
            StatusCode::FORBIDDEN
        }
        _ => StatusCode::BAD_REQUEST,
    }
}

/// `PUT /v1/policy/bundle {key_id, document, signature}` — a tenant
/// administrator publishes a bundle the organisation's key signed.
pub(crate) async fn publish_bundle(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
    Json(body): Json<Value>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let p = require_tenant_admin(&state, &ext).await?;
    let signed: SignedBundle = serde_json::from_value(body.clone()).map_err(|e| {
        ApiError::bad(format!(
            "a signed bundle is {{key_id, document, signature}}: {e}"
        ))
    })?;
    let keys = state.store.org_keys(p.tenant_id).await?;
    let current = state.store.policy_generation(p.tenant_id).await?;
    let verdict = policy_bundle::verify(
        &signed,
        &Expect {
            keys: &keys,
            tenant_id: &p.tenant_id.to_string(),
            now_ms: modbit_domain::Timestamp::now().millis(),
            min_generation: current + 1,
            protocol_major: modbit_protocol::PROTOCOL_VERSION.major,
        },
    );
    let doc = match verdict {
        Ok(d) => d,
        Err(r) => {
            state
                .store
                .record_denial(
                    Some(p.tenant_id),
                    Some(p.principal_id),
                    "policy:bundle",
                    &format!("refused {}", r.code()),
                )
                .await?;
            return Err(ApiError::new(refusal_status(&r), r.code(), r.to_string()));
        }
    };
    let stored = state
        .store
        .publish_policy_bundle(
            p.tenant_id,
            doc.generation,
            &signed.key_id,
            &body,
            p.principal_id,
        )
        .await?;
    if !stored {
        // Another publisher got there first.
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "BUNDLE_STALE_GENERATION",
            "a bundle of that generation or newer was published meanwhile",
        ));
    }
    state
        .store
        .audit_provisioning(
            &format!("principal:{}", p.principal_id),
            Some(p.tenant_id),
            "policy.publish",
            &doc.generation.to_string(),
            json!({"key_id": signed.key_id, "expires_at_ms": doc.expires_at_ms, "document_sha256": hex::encode(Sha256::digest(signed.document.as_bytes()))}),
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"generation": doc.generation, "expires_at_ms": doc.expires_at_ms})),
    ))
}

/// `GET /v1/policy/bundle` — the tenant's current signed bundle, as published.
pub(crate) async fn current_bundle(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
) -> ApiResult<Json<Value>> {
    let p = caller(&ext);
    let Some((generation, signed)) = state.store.current_policy_bundle(p.tenant_id).await? else {
        return Err(ApiError::not_found("a policy bundle for this tenant"));
    };
    Ok(Json(json!({"generation": generation, "bundle": signed})))
}
