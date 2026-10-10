//! The chat client and the spend meter against a scripted OpenAI-compatible
//! server (offline: the model is a stand-in, the HTTP and the accounting are
//! real).

use std::collections::HashMap;
use std::sync::Arc;

use modbit_bench_context_economics::chat::scripted::{Scripted, serve};
use modbit_bench_context_economics::chat::{Chat, ChatError};
use modbit_bench_context_economics::spend::SpendMeter;

const KEY: &str = "sk-live-0123456789abcdef0123456789";

fn chat(base: &str) -> Chat {
    let m: HashMap<&str, String> = HashMap::from([
        ("OPENAI_API_KEY", KEY.to_owned()),
        ("MODBIT_OPENAI_BASE_URL", base.to_owned()),
        ("MODBIT_OPENAI_MODELS", "m=1.00/2.00;ctx=1000".to_owned()),
    ]);
    Chat::from_lookup(&|k| m.get(k).cloned(), "m").unwrap()
}

#[tokio::test]
async fn a_reply_is_charged_from_the_usage_the_gateway_returned() {
    let (base, seen) = serve(Arc::new(|_| Scripted {
        text: "hi".into(),
        usage: Some((1_000, 500)),
        ..Scripted::default()
    }))
    .await;
    let c = chat(&base);
    let meter = SpendMeter::new(1.0).unwrap();
    let r = c
        .complete(
            &meter,
            0.01,
            &[serde_json::json!({"role":"user","content":"x"})],
            &[],
            100,
        )
        .await
        .unwrap();
    assert_eq!(
        (r.text.as_str(), r.input_tokens, r.output_tokens),
        ("hi", 1_000, 500)
    );
    // 1000 * 1/M + 500 * 2/M
    assert!((meter.record().spent_usd - 0.002).abs() < 1e-12);
    assert_eq!(seen.lock().unwrap()[0]["model"], "m");
    assert!(!format!("{c:?}").contains(KEY));
}

#[tokio::test]
async fn a_reply_without_usage_is_counted_as_unknown_never_as_free() {
    let (base, _) = serve(Arc::new(|_| Scripted {
        text: "hi".into(),
        usage: None,
        ..Scripted::default()
    }))
    .await;
    let meter = SpendMeter::new(1.0).unwrap();
    let r = chat(&base)
        .complete(&meter, 0.0, &[], &[], 10)
        .await
        .unwrap();
    assert!(!r.usage_known);
    let rec = meter.record();
    assert_eq!((rec.unknown_usage_calls, rec.spent_usd), (1, 0.0));
}

#[tokio::test]
async fn the_cap_stops_the_next_call_before_it_is_sent() {
    let (base, seen) = serve(Arc::new(|_| Scripted {
        text: "x".into(),
        usage: Some((300_000, 100_000)),
        ..Scripted::default()
    }))
    .await;
    let c = chat(&base);
    // One call costs 0.3 + 0.2 = 0.5.
    let meter = SpendMeter::new(0.6).unwrap();
    c.complete(&meter, 0.1, &[], &[], 10).await.unwrap();
    let err = c.complete(&meter, 0.2, &[], &[], 10).await.unwrap_err();
    assert!(matches!(err, ChatError::Cap(_)), "{err:?}");
    assert_eq!(
        seen.lock().unwrap().len(),
        1,
        "the refused call never reached the server"
    );
    assert!(meter.record().stopped_by_cap);
}

#[tokio::test]
async fn a_rejected_request_is_an_error_and_the_key_is_not_in_it() {
    let (base, _) = serve(Arc::new(|_| Scripted {
        status: Some((401, "{\"error\":\"bad key\"}".into())),
        ..Scripted::default()
    }))
    .await;
    let meter = SpendMeter::new(1.0).unwrap();
    let err = chat(&base)
        .complete(&meter, 0.0, &[], &[], 10)
        .await
        .unwrap_err();
    let text = err.to_string();
    assert!(text.contains("401") && !text.contains(KEY), "{text}");
    assert_eq!(meter.record().calls, 0);
}

#[test]
fn a_missing_key_or_an_unpriced_model_is_refused() {
    let none = |_: &str| None::<String>;
    assert!(Chat::from_lookup(&none, "m").is_err());
    let m: HashMap<&str, String> = HashMap::from([
        ("OPENAI_API_KEY", KEY.to_owned()),
        ("MODBIT_OPENAI_MODELS", "other=1/2".to_owned()),
    ]);
    let err = Chat::from_lookup(&|k| m.get(k).cloned(), "m").unwrap_err();
    assert!(err.contains("does not list"), "{err}");
}
