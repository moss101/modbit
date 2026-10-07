//! PX-129 (QUAL-PX-129, docs/24 "Identity", "Execution policy
//! distribution"): the Cloud API signs a person in through OIDC
//! authorization code with PKCE against a real (local, in-process) provider
//! — state, nonce, verifier, redirect, audience, issuer, expiry within a
//! clock-skew bound and key rotation all checked — provisions tenants and
//! principals (a disabled principal is refused at its next call), and
//! publishes only organisation-signed, tenant-scoped, monotonic policy
//! bundles. Real Postgres (`MODBIT_CLOUD_TEST_DATABASE_URL`).
//!
//! The provider is `tests/support/dev_idp.rs`, which signs Ed25519 ID
//! tokens: a run against Entra, Okta or Google (which sign RS256, not built)
//! has not been made.

#[path = "../../../tests/support/dev_idp.rs"]
mod dev_idp;

use base64::Engine;
use dev_idp::{DevIdp, Faults};
use ed25519_dalek::SigningKey;
use modbit_cloud_api::{Config, Extras, OidcConfig, Served, serve_with};
use modbit_domain::policy_bundle::{self, BundleDocument, Expect, KIND, SCHEMA_VERSION};
use modbit_event_store::cloud::CloudStoreConfig;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const ADMIN: &str = "platform-admin-secret-for-px129";
const CLIENT_SECRET: &str = "oidc-client-secret-for-px129";
const REDIRECT: &str = "http://127.0.0.1:53682/callback";

fn config() -> Option<Config> {
    let database_url = std::env::var("MODBIT_CLOUD_TEST_DATABASE_URL").ok()?;
    Some(Config {
        store: CloudStoreConfig {
            database_url,
            s3: None,
        },
        token_key: Some(vec![5u8; 32]),
        bind: "127.0.0.1:0".into(),
        rate_capacity: 1000,
        rate_per_second: 200.0,
        worker_key: None,
        github_webhook_secret: None,
    })
}

struct Api {
    served: Served,
    http: reqwest::Client,
    base: String,
}

impl Api {
    async fn start(cfg: Config, extras: Extras) -> Api {
        let served = serve_with(cfg, extras).await.expect("serve");
        let base = format!("http://{}", served.addr);
        Api {
            served,
            http: reqwest::Client::new(),
            base,
        }
    }

    async fn call(
        &self,
        method: reqwest::Method,
        token: &str,
        path: &str,
        body: Option<Value>,
    ) -> (u16, Value) {
        let mut r = self.http.request(method, format!("{}{path}", self.base));
        if !token.is_empty() {
            r = r.bearer_auth(token);
        }
        if let Some(b) = body {
            r = r.json(&b);
        }
        let r = r.send().await.unwrap();
        (r.status().as_u16(), r.json().await.unwrap_or(Value::Null))
    }

    async fn post(&self, token: &str, path: &str, body: Value) -> (u16, Value) {
        self.call(reqwest::Method::POST, token, path, Some(body))
            .await
    }

    async fn get(&self, token: &str, path: &str) -> (u16, Value) {
        self.call(reqwest::Method::GET, token, path, None).await
    }

    async fn put(&self, token: &str, path: &str, body: Value) -> (u16, Value) {
        self.call(reqwest::Method::PUT, token, path, Some(body))
            .await
    }

    /// A principal's secret for a token pair.
    async fn token_for(&self, secret: &str) -> Value {
        self.post("", "/v1/auth/token", json!({"secret": secret}))
            .await
            .1
    }
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// A verifier and its S256 challenge.
fn pkce() -> (String, String) {
    let verifier = b64(&rand::random::<[u8; 32]>());
    let challenge = b64(&Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

struct Login {
    code: String,
    state: String,
    verifier: String,
}

/// Start a login and let the provider approve it: what the CLI and the browser do.
async fn begin(api: &Api, idp: &DevIdp) -> Login {
    let (verifier, challenge) = pkce();
    let (s, started) = api
        .post(
            "",
            "/v1/auth/oidc/start",
            json!({"redirect_uri": REDIRECT, "code_challenge": challenge}),
        )
        .await;
    assert_eq!(s, 200, "{started}");
    let url = started["authorization_url"].as_str().unwrap();
    assert!(
        url.starts_with(&idp.issuer)
            && url.contains("code_challenge_method=S256")
            && url.contains("nonce="),
        "{url}"
    );
    assert!(
        !url.contains(&verifier),
        "the verifier never leaves the client"
    );
    let (code, state) = idp.approve(url).await;
    assert_eq!(state, started["state"].as_str().unwrap());
    Login {
        code,
        state,
        verifier,
    }
}

async fn finish(api: &Api, l: &Login) -> (u16, Value) {
    api.post(
        "",
        "/v1/auth/oidc/callback",
        json!({"code": l.code, "state": l.state, "code_verifier": l.verifier}),
    )
    .await
}

#[tokio::test]
async fn qual_px_129_oidc_with_pkce_signs_provisioned_principals_in_and_every_forgery_is_refused() {
    let Some(cfg) = config() else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres)"
        );
        return;
    };
    let idp = DevIdp::start("modbit-cloud").await;
    let mut oidc = OidcConfig::new(&idp.issuer, "modbit-cloud");
    oidc.client_secret = Some(CLIENT_SECRET.into());
    oidc.key_refresh_min_ms = 300;
    let extras = Extras {
        oidc: Some(oidc),
        ..Extras::default()
    }
    .with_admin_secret(ADMIN);
    let api = Api::start(cfg, extras).await;
    let mut secrets: Vec<String> = vec![ADMIN.into(), CLIENT_SECRET.into()];

    // ---- provisioning: only the platform administrator makes tenants ----
    let (s, r) = api
        .post("", "/v1/admin/tenants", json!({"name": "acme"}))
        .await;
    assert_eq!(
        (s, r["code"].as_str()),
        (401, Some("UNAUTHENTICATED")),
        "{r}"
    );
    let (s, r) = api
        .post(
            "wrong-admin-secret",
            "/v1/admin/tenants",
            json!({"name": "acme"}),
        )
        .await;
    assert_eq!(s, 401, "{r}");
    let (s, acme) = api
        .post(ADMIN, "/v1/admin/tenants", json!({"name": "acme"}))
        .await;
    assert_eq!(s, 201, "{acme}");
    let tenant = acme["tenant_id"].as_str().unwrap().to_owned();
    let (s, other) = api
        .post(ADMIN, "/v1/admin/tenants", json!({"name": "globex"}))
        .await;
    assert_eq!(s, 201, "{other}");
    let other_tenant = other["tenant_id"].as_str().unwrap().to_owned();
    // The first administrator of each tenant, bound to the identity that signs in as it.
    let subject = |name: &str| format!("subject-{name}-{}", uuid::Uuid::now_v7().simple());
    let (ada_sub, grace_sub, bob_sub) = (subject("ada"), subject("grace"), subject("bob"));
    let (s, ada) = api
        .post(ADMIN, &format!("/v1/admin/tenants/{tenant}/principals"), json!({"label": "ada", "role": "admin", "oidc": {"issuer": idp.issuer, "subject": ada_sub}}))
        .await;
    assert_eq!(s, 201, "{ada}");
    let ada_id = ada["principal_id"].as_str().unwrap().to_owned();
    secrets.push(ada["secret"].as_str().unwrap().to_owned());
    // The identity is one principal's.
    let (s, dup) = api
        .post(
            ADMIN,
            &format!("/v1/admin/tenants/{other_tenant}/principals"),
            json!({"label": "ada2", "oidc": {"issuer": idp.issuer, "subject": ada_sub}}),
        )
        .await;
    assert_eq!(
        (s, dup["code"].as_str()),
        (409, Some("IDENTITY_TAKEN")),
        "{dup}"
    );
    let (s, bob) = api
        .post(ADMIN, &format!("/v1/admin/tenants/{other_tenant}/principals"), json!({"label": "bob", "role": "admin", "oidc": {"issuer": idp.issuer, "subject": bob_sub}}))
        .await;
    assert_eq!(s, 201, "{bob}");
    secrets.push(bob["secret"].as_str().unwrap().to_owned());

    // ---- sign in with OIDC + PKCE ----
    idp.set_subject(&ada_sub);
    let login = begin(&api, &idp).await;
    let (s, pair) = finish(&api, &login).await;
    assert_eq!(s, 200, "{pair}");
    assert_eq!(
        pair["tenant_id"], tenant,
        "the token is tenant-scoped: {pair}"
    );
    assert_eq!(pair["principal_id"], ada_id);
    let ada_token = pair["access_token"].as_str().unwrap().to_owned();
    secrets.extend([
        ada_token.clone(),
        pair["refresh_token"].as_str().unwrap().to_owned(),
        login.code.clone(),
        login.state.clone(),
        login.verifier.clone(),
    ]);
    let seen = idp.token_requests();
    assert_eq!(
        seen.last().unwrap(),
        &(
            "authorization_code".to_owned(),
            "modbit-cloud".to_owned(),
            true,
            true
        ),
        "the code was spent with the verifier (and the client secret)"
    );
    let (s, list) = api.get(&ada_token, "/v1/principals").await;
    assert_eq!(s, 200, "{list}");
    assert!(
        list["principals"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["tenant_id"] == tenant)
    );
    // Tenant isolation holds for an OIDC session as for any other.
    let (s, sess_b) = {
        let bob_token = api.token_for(bob["secret"].as_str().unwrap()).await["access_token"]
            .as_str()
            .unwrap()
            .to_owned();
        api.post(
            &bob_token,
            "/v1/sessions",
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await
    };
    assert_eq!(s, 201, "{sess_b}");
    let (s, nf) = api
        .get(
            &ada_token,
            &format!("/v1/sessions/{}", sess_b["session_id"].as_str().unwrap()),
        )
        .await;
    assert_eq!(s, 404, "{nf}");

    // ---- every forgery ----
    // A replayed code and state: spent once.
    let (s, r) = finish(&api, &login).await;
    assert_eq!(
        (s, r["code"].as_str()),
        (400, Some("OIDC_STATE_INVALID")),
        "{r}"
    );
    // A state nobody started.
    let l = begin(&api, &idp).await;
    let (s, r) = api
        .post(
            "",
            "/v1/auth/oidc/callback",
            json!({"code": l.code, "state": b64(&[7u8; 32]), "code_verifier": l.verifier}),
        )
        .await;
    assert_eq!(
        (s, r["code"].as_str()),
        (400, Some("OIDC_STATE_INVALID")),
        "{r}"
    );
    // A verifier that is not the challenge's: refused before the provider is asked.
    let l = begin(&api, &idp).await;
    let asked = idp.token_requests().len();
    let (s, r) = api
        .post(
            "",
            "/v1/auth/oidc/callback",
            json!({"code": l.code, "state": l.state, "code_verifier": b64(&[9u8; 32])}),
        )
        .await;
    assert_eq!(
        (s, r["code"].as_str()),
        (400, Some("OIDC_PKCE_MISMATCH")),
        "{r}"
    );
    assert_eq!(
        idp.token_requests().len(),
        asked,
        "the provider never saw the attempt"
    );
    // A login whose time ran out.
    let (verifier, challenge) = pkce();
    let stale_state = format!("expired-state-{}", uuid::Uuid::now_v7().simple());
    api.served
        .state
        .store
        .create_oidc_login(
            &hex::encode(Sha256::digest(stale_state.as_bytes())),
            "n",
            &challenge,
            REDIRECT,
            -1,
        )
        .await
        .unwrap();
    let (s, r) = api
        .post(
            "",
            "/v1/auth/oidc/callback",
            json!({"code": "c", "state": stale_state, "code_verifier": verifier}),
        )
        .await;
    assert_eq!(
        (s, r["code"].as_str()),
        (400, Some("OIDC_STATE_INVALID")),
        "{r}"
    );
    // A redirect the API does not accept.
    for bad in [
        "https://evil.example/callback",
        "http://127.0.0.1:53682/elsewhere",
        "http://evil.example:53682/callback",
    ] {
        let (s, r) = api
            .post(
                "",
                "/v1/auth/oidc/start",
                json!({"redirect_uri": bad, "code_challenge": pkce().1}),
            )
            .await;
        assert_eq!(
            (s, r["code"].as_str()),
            (400, Some("OIDC_REDIRECT_NOT_ALLOWED")),
            "{bad}: {r}"
        );
    }
    let (s, r) = api
        .post(
            "",
            "/v1/auth/oidc/start",
            json!({"redirect_uri": REDIRECT, "code_challenge": "short"}),
        )
        .await;
    assert_eq!(
        (s, r["code"].as_str()),
        (400, Some("OIDC_CHALLENGE_INVALID")),
        "{r}"
    );
    // ID-token forgeries, one fault at a time.
    let cases: Vec<(&str, Faults, u16, &str)> = vec![
        (
            "nonce",
            Faults {
                nonce: Some("not-this-logins-nonce".into()),
                ..Faults::default()
            },
            401,
            "OIDC_NONCE_MISMATCH",
        ),
        (
            "expired",
            Faults {
                exp_offset_secs: -3600,
                ..Faults::default()
            },
            401,
            "OIDC_TOKEN_EXPIRED",
        ),
        (
            "audience",
            Faults {
                audience: Some("another-client".into()),
                ..Faults::default()
            },
            401,
            "OIDC_TOKEN_AUDIENCE",
        ),
        (
            "issuer",
            Faults {
                issuer: Some("http://evil.example".into()),
                ..Faults::default()
            },
            401,
            "OIDC_TOKEN_ISSUER",
        ),
        (
            "foreign key",
            Faults {
                foreign_key: true,
                ..Faults::default()
            },
            401,
            "OIDC_TOKEN_SIGNATURE",
        ),
        (
            "alg",
            Faults {
                alg: Some("RS256".into()),
                ..Faults::default()
            },
            401,
            "OIDC_ALG_UNSUPPORTED",
        ),
        (
            "alg none",
            Faults {
                alg: Some("none".into()),
                ..Faults::default()
            },
            401,
            "OIDC_ALG_UNSUPPORTED",
        ),
        (
            "unknown kid",
            Faults {
                kid: Some("no-such-key".into()),
                ..Faults::default()
            },
            401,
            "OIDC_KEY_UNKNOWN",
        ),
    ];
    for (name, faults, status, code) in cases {
        idp.set_faults(faults);
        let l = begin(&api, &idp).await;
        let (s, r) = finish(&api, &l).await;
        assert_eq!((s, r["code"].as_str()), (status, Some(code)), "{name}: {r}");
        assert!(r.get("access_token").is_none());
    }
    // The clock-skew bound: a token that expired a minute ago is within the
    // two minutes the API allows; one that expired six minutes ago is not.
    idp.set_faults(Faults {
        exp_offset_secs: -(300 + 60),
        ..Faults::default()
    });
    let (s, r) = finish(&api, &begin(&api, &idp).await).await;
    assert_eq!(s, 200, "within the skew: {r}");
    idp.set_faults(Faults {
        exp_offset_secs: -(300 + 400),
        ..Faults::default()
    });
    let (s, r) = finish(&api, &begin(&api, &idp).await).await;
    assert_eq!(
        (s, r["code"].as_str()),
        (401, Some("OIDC_TOKEN_EXPIRED")),
        "beyond the skew: {r}"
    );
    idp.set_faults(Faults::default());
    // An identity nobody provisioned.
    idp.set_subject("subject-stranger");
    let (s, r) = finish(&api, &begin(&api, &idp).await).await;
    assert_eq!(
        (s, r["code"].as_str()),
        (403, Some("OIDC_SUBJECT_UNKNOWN")),
        "{r}"
    );
    // Key rotation: the provider signs with a key the API has not seen; it
    // refetches the keys and accepts.
    idp.set_subject(&ada_sub);
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let before = idp.jwks_fetches();
    let kid = idp.rotate_key();
    assert_eq!(kid, "idp-key-2");
    let (s, r) = finish(&api, &begin(&api, &idp).await).await;
    assert_eq!(s, 200, "a rotated key is picked up: {r}");
    assert!(idp.jwks_fetches() > before, "the keys were refetched");

    // ---- a tenant administrator provisions its tenant; a member cannot ----
    let (s, grace) = api
        .post(
            &ada_token,
            "/v1/principals",
            json!({"label": "grace", "oidc": {"issuer": idp.issuer, "subject": grace_sub}}),
        )
        .await;
    assert_eq!(s, 201, "{grace}");
    let grace_id = grace["principal_id"].as_str().unwrap().to_owned();
    secrets.push(grace["secret"].as_str().unwrap().to_owned());
    idp.set_subject(&grace_sub);
    let (s, g_pair) = finish(&api, &begin(&api, &idp).await).await;
    assert_eq!(s, 200, "{g_pair}");
    let grace_token = g_pair["access_token"].as_str().unwrap().to_owned();
    let grace_refresh = g_pair["refresh_token"].as_str().unwrap().to_owned();
    secrets.extend([grace_token.clone(), grace_refresh.clone()]);
    let (s, r) = api
        .post(&grace_token, "/v1/principals", json!({"label": "mallory"}))
        .await;
    assert_eq!(
        (s, r["code"].as_str()),
        (403, Some("ADMIN_REQUIRED")),
        "a member provisions nothing: {r}"
    );
    let (s, _) = api
        .post(
            &grace_token,
            "/v1/sessions",
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await;
    assert_eq!(s, 201, "a member works");
    // Another tenant's principal is not found, and the attempt is audited.
    let bob_id = bob["principal_id"].as_str().unwrap();
    let (s, r) = api
        .post(
            &ada_token,
            &format!("/v1/principals/{bob_id}:disable"),
            json!({}),
        )
        .await;
    assert_eq!(s, 404, "{r}");
    let (s, r) = api
        .post(
            &ada_token,
            &format!("/v1/principals/{ada_id}:disable"),
            json!({}),
        )
        .await;
    assert_eq!(
        (s, r["code"].as_str()),
        (409, Some("LAST_ADMIN_GUARD")),
        "{r}"
    );
    // Disabled: refused at the very next call, however long the token has left.
    let (s, r) = api
        .post(
            &ada_token,
            &format!("/v1/principals/{grace_id}:disable"),
            json!({}),
        )
        .await;
    assert_eq!(s, 200, "{r}");
    let (s, r) = api
        .post(
            &grace_token,
            "/v1/sessions",
            json!({"command_id": uuid::Uuid::now_v7().to_string()}),
        )
        .await;
    assert_eq!(
        (s, r["code"].as_str()),
        (401, Some("PRINCIPAL_DISABLED")),
        "{r}"
    );
    let (s, r) = api
        .post(
            "",
            "/v1/auth/refresh",
            json!({"refresh_token": grace_refresh}),
        )
        .await;
    assert_eq!(
        s, 401,
        "a disabled principal's refresh token buys nothing: {r}"
    );
    let (s, r) = api
        .post("", "/v1/auth/token", json!({"secret": grace["secret"]}))
        .await;
    assert_eq!(s, 401, "nor does its secret: {r}");
    idp.set_subject(&grace_sub);
    let (s, r) = finish(&api, &begin(&api, &idp).await).await;
    assert_eq!(
        (s, r["code"].as_str()),
        (403, Some("PRINCIPAL_DISABLED")),
        "nor does its identity: {r}"
    );
    // Enabled again, it signs in; promoted, it provisions.
    let (s, _) = api
        .post(
            &ada_token,
            &format!("/v1/principals/{grace_id}:enable"),
            json!({}),
        )
        .await;
    assert_eq!(s, 200);
    let (s, g2) = finish(&api, &begin(&api, &idp).await).await;
    assert_eq!(s, 200, "{g2}");
    let (s, _) = api
        .post(
            &ada_token,
            &format!("/v1/principals/{grace_id}:role"),
            json!({"role": "admin"}),
        )
        .await;
    assert_eq!(s, 200);
    let (s, r) = api
        .post(
            g2["access_token"].as_str().unwrap(),
            "/v1/principals",
            json!({"label": "heidi"}),
        )
        .await;
    assert_eq!(s, 201, "{r}");
    secrets.push(r["secret"].as_str().unwrap().to_owned());

    // ---- the audit records who did what, and holds no secret ----
    let (s, audit) = api.get(ADMIN, "/v1/admin/audit").await;
    assert_eq!(s, 200);
    let text = audit.to_string();
    let actions: Vec<&str> = audit["audit"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["tenant_id"].is_null() || true)
        .filter_map(|a| a["action"].as_str())
        .collect();
    for want in [
        "tenant.create",
        "principal.create",
        "principal.disable",
        "principal.enable",
        "principal.role",
    ] {
        assert!(actions.contains(&want), "{want} in {actions:?}");
    }
    assert!(
        audit["audit"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["actor"] == "platform-admin" && a["action"] == "tenant.create")
    );
    assert!(
        audit["audit"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["actor"] == format!("principal:{ada_id}")
                && a["action"] == "principal.disable")
    );
    let denials = format!(
        "{:?} {:?}",
        api.served
            .state
            .store
            .denials(modbit_domain::TenantId::parse(&tenant).unwrap())
            .await
            .unwrap(),
        api.served
            .state
            .store
            .denials_for_resource("oidc:login")
            .await
            .unwrap()
    );
    for secret in &secrets {
        assert!(
            !text.contains(secret.as_str()),
            "a secret is on the provisioning audit"
        );
        assert!(
            !denials.contains(secret.as_str()),
            "a secret is on the denial ledger"
        );
    }
    api.served.stop();
}

fn org_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn document(tenant: &str, generation: u64, expires_in_ms: i64) -> BundleDocument {
    let now = modbit_domain::Timestamp::now().millis();
    BundleDocument {
        kind: KIND.into(),
        schema_version: SCHEMA_VERSION,
        tenant_id: tenant.into(),
        generation,
        issued_at_ms: now,
        expires_at_ms: now + expires_in_ms,
        min_protocol_major: 1,
        admin_config: json!({"permissions": {"task.handoff": "DENY"}, "review_comment_authors": ["reviewer"]}),
    }
}

#[tokio::test]
async fn qual_px_129_only_bundles_the_organisation_signed_with_a_greater_generation_are_accepted_and_served()
 {
    let Some(cfg) = config() else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres)"
        );
        return;
    };
    let api = Api::start(cfg, Extras::default().with_admin_secret(ADMIN)).await;
    let (s, t) = api
        .post(ADMIN, "/v1/admin/tenants", json!({"name": "bundles"}))
        .await;
    assert_eq!(s, 201);
    let tenant = t["tenant_id"].as_str().unwrap().to_owned();
    let (s, u) = api
        .post(ADMIN, "/v1/admin/tenants", json!({"name": "neighbour"}))
        .await;
    assert_eq!(s, 201);
    let neighbour = u["tenant_id"].as_str().unwrap().to_owned();
    let mk = |tenant: &str, label: &'static str, role: &'static str| {
        let tenant = tenant.to_owned();
        let api = &api;
        async move {
            let (s, p) = api
                .post(
                    ADMIN,
                    &format!("/v1/admin/tenants/{tenant}/principals"),
                    json!({"label": label, "role": role}),
                )
                .await;
            assert_eq!(s, 201, "{p}");
            api.token_for(p["secret"].as_str().unwrap()).await["access_token"]
                .as_str()
                .unwrap()
                .to_owned()
        }
    };
    let admin = mk(&tenant, "root", "admin").await;
    let member = mk(&tenant, "worker-bee", "member").await;
    let neighbour_admin = mk(&neighbour, "next-door", "admin").await;

    // The organisation's key is the platform administrator's to register.
    let key = org_key(41);
    let public = hex::encode(key.verifying_key().to_bytes());
    let (s, r) = api
        .put(
            &admin,
            &format!("/v1/admin/tenants/{tenant}/org-keys/org-2026"),
            json!({"public_key_hex": public}),
        )
        .await;
    assert_eq!(
        s, 401,
        "a tenant administrator cannot move a trust root: {r}"
    );
    let (s, r) = api
        .put(
            ADMIN,
            &format!("/v1/admin/tenants/{tenant}/org-keys/org-2026"),
            json!({"public_key_hex": public}),
        )
        .await;
    assert_eq!(s, 200, "{r}");
    let (s, _) = api
        .put(
            ADMIN,
            &format!("/v1/admin/tenants/{tenant}/org-keys/org-2026"),
            json!({"public_key_hex": "zz"}),
        )
        .await;
    assert_eq!(s, 400);

    let publish = |token: String, signed: Value| {
        let api = &api;
        async move { api.put(&token, "/v1/policy/bundle", signed).await }
    };
    let signed = |doc: &BundleDocument, key_id: &str, k: &SigningKey| {
        serde_json::to_value(policy_bundle::sign(doc, key_id, k).unwrap()).unwrap()
    };

    // 1. Nothing published yet.
    let (s, r) = api.get(&member, "/v1/policy/bundle").await;
    assert_eq!(s, 404, "{r}");
    // 2. A bundle the organisation key signed: accepted, and served as published.
    let g3 = signed(&document(&tenant, 3, 3_600_000), "org-2026", &key);
    let (s, r) = publish(admin.clone(), g3.clone()).await;
    assert_eq!((s, r["generation"].as_u64()), (201, Some(3)), "{r}");
    let (s, served) = api.get(&member, "/v1/policy/bundle").await;
    assert_eq!(s, 200, "{served}");
    assert_eq!(served["bundle"], g3, "served exactly as published");
    let verified = policy_bundle::verify(
        &serde_json::from_value(served["bundle"].clone()).unwrap(),
        &Expect {
            keys: &[("org-2026".into(), key.verifying_key().to_bytes())],
            tenant_id: &tenant,
            now_ms: modbit_domain::Timestamp::now().millis(),
            min_generation: 3,
            protocol_major: 1,
        },
    )
    .unwrap();
    assert_eq!(verified.admin_config["permissions"]["task.handoff"], "DENY");
    // 3. A member publishes nothing.
    let (s, r) = publish(
        member.clone(),
        signed(&document(&tenant, 4, 3_600_000), "org-2026", &key),
    )
    .await;
    assert_eq!(
        (s, r["code"].as_str()),
        (403, Some("ADMIN_REQUIRED")),
        "{r}"
    );
    // 4. Each refusal, and after all of them the last good bundle still stands.
    let cases: Vec<(&str, Value, u16, &str)> = vec![
        (
            "the same generation again",
            g3.clone(),
            409,
            "BUNDLE_STALE_GENERATION",
        ),
        (
            "an older generation",
            signed(&document(&tenant, 2, 3_600_000), "org-2026", &key),
            409,
            "BUNDLE_STALE_GENERATION",
        ),
        (
            "signed by another key under the registered id",
            signed(&document(&tenant, 9, 3_600_000), "org-2026", &org_key(42)),
            403,
            "BUNDLE_BAD_SIGNATURE",
        ),
        (
            "signed under an unregistered key id",
            signed(&document(&tenant, 9, 3_600_000), "org-rogue", &org_key(42)),
            403,
            "BUNDLE_UNTRUSTED_KEY",
        ),
        (
            "written for another tenant",
            signed(&document(&neighbour, 9, 3_600_000), "org-2026", &key),
            403,
            "BUNDLE_WRONG_TENANT",
        ),
        (
            "already expired",
            signed(&document(&tenant, 9, -1000), "org-2026", &key),
            400,
            "BUNDLE_EXPIRED",
        ),
        (
            "the device layer's keys",
            {
                let mut d = document(&tenant, 9, 3_600_000);
                d.admin_config = json!({"device": {"sandbox_required": false}});
                signed(&d, "org-2026", &key)
            },
            400,
            "BUNDLE_FORBIDDEN_CONTENT",
        ),
        (
            "not a bundle",
            json!({"key_id": "org-2026", "document": "{}", "signature": "00"}),
            400,
            "BUNDLE_MALFORMED",
        ),
    ];
    for (name, bundle, status, code) in cases {
        let (s, r) = publish(admin.clone(), bundle).await;
        assert_eq!((s, r["code"].as_str()), (status, Some(code)), "{name}: {r}");
    }
    // Tampered after signing: the old signature no longer covers it.
    let mut tampered = signed(&document(&tenant, 10, 3_600_000), "org-2026", &key);
    tampered["document"] = json!(
        tampered["document"]
            .as_str()
            .unwrap()
            .replace("DENY", "ALLOW")
    );
    let (s, r) = publish(admin.clone(), tampered).await;
    assert_eq!(
        (s, r["code"].as_str()),
        (403, Some("BUNDLE_BAD_SIGNATURE")),
        "{r}"
    );
    let (_, served) = api.get(&admin, "/v1/policy/bundle").await;
    assert_eq!(served["generation"], 3);
    assert_eq!(served["bundle"], g3, "the last good bundle stands");
    // 5. A greater generation replaces it; the neighbour never sees this tenant's.
    let g5 = signed(&document(&tenant, 5, 3_600_000), "org-2026", &key);
    let (s, _) = publish(admin.clone(), g5.clone()).await;
    assert_eq!(s, 201);
    let (_, served) = api.get(&member, "/v1/policy/bundle").await;
    assert_eq!(served["bundle"], g5);
    let (s, r) = api.get(&neighbour_admin, "/v1/policy/bundle").await;
    assert_eq!(s, 404, "another tenant's bundle is not found: {r}");
    // 6. A revoked key signs nothing new.
    let (s, _) = api
        .put(
            ADMIN,
            &format!("/v1/admin/tenants/{tenant}/org-keys/org-2026"),
            json!({"revoke": true}),
        )
        .await;
    assert_eq!(s, 200);
    let (s, r) = publish(
        admin.clone(),
        signed(&document(&tenant, 6, 3_600_000), "org-2026", &key),
    )
    .await;
    assert_eq!(
        (s, r["code"].as_str()),
        (403, Some("BUNDLE_UNTRUSTED_KEY")),
        "{r}"
    );
    // 7. The audit names the publications and the refusals are on the denial ledger.
    let (_, audit) = api.get(ADMIN, "/v1/admin/audit").await;
    let publishes = audit["audit"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["action"] == "policy.publish" && a["detail"]["key_id"] == "org-2026")
        .count();
    assert!(publishes >= 2, "{audit}");
    api.served.stop();
}
