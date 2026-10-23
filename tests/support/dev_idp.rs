//! A local OpenID Connect provider for tests (PX-129): discovery, a JWKS,
//! an authorization endpoint that approves at once and redirects with a
//! code, and a token endpoint that spends each code once, checks the PKCE
//! verifier (S256) and the redirect URI, and signs an Ed25519 ID token. It
//! can be told to misbehave (a wrong nonce, an expired or foreign token,
//! another key, an unsupported algorithm) and to rotate its key.
//!
//! It is a development provider, not Entra, Okta or Google: the real
//! provider run of the QUAL has not been made, and those sign with RS256,
//! which the API under test does not verify.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// How the provider misbehaves on the next token responses.
#[derive(Clone, Debug, Default)]
pub struct Faults {
    pub nonce: Option<String>,
    /// Seconds added to `exp` (negative: expired).
    pub exp_offset_secs: i64,
    pub audience: Option<String>,
    pub issuer: Option<String>,
    /// Sign with a key the JWKS does not publish.
    pub foreign_key: bool,
    pub alg: Option<String>,
    /// Sign with a published key under this `kid` the JWKS did not carry at first.
    pub kid: Option<String>,
}

struct Code {
    client_id: String,
    redirect_uri: String,
    challenge: String,
    nonce: String,
    subject: String,
}

struct State_ {
    issuer: String,
    client_id: String,
    keys: Vec<(String, SigningKey)>,
    foreign: SigningKey,
    codes: HashMap<String, Code>,
    subject: String,
    faults: Faults,
    /// Token requests, secrets elided: (grant_type, client_id, has_verifier, has_secret).
    token_requests: Vec<(String, String, bool, bool)>,
    jwks_fetches: u32,
}

/// The running provider.
#[derive(Clone)]
pub struct DevIdp {
    pub issuer: String,
    pub client_id: String,
    state: Arc<Mutex<State_>>,
}

type Shared = Arc<Mutex<State_>>;

impl DevIdp {
    pub async fn start(client_id: &str) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let issuer = format!("http://{addr}");
        let state = Arc::new(Mutex::new(State_ {
            issuer: issuer.clone(),
            client_id: client_id.to_owned(),
            keys: vec![("idp-key-1".into(), SigningKey::from_bytes(&rand::random()))],
            foreign: SigningKey::from_bytes(&rand::random()),
            codes: HashMap::new(),
            subject: "subject-ada".into(),
            faults: Faults::default(),
            token_requests: vec![],
            jwks_fetches: 0,
        }));
        let app = Router::new()
            .route("/.well-known/openid-configuration", get(discovery))
            .route("/jwks", get(jwks))
            .route("/authorize", get(authorize))
            .route("/token", post(token))
            .with_state(Arc::clone(&state));
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Self {
            issuer,
            client_id: client_id.to_owned(),
            state,
        }
    }

    /// Who the next logins sign in as.
    pub fn set_subject(&self, subject: &str) {
        self.state.lock().unwrap().subject = subject.to_owned();
    }

    pub fn set_faults(&self, f: Faults) {
        self.state.lock().unwrap().faults = f;
    }

    /// Publish a second key and sign with it from now on (a rotation).
    pub fn rotate_key(&self) -> String {
        let mut s = self.state.lock().unwrap();
        let kid = format!("idp-key-{}", s.keys.len() + 1);
        s.keys
            .push((kid.clone(), SigningKey::from_bytes(&rand::random())));
        kid
    }

    pub fn token_requests(&self) -> Vec<(String, String, bool, bool)> {
        self.state.lock().unwrap().token_requests.clone()
    }

    pub fn jwks_fetches(&self) -> u32 {
        self.state.lock().unwrap().jwks_fetches
    }

    /// The browser: open the authorization URL, let the provider approve,
    /// and return the `(code, state)` it redirected back with.
    pub async fn approve(&self, authorization_url: &str) -> (String, String) {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let r = http.get(authorization_url).send().await.unwrap();
        assert_eq!(r.status().as_u16(), 302, "the provider redirects back");
        let loc = r.headers()[header::LOCATION].to_str().unwrap().to_owned();
        let url = reqwest::Url::parse(&loc).unwrap();
        let q: HashMap<String, String> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        (q["code"].clone(), q["state"].clone())
    }
}

async fn discovery(State(s): State<Shared>) -> Json<Value> {
    let issuer = s.lock().unwrap().issuer.clone();
    Json(json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/authorize"),
        "token_endpoint": format!("{issuer}/token"),
        "jwks_uri": format!("{issuer}/jwks"),
        "response_types_supported": ["code"],
        "id_token_signing_alg_values_supported": ["EdDSA"],
        "code_challenge_methods_supported": ["S256"],
    }))
}

async fn jwks(State(s): State<Shared>) -> Json<Value> {
    let mut g = s.lock().unwrap();
    g.jwks_fetches += 1;
    let keys: Vec<Value> = g
        .keys
        .iter()
        .map(|(kid, k)| json!({"kty": "OKP", "crv": "Ed25519", "kid": kid, "use": "sig", "alg": "EdDSA", "x": b64(&k.verifying_key().to_bytes())}))
        .collect();
    Json(json!({"keys": keys}))
}

async fn authorize(State(s): State<Shared>, Query(q): Query<HashMap<String, String>>) -> Response {
    let mut g = s.lock().unwrap();
    let get = |k: &str| q.get(k).cloned().unwrap_or_default();
    if get("response_type") != "code"
        || get("client_id") != g.client_id
        || get("code_challenge_method") != "S256"
        || get("code_challenge").is_empty()
        || get("state").is_empty()
        || get("nonce").is_empty()
    {
        return (StatusCode::BAD_REQUEST, "invalid_request").into_response();
    }
    let code = b64(&rand::random::<[u8; 24]>());
    let subject = g.subject.clone();
    g.codes.insert(
        code.clone(),
        Code {
            client_id: get("client_id"),
            redirect_uri: get("redirect_uri"),
            challenge: get("code_challenge"),
            nonce: get("nonce"),
            subject,
        },
    );
    let mut url = reqwest::Url::parse(&get("redirect_uri")).unwrap();
    url.query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &get("state"));
    let mut headers = HeaderMap::new();
    headers.insert(
        header::LOCATION,
        HeaderValue::from_str(url.as_str()).unwrap(),
    );
    (StatusCode::FOUND, headers).into_response()
}

async fn token(State(s): State<Shared>, body: axum::body::Bytes) -> Response {
    // application/x-www-form-urlencoded, read with the URL parser.
    let f: HashMap<String, String> = reqwest::Url::parse(&format!(
        "http://form.invalid/?{}",
        String::from_utf8_lossy(&body)
    ))
    .map(|u| {
        u.query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    })
    .unwrap_or_default();
    let mut g = s.lock().unwrap();
    let get = |k: &str| f.get(k).cloned().unwrap_or_default();
    g.token_requests.push((
        get("grant_type"),
        get("client_id"),
        f.contains_key("code_verifier"),
        f.contains_key("client_secret"),
    ));
    let invalid = |why: &str| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid_grant", "error_description": why})),
        )
            .into_response()
    };
    if get("grant_type") != "authorization_code" {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "unsupported_grant_type"})),
        )
            .into_response();
    }
    // A code is spent once, whatever follows.
    let Some(code) = g.codes.remove(&get("code")) else {
        return invalid("unknown or spent code");
    };
    if code.client_id != get("client_id") || code.redirect_uri != get("redirect_uri") {
        return invalid("client or redirect mismatch");
    }
    if b64(&Sha256::digest(get("code_verifier").as_bytes())) != code.challenge {
        return invalid("PKCE verifier mismatch");
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let faults = g.faults.clone();
    let alg = faults.alg.clone().unwrap_or_else(|| "EdDSA".into());
    let (kid, key) = if faults.foreign_key {
        ("idp-key-1".to_owned(), g.foreign.clone())
    } else {
        let (kid, key) = g.keys.last().unwrap().clone();
        (faults.kid.clone().unwrap_or(kid), key)
    };
    let header = json!({"alg": alg, "typ": "JWT", "kid": kid});
    let payload = json!({
        "iss": faults.issuer.clone().unwrap_or_else(|| g.issuer.clone()),
        "sub": code.subject,
        "aud": faults.audience.clone().unwrap_or_else(|| g.client_id.clone()),
        "exp": now + 300 + faults.exp_offset_secs,
        "iat": now,
        "nonce": faults.nonce.clone().unwrap_or(code.nonce),
    });
    let signing_input = format!(
        "{}.{}",
        b64(header.to_string().as_bytes()),
        b64(payload.to_string().as_bytes())
    );
    let sig = key.sign(signing_input.as_bytes());
    Json(json!({
        "access_token": b64(&rand::random::<[u8; 16]>()),
        "token_type": "Bearer",
        "expires_in": 300,
        "id_token": format!("{signing_input}.{}", b64(&sig.to_bytes())),
    }))
    .into_response()
}
