//! The live mode refuses to run without a real provider and never leaks the
//! key (PX-114 harness; QUAL-PX-136 "records nothing fake").

use std::collections::HashMap;

use modbit_bench_context_economics::live::{LiveRefused, live_config};

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let m: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    move |k| m.get(k).cloned()
}

const KEY: &str = "sk-live-0123456789abcdef0123456789";

fn base<'a>() -> Vec<(&'a str, &'a str)> {
    vec![
        ("MODBIT_LIVE", "1"),
        ("MODBIT_LIVE_API_KEY", KEY),
        ("MODBIT_LIVE_MODEL", "m"),
    ]
}

fn with<'a>(extra: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut v = base();
    v.extend_from_slice(extra);
    v
}

#[test]
fn nothing_starts_without_an_explicit_request_a_key_and_a_model() {
    assert_eq!(
        live_config(&env(&[
            ("MODBIT_LIVE_API_KEY", KEY),
            ("MODBIT_LIVE_MODEL", "m")
        ]))
        .unwrap_err(),
        LiveRefused::NotRequested
    );
    assert_eq!(
        live_config(&env(&[("MODBIT_LIVE", "1"), ("MODBIT_LIVE_MODEL", "m")])).unwrap_err(),
        LiveRefused::Missing("MODBIT_LIVE_API_KEY")
    );
    assert_eq!(
        live_config(&env(&[
            ("MODBIT_LIVE", "1"),
            ("MODBIT_LIVE_API_KEY", "   "),
            ("MODBIT_LIVE_MODEL", "m")
        ]))
        .unwrap_err(),
        LiveRefused::Missing("MODBIT_LIVE_API_KEY")
    );
    assert_eq!(
        live_config(&env(&[("MODBIT_LIVE", "1"), ("MODBIT_LIVE_API_KEY", KEY)])).unwrap_err(),
        LiveRefused::Missing("MODBIT_LIVE_MODEL")
    );
    let msg = live_config(&env(&[])).unwrap_err().to_string();
    assert!(msg.starts_with("LIVE: NOT RUN"), "{msg}");
}

#[test]
fn a_placeholder_key_and_a_loopback_gateway_are_stand_ins_and_are_refused() {
    for placeholder in [
        "test",
        "dummy-key-dummy-key",
        "scripted-provider-key",
        "short",
    ] {
        assert_eq!(
            live_config(&env(&[
                ("MODBIT_LIVE", "1"),
                ("MODBIT_LIVE_API_KEY", placeholder),
                ("MODBIT_LIVE_MODEL", "m")
            ]))
            .unwrap_err(),
            LiveRefused::PlaceholderKey,
            "{placeholder}"
        );
    }
    for url in [
        "http://127.0.0.1:8080/v1",
        "http://localhost:9/v1",
        "http://[::1]:1/v1",
        "http://user@127.0.0.2/v1",
    ] {
        assert_eq!(
            live_config(&env(&with(&[("MODBIT_LIVE_BASE_URL", url)]))).unwrap_err(),
            LiveRefused::LoopbackBaseUrl,
            "{url}"
        );
    }
    // The owner's explicit override for a real gateway on this host.
    assert!(
        live_config(&env(&with(&[
            ("MODBIT_LIVE_BASE_URL", "http://127.0.0.1:8080/v1"),
            ("MODBIT_LIVE_ALLOW_LOOPBACK", "1")
        ])))
        .is_ok()
    );
}

#[test]
fn a_configured_run_is_live_hands_the_key_only_to_the_core_and_never_prints_it() {
    let cfg = live_config(&env(&[
        ("MODBIT_LIVE", "1"),
        ("MODBIT_LIVE_API_KEY", KEY),
        ("MODBIT_LIVE_MODEL", "gpt-5-mini"),
        ("MODBIT_LIVE_BASE_URL", "https://gateway.example.com/v1"),
        ("MODBIT_LIVE_REPEATS", "2"),
    ]))
    .unwrap();
    assert!(cfg.run.live);
    assert_eq!((cfg.run.model.as_str(), cfg.repeats), ("gpt-5-mini", 2));
    assert!(cfg.run.env.contains(&("OPENAI_API_KEY".into(), KEY.into())));
    assert!(
        cfg.run
            .env
            .iter()
            .any(|(k, v)| k == "MODBIT_OPENAI_BASE_URL" && v.starts_with("https://"))
    );
    assert!(!format!("{cfg:?}").contains(KEY));
    let bad = live_config(&env(&with(&[("MODBIT_LIVE_REPEATS", "0")]))).unwrap_err();
    assert!(!bad.to_string().contains(KEY));
}

#[test]
fn the_live_binary_refuses_without_configuration_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("report.json");
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_live_trial"))
        .env_remove("MODBIT_LIVE")
        .env("MODBIT_LIVE_OUT", &out)
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&run.stderr).contains("LIVE: NOT RUN"));
    assert!(!out.exists(), "a refused run records nothing");
    // Asked for, but with no key: still nothing.
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_live_trial"))
        .env("MODBIT_LIVE", "1")
        .env_remove("MODBIT_LIVE_API_KEY")
        .env("MODBIT_LIVE_MODEL", "m")
        .env("MODBIT_LIVE_OUT", &out)
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(2));
    assert!(!out.exists());
}
