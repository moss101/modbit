//! PX-138 live half, offline: the `compaction-eval-live` binary against a
//! scripted OpenAI-compatible server. The scripted model answers from the
//! context it is given (it finds the expected strings in the request text),
//! so recall differs between arms mechanically; it is a stand-in for the
//! model and nothing here has been run live.

use std::path::Path;
use std::process::{Command, Output};
use std::sync::Arc;

use modbit_bench_context_economics::chat::scripted::{Scripted, serve};
use modbit_bench_context_economics::compaction_eval::{Arm, long_runs};
use modbit_bench_context_economics::compaction_live::{
    Kind, build_messages, parse_numbered, score_recall, score_task,
};
use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_compaction-eval-live");
const KEY: &str = "sk-live-0123456789abcdef0123456789";

/// The honest scripted model: answers a question from the context only.
fn model(runs: usize) -> Arc<dyn Fn(&Value) -> Scripted + Send + Sync> {
    let all = long_runs(runs);
    Arc::new(move |body: &Value| {
        let msgs = body["messages"].as_array().cloned().unwrap_or_default();
        let user = msgs
            .iter()
            .find(|m| m["role"] == "user")
            .and_then(|m| m["content"].as_str())
            .unwrap_or_default()
            .to_owned();
        // The first user message of the run is retained by every arm.
        let Some(run) = all
            .iter()
            .find(|r| user.contains(r.entries[0].text.as_str()))
        else {
            return Scripted {
                text: "unknown".into(),
                usage: Some((100, 5)),
                ..Scripted::default()
            };
        };
        let usage = Some((user.len() as u64 / 4, 60));
        if user.contains("QUESTIONS:") {
            let (ctx, qs) = user.split_once("QUESTIONS:").unwrap();
            let lines: Vec<String> = qs
                .lines()
                .filter(|l| !l.trim().is_empty())
                .enumerate()
                .map(|(i, _)| {
                    let p = &run.probes[i];
                    if p.answer.iter().all(|a| ctx.contains(a.as_str())) {
                        format!("{}. {}", i + 1, p.answer.join("; "))
                    } else {
                        format!("{}. unknown", i + 1)
                    }
                })
                .collect();
            Scripted {
                text: lines.join("\n"),
                usage,
                ..Scripted::default()
            }
        } else {
            let t = &run.task;
            let has = |i: usize| user.contains(run.entries[i].text.as_str());
            let plan = json!({
                "next_step": if has(t.next_step_entry) { run.entries[t.next_step_entry].text.clone() } else { "unknown".into() },
                "do_not_touch": if has(0) { t.forbidden_dir.clone() } else { "unknown".into() },
                "user_constraint": if has(t.constraint_entry) { run.entries[t.constraint_entry].text.clone() } else { "unknown".into() },
                "files_to_edit": [],
            });
            Scripted {
                text: format!("```json\n{plan}\n```"),
                usage,
                ..Scripted::default()
            }
        }
    })
}

fn run_bin(base: &str, out: &Path, extra: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut c = Command::new(BIN);
    for v in [
        "MODBIT_LIVE",
        "MODBIT_LIVE_API_KEY",
        "MODBIT_LIVE_MODEL",
        "MODBIT_LIVE_BASE_URL",
        "MODBIT_LIVE_MAX_COST_USD",
        "MODBIT_LIVE_ALLOW_LOOPBACK",
        "OPENAI_API_KEY",
        "MODBIT_OPENAI_BASE_URL",
        "MODBIT_OPENAI_MODELS",
        "MODBIT_OPENAI_AUTH",
        "MODBIT_OPENAI_EXTRA_BODY",
    ] {
        c.env_remove(v);
    }
    c.args(["--out-dir", out.to_str().unwrap()]).args(extra);
    c.env("MODBIT_LIVE", "1")
        .env("MODBIT_LIVE_API_KEY", KEY)
        .env("MODBIT_LIVE_MODEL", "m")
        .env("MODBIT_LIVE_BASE_URL", base)
        .env("MODBIT_LIVE_ALLOW_LOOPBACK", "1")
        .env("MODBIT_OPENAI_MODELS", "m=0.15/0.50;ctx=200000");
    for (k, v) in envs {
        c.env(k, v);
    }
    c.output().unwrap()
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f).await.unwrap()
}

fn json_file(p: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
}

fn arm<'a>(result: &'a Value, label: &str) -> &'a Value {
    result["report"]["arms"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["arm"] == label)
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_good_run_is_complete_and_the_lossy_arm_scores_below_structured() {
    let (base, seen) = serve(model(20)).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("o");
    let (b, o) = (base.clone(), out.clone());
    let res =
        blocking(move || run_bin(&b, &o, &["--runs", "20", "--max-cost-usd", "5"], &[])).await;
    assert!(
        res.status.success(),
        "{}",
        String::from_utf8_lossy(&res.stderr)
    );
    // 20 runs x 4 arms x (recall + task).
    assert_eq!(seen.lock().unwrap().len(), 160);
    let status = json_file(&out.join("status.json"));
    assert_eq!(status["row"], "px138");
    assert_eq!(status["complete"], true, "{status}");
    assert_eq!(status["missing"].as_array().unwrap().len(), 0);
    assert_eq!(status["spend"]["stopped_by_cap"], false);
    let result = json_file(&out.join("result.json"));
    assert_eq!(result["report"]["runs_scored"], 20);
    let rec = |l: &str| arm(&result, l)["recall"]["p"].as_f64().unwrap();
    assert!(
        (rec("uncompacted") - 1.0).abs() < 1e-12,
        "the upper bound is reached"
    );
    assert!(
        rec("uncompacted") > rec("structured"),
        "structured drops something"
    );
    assert!(rec("structured") > rec("lossy"), "the metric can fail live");
    assert!(rec("extractive") < rec("uncompacted"));
    let lv = &result["report"]["lossy_vs_structured"];
    assert_eq!(lv["lossy_below_structured"], true);
    assert_eq!(lv["intervals_separated"], true, "{lv}");
    // Intervals on every proportion; task success scored for every run and arm.
    for l in ["uncompacted", "extractive", "structured", "lossy"] {
        let a = arm(&result, l);
        for k in ["recall", "task"] {
            assert!(a[k]["lo"].as_f64().unwrap() <= a[k]["hi"].as_f64().unwrap());
        }
        assert_eq!(a["task"]["n"], 20);
        assert!(a["cost_usd_total"].as_f64().unwrap() > 0.0);
    }
    assert!((arm(&result, "uncompacted")["task"]["p"].as_f64().unwrap() - 1.0).abs() < 1e-12);
    assert!(
        arm(&result, "lossy")["task"]["p"].as_f64().unwrap()
            <= arm(&result, "uncompacted")["task"]["p"].as_f64().unwrap()
    );
    assert!(
        arm(&result, "structured")["tokens_saved"]["mean"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    let summary = std::fs::read_to_string(out.join("summary.md")).unwrap();
    assert!(summary.contains("COMPLETE") && summary.contains("Wilson"));
    // Every exchange is retained and the digest reproduces without a network.
    assert_eq!(
        std::fs::read_dir(out.join("exchanges")).unwrap().count(),
        160
    );
    let o = out.clone();
    let re = blocking(move || {
        Command::new(BIN)
            .args(["--rescore", o.to_str().unwrap()])
            .output()
            .unwrap()
    })
    .await;
    assert!(
        re.status.success(),
        "{}",
        String::from_utf8_lossy(&re.stderr)
    );
    assert!(
        String::from_utf8_lossy(&re.stdout).contains(result["report"]["digest"].as_str().unwrap())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_call_is_a_missing_measurement_and_exit_one() {
    let inner = model(3);
    // Fail one request (HTTP 400: no retry, nothing charged).
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let reply: Arc<dyn Fn(&Value) -> Scripted + Send + Sync> = Arc::new(move |body| {
        if count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 4 {
            return Scripted {
                status: Some((400, "{\"error\":\"scripted failure\"}".into())),
                ..Scripted::default()
            };
        }
        inner(body)
    });
    let (base, _) = serve(reply).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("o");
    let (b, o) = (base.clone(), out.clone());
    let res = blocking(move || run_bin(&b, &o, &["--runs", "3", "--max-cost-usd", "5"], &[])).await;
    assert_eq!(res.status.code(), Some(1));
    let status = json_file(&out.join("status.json"));
    assert_eq!(status["complete"], false);
    let missing = status["missing"].to_string();
    assert!(
        missing.contains("exchange failed") && missing.contains("scripted failure"),
        "{missing}"
    );
    assert!(missing.contains("at least 20"), "{missing}");
    assert!(out.join("result.json").exists() && out.join("summary.md").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reply_without_usage_makes_cost_a_missing_measurement() {
    let inner = model(2);
    let reply: Arc<dyn Fn(&Value) -> Scripted + Send + Sync> = Arc::new(move |b| {
        let mut s = inner(b);
        s.usage = None;
        s
    });
    let (base, _) = serve(reply).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("o");
    let (b, o) = (base.clone(), out.clone());
    let res = blocking(move || run_bin(&b, &o, &["--runs", "2", "--max-cost-usd", "5"], &[])).await;
    assert_eq!(res.status.code(), Some(1));
    let status = json_file(&out.join("status.json"));
    assert!(
        status["missing"].to_string().contains("carried no usage"),
        "{status}"
    );
    assert_eq!(status["spend"]["unknown_usage_calls"], 16);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tiny_cap_stops_the_run_with_a_bounded_number_of_requests_and_a_partial_result() {
    let (base, seen) = serve(model(20)).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("o");
    let (b, o) = (base.clone(), out.clone());
    // 100 USD per Mtok in and out: a request reserves about $0.4 for its
    // output budget alone, so a $1 cap admits only a few.
    let res = blocking(move || {
        run_bin(
            &b,
            &o,
            &["--runs", "20", "--max-cost-usd", "1"],
            &[("MODBIT_OPENAI_MODELS", "m=100/100;ctx=200000")],
        )
    })
    .await;
    assert_eq!(res.status.code(), Some(1));
    let n = seen.lock().unwrap().len();
    assert!((1..=12).contains(&n), "{n} requests reached the server");
    let status = json_file(&out.join("status.json"));
    assert_eq!(status["complete"], false);
    assert_eq!(status["spend"]["stopped_by_cap"], true);
    assert!(
        status["missing"].to_string().contains("spend cap"),
        "{status}"
    );
    let result = json_file(&out.join("result.json"));
    assert!(result["report"]["runs_scored"].as_u64().unwrap() < 20);
    // Whole runs only are scored: arms stay balanced.
    let runs: Vec<u64> = result["report"]["arms"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["runs"].as_u64().unwrap())
        .collect();
    assert!(runs.iter().all(|r| *r == runs[0]), "{runs:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn rescore_reproduces_the_digest_and_fails_on_an_altered_exchange() {
    let (base, _) = serve(model(4)).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("o");
    let (b, o) = (base.clone(), out.clone());
    let res = blocking(move || run_bin(&b, &o, &["--runs", "4", "--max-cost-usd", "5"], &[])).await;
    assert_eq!(
        res.status.code(),
        Some(1),
        "four runs are below the required twenty"
    );
    let rescore = |o: std::path::PathBuf| {
        blocking(move || {
            Command::new(BIN)
                .args(["--rescore", o.to_str().unwrap()])
                .output()
                .unwrap()
        })
    };
    assert!(rescore(out.clone()).await.status.success());
    // Alter the reply of one exchange so a probe flips: the digest changes.
    let f = out
        .join("exchanges")
        .join("long-00__lossy_double__recall.json");
    let mut v = json_file(&f);
    v["reply_text"] = Value::String("1. unknown".into());
    std::fs::write(&f, serde_json::to_vec_pretty(&v).unwrap()).unwrap();
    let r = rescore(out.clone()).await;
    assert_eq!(r.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&r.stderr).contains("digest does not reproduce"));
    // A change that cannot move a score still moves the exchange hash.
    v["reply_text"] = Value::String("different".into());
    v["latency_ms"] = json!(1);
    std::fs::write(&f, serde_json::to_vec_pretty(&v).unwrap()).unwrap();
    assert_eq!(rescore(out.clone()).await.status.code(), Some(1));
    // An altered request fails its own hash.
    let mut v = json_file(&f);
    v["messages"][1]["content"] = Value::String("forged".into());
    std::fs::write(&f, serde_json::to_vec_pretty(&v).unwrap()).unwrap();
    let r = rescore(out.clone()).await;
    assert!(String::from_utf8_lossy(&r.stderr).contains("altered"));
}

#[test]
fn the_binary_refuses_without_live_mode_a_cap_or_an_out_dir_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("o");
    let mut c = Command::new(BIN);
    c.args(["--out-dir", out.to_str().unwrap(), "--max-cost-usd", "5"]);
    for v in ["MODBIT_LIVE", "MODBIT_LIVE_API_KEY", "MODBIT_LIVE_MODEL"] {
        c.env_remove(v);
    }
    let r = c.output().unwrap();
    assert_eq!(r.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&r.stderr).contains("LIVE: NOT RUN"));
    assert!(!out.exists());
    // Live mode asked for but no cap: still refused.
    let r = run_bin("http://example.invalid/v4", &out, &[], &[]);
    assert_eq!(r.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&r.stderr).contains("max-cost-usd"));
    assert!(!out.join("result.json").exists());
}

#[test]
fn the_scorers_follow_the_offline_oracle_and_the_task_criteria() {
    let run = &long_runs(1)[0];
    let msgs = build_messages(Kind::Recall, "ctx", &run.probes);
    assert!(msgs[1]["content"].as_str().unwrap().contains("1. "));
    let all: Vec<String> = run
        .probes
        .iter()
        .enumerate()
        .map(|(i, p)| format!("{}) {}", i + 1, p.answer.join(" and ")))
        .collect();
    let s = score_recall(&run.probes, &all.join("\n"));
    assert!(s.iter().all(|p| p.correct));
    // A missing answer string, an unknown and a missing line are misses.
    let s = score_recall(&run.probes, "1. nothing here\n2. unknown");
    assert!(!s[0].correct && !s[0].unknown && s[1].unknown && s[2].unparsed);
    assert_eq!(
        parse_numbered("<think>1. x</think>\n1. a\ncont\n3: c", 3)[0].as_deref(),
        Some("a\ncont")
    );
    // Task: all three criteria from the generator's record.
    let t = &run.task;
    let good = json!({"next_step": "Run the suite and update the CHANGELOG", "do_not_touch": t.forbidden_dir,
        "user_constraint": format!("keep the {} timeout at {}ms", t.constraint_component, t.constraint_ms),
        "files_to_edit": ["src/x.rs"]});
    let mut tk = t.clone();
    tk.next_step_keys = vec!["changelog".into()];
    assert!(score_task(&tk, &good.to_string()).success);
    let mut bad = good.clone();
    bad["files_to_edit"] = json!([format!("{}a.rs", t.forbidden_dir)]);
    let sc = score_task(&tk, &bad.to_string());
    assert!(!sc.forbidden && !sc.success && sc.next_step && sc.constraint);
    assert!(!score_task(&tk, "no json here").parsed);
    let _ = Arm::ALL;
}
