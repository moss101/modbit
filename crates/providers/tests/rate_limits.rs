//! Rate limits and concurrency at the gateway (audit G; FIX-14), against a
//! real HTTP server on a loopback socket that answers 429 with `Retry-After`
//! or holds requests open: a provider's own `Retry-After` is waited, not
//! replaced by the gateway's jittered backoff; the wait is endpoint-wide, so
//! a request that starts after a rate limit does not walk into it; a wait
//! that would outlive the request is reported at once; and an endpoint is
//! asked to serve no more requests at once than it was configured for, with a
//! queue that never outlives the request's own deadline.
//!
//! These prove the gateway's behaviour on the wire; they do not claim what a
//! live provider does, which only a live run shows.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};

use modbit_providers::gateway::retry_after_of;
use modbit_providers::{
    AuthScheme, Endpoint, Message, ModelCapability, ModelEvent, ModelPolicy, ModelRequest,
    ProviderGateway, ProviderKind, Requirements, Role, SecretHandle,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

/// What the server does with the request it has just read.
#[derive(Clone)]
struct Behaviour {
    /// The first `limited` requests are answered 429 with this header
    /// (`name: value`), the rest succeed.
    limited: usize,
    retry_after: Option<(&'static str, String)>,
    /// Every successful request is held this long before it is answered.
    hold: Duration,
}

struct Server {
    base_url: String,
    /// Arrival time of every request, in order.
    arrivals: Arc<Mutex<Vec<Instant>>>,
    /// Most requests in flight at once.
    max_in_flight: Arc<AtomicUsize>,
}

async fn serve(behaviour: Behaviour) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let arrivals = Arc::new(Mutex::new(Vec::<Instant>::new()));
    let in_flight = Arc::new(AtomicUsize::new(0));
    let max_in_flight = Arc::new(AtomicUsize::new(0));
    let (arrivals2, in_flight2, max2) = (
        Arc::clone(&arrivals),
        Arc::clone(&in_flight),
        Arc::clone(&max_in_flight),
    );
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let (b, arrivals, in_flight, max) = (
                behaviour.clone(),
                Arc::clone(&arrivals2),
                Arc::clone(&in_flight2),
                Arc::clone(&max2),
            );
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 4096];
                let (head_end, len) = loop {
                    let n = sock.read(&mut tmp).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..i]).to_string();
                        let len = head
                            .lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.eq_ignore_ascii_case("content-length")
                                    .then(|| v.trim().parse::<usize>().ok())
                                    .flatten()
                            })
                            .unwrap_or(0);
                        break (i + 4, len);
                    }
                };
                while buf.len() < head_end + len {
                    let n = sock.read(&mut tmp).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                }
                let nth = {
                    let mut a = arrivals.lock().unwrap();
                    a.push(Instant::now());
                    a.len()
                };
                let now_in_flight = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                max.fetch_max(now_in_flight, Ordering::SeqCst);
                if nth <= b.limited {
                    let extra = b
                        .retry_after
                        .as_ref()
                        .map(|(n, v)| format!("{n}: {v}\r\n"))
                        .unwrap_or_default();
                    let body = r#"{"error":{"message":"slow down"}}"#;
                    let _ = sock
                        .write_all(
                            format!(
                                "HTTP/1.1 429 Too Many Requests\r\ncontent-type: application/json\r\n{extra}content-length: {}\r\nconnection: close\r\n\r\n{body}",
                                body.len()
                            )
                            .as_bytes(),
                        )
                        .await;
                } else {
                    tokio::time::sleep(b.hold).await;
                    let frames = [
                        r#"{"id":"c","model":"m","choices":[{"index":0,"delta":{"content":"hi"},"finish_reason":null}]}"#,
                        r#"{"id":"c","model":"m","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":1}}"#,
                        "[DONE]",
                    ];
                    let _ = sock
                        .write_all(
                            b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\ntransfer-encoding: chunked\r\n\r\n",
                        )
                        .await;
                    for f in frames {
                        let frame = format!("data: {f}\n\n");
                        let _ = sock
                            .write_all(format!("{:x}\r\n{frame}\r\n", frame.len()).as_bytes())
                            .await;
                    }
                    let _ = sock.write_all(b"0\r\n\r\n").await;
                }
                let _ = sock.flush().await;
                let _ = sock.shutdown().await;
                in_flight.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    Server {
        base_url: format!("http://127.0.0.1:{port}"),
        arrivals,
        max_in_flight,
    }
}

fn gateway(server: &Server, max_concurrency: u32, max_retries: u32) -> ProviderGateway {
    ProviderGateway::new(vec![Endpoint {
        name: "ep".into(),
        kind: ProviderKind::OpenAi,
        base_url: server.base_url.clone(),
        credential: SecretHandle::None,
        models: vec![ModelCapability {
            model: "m".into(),
            context_tokens: 128_000,
            max_output_tokens: 8192,
            tools: true,
            parallel_tools: true,
            vision: false,
            input_modalities: vec!["text".into()],
            reasoning: false,
            structured_output: true,
            agent_loop: true,
            input_price_per_mtok: 1.0,
            output_price_per_mtok: 2.0,
            output_budget_tokens: 0,
            request_timeout_ms: 0,
            default_reasoning_effort: None,
            default_service_tier: None,
        }],
        max_retries,
        auth: AuthScheme::Native,
        extra_body: Default::default(),
        max_concurrency,
    }])
}

fn request(id: &str, timeout_ms: u64) -> ModelRequest {
    ModelRequest {
        request_id: id.into(),
        model_policy: ModelPolicy {
            endpoint: "ep".into(),
            model: "m".into(),
            reasoning_effort: None,
            service_tier: None,
        },
        messages: vec![Message::text(Role::User, "hi")],
        tool_projection: vec![],
        response_format: None,
        cache_key: None,
        cache_breakpoints: vec![],
        max_output_tokens: 64,
        timeout_ms,
        policy_tags: vec![],
    }
}

/// Run one request to its end: its events, and when the last one arrived.
async fn run(gw: &ProviderGateway, id: &str, timeout_ms: u64) -> (Vec<ModelEvent>, Instant) {
    let mut stream = gw
        .stream(
            request(id, timeout_ms),
            &Requirements::default(),
            CancellationToken::new(),
        )
        .unwrap();
    let mut events = Vec::new();
    while let Some(e) = stream.events.recv().await {
        let end = matches!(e, ModelEvent::Completed { .. } | ModelEvent::Error { .. });
        events.push(e);
        if end {
            break;
        }
    }
    (events, Instant::now())
}

fn completed(events: &[ModelEvent]) -> bool {
    events
        .iter()
        .any(|e| matches!(e, ModelEvent::Completed { .. }))
}

/// A 429 with `Retry-After: 1` is waited: the retry reaches the server a
/// second after the refusal, not after the 200-300 ms of jittered backoff the
/// gateway would use alone, and the request then completes.
#[tokio::test]
async fn a_429_with_retry_after_is_waited_not_hammered() {
    let server = serve(Behaviour {
        limited: 1,
        retry_after: Some(("retry-after", "1".into())),
        hold: Duration::ZERO,
    })
    .await;
    let gw = gateway(&server, 0, 3);
    let (events, _) = run(&gw, "r1", 30_000).await;
    assert!(completed(&events), "{events:?}");
    let arrivals = server.arrivals.lock().unwrap().clone();
    assert_eq!(arrivals.len(), 2, "one refused attempt, one retry");
    let gap = arrivals[1].duration_since(arrivals[0]);
    assert!(
        gap >= Duration::from_millis(950),
        "the retry came after {gap:?}, before the provider's Retry-After"
    );
    assert!(gap < Duration::from_secs(5), "{gap:?}");
    assert_eq!(gw.health("ep").rate_limited, 1);
}

/// The millisecond form OpenAI sends is honoured the same way.
#[tokio::test]
async fn retry_after_ms_is_honoured_too() {
    let server = serve(Behaviour {
        limited: 1,
        retry_after: Some(("retry-after-ms", "700".into())),
        hold: Duration::ZERO,
    })
    .await;
    let gw = gateway(&server, 0, 3);
    let (events, _) = run(&gw, "r1", 30_000).await;
    assert!(completed(&events), "{events:?}");
    let arrivals = server.arrivals.lock().unwrap().clone();
    assert!(
        arrivals[1].duration_since(arrivals[0]) >= Duration::from_millis(650),
        "{:?}",
        arrivals[1].duration_since(arrivals[0])
    );
}

/// The wait belongs to the endpoint: a request that starts after another was
/// told to wait does not send until the wait is over.
#[tokio::test]
async fn the_wait_is_endpoint_wide_not_per_request() {
    let server = serve(Behaviour {
        limited: 1,
        retry_after: Some(("retry-after", "1".into())),
        hold: Duration::ZERO,
    })
    .await;
    let gw = gateway(&server, 0, 3);
    let first = {
        let gw = gw.clone();
        tokio::spawn(async move { run(&gw, "a", 30_000).await })
    };
    // Let the first request be refused and the cooldown set.
    while server.arrivals.lock().unwrap().is_empty() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    tokio::time::sleep(Duration::from_millis(150)).await;
    let (second_events, _) = run(&gw, "b", 30_000).await;
    let (first_events, _) = first.await.unwrap();
    assert!(completed(&first_events) && completed(&second_events));
    let arrivals = server.arrivals.lock().unwrap().clone();
    assert_eq!(arrivals.len(), 3, "the refusal and one send per request");
    let t0 = arrivals[0];
    for (n, at) in arrivals.iter().enumerate().skip(1) {
        assert!(
            at.duration_since(t0) >= Duration::from_millis(950),
            "request {n} reached the endpoint {:?} after the refusal, inside its Retry-After",
            at.duration_since(t0)
        );
    }
}

/// A wait that would outlive the request is reported now, as the rate limit
/// it is, instead of sleeping until the deadline and calling it a timeout.
#[tokio::test]
async fn a_retry_after_longer_than_the_request_is_reported_at_once() {
    let server = serve(Behaviour {
        limited: usize::MAX,
        retry_after: Some(("retry-after", "3600".into())),
        hold: Duration::ZERO,
    })
    .await;
    let gw = gateway(&server, 0, 3);
    let started = Instant::now();
    let (events, _) = run(&gw, "r1", 5_000).await;
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    let Some(ModelEvent::Error {
        code,
        message,
        retryable,
    }) = events.last()
    else {
        panic!("{events:?}");
    };
    assert_eq!(code, "RATE_LIMITED");
    assert!(*retryable, "the caller may try again later");
    assert!(message.contains("asked for 3600 s"), "{message}");
    assert_eq!(server.arrivals.lock().unwrap().len(), 1, "no second send");
}

/// With no `Retry-After` the gateway's own bounded backoff is unchanged.
#[tokio::test]
async fn a_429_without_retry_after_keeps_the_bounded_backoff() {
    let server = serve(Behaviour {
        limited: 1,
        retry_after: None,
        hold: Duration::ZERO,
    })
    .await;
    let gw = gateway(&server, 0, 3);
    let (events, _) = run(&gw, "r1", 30_000).await;
    assert!(completed(&events), "{events:?}");
    let arrivals = server.arrivals.lock().unwrap().clone();
    let gap = arrivals[1].duration_since(arrivals[0]);
    assert!(gap < Duration::from_millis(900), "{gap:?}");
}

/// An endpoint configured for two concurrent requests is never sent a third:
/// six requests are served two at a time.
#[tokio::test]
async fn an_endpoint_is_never_asked_for_more_than_its_concurrency() {
    let server = serve(Behaviour {
        limited: 0,
        retry_after: None,
        hold: Duration::from_millis(300),
    })
    .await;
    let gw = gateway(&server, 2, 0);
    let started = Instant::now();
    let mut tasks = Vec::new();
    for n in 0..6 {
        let gw = gw.clone();
        tasks.push(tokio::spawn(async move {
            run(&gw, &format!("r{n}"), 30_000).await
        }));
    }
    for t in tasks {
        let (events, _) = t.await.unwrap();
        assert!(completed(&events), "{events:?}");
    }
    assert_eq!(
        server.max_in_flight.load(Ordering::SeqCst),
        2,
        "the endpoint saw exactly its limit at once, never more"
    );
    assert!(
        started.elapsed() >= Duration::from_millis(850),
        "three waves of 300 ms: {:?}",
        started.elapsed()
    );
    assert_eq!(server.arrivals.lock().unwrap().len(), 6);
}

/// A request queued for a slot gives up at its own deadline, and never reaches
/// the endpoint.
#[tokio::test]
async fn a_queued_request_never_outlives_its_deadline() {
    let server = serve(Behaviour {
        limited: 0,
        retry_after: None,
        hold: Duration::from_secs(2),
    })
    .await;
    let gw = gateway(&server, 1, 0);
    let holder = {
        let gw = gw.clone();
        tokio::spawn(async move { run(&gw, "holder", 30_000).await })
    };
    while server.arrivals.lock().unwrap().is_empty() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let started = Instant::now();
    let (events, _) = run(&gw, "queued", 500).await;
    assert!(
        started.elapsed() < Duration::from_millis(1500),
        "{:?}",
        started.elapsed()
    );
    assert!(
        matches!(events.last(), Some(ModelEvent::Error { code, .. }) if code == "TIMEOUT"),
        "{events:?}"
    );
    assert_eq!(
        server.arrivals.lock().unwrap().len(),
        1,
        "the queued request was never sent"
    );
    let (held, _) = holder.await.unwrap();
    assert!(completed(&held), "the request holding the slot finished");
}

/// `Retry-After` forms: seconds, fractional seconds, `retry-after-ms`, an
/// HTTP date (against a fixed clock), a date already past, and garbage.
#[test]
fn retry_after_is_read_in_every_form_a_provider_sends() {
    use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
    let headers = |name: &str, value: &str| {
        let mut h = HeaderMap::new();
        h.insert(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
        h
    };
    // 1994-11-06 08:49:37 GMT is 784111777 seconds after the epoch.
    let at = UNIX_EPOCH + Duration::from_secs(784_111_777);
    let ra = |n: &str, v: &str| retry_after_of(&headers(n, v), at);
    assert_eq!(ra("retry-after", "7"), Some(Duration::from_secs(7)));
    assert_eq!(ra("retry-after", " 0 "), Some(Duration::ZERO));
    assert_eq!(ra("retry-after", "1.5"), Some(Duration::from_millis(1500)));
    assert_eq!(
        ra("retry-after-ms", "250"),
        Some(Duration::from_millis(250))
    );
    assert_eq!(
        ra("retry-after", "Sun, 06 Nov 1994 08:49:47 GMT"),
        Some(Duration::from_secs(10))
    );
    assert_eq!(
        ra("retry-after", "Sun, 06 Nov 1994 08:49:00 GMT"),
        None,
        "a time already past asks for nothing"
    );
    assert_eq!(ra("retry-after", "-3"), None);
    assert_eq!(ra("retry-after", "soon"), None);
    assert_eq!(ra("x-other", "5"), None);
}
