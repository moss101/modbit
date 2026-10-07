//! OpenTelemetry export over OTLP/HTTP+JSON (REQ-PX-139, docs/34).
//!
//! Spans and metrics are *derived* from the canonical log and the accounting
//! record by the Core; this module only encodes, bounds and ships them. It
//! is never a second source of truth: nothing here is read back to decide
//! anything.
//!
//! What it guarantees:
//!
//! * **Off by default.** Nothing is built, queued or sent until a Core hands
//!   it a configured endpoint.
//! * **Bounded.** A span carries at most [`MAX_ATTRS`] attributes of at most
//!   [`MAX_VALUE_BYTES`] bytes each; the queue holds at most `queue_spans`
//!   spans and drops (and counts) the oldest when it is full; a request body
//!   is cut to `max_body_bytes` worth of spans.
//! * **Redacted.** Every string that leaves passes the caller's redactor.
//!   Attribute keys are an allow-list shape (`modbit.*`, `gen_ai.*`,
//!   `service.*`); the Core only ever offers ids, enums, counts, durations
//!   and costs — never a prompt, a file body, a tool argument or a path.
//! * **Never blocking.** The exporter runs on a task of its own with hard
//!   timeouts; a slow, absent or failing collector costs the run nothing and
//!   every loss is counted, not hidden.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

/// Most attributes one span or data point carries.
pub const MAX_ATTRS: usize = 32;
/// Longest attribute value, in bytes.
pub const MAX_VALUE_BYTES: usize = 256;
/// Longest span or metric name, in bytes.
pub const MAX_NAME_BYTES: usize = 64;

/// A value of an attribute.
#[derive(Clone, Debug, PartialEq)]
pub enum Attr {
    /// Text (bounded and redacted when encoded).
    Str(String),
    /// A count, a duration or an amount in minor units.
    Int(i64),
    /// A ratio.
    Double(f64),
    /// A flag.
    Bool(bool),
}

impl From<&str> for Attr {
    fn from(s: &str) -> Self {
        Attr::Str(s.to_owned())
    }
}
impl From<String> for Attr {
    fn from(s: String) -> Self {
        Attr::Str(s)
    }
}
impl From<u64> for Attr {
    fn from(v: u64) -> Self {
        Attr::Int(i64::try_from(v).unwrap_or(i64::MAX))
    }
}
impl From<u32> for Attr {
    fn from(v: u32) -> Self {
        Attr::Int(i64::from(v))
    }
}
impl From<bool> for Attr {
    fn from(v: bool) -> Self {
        Attr::Bool(v)
    }
}

/// One span.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    /// The trace (16 bytes).
    pub trace_id: [u8; 16],
    /// This span (8 bytes).
    pub span_id: [u8; 8],
    /// Its parent, absent for a root.
    pub parent_span_id: Option<[u8; 8]>,
    /// Name, e.g. `modbit.run`.
    pub name: String,
    /// Start, nanoseconds since the Unix epoch.
    pub start_ns: u64,
    /// End, nanoseconds since the Unix epoch.
    pub end_ns: u64,
    /// Attributes, in order.
    pub attrs: Vec<(String, Attr)>,
    /// Whether the work succeeded (`false` is an error status).
    pub ok: bool,
}

/// How a metric's points add up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricKind {
    /// A monotonic cumulative sum (tokens, cost).
    Sum,
    /// A value at a moment (latency, queue depth).
    Gauge,
}

/// One data point of a metric.
#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    /// Attributes (e.g. the token type).
    pub attrs: Vec<(String, Attr)>,
    /// The value.
    pub value: f64,
    /// Whether the value is integral (encoded as `asInt`).
    pub integral: bool,
}

/// One metric.
#[derive(Clone, Debug, PartialEq)]
pub struct Metric {
    /// Name, e.g. `modbit.tokens`.
    pub name: String,
    /// Unit, e.g. `{token}`.
    pub unit: String,
    /// Sum or gauge.
    pub kind: MetricKind,
    /// Its points.
    pub points: Vec<Point>,
}

/// Cut `s` to `max` bytes on a character boundary.
#[must_use]
pub fn bounded(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_owned()
}

/// Whether `key` is an attribute key this exporter will send.
#[must_use]
pub fn key_allowed(key: &str) -> bool {
    let shaped = !key.is_empty()
        && key.len() <= MAX_NAME_BYTES
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_'));
    shaped
        && ["modbit.", "gen_ai.", "service.", "telemetry.", "http."]
            .iter()
            .any(|p| key.starts_with(p))
}

fn attr_json(key: &str, v: &Attr, redact: &dyn Fn(&str) -> String) -> Value {
    let value = match v {
        Attr::Str(s) => json!({"stringValue": bounded(&redact(s), MAX_VALUE_BYTES)}),
        Attr::Int(i) => json!({"intValue": i.to_string()}),
        Attr::Double(d) if d.is_finite() => json!({"doubleValue": d}),
        Attr::Double(_) => json!({"stringValue": "NaN"}),
        Attr::Bool(b) => json!({"boolValue": b}),
    };
    json!({"key": key, "value": value})
}

fn attrs_json(attrs: &[(String, Attr)], redact: &dyn Fn(&str) -> String) -> Vec<Value> {
    attrs
        .iter()
        .filter(|(k, _)| key_allowed(k))
        .take(MAX_ATTRS)
        .map(|(k, v)| attr_json(k, v, redact))
        .collect()
}

fn resource_json(service_name: &str, extra: &[(String, Attr)]) -> Value {
    let mut attrs = vec![
        json!({"key": "service.name", "value": {"stringValue": service_name}}),
        json!({"key": "telemetry.sdk.name", "value": {"stringValue": "modbit"}}),
    ];
    attrs.extend(
        extra
            .iter()
            .filter(|(k, _)| key_allowed(k))
            .map(|(k, v)| attr_json(k, v, &|s: &str| s.to_owned())),
    );
    json!({"attributes": attrs})
}

fn span_json(s: &Span, redact: &dyn Fn(&str) -> String) -> Value {
    let mut v = json!({
        "traceId": hex::encode(s.trace_id),
        "spanId": hex::encode(s.span_id),
        "name": bounded(&redact(&s.name), MAX_NAME_BYTES),
        "kind": 1,
        "startTimeUnixNano": s.start_ns.to_string(),
        "endTimeUnixNano": s.end_ns.max(s.start_ns).to_string(),
        "attributes": attrs_json(&s.attrs, redact),
        "status": {"code": if s.ok { 1 } else { 2 }},
    });
    if let Some(p) = s.parent_span_id {
        v["parentSpanId"] = json!(hex::encode(p));
    }
    v
}

/// The OTLP/HTTP JSON body of a trace export.
#[must_use]
pub fn encode_traces(
    service_name: &str,
    resource: &[(String, Attr)],
    spans: &[Span],
    redact: &dyn Fn(&str) -> String,
) -> Vec<u8> {
    let body = json!({
        "resourceSpans": [{
            "resource": resource_json(service_name, resource),
            "scopeSpans": [{
                "scope": {"name": "modbit", "version": "1"},
                "spans": spans.iter().map(|s| span_json(s, redact)).collect::<Vec<_>>(),
            }],
        }],
    });
    serde_json::to_vec(&body).unwrap_or_default()
}

/// The OTLP/HTTP JSON body of a metrics export.
#[must_use]
pub fn encode_metrics(
    service_name: &str,
    resource: &[(String, Attr)],
    metrics: &[Metric],
    now_ns: u64,
    redact: &dyn Fn(&str) -> String,
) -> Vec<u8> {
    let metric_json = |m: &Metric| {
        let points: Vec<Value> = m
            .points
            .iter()
            .map(|p| {
                let mut v = json!({
                    "attributes": attrs_json(&p.attrs, redact),
                    "timeUnixNano": now_ns.to_string(),
                });
                if p.integral {
                    // OTLP JSON carries 64-bit integers as strings.
                    v["asInt"] = json!((p.value as i64).to_string());
                } else {
                    v["asDouble"] = json!(p.value);
                }
                v
            })
            .collect();
        let mut out = json!({
            "name": bounded(&m.name, MAX_NAME_BYTES),
            "unit": m.unit,
        });
        match m.kind {
            MetricKind::Sum => {
                out["sum"] = json!({
                    // CUMULATIVE
                    "aggregationTemporality": 2,
                    "isMonotonic": true,
                    "dataPoints": points,
                });
            }
            MetricKind::Gauge => out["gauge"] = json!({"dataPoints": points}),
        }
        out
    };
    let body = json!({
        "resourceMetrics": [{
            "resource": resource_json(service_name, resource),
            "scopeMetrics": [{
                "scope": {"name": "modbit", "version": "1"},
                "metrics": metrics.iter().map(metric_json).collect::<Vec<_>>(),
            }],
        }],
    });
    serde_json::to_vec(&body).unwrap_or_default()
}

// ----------------------------------------------------------------- the queue

/// What the exporter did, for the health endpoint and the tests. Every loss
/// is a number here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportStats {
    /// Spans handed to the queue.
    pub spans_queued: u64,
    /// Spans the collector accepted.
    pub spans_sent: u64,
    /// Spans dropped because the queue was full.
    pub spans_dropped_overflow: u64,
    /// Spans dropped after their attempts failed.
    pub spans_lost: u64,
    /// Requests the collector accepted.
    pub requests_ok: u64,
    /// Requests that failed (timeout, refused, non-2xx).
    pub requests_failed: u64,
    /// Metric exports the collector accepted.
    pub metric_exports_ok: u64,
    /// Metric exports that failed.
    pub metric_exports_failed: u64,
    /// The last failure, redacted and bounded; empty when none.
    pub last_error: String,
    /// When the collector last accepted a request (ms since epoch, 0 = never).
    pub last_success_ms: i64,
    /// When the last attempt was made (ms since epoch, 0 = never).
    pub last_attempt_ms: i64,
    /// Spans waiting now.
    pub queue_depth: u64,
}

struct Pending {
    spans: Vec<Span>,
    attempts: u32,
}

/// The bounded queue between the Core's span builder and the sender.
pub struct ExportQueue {
    cap: usize,
    spans: Mutex<VecDeque<Span>>,
    retry: Mutex<Vec<Pending>>,
    metrics: Mutex<Vec<Metric>>,
    stats: Mutex<ExportStats>,
}

impl ExportQueue {
    /// A queue holding at most `cap` spans.
    #[must_use]
    pub fn new(cap: usize) -> Arc<Self> {
        Arc::new(Self {
            cap: cap.max(1),
            spans: Mutex::new(VecDeque::new()),
            retry: Mutex::new(Vec::new()),
            metrics: Mutex::new(Vec::new()),
            stats: Mutex::new(ExportStats::default()),
        })
    }

    fn stats_mut(&self) -> std::sync::MutexGuard<'_, ExportStats> {
        self.stats.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Queue spans; when the queue is full the oldest are dropped and counted.
    pub fn push(&self, spans: Vec<Span>) {
        let n = spans.len() as u64;
        let mut q = self.spans.lock().unwrap_or_else(|e| e.into_inner());
        let mut dropped = 0u64;
        for s in spans {
            if q.len() >= self.cap {
                q.pop_front();
                dropped += 1;
            }
            q.push_back(s);
        }
        let depth = q.len() as u64;
        drop(q);
        let mut st = self.stats_mut();
        st.spans_queued += n;
        st.spans_dropped_overflow += dropped;
        st.queue_depth = depth;
    }

    /// Replace the metrics the next metrics export carries.
    pub fn set_metrics(&self, metrics: Vec<Metric>) {
        *self.metrics.lock().unwrap_or_else(|e| e.into_inner()) = metrics;
    }

    /// A copy of the counters.
    #[must_use]
    pub fn stats(&self) -> ExportStats {
        let mut s = self.stats_mut().clone();
        s.queue_depth = self.spans.lock().unwrap_or_else(|e| e.into_inner()).len() as u64
            + self
                .retry
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .map(|p| p.spans.len() as u64)
                .sum::<u64>();
        s
    }

    fn take_batch(&self, max: usize) -> Option<Pending> {
        // Spans that failed before go first.
        {
            let mut r = self.retry.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(p) = r.pop() {
                return Some(p);
            }
        }
        let mut q = self.spans.lock().unwrap_or_else(|e| e.into_inner());
        if q.is_empty() {
            return None;
        }
        let n = q.len().min(max.max(1));
        Some(Pending {
            spans: q.drain(..n).collect(),
            attempts: 0,
        })
    }
}

/// How the exporter reaches the collector.
#[derive(Clone, Debug)]
pub struct OtlpConfig {
    /// The collector's base URL (`http://host:4318`); `/v1/traces` and
    /// `/v1/metrics` are appended.
    pub endpoint: String,
    /// `service.name`.
    pub service_name: String,
    /// How often the sender wakes.
    pub interval: Duration,
    /// Most spans in one request.
    pub batch_spans: usize,
    /// Most bytes of one request body.
    pub max_body_bytes: usize,
    /// The whole of one request, connect included.
    pub request_timeout: Duration,
    /// Attempts a batch gets before it is counted lost.
    pub max_attempts: u32,
    /// Extra resource attributes (ids of the Core, never secrets).
    pub resource: Vec<(String, Attr)>,
}

impl OtlpConfig {
    /// Defaults for a collector at `endpoint`.
    #[must_use]
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            service_name: "modbit-core".into(),
            interval: Duration::from_secs(5),
            batch_spans: 256,
            max_body_bytes: 1 << 20,
            request_timeout: Duration::from_secs(2),
            max_attempts: 3,
            resource: vec![],
        }
    }
}

/// Extra request headers, computed at each send so a rotated or revoked
/// credential applies to the next request. Values are never logged.
pub type Headers = Arc<dyn Fn() -> Vec<(String, String)> + Send + Sync>;

/// Redacts a string before it leaves.
pub type Redact = Arc<dyn Fn(&str) -> String + Send + Sync>;

fn now_ms() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis()),
    )
    .unwrap_or(0)
}

fn now_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
}

/// POST `body` to `url`; the error is already redacted and bounded.
async fn post(
    client: &reqwest::Client,
    url: &str,
    body: Vec<u8>,
    headers: &[(String, String)],
    redact: &(dyn Fn(&str) -> String + Send + Sync),
) -> Result<(), String> {
    let mut req = client
        .post(url)
        .header("content-type", "application/json")
        .body(body);
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    match req.send().await {
        Ok(r) if r.status().is_success() => Ok(()),
        Ok(r) => Err(format!("collector answered HTTP {}", r.status().as_u16())),
        Err(e) => Err(bounded(&redact(&e.without_url().to_string()), 200)),
    }
}

/// Run the sender until the process ends: each tick it ships one batch of
/// spans (retrying a failed batch up to `max_attempts` times, then counting
/// it lost) and the latest metrics. Every await is bounded by the request
/// timeout, and nothing here is ever awaited by the Core's run loop.
pub async fn run_sender(
    queue: Arc<ExportQueue>,
    cfg: OtlpConfig,
    headers: Headers,
    redact: Redact,
) {
    let client = match reqwest::Client::builder()
        .timeout(cfg.request_timeout)
        .connect_timeout(cfg.request_timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            queue.stats_mut().last_error = format!("exporter not started: {e}");
            return;
        }
    };
    let base = cfg.endpoint.trim_end_matches('/').to_owned();
    let mut backoff = 0u32;
    loop {
        // A failing collector is asked less often, up to 30 s apart.
        let wait = cfg
            .interval
            .saturating_mul(1u32 << backoff.min(5))
            .min(Duration::from_secs(30).max(cfg.interval));
        tokio::time::sleep(wait).await;
        let hdrs = headers();
        let mut failed = false;
        if let Some(mut batch) = queue.take_batch(cfg.batch_spans) {
            let mut spans = std::mem::take(&mut batch.spans);
            // Cut the batch to the body cap; the rest waits for the next tick.
            let mut body = encode_traces(&cfg.service_name, &cfg.resource, &spans, &*redact);
            while body.len() > cfg.max_body_bytes && spans.len() > 1 {
                let keep = spans.len() / 2;
                let rest = spans.split_off(keep);
                queue
                    .retry
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(Pending {
                        spans: rest,
                        attempts: batch.attempts,
                    });
                body = encode_traces(&cfg.service_name, &cfg.resource, &spans, &*redact);
            }
            queue.stats_mut().last_attempt_ms = now_ms();
            match post(&client, &format!("{base}/v1/traces"), body, &hdrs, &*redact).await {
                Ok(()) => {
                    let mut st = queue.stats_mut();
                    st.requests_ok += 1;
                    st.spans_sent += spans.len() as u64;
                    st.last_success_ms = now_ms();
                    st.last_error.clear();
                }
                Err(e) => {
                    failed = true;
                    let mut st = queue.stats_mut();
                    st.requests_failed += 1;
                    st.last_error = e;
                    drop(st);
                    batch.attempts += 1;
                    if batch.attempts >= cfg.max_attempts {
                        queue.stats_mut().spans_lost += spans.len() as u64;
                    } else {
                        batch.spans = spans;
                        queue
                            .retry
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .push(batch);
                    }
                }
            }
        }
        let metrics = queue
            .metrics
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if !metrics.is_empty() && !failed {
            let body = encode_metrics(
                &cfg.service_name,
                &cfg.resource,
                &metrics,
                now_ns(),
                &*redact,
            );
            queue.stats_mut().last_attempt_ms = now_ms();
            match post(
                &client,
                &format!("{base}/v1/metrics"),
                body,
                &hdrs,
                &*redact,
            )
            .await
            {
                Ok(()) => {
                    let mut st = queue.stats_mut();
                    st.metric_exports_ok += 1;
                    st.last_success_ms = now_ms();
                }
                Err(e) => {
                    failed = true;
                    let mut st = queue.stats_mut();
                    st.metric_exports_failed += 1;
                    st.last_error = e;
                }
            }
        }
        backoff = if failed { backoff + 1 } else { 0 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(name: &str, attrs: Vec<(String, Attr)>) -> Span {
        Span {
            trace_id: [1; 16],
            span_id: [2; 8],
            parent_span_id: Some([3; 8]),
            name: name.into(),
            start_ns: 10,
            end_ns: 20,
            attrs,
            ok: true,
        }
    }

    #[test]
    fn a_span_encodes_as_otlp_json_with_hex_ids_and_string_integers() {
        let s = span(
            "modbit.run",
            vec![
                ("modbit.cost.minor".into(), Attr::Int(42)),
                ("modbit.state".into(), Attr::Str("done".into())),
            ],
        );
        let body = encode_traces("modbit-core", &[], &[s], &|x: &str| x.to_owned());
        let v: Value = serde_json::from_slice(&body).unwrap();
        let sp = &v["resourceSpans"][0]["scopeSpans"][0]["spans"][0];
        assert_eq!(sp["traceId"], "01".repeat(16));
        assert_eq!(sp["spanId"], "02".repeat(8));
        assert_eq!(sp["parentSpanId"], "03".repeat(8));
        assert_eq!(sp["startTimeUnixNano"], "10");
        assert_eq!(sp["attributes"][0]["value"]["intValue"], "42");
        assert_eq!(sp["attributes"][1]["value"]["stringValue"], "done");
        assert_eq!(sp["status"]["code"], 1);
    }

    #[test]
    fn attributes_are_allow_listed_bounded_and_redacted() {
        let secret = "sk-planted-secret-0123456789";
        let long = "x".repeat(10_000);
        let mut attrs: Vec<(String, Attr)> = vec![
            (
                "modbit.note".into(),
                Attr::Str(format!("key {secret} here")),
            ),
            ("modbit.long".into(), Attr::Str(long)),
            // Not in the allow-list shape: dropped, whatever it holds.
            ("prompt".into(), Attr::Str("the whole prompt".into())),
            ("Modbit.Upper".into(), Attr::Str("x".into())),
            ("other.thing".into(), Attr::Str("x".into())),
        ];
        for i in 0..100 {
            attrs.push((format!("modbit.n{i}"), Attr::Int(i)));
        }
        let redact = |s: &str| s.replace(secret, "[REDACTED]");
        let body = encode_traces("svc", &[], &[span("n", attrs)], &redact);
        let text = String::from_utf8(body.clone()).unwrap();
        assert!(!text.contains(secret));
        assert!(!text.contains("the whole prompt"));
        let v: Value = serde_json::from_slice(&body).unwrap();
        let out = v["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["attributes"]
            .as_array()
            .unwrap();
        assert!(out.len() <= MAX_ATTRS);
        assert!(
            out[0]["value"]["stringValue"]
                .as_str()
                .unwrap()
                .contains("[REDACTED]")
        );
        assert!(
            out[1]["value"]["stringValue"].as_str().unwrap().len() <= MAX_VALUE_BYTES,
            "values are bounded"
        );
        assert!(out.iter().all(|a| a["key"] != "prompt"));
    }

    #[test]
    fn metrics_encode_sums_and_gauges() {
        let m = vec![
            Metric {
                name: "modbit.tokens".into(),
                unit: "{token}".into(),
                kind: MetricKind::Sum,
                points: vec![Point {
                    attrs: vec![("modbit.token.type".into(), Attr::from("input"))],
                    value: 1200.0,
                    integral: true,
                }],
            },
            Metric {
                name: "modbit.queue.depth".into(),
                unit: "{span}".into(),
                kind: MetricKind::Gauge,
                points: vec![Point {
                    attrs: vec![],
                    value: 0.5,
                    integral: false,
                }],
            },
        ];
        let body = encode_metrics("svc", &[], &m, 7, &|s: &str| s.to_owned());
        let v: Value = serde_json::from_slice(&body).unwrap();
        let ms = &v["resourceMetrics"][0]["scopeMetrics"][0]["metrics"];
        assert_eq!(ms[0]["sum"]["isMonotonic"], true);
        assert_eq!(ms[0]["sum"]["dataPoints"][0]["asInt"], "1200");
        assert_eq!(ms[1]["gauge"]["dataPoints"][0]["asDouble"], 0.5);
    }

    #[test]
    fn a_full_queue_drops_the_oldest_and_counts_them() {
        let q = ExportQueue::new(3);
        q.push((0..5).map(|i| span(&format!("s{i}"), vec![])).collect());
        let st = q.stats();
        assert_eq!(st.spans_queued, 5);
        assert_eq!(st.spans_dropped_overflow, 2);
        assert_eq!(st.queue_depth, 3);
        let b = q.take_batch(10).unwrap();
        assert_eq!(b.spans[0].name, "s2", "the oldest two were dropped");
    }

    #[test]
    fn key_shapes() {
        assert!(key_allowed("modbit.cost.minor"));
        assert!(key_allowed("gen_ai.usage.input_tokens"));
        assert!(!key_allowed("prompt"));
        assert!(!key_allowed("modbit.Bad"));
        assert!(!key_allowed("modbit. space"));
        assert!(!key_allowed(""));
    }
}
