//! The mechanics of the live-gateway legs (`live_core.rs`), proven offline:
//! a leg refuses to run without the credential, the model, its priced
//! catalog entry and a spend cap (it never skips silently as a pass); the
//! spend cap stops a run; a failed assertion fails the leg, and the failure
//! is retained in a result record; and the job's checker
//! (`.github/scripts/live_core_check.py`) fails for a missing, failed,
//! capped or non-live record. Nothing here talks to a model.

mod live_support;

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use live_support::{Leg, LiveEnv, catalog_window, percentile};

const KEY: &str = "sk-live-0123456789abcdef0123456789";
const CATALOG: &str = "glm-5.3-flash=0.15/0.50;ctx=200000;out=131072";

fn vars(extra: &[(&str, &str)]) -> BTreeMap<String, String> {
    let mut m: BTreeMap<String, String> = [
        ("MODBIT_LIVE", "1"),
        ("MODBIT_LIVE_API_KEY", KEY),
        ("MODBIT_LIVE_MODEL", "glm-5.3-flash"),
        ("MODBIT_LIVE_BASE_URL", "https://gateway.example/api/v4"),
        ("MODBIT_OPENAI_MODELS", CATALOG),
        ("MODBIT_LIVE_MAX_COST_USD", "5"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect();
    for (k, v) in extra {
        if v.is_empty() {
            m.remove(*k);
        } else {
            m.insert((*k).to_owned(), (*v).to_owned());
        }
    }
    m
}

fn env_with(m: &BTreeMap<String, String>, dir: &std::path::Path) -> Arc<LiveEnv> {
    let mut m = m.clone();
    m.insert(
        "MODBIT_LIVE_RESULTS_DIR".into(),
        dir.to_string_lossy().into_owned(),
    );
    Arc::new(LiveEnv::from_vars(&|k| m.get(k).cloned()).unwrap())
}

fn refused(extra: &[(&str, &str)]) -> String {
    let m = vars(extra);
    match LiveEnv::from_vars(&|k| m.get(k).cloned()) {
        Ok(_) => panic!("the run was not refused"),
        Err(why) => why,
    }
}

#[test]
fn a_leg_is_refused_without_the_credential_the_model_the_catalog_or_a_cap() {
    // Not requested as live.
    assert!(refused(&[("MODBIT_LIVE", "")]).contains("MODBIT_LIVE=1"));
    // No credential.
    assert!(refused(&[("MODBIT_LIVE_API_KEY", "")]).contains("MODBIT_LIVE_API_KEY"));
    // A placeholder is not a credential.
    assert!(refused(&[("MODBIT_LIVE_API_KEY", "test-key-placeholder")]).contains("placeholder"));
    // No model.
    assert!(refused(&[("MODBIT_LIVE_MODEL", "")]).contains("MODBIT_LIVE_MODEL"));
    // A model the catalog does not price has no cost to charge.
    let why = refused(&[("MODBIT_OPENAI_MODELS", "other-model=1/2")]);
    assert!(why.contains("does not list glm-5.3-flash"), "{why}");
    assert!(refused(&[("MODBIT_OPENAI_MODELS", "")]).contains("MODBIT_OPENAI_MODELS"));
    // A live leg spends money: it needs a hard cap.
    assert!(refused(&[("MODBIT_LIVE_MAX_COST_USD", "")]).contains("MODBIT_LIVE_MAX_COST_USD"));
    // A loopback gateway is a stand-in, not a live one.
    assert!(refused(&[("MODBIT_LIVE_BASE_URL", "http://127.0.0.1:9000")]).contains("loopback"));
}

#[test]
fn the_key_never_appears_in_a_refusal_or_in_a_record() {
    let why = refused(&[("MODBIT_OPENAI_MODELS", "")]);
    assert!(!why.contains(KEY));
    let dir = tempfile::tempdir().unwrap();
    let env = env_with(&vars(&[]), dir.path());
    let mut leg = Leg::begin(&env, "px999", "secret", 0.01);
    leg.check("ok", true, "fine");
    leg.finish();
    let path = leg.path();
    drop(leg);
    let text = std::fs::read_to_string(path).unwrap();
    assert!(!text.contains(KEY), "{text}");
    assert!(text.contains("glm-5.3-flash") && text.contains("gateway.example"));
}

#[test]
fn the_catalog_window_and_the_price_are_read_from_the_catalog() {
    assert_eq!(catalog_window(CATALOG, "glm-5.3-flash"), Some(200_000));
    assert_eq!(catalog_window("m=1/2", "m"), None);
    assert_eq!(catalog_window(CATALOG, "other"), None);
    let dir = tempfile::tempdir().unwrap();
    let env = env_with(&vars(&[]), dir.path());
    assert_eq!(env.window, Some(200_000));
    assert!((env.price.cost_usd(1_000_000, 1_000_000) - 0.65).abs() < 1e-9);
    assert_eq!(
        env.core_env
            .iter()
            .find(|(k, _)| k == "ANTHROPIC_API_KEY")
            .map(|(_, v)| v.as_str()),
        Some("")
    );
    assert_eq!(percentile(&[5, 1, 9, 3, 7], 95.0), 9);
    assert_eq!(percentile(&[5, 1, 9, 3, 7], 50.0), 5);
    assert_eq!(percentile(&[], 95.0), 0);
}

#[test]
fn a_passing_leg_writes_a_complete_record() {
    let dir = tempfile::tempdir().unwrap();
    let env = env_with(&vars(&[]), dir.path());
    let mut leg = Leg::begin(&env, "px998", "good", 0.5);
    leg.count("deltas", 12);
    leg.charge(3, 2_000, 400);
    leg.check("sum", true, "9 == 9");
    leg.note("the gateway gave no cached count");
    leg.finish();
    let path = leg.path();
    drop(leg);
    let rec: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(rec["passed"], true);
    assert_eq!(rec["row"], "px998");
    assert_eq!(rec["model_calls"], 3);
    assert_eq!(rec["input_tokens"], 2_000);
    assert_eq!(rec["counts"]["deltas"], 12);
    assert_eq!(rec["checks"][0]["name"], "sum");
    assert_eq!(rec["live"], true);
    // 2000 in at 0.15 and 400 out at 0.50 per million tokens.
    assert!(
        (rec["cost_usd"].as_f64().unwrap() - 0.0005).abs() < 1e-9,
        "{rec}"
    );
}

#[test]
fn a_failed_assertion_fails_the_leg_and_the_failure_is_retained() {
    let dir = tempfile::tempdir().unwrap();
    let env = env_with(&vars(&[]), dir.path());
    let path = dir.path().join("px997-bad.json");
    let r = catch_unwind(AssertUnwindSafe(|| {
        let mut leg = Leg::begin(&env, "px997", "bad", 0.1);
        leg.check("fine", true, "yes");
        leg.check(
            "total_equals_provider",
            false,
            "total 9 differs from the provider's 12",
        );
        leg.finish();
    }));
    let msg = r.expect_err("a failed check must panic, which fails the test");
    let msg = msg
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| msg.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_default();
    assert!(msg.contains("total_equals_provider"), "{msg}");
    let rec: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(rec["passed"], false, "{rec}");
    assert_eq!(rec["checks"][1]["ok"], false);
    assert!(
        rec["checks"][1]["detail"]
            .as_str()
            .unwrap()
            .contains("differs")
    );
}

#[test]
fn a_leg_that_panics_or_returns_early_is_not_a_pass() {
    let dir = tempfile::tempdir().unwrap();
    let env = env_with(&vars(&[]), dir.path());
    let r = catch_unwind(AssertUnwindSafe(|| {
        let mut leg = Leg::begin(&env, "px996", "crash", 0.1);
        leg.check("fine", true, "yes");
        panic!("the Core did not answer");
    }));
    assert!(r.is_err());
    let rec: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join("px996-crash.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(rec["passed"], false);
    assert!(
        rec["failure"]
            .as_str()
            .unwrap()
            .contains("before it finished")
    );
    // An early return without finish() is also not a pass.
    {
        let mut leg = Leg::begin(&env, "px995", "early", 0.1);
        leg.check("fine", true, "yes");
    }
    let rec: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join("px995-early.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(rec["passed"], false);
}

#[test]
fn the_spend_cap_refuses_the_next_leg_and_records_it() {
    let dir = tempfile::tempdir().unwrap();
    let env = env_with(&vars(&[("MODBIT_LIVE_MAX_COST_USD", "0.01")]), dir.path());
    // The first leg spends 0.0098 USD (65 000 in, 0 out at 0.15 / Mtok).
    {
        let mut leg = Leg::begin(&env, "px994", "first", 0.005);
        leg.charge(1, 65_000, 0);
        leg.check("ok", true, "spent");
        leg.finish();
    }
    // The second would pass the cap: refused before it starts.
    let r = catch_unwind(AssertUnwindSafe(|| {
        let _ = Leg::begin(&env, "px994", "second", 0.005);
    }));
    let msg = r.expect_err("the cap must stop the run");
    let msg = msg.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(msg.contains("spend cap reached"), "{msg}");
    let rec: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join("px994-second.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(rec["passed"], false);
    assert_eq!(rec["stopped_by_cap"], true);
    assert!(env.meter.record().stopped_by_cap);
    // A leg also stops itself between phases.
    let env = env_with(&vars(&[("MODBIT_LIVE_MAX_COST_USD", "0.02")]), dir.path());
    let r = catch_unwind(AssertUnwindSafe(|| {
        let mut leg = Leg::begin(&env, "px993", "phases", 0.001);
        leg.charge(1, 100_000, 0);
        leg.admit(0.01);
        leg.finish();
    }));
    assert!(r.is_err(), "the second phase was not covered by the cap");
}

// ---- the job's checker ----

#[cfg(unix)]
mod checker {
    use std::process::Command;

    fn script() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.github/scripts/live_core_check.py")
    }

    fn run(dir: &std::path::Path, rows: &str, cap: &str) -> (bool, String) {
        let out = Command::new("python3")
            .arg("-I")
            .arg(script())
            .arg(dir)
            .arg(rows)
            .arg(cap)
            .output()
            .expect("python3 is needed for the checker test");
        (
            out.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    }

    fn write(dir: &std::path::Path, name: &str, rec: serde_json::Value) {
        std::fs::write(dir.join(name), rec.to_string()).unwrap();
    }

    fn good(row: &str, cost: f64) -> serde_json::Value {
        serde_json::json!({"row": row, "leg": "main", "passed": true, "live": true, "stopped_by_cap": false,
            "cost_usd": cost, "model_calls": 1, "input_tokens": 1, "output_tokens": 1, "checks": []})
    }

    #[test]
    fn the_checker_passes_whole_records_and_fails_everything_else() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "px001-main.json", good("px001", 0.1));
        let (ok, text) = run(d.path(), "px001", "5");
        assert!(ok, "{text}");
        // A requested row with no record fails.
        let (ok, text) = run(d.path(), "px001,px002", "5");
        assert!(
            !ok && text.contains("px002") && text.contains("NO RECORD"),
            "{text}"
        );
        // A failed record fails, naming the failed check.
        let mut bad = good("px003", 0.0);
        bad["passed"] = serde_json::json!(false);
        bad["checks"] = serde_json::json!([{"name": "sum", "ok": false, "detail": "9 != 12"}]);
        write(d.path(), "px003-main.json", bad);
        let (ok, text) = run(d.path(), "px003", "5");
        assert!(
            !ok && text.contains("sum") && text.contains("9 != 12"),
            "{text}"
        );
        // A record stopped by the cap fails even if it says passed.
        let d2 = tempfile::tempdir().unwrap();
        let mut capped = good("px004", 0.1);
        capped["stopped_by_cap"] = serde_json::json!(true);
        write(d2.path(), "px004-main.json", capped);
        assert!(!run(d2.path(), "px004", "5").0);
        // A scripted stand-in is not a live record.
        let d3 = tempfile::tempdir().unwrap();
        let mut scripted = good("px005", 0.1);
        scripted["live"] = serde_json::json!(false);
        write(d3.path(), "px005-main.json", scripted);
        assert!(!run(d3.path(), "px005", "5").0);
        // Total spend above the cap fails.
        let d4 = tempfile::tempdir().unwrap();
        write(d4.path(), "px006-main.json", good("px006", 6.0));
        let (ok, text) = run(d4.path(), "px006", "5");
        assert!(!ok && text.contains("over the"), "{text}");
        // An unreadable record fails.
        let d5 = tempfile::tempdir().unwrap();
        std::fs::write(d5.path().join("px007-main.json"), "{ not json").unwrap();
        assert!(!run(d5.path(), "px007", "5").0);
    }
}
