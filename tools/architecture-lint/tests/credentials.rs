//! PX-130: the credential-read lint, on disposable source trees and on this
//! repository itself (QUAL-PX-130 "the architecture lint fails a fixture that
//! reads a credential outside the interface").

use std::fs;
use std::path::Path;

use architecture_lint::Rules;
use architecture_lint::credentials::check_credentials;

const RULES: &str = r#"
[[credentials]]
roots = ["crates", "services"]
markers = ["API_KEY", "TOKEN", "SECRET"]
exempt = ["BUDGET", "TOKENS"]
allowed = ["services/core/src/credentials.rs"]
grandfathered = ["crates/old/src/boot.rs::OLD_SERVICE_TOKEN"]
reason = "test: credentials come from the broker"
"#;

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

#[test]
fn a_credential_read_outside_the_broker_is_reported_with_its_place() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    // The broker itself may read.
    write(
        r,
        "services/core/src/credentials.rs",
        "fn k() { let _ = std::env::var(\"OPENAI_API_KEY\"); }\n",
    );
    // A grandfathered read is allowed; a second variable in the same file is not.
    write(
        r,
        "crates/old/src/boot.rs",
        "fn a() { let _ = std::env::var(\"OLD_SERVICE_TOKEN\"); }\nfn b() { let _ = std::env::var(\"NEW_SERVICE_TOKEN\"); }\n",
    );
    // A fixture that reads a credential directly, including a split call and var_os.
    write(
        r,
        "crates/rogue/src/lib.rs",
        "fn a() {}\nfn b() { let _ = std::env::var(\"GITHUB_TOKEN\"); }\nfn c() {\n    let _ = std::env::var_os(\n        \"STRIPE_SECRET\",\n    );\n}\n",
    );
    // Not credentials, comments, test code and tests/ directories.
    write(
        r,
        "crates/fine/src/lib.rs",
        "// std::env::var(\"X_TOKEN\") in a comment\nfn a() { let _ = std::env::var(\"PACK_TOKENS\"); let _ = std::env::var(\"CONTEXT_TOKEN_BUDGET\"); let _ = std::env::var(\"HOME\"); }\n#[cfg(test)]\nmod t { fn x() { let _ = std::env::var(\"TEST_API_KEY\"); } }\n",
    );
    write(
        r,
        "crates/fine/tests/it.rs",
        "fn t() { let _ = std::env::var(\"TEST_API_KEY\"); }\n",
    );
    let rules = Rules::parse(RULES).unwrap();
    let v = check_credentials(r, &rules.credentials[0]).unwrap();
    let got: Vec<(String, usize, String)> = v
        .iter()
        .map(|v| (v.path.clone(), v.line, v.name.clone()))
        .collect();
    assert_eq!(
        got,
        vec![
            (
                "crates/old/src/boot.rs".into(),
                2,
                "NEW_SERVICE_TOKEN".into()
            ),
            ("crates/rogue/src/lib.rs".into(), 2, "GITHUB_TOKEN".into()),
            ("crates/rogue/src/lib.rs".into(), 4, "STRIPE_SECRET".into()),
        ],
        "{v:?}"
    );
    assert!(
        v[0].to_string()
            .contains("credentials come from the broker")
    );
}

/// The repository itself: no credential is read outside the broker except the
/// listed entry points.
#[test]
fn this_repository_reads_credentials_only_through_the_broker_and_the_listed_entry_points() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let rules = Rules::load(&root.join("tools/architecture-lint/rules.toml")).unwrap();
    assert!(!rules.credentials.is_empty(), "the rule is configured");
    for rule in &rules.credentials {
        let v = check_credentials(&root, rule).unwrap();
        assert!(
            v.is_empty(),
            "a credential read outside the broker:\n{}",
            v.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}
