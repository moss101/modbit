//! PX-129 (QUAL-PX-129), the client half: the operator signs a policy
//! bundle offline with `modbit-cli cloud policy sign`, publishes it, and a
//! client verifies the bundle it fetches against the organisation key it
//! was told to trust — and applies nothing otherwise: a tampered row, a
//! key it does not trust, a generation below the one it already accepted
//! and an expired bundle each fail closed, naming why.
//!
//! Real `modbit-cli`, real cloud API, real Postgres. Runs only where
//! `MODBIT_CLOUD_TEST_DATABASE_URL` names a database.

mod px_cloud_common;

use std::path::Path;

use modbit_cloud_api::Extras;
use px_cloud_common::*;
use serde_json::{Value, json};

const ADMIN: &str = "px129-cli-platform-admin";

async fn cli(profile: &Path, secret: &str, args: &[&str]) -> (i32, String, String) {
    let out = tokio::process::Command::new(cli_bin())
        .env("MODBIT_CLOUD_SECRET", secret)
        .arg("--data-dir")
        .arg(profile)
        .args(args)
        .output()
        .await
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn qual_px_129_the_cli_signs_and_publishes_a_bundle_and_applies_only_what_it_verifies() {
    let Some(store_cfg) = store_config().await else {
        eprintln!(
            "SKIPPED: MODBIT_CLOUD_TEST_DATABASE_URL unset (the hosted cloud job runs this against Postgres)"
        );
        return;
    };
    assert!(cli_bin().exists(), "modbit-cli at {}", cli_bin().display());
    let keep = tempfile::tempdir().unwrap();
    let dir = keep.path();
    let api = Api::start_with(&store_cfg, None, Extras::default().with_admin_secret(ADMIN)).await;
    let (s, t) = api
        .post(ADMIN, "/v1/admin/tenants", json!({"name": "px129-cli"}))
        .await;
    assert_eq!(s, 201, "{t}");
    let tenant = t["tenant_id"].as_str().unwrap().to_owned();
    let (s, p) = api
        .post(
            ADMIN,
            &format!("/v1/admin/tenants/{tenant}/principals"),
            json!({"label": "ops", "role": "admin"}),
        )
        .await;
    assert_eq!(s, 201, "{p}");
    let secret = p["secret"].as_str().unwrap().to_owned();
    let profile = dir.join("profile");
    std::fs::create_dir_all(&profile).unwrap();
    let (code, out, err) = cli(&profile, &secret, &["cloud", "login", "--api", &api.base]).await;
    assert_eq!(code, 0, "{out}{err}");

    // The operator's key, minted offline; the platform administrator registers its public half.
    let key_file = dir.join("org.key");
    let (code, out, err) = cli(
        &profile,
        &secret,
        &[
            "cloud",
            "policy",
            "keygen",
            "--out",
            key_file.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(code, 0, "{out}{err}");
    let public = out
        .split("public_key=")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    assert_eq!(public.len(), 64);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&key_file).unwrap().permissions().mode() & 0o777,
            0o600,
            "the signing key is the operator's alone"
        );
    }
    let (s, r) = api
        .call_put(
            ADMIN,
            &format!("/v1/admin/tenants/{tenant}/org-keys/org-1"),
            json!({"public_key_hex": public}),
        )
        .await;
    assert_eq!(s, 200, "{r}");

    let config = dir.join("admin.json");
    std::fs::write(
        &config,
        r#"{"permissions": {"task.handoff": "DENY"}, "review_comment_authors": ["reviewer"]}"#,
    )
    .unwrap();
    let sign = |generation: &str, ttl: &str, out: &Path| {
        let (key, config, out) = (key_file.clone(), config.clone(), out.to_path_buf());
        let (tenant, profile, secret) = (tenant.clone(), profile.clone(), secret.clone());
        let (generation, ttl) = (generation.to_owned(), ttl.to_owned());
        async move {
            cli(
                &profile,
                &secret,
                &[
                    "cloud",
                    "policy",
                    "sign",
                    "--key",
                    key.to_str().unwrap(),
                    "--key-id",
                    "org-1",
                    "--tenant",
                    &tenant,
                    "--generation",
                    &generation,
                    "--ttl-secs",
                    &ttl,
                    "--config",
                    config.to_str().unwrap(),
                    "--out",
                    out.to_str().unwrap(),
                ],
            )
            .await
        }
    };
    let signed1 = dir.join("g1.json");
    let (code, out, err) = sign("1", "3600", &signed1).await;
    assert_eq!(code, 0, "{out}{err}");
    let (code, out, err) = cli(
        &profile,
        &secret,
        &[
            "cloud",
            "policy",
            "publish",
            "--file",
            signed1.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("policy-published generation=1"), "{out}");
    // A generation below the published one is refused by the API, too.
    let (code, _, err) = cli(
        &profile,
        &secret,
        &[
            "cloud",
            "policy",
            "publish",
            "--file",
            signed1.to_str().unwrap(),
        ],
    )
    .await;
    assert_ne!(code, 0);
    assert!(err.contains("BUNDLE_STALE_GENERATION"), "{err}");

    // ---- fetch: verified against the key it was told to trust ----
    let applied = dir.join("applied.json");
    let trust = format!("org-1={public}");
    let (code, out, err) = cli(
        &profile,
        &secret,
        &[
            "cloud",
            "policy",
            "fetch",
            "--trust",
            &trust,
            "--write-admin-config",
            applied.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        out.contains("policy generation=1") && out.contains("verified=true"),
        "{out}"
    );
    let applied_json: Value = serde_json::from_slice(&std::fs::read(&applied).unwrap()).unwrap();
    assert_eq!(applied_json["permissions"]["task.handoff"], "DENY");
    // A bundle is never trusted for its own say-so.
    let (code, _, err) = cli(&profile, &secret, &["cloud", "policy", "fetch"]).await;
    assert_ne!(code, 0);
    assert!(err.contains("--trust"), "{err}");

    // ---- fail closed ----
    let db = {
        let (c, conn) = tokio_postgres::connect(&store_cfg.database_url, tokio_postgres::NoTls)
            .await
            .unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        c
    };
    let tenant_uuid = uuid::Uuid::parse_str(&tenant).unwrap();
    let before = std::fs::read(&applied).unwrap();
    let applied2 = dir.join("applied2.json");
    let fetch = |trust: String, out: &Path| {
        let (profile, secret, out) = (profile.clone(), secret.clone(), out.to_path_buf());
        async move {
            cli(
                &profile,
                &secret,
                &[
                    "cloud",
                    "policy",
                    "fetch",
                    "--trust",
                    &trust,
                    "--write-admin-config",
                    out.to_str().unwrap(),
                ],
            )
            .await
        }
    };
    // A tampered row (written past the API).
    let mut tampered: Value = serde_json::from_slice(&std::fs::read(&signed1).unwrap()).unwrap();
    tampered["document"] = json!(
        tampered["document"]
            .as_str()
            .unwrap()
            .replace("DENY", "ALLOW")
            .replace("\"generation\":1", "\"generation\":2")
    );
    db.execute("INSERT INTO policy_bundles (tenant_id, generation, key_id, signed, published_at_ms) VALUES ($1, 2, 'org-1', $2, 0)", &[&tenant_uuid, &tampered]).await.unwrap();
    let (code, out, err) = fetch(trust.clone(), &applied2).await;
    assert_ne!(code, 0, "{out}{err}");
    assert!(
        err.contains("REFUSED BUNDLE_BAD_SIGNATURE") && err.contains("nothing was applied"),
        "{err}"
    );
    assert!(!applied2.exists(), "nothing was written");
    db.execute(
        "DELETE FROM policy_bundles WHERE tenant_id = $1 AND generation = 2",
        &[&tenant_uuid],
    )
    .await
    .unwrap();
    // A key it does not trust.
    let other = hex::encode(
        ed25519_dalek::SigningKey::from_bytes(&[9u8; 32])
            .verifying_key()
            .to_bytes(),
    );
    let (code, _, err) = fetch(format!("org-1={other}"), &applied2).await;
    assert_ne!(code, 0);
    assert!(err.contains("REFUSED BUNDLE_BAD_SIGNATURE"), "{err}");
    let (code, _, err) = fetch(format!("org-rogue={public}"), &applied2).await;
    assert_ne!(code, 0);
    assert!(err.contains("REFUSED BUNDLE_UNTRUSTED_KEY"), "{err}");
    assert!(!applied2.exists());
    // A newer bundle is accepted (the floor moves to 2) ...
    let signed2 = dir.join("g2.json");
    assert_eq!(sign("2", "3600", &signed2).await.0, 0);
    assert_eq!(
        cli(
            &profile,
            &secret,
            &[
                "cloud",
                "policy",
                "publish",
                "--file",
                signed2.to_str().unwrap()
            ]
        )
        .await
        .0,
        0
    );
    let (code, out, err) = fetch(trust.clone(), &applied2).await;
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("policy generation=2"), "{out}");
    // ... and a row put back to generation 1 (a rollback) is not.
    db.execute(
        "DELETE FROM policy_bundles WHERE tenant_id = $1 AND generation = 2",
        &[&tenant_uuid],
    )
    .await
    .unwrap();
    let applied3 = dir.join("applied3.json");
    let (code, _, err) = fetch(trust.clone(), &applied3).await;
    assert_ne!(code, 0);
    assert!(err.contains("REFUSED BUNDLE_STALE_GENERATION"), "{err}");
    assert!(!applied3.exists());
    // An expired bundle.
    let signed3 = dir.join("g3.json");
    assert_eq!(sign("3", "2", &signed3).await.0, 0);
    assert_eq!(
        cli(
            &profile,
            &secret,
            &[
                "cloud",
                "policy",
                "publish",
                "--file",
                signed3.to_str().unwrap()
            ]
        )
        .await
        .0,
        0
    );
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let (code, _, err) = fetch(trust.clone(), &applied3).await;
    assert_ne!(code, 0);
    assert!(err.contains("REFUSED BUNDLE_EXPIRED"), "{err}");
    assert!(!applied3.exists());
    assert_eq!(
        std::fs::read(&applied).unwrap(),
        before,
        "what was applied before is untouched"
    );

    // ---- no secret or token in anything the client printed or kept ----
    let session = std::fs::read_to_string(profile.join("cloud/session.json")).unwrap();
    assert!(!session.contains(&secret));
    api.served.stop();
}
