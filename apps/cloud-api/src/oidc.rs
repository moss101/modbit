//! OIDC authorization code with PKCE, the Cloud API as the relying party
//! (PX-129; docs/24 "Identity"). A client (the CLI, the desktop) starts a
//! login with the challenge of a verifier only it holds; the API answers with
//! the provider's authorization URL carrying a state and a nonce it keeps
//! (only the state's hash); the person signs in with the provider in a
//! browser, which redirects back to the client with a code and the state; the
//! client hands the code, the state and the verifier to the API, which
//! spends the login exactly once, checks the verifier against the challenge,
//! exchanges the code at the provider's token endpoint with the verifier,
//! verifies the ID token (signature under the provider's published keys,
//! issuer, audience, expiry and issue time within a clock-skew bound, the
//! nonce) and signs in the principal that identity was provisioned for — a
//! provisioned, enabled principal or nobody. The session it issues is the
//! same short-lived, tenant-scoped token pair a principal's secret buys.
//!
//! The provider's keys are fetched from its discovery document and cached;
//! a key id not in the cache refetches them (a rotation), at most once per
//! few seconds. Only Ed25519 (`EdDSA`) ID tokens are verified: RS256, the
//! algorithm every large provider signs with, needs an RSA implementation
//! the dependency policy has not admitted, and a token signed with anything
//! else is refused `OIDC_ALG_UNSUPPORTED`.
//!
//! No code, verifier, state, nonce or token is ever logged or put on the
//! denial ledger; a refusal names its reason only.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::AppState;
use crate::routes::{ApiError, ApiResult, token_response};

/// The relying party's settings.
#[derive(Clone)]
pub struct OidcConfig {
    /// The provider's issuer URL (its discovery document lives under it).
    pub issuer: String,
    /// This relying party's client id at the provider.
    pub client_id: String,
    /// The client secret, when the provider issued one (a public client has none).
    pub client_secret: Option<String>,
    /// Redirect URIs a login may name (exact match).
    pub redirect_uris: Vec<String>,
    /// Also accept an `http` loopback redirect (`127.0.0.1`, `[::1]`,
    /// `localhost`, any port, path `/callback`) — the native-app rule.
    pub allow_loopback_redirect: bool,
    /// How long a started login may be completed.
    pub login_ttl_ms: i64,
    /// How far an ID token's times may be off this clock.
    pub clock_skew_ms: i64,
    /// How long the provider's metadata and keys are cached.
    pub cache_ttl_ms: i64,
    /// The least time between two refetches of the provider's keys for a key
    /// id the cache has not seen (a flood of unknown ids refetches once).
    pub key_refresh_min_ms: i64,
}

impl std::fmt::Debug for OidcConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OidcConfig")
            .field("issuer", &self.issuer)
            .field("client_id", &self.client_id)
            .field("client_secret", &self.client_secret.as_ref().map(|_| "…"))
            .field("redirect_uris", &self.redirect_uris)
            .finish_non_exhaustive()
    }
}

impl OidcConfig {
    /// A relying party for `issuer`/`client_id` with the defaults (a ten
    /// minute login, two minutes of skew, a five minute key cache).
    #[must_use]
    pub fn new(issuer: &str, client_id: &str) -> Self {
        Self {
            issuer: issuer.trim_end_matches('/').to_owned(),
            client_id: client_id.to_owned(),
            client_secret: None,
            redirect_uris: vec![],
            allow_loopback_redirect: true,
            login_ttl_ms: 10 * 60 * 1000,
            clock_skew_ms: 2 * 60 * 1000,
            cache_ttl_ms: 5 * 60 * 1000,
            key_refresh_min_ms: 5_000,
        }
    }

    /// From the environment (`MODBIT_CLOUD_OIDC_ISSUER`, `_CLIENT_ID`,
    /// `_CLIENT_SECRET`, `_REDIRECT_URIS` comma separated).
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let issuer = std::env::var("MODBIT_CLOUD_OIDC_ISSUER")
            .ok()
            .filter(|s| !s.is_empty())?;
        let client_id = std::env::var("MODBIT_CLOUD_OIDC_CLIENT_ID")
            .ok()
            .filter(|s| !s.is_empty())?;
        let mut c = Self::new(&issuer, &client_id);
        c.client_secret = std::env::var("MODBIT_CLOUD_OIDC_CLIENT_SECRET")
            .ok()
            .filter(|s| !s.is_empty());
        c.redirect_uris = std::env::var("MODBIT_CLOUD_OIDC_REDIRECT_URIS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect();
        Some(c)
    }

    fn redirect_allowed(&self, uri: &str) -> bool {
        if self.redirect_uris.iter().any(|u| u == uri) {
            return true;
        }
        if !self.allow_loopback_redirect {
            return false;
        }
        let Ok(u) = reqwest::Url::parse(uri) else {
            return false;
        };
        u.scheme() == "http"
            && matches!(u.host_str(), Some("127.0.0.1" | "[::1]" | "localhost"))
            && u.path() == "/callback"
            && u.query().is_none()
            && u.fragment().is_none()
    }
}

/// The provider's discovery metadata, cached.
#[derive(Clone, Debug)]
struct Meta {
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
}

/// An Ed25519 key the provider published.
#[derive(Clone, Debug)]
struct Jwk {
    kid: String,
    key: [u8; 32],
}

/// What the API remembers of the provider between logins.
#[derive(Default)]
pub struct OidcCache {
    meta: Option<(Meta, i64)>,
    keys: Option<(Vec<Jwk>, i64)>,
    last_key_refresh_ms: i64,
}

fn now_ms() -> i64 {
    modbit_domain::Timestamp::now().millis()
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn unb64(text: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(text.trim_end_matches('='))
        .ok()
}

/// Bytes compared without an early exit.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

fn refused(status: StatusCode, code: &'static str, message: impl Into<String>) -> ApiError {
    ApiError::new(status, code, message)
}

fn unavailable(what: impl std::fmt::Display) -> ApiError {
    refused(
        StatusCode::BAD_GATEWAY,
        "OIDC_PROVIDER_UNAVAILABLE",
        format!("the identity provider could not be reached: {what}"),
    )
}

fn http() -> ApiResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(unavailable)
}

/// An endpoint the provider named must be `https` (an `http` loopback issuer
/// is a development provider).
fn endpoint_ok(cfg: &OidcConfig, url: &str) -> bool {
    let Ok(u) = reqwest::Url::parse(url) else {
        return false;
    };
    match u.scheme() {
        "https" => true,
        "http" => reqwest::Url::parse(&cfg.issuer).is_ok_and(|i| {
            i.scheme() == "http"
                && matches!(i.host_str(), Some("127.0.0.1" | "[::1]" | "localhost"))
        }),
        _ => false,
    }
}

async fn meta(state: &AppState, cfg: &OidcConfig) -> ApiResult<Meta> {
    let mut cache = state.oidc_cache.lock().await;
    if let Some((m, at)) = &cache.meta
        && now_ms() - at < cfg.cache_ttl_ms
    {
        return Ok(m.clone());
    }
    let url = format!("{}/.well-known/openid-configuration", cfg.issuer);
    let doc: Value = http()?
        .get(&url)
        .send()
        .await
        .map_err(unavailable)?
        .error_for_status()
        .map_err(unavailable)?
        .json()
        .await
        .map_err(unavailable)?;
    if doc["issuer"].as_str().map(|s| s.trim_end_matches('/')) != Some(cfg.issuer.as_str()) {
        return Err(refused(
            StatusCode::BAD_GATEWAY,
            "OIDC_ISSUER_MISMATCH",
            "the provider's metadata names another issuer",
        ));
    }
    let take = |k: &str| -> ApiResult<String> {
        doc[k]
            .as_str()
            .filter(|u| endpoint_ok(cfg, u))
            .map(str::to_owned)
            .ok_or_else(|| {
                refused(
                    StatusCode::BAD_GATEWAY,
                    "OIDC_METADATA_INVALID",
                    format!("the provider's metadata has no usable `{k}`"),
                )
            })
    };
    let m = Meta {
        authorization_endpoint: take("authorization_endpoint")?,
        token_endpoint: take("token_endpoint")?,
        jwks_uri: take("jwks_uri")?,
    };
    cache.meta = Some((m.clone(), now_ms()));
    Ok(m)
}

async fn fetch_keys(cfg: &OidcConfig, m: &Meta) -> ApiResult<Vec<Jwk>> {
    let _ = cfg;
    let doc: Value = http()?
        .get(&m.jwks_uri)
        .send()
        .await
        .map_err(unavailable)?
        .error_for_status()
        .map_err(unavailable)?
        .json()
        .await
        .map_err(unavailable)?;
    Ok(doc["keys"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|k| k["kty"] == "OKP" && k["crv"] == "Ed25519")
                .filter_map(|k| {
                    let x = unb64(k["x"].as_str()?)?;
                    Some(Jwk {
                        kid: k["kid"].as_str().unwrap_or_default().to_owned(),
                        key: <[u8; 32]>::try_from(x.as_slice()).ok()?,
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

/// The key that signed a token: from the cache, or — for a key id the cache
/// has not seen — after one refetch (a rotation), at most every few seconds.
async fn key_for(state: &AppState, cfg: &OidcConfig, m: &Meta, kid: &str) -> ApiResult<Jwk> {
    let mut cache = state.oidc_cache.lock().await;
    let fresh = cache
        .keys
        .as_ref()
        .is_some_and(|(_, at)| now_ms() - at < cfg.cache_ttl_ms);
    if !fresh {
        let keys = fetch_keys(cfg, m).await?;
        cache.keys = Some((keys, now_ms()));
        cache.last_key_refresh_ms = now_ms();
    }
    let find = |c: &OidcCache| {
        c.keys
            .as_ref()
            .and_then(|(ks, _)| ks.iter().find(|k| k.kid == kid).cloned())
    };
    if let Some(k) = find(&cache) {
        return Ok(k);
    }
    if now_ms() - cache.last_key_refresh_ms > cfg.key_refresh_min_ms {
        let keys = fetch_keys(cfg, m).await?;
        cache.keys = Some((keys, now_ms()));
        cache.last_key_refresh_ms = now_ms();
        if let Some(k) = find(&cache) {
            return Ok(k);
        }
    }
    Err(refused(
        StatusCode::UNAUTHORIZED,
        "OIDC_KEY_UNKNOWN",
        "the ID token is signed with a key the provider does not publish",
    ))
}

/// The claims of a verified ID token.
struct Verified {
    subject: String,
}

async fn verify_id_token(
    state: &AppState,
    cfg: &OidcConfig,
    m: &Meta,
    token: &str,
    nonce: &str,
) -> ApiResult<Verified> {
    let bad =
        |code: &'static str, why: &str| refused(StatusCode::UNAUTHORIZED, code, why.to_owned());
    let mut parts = token.split('.');
    let (Some(h), Some(p), Some(s), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(bad("OIDC_TOKEN_MALFORMED", "the ID token is not a JWT"));
    };
    let header: Value = unb64(h)
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or_else(|| bad("OIDC_TOKEN_MALFORMED", "the ID token header does not parse"))?;
    if header["alg"] != "EdDSA" {
        return Err(bad(
            "OIDC_ALG_UNSUPPORTED",
            "only EdDSA ID tokens are verified by this build",
        ));
    }
    let kid = header["kid"].as_str().unwrap_or_default();
    let jwk = key_for(state, cfg, m, kid).await?;
    let sig = unb64(s)
        .and_then(|b| Signature::from_slice(&b).ok())
        .ok_or_else(|| {
            bad(
                "OIDC_TOKEN_MALFORMED",
                "the ID token signature does not parse",
            )
        })?;
    VerifyingKey::from_bytes(&jwk.key)
        .map_err(|_| {
            bad(
                "OIDC_KEY_UNKNOWN",
                "the provider's key is not an Ed25519 key",
            )
        })?
        .verify_strict(format!("{h}.{p}").as_bytes(), &sig)
        .map_err(|_| {
            bad(
                "OIDC_TOKEN_SIGNATURE",
                "the ID token's signature does not verify",
            )
        })?;
    let claims: Value = unb64(p)
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or_else(|| bad("OIDC_TOKEN_MALFORMED", "the ID token claims do not parse"))?;
    if claims["iss"].as_str().map(|s| s.trim_end_matches('/')) != Some(cfg.issuer.as_str()) {
        return Err(bad(
            "OIDC_TOKEN_ISSUER",
            "the ID token is from another issuer",
        ));
    }
    let audiences: Vec<&str> = match &claims["aud"] {
        Value::String(a) => vec![a.as_str()],
        Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
        _ => vec![],
    };
    if !audiences.contains(&cfg.client_id.as_str()) {
        return Err(bad(
            "OIDC_TOKEN_AUDIENCE",
            "the ID token is for another client",
        ));
    }
    if audiences.len() > 1 && claims["azp"].as_str() != Some(cfg.client_id.as_str()) {
        return Err(bad(
            "OIDC_TOKEN_AUDIENCE",
            "the ID token's authorised party is not this client",
        ));
    }
    let now_s = now_ms() / 1000;
    let skew_s = cfg.clock_skew_ms / 1000;
    let Some(exp) = claims["exp"].as_i64() else {
        return Err(bad("OIDC_TOKEN_EXPIRED", "the ID token has no expiry"));
    };
    if exp + skew_s < now_s {
        return Err(bad("OIDC_TOKEN_EXPIRED", "the ID token has expired"));
    }
    if claims["iat"]
        .as_i64()
        .is_some_and(|iat| iat - skew_s > now_s)
        || claims["nbf"]
            .as_i64()
            .is_some_and(|nbf| nbf - skew_s > now_s)
    {
        return Err(bad(
            "OIDC_TOKEN_NOT_YET_VALID",
            "the ID token is issued in the future",
        ));
    }
    if !claims["nonce"]
        .as_str()
        .is_some_and(|n| same(n.as_bytes(), nonce.as_bytes()))
    {
        return Err(bad(
            "OIDC_NONCE_MISMATCH",
            "the ID token's nonce is not this login's",
        ));
    }
    let subject = claims["sub"].as_str().unwrap_or_default().to_owned();
    if subject.is_empty() || subject.len() > 255 {
        return Err(bad(
            "OIDC_TOKEN_MALFORMED",
            "the ID token has no usable subject",
        ));
    }
    Ok(Verified { subject })
}

fn oidc(state: &AppState) -> ApiResult<&OidcConfig> {
    state.extras.oidc.as_ref().ok_or_else(|| {
        refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "OIDC_DISABLED",
            "this API has no identity provider configured",
        )
    })
}

fn admit(state: &AppState) -> ApiResult<()> {
    if state.limiter.admit("oidc") {
        Ok(())
    } else {
        Err(refused(
            StatusCode::TOO_MANY_REQUESTS,
            "RATE_LIMITED",
            "too many sign-in attempts",
        ))
    }
}

/// `POST /v1/auth/oidc/start {redirect_uri, code_challenge}`: begin a login;
/// the answer is the provider's authorization URL and the state the client
/// must present back. The challenge is the S256 of a verifier only the client
/// holds.
pub(crate) async fn start(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let cfg = oidc(&state)?;
    admit(&state)?;
    let redirect = body["redirect_uri"].as_str().unwrap_or_default();
    let challenge = body["code_challenge"].as_str().unwrap_or_default();
    if !cfg.redirect_allowed(redirect) {
        return Err(refused(
            StatusCode::BAD_REQUEST,
            "OIDC_REDIRECT_NOT_ALLOWED",
            "that redirect_uri is not one this API accepts",
        ));
    }
    if challenge.len() != 43 || unb64(challenge).is_none_or(|b| b.len() != 32) {
        return Err(refused(
            StatusCode::BAD_REQUEST,
            "OIDC_CHALLENGE_INVALID",
            "code_challenge must be the base64url S256 of a verifier",
        ));
    }
    let m = meta(&state, cfg).await?;
    let state_value = b64(&rand::random::<[u8; 32]>());
    let nonce = b64(&rand::random::<[u8; 32]>());
    state
        .store
        .create_oidc_login(
            &hex::encode(Sha256::digest(state_value.as_bytes())),
            &nonce,
            challenge,
            redirect,
            cfg.login_ttl_ms,
        )
        .await?;
    let mut url = reqwest::Url::parse(&m.authorization_endpoint).map_err(unavailable)?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &cfg.client_id)
        .append_pair("redirect_uri", redirect)
        .append_pair("scope", "openid")
        .append_pair("state", &state_value)
        .append_pair("nonce", &nonce)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(Json(json!({
        "authorization_url": url.to_string(),
        "state": state_value,
        "expires_in_ms": cfg.login_ttl_ms,
    })))
}

/// `POST /v1/auth/oidc/callback {code, state, code_verifier}`: finish a login
/// and sign the provisioned principal in.
pub(crate) async fn callback(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let cfg = oidc(&state)?;
    admit(&state)?;
    let code = body["code"].as_str().unwrap_or_default();
    let state_value = body["state"].as_str().unwrap_or_default();
    let verifier = body["code_verifier"].as_str().unwrap_or_default();
    if code.is_empty()
        || code.len() > 2048
        || state_value.len() > 256
        || verifier.len() < 43
        || verifier.len() > 128
    {
        return Err(ApiError::bad(
            "code, state and code_verifier (43 to 128 characters) are required",
        ));
    }
    let deny = |reason: &'static str| {
        let state = Arc::clone(&state);
        async move {
            let _ = state
                .store
                .record_denial(None, None, "oidc:login", reason)
                .await;
        }
    };
    // 1. The login is spent exactly once: an unknown, expired or already
    //    used state is refused, whatever else is right.
    let Some((nonce, challenge, redirect)) = state
        .store
        .consume_oidc_login(&hex::encode(Sha256::digest(state_value.as_bytes())))
        .await?
    else {
        deny("state unknown, expired or already used").await;
        return Err(refused(
            StatusCode::BAD_REQUEST,
            "OIDC_STATE_INVALID",
            "the login is unknown, expired or already completed",
        ));
    };
    // 2. PKCE: the verifier must be the one whose challenge started it.
    if !same(
        b64(&Sha256::digest(verifier.as_bytes())).as_bytes(),
        challenge.as_bytes(),
    ) {
        deny("code verifier does not match the challenge").await;
        return Err(refused(
            StatusCode::BAD_REQUEST,
            "OIDC_PKCE_MISMATCH",
            "the code verifier does not match the login's challenge",
        ));
    }
    // 3. The code, spent at the provider with the verifier.
    let m = meta(&state, cfg).await?;
    let mut form = vec![
        ("grant_type", "authorization_code".to_owned()),
        ("code", code.to_owned()),
        ("redirect_uri", redirect),
        ("client_id", cfg.client_id.clone()),
        ("code_verifier", verifier.to_owned()),
    ];
    if let Some(secret) = &cfg.client_secret {
        form.push(("client_secret", secret.clone()));
    }
    let resp = http()?
        .post(&m.token_endpoint)
        .form(&form)
        .send()
        .await
        .map_err(unavailable)?;
    let status = resp.status();
    let reply: Value = resp.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        deny("the provider refused the code").await;
        let why = reply["error"]
            .as_str()
            .unwrap_or("unknown")
            .chars()
            .take(64)
            .collect::<String>();
        return Err(refused(
            StatusCode::BAD_REQUEST,
            "OIDC_CODE_REJECTED",
            format!("the provider refused the authorization code ({why})"),
        ));
    }
    let Some(id_token) = reply["id_token"].as_str() else {
        return Err(refused(
            StatusCode::BAD_GATEWAY,
            "OIDC_TOKEN_MISSING",
            "the provider's answer carries no ID token",
        ));
    };
    // 4. The ID token.
    let verified = match verify_id_token(&state, cfg, &m, id_token, &nonce).await {
        Ok(v) => v,
        Err(e) => {
            deny(e.code).await;
            return Err(e);
        }
    };
    // 5. The principal this identity was provisioned for.
    let Some(p) = state
        .store
        .principal_for_identity(&cfg.issuer, &verified.subject)
        .await?
    else {
        deny("the identity is not provisioned").await;
        return Err(refused(
            StatusCode::FORBIDDEN,
            "OIDC_SUBJECT_UNKNOWN",
            "this identity is not provisioned for any principal",
        ));
    };
    if p.disabled {
        deny("the principal is disabled").await;
        return Err(refused(
            StatusCode::FORBIDDEN,
            "PRINCIPAL_DISABLED",
            "the principal this identity signs in as is disabled",
        ));
    }
    let principal = modbit_event_store::cloud::Principal {
        principal_id: p.principal_id,
        tenant_id: p.tenant_id,
        kind: p.kind,
        label: p.label,
    };
    let refresh = state
        .store
        .issue_refresh_token(&principal, state.refresh_ttl_ms)
        .await?;
    Ok(Json(token_response(&state, &principal, refresh)))
}
