//! A fault-injecting scripted provider server (PX-171), for both wire
//! formats: request `n` is answered by fault `n` of the plan (the last one
//! repeats), each fault a real HTTP exchange on a loopback socket. It is
//! the provider half of the failures docs/54 lists — a status with
//! `retry-after` / `retry-after-ms`, a body cut in the middle of an event,
//! an idle stall past the read timeout, a malformed event — and a server
//! that answers normally, so a suite can prove the recovery as well as the
//! failure. It is not a provider and proves nothing about one.
//!
//! Std only, so any test crate includes it by `#[path]`.
#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The wire format the server speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wire {
    /// OpenAI-compatible Chat Completions (`data:` chunks, `[DONE]`).
    OpenAi,
    /// Anthropic Messages (named events, `message_stop`).
    Anthropic,
}

/// What a successful answer says.
#[derive(Clone, Debug)]
pub enum Answer {
    /// Assistant text.
    Text(String),
    /// One tool call, with its arguments as JSON.
    Tool {
        /// Tool name.
        name: String,
        /// Arguments.
        args: serde_json::Value,
    },
}

/// How one request is answered.
#[derive(Clone, Debug)]
pub enum Fault {
    /// A whole streamed answer.
    Ok(Answer),
    /// A status with these extra headers and this JSON body (a 429 with
    /// `retry-after: 1`, a 503 with `retry-after-ms: 300`, …).
    Status {
        /// HTTP status.
        status: u16,
        /// Extra headers, `(name, value)`.
        headers: Vec<(String, String)>,
        /// Response body.
        body: String,
    },
    /// The answer's first `keep` events, then half of the next one, then
    /// the socket closes: a body cut in the middle of an event.
    CutMidEvent {
        /// The answer being cut.
        answer: Answer,
        /// Whole events sent before the cut.
        keep: usize,
    },
    /// The answer's first `keep` events, then the body ends cleanly (the
    /// chunked terminator) without the answer's end: a stream that stops
    /// short with no transport error.
    EndEarly {
        /// The answer that stops short.
        answer: Answer,
        /// Whole events sent before the end.
        keep: usize,
    },
    /// The answer's first `keep` events, then silence for `ms` with the
    /// connection open (an idle stall), then close.
    Stall {
        /// The answer that stalls.
        answer: Answer,
        /// Whole events sent before the stall.
        keep: usize,
        /// How long the server stays silent.
        ms: u64,
    },
    /// The answer's first `keep` events, then an event whose data is not
    /// JSON, then nothing more.
    Malformed {
        /// The answer before the bad event.
        answer: Answer,
        /// Whole events sent before it.
        keep: usize,
    },
}

/// One request the server received.
#[derive(Clone, Debug)]
pub struct Received {
    /// When it arrived.
    pub at: Instant,
    /// Its JSON body.
    pub body: serde_json::Value,
}

/// A running server.
pub struct FaultServer {
    /// `http://127.0.0.1:<port>`.
    pub base_url: String,
    /// Every request, in arrival order.
    pub received: Arc<Mutex<Vec<Received>>>,
}

impl FaultServer {
    /// How many requests arrived.
    pub fn count(&self) -> usize {
        self.received.lock().unwrap().len()
    }

    /// Arrival times, in order.
    pub fn arrivals(&self) -> Vec<Instant> {
        self.received.lock().unwrap().iter().map(|r| r.at).collect()
    }
}

/// The events of `answer` on `wire`, as `(event name, data)`.
pub fn events(wire: Wire, answer: &Answer) -> Vec<(Option<&'static str>, String)> {
    use serde_json::json;
    match (wire, answer) {
        (Wire::OpenAi, Answer::Text(t)) => vec![
            (None, json!({"id":"c","model":"m","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}).to_string()),
            (None, json!({"id":"c","model":"m","choices":[{"index":0,"delta":{"content":t},"finish_reason":null}]}).to_string()),
            (None, json!({"id":"c","model":"m","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":20,"completion_tokens":5}}).to_string()),
            (None, "[DONE]".to_owned()),
        ],
        (Wire::OpenAi, Answer::Tool { name, args }) => {
            let a = args.to_string();
            let (head, tail) = a.split_at(a.len() / 2);
            vec![
                (None, json!({"id":"c","model":"m","choices":[{"index":0,"delta":{"role":"assistant","content":"Working."},"finish_reason":null}]}).to_string()),
                (None, json!({"id":"c","model":"m","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_f","type":"function","function":{"name":name,"arguments":""}}]},"finish_reason":null}]}).to_string()),
                (None, json!({"id":"c","model":"m","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":head}}]},"finish_reason":null}]}).to_string()),
                (None, json!({"id":"c","model":"m","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":tail}}]},"finish_reason":null}]}).to_string()),
                (None, json!({"id":"c","model":"m","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":20,"completion_tokens":9}}).to_string()),
                (None, "[DONE]".to_owned()),
            ]
        }
        (Wire::Anthropic, Answer::Text(t)) => vec![
            (Some("message_start"), json!({"type":"message_start","message":{"id":"msg_f","model":"m","usage":{"input_tokens":20,"output_tokens":1}}}).to_string()),
            (Some("content_block_start"), json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}).to_string()),
            (Some("content_block_delta"), json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":t}}).to_string()),
            (Some("content_block_stop"), json!({"type":"content_block_stop","index":0}).to_string()),
            (Some("message_delta"), json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}).to_string()),
            (Some("message_stop"), json!({"type":"message_stop"}).to_string()),
        ],
        (Wire::Anthropic, Answer::Tool { name, args }) => {
            let a = args.to_string();
            let (head, tail) = a.split_at(a.len() / 2);
            vec![
                (Some("message_start"), json!({"type":"message_start","message":{"id":"msg_f","model":"m","usage":{"input_tokens":20,"output_tokens":1}}}).to_string()),
                (Some("content_block_start"), json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}).to_string()),
                (Some("content_block_delta"), json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Working."}}).to_string()),
                (Some("content_block_stop"), json!({"type":"content_block_stop","index":0}).to_string()),
                (Some("content_block_start"), json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_f","name":name,"input":{}}}).to_string()),
                (Some("content_block_delta"), json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":head}}).to_string()),
                (Some("content_block_delta"), json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":tail}}).to_string()),
                (Some("content_block_stop"), json!({"type":"content_block_stop","index":1}).to_string()),
                (Some("message_delta"), json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":12}}).to_string()),
                (Some("message_stop"), json!({"type":"message_stop"}).to_string()),
            ]
        }
    }
}

fn frame(event: Option<&str>, data: &str) -> String {
    let mut f = String::new();
    if let Some(e) = event {
        f.push_str(&format!("event: {e}\n"));
    }
    f.push_str(&format!("data: {data}\n\n"));
    f
}

fn chunk(s: &mut TcpStream, bytes: &str) -> bool {
    s.write_all(format!("{:x}\r\n{bytes}\r\n", bytes.len()).as_bytes())
        .is_ok()
        && s.flush().is_ok()
}

const SSE_HEAD: &str = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nx-request-id: req_fault\r\nconnection: close\r\ntransfer-encoding: chunked\r\n\r\n";

fn answer_with(s: &mut TcpStream, wire: Wire, fault: &Fault) {
    match fault {
        Fault::Status {
            status,
            headers,
            body,
        } => {
            let extra: String = headers
                .iter()
                .map(|(k, v)| format!("{k}: {v}\r\n"))
                .collect();
            let _ = s.write_all(
                format!(
                    "HTTP/1.1 {status} Fault\r\ncontent-type: application/json\r\n{extra}content-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            );
        }
        Fault::Ok(answer) => {
            let _ = s.write_all(SSE_HEAD.as_bytes());
            for (e, d) in events(wire, answer) {
                if !chunk(s, &frame(e, &d)) {
                    return;
                }
            }
            let _ = s.write_all(b"0\r\n\r\n");
        }
        Fault::CutMidEvent { answer, keep } => {
            let _ = s.write_all(SSE_HEAD.as_bytes());
            let ev = events(wire, answer);
            for (e, d) in ev.iter().take(*keep) {
                if !chunk(s, &frame(*e, d)) {
                    return;
                }
            }
            if let Some((e, d)) = ev.get(*keep) {
                let whole = frame(*e, d);
                let half = &whole[..whole.len() / 2];
                // A chunk header announcing the whole event, then half of it.
                let _ = s.write_all(format!("{:x}\r\n{half}", whole.len()).as_bytes());
                let _ = s.flush();
            }
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
        Fault::EndEarly { answer, keep } => {
            let _ = s.write_all(SSE_HEAD.as_bytes());
            for (e, d) in events(wire, answer).iter().take(*keep) {
                if !chunk(s, &frame(*e, d)) {
                    return;
                }
            }
            let _ = s.write_all(b"0\r\n\r\n");
        }
        Fault::Stall { answer, keep, ms } => {
            let _ = s.write_all(SSE_HEAD.as_bytes());
            for (e, d) in events(wire, answer).iter().take(*keep) {
                if !chunk(s, &frame(*e, d)) {
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(*ms));
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
        Fault::Malformed { answer, keep } => {
            let _ = s.write_all(SSE_HEAD.as_bytes());
            for (e, d) in events(wire, answer).iter().take(*keep) {
                if !chunk(s, &frame(*e, d)) {
                    return;
                }
            }
            let name = match wire {
                Wire::OpenAi => None,
                Wire::Anthropic => Some("content_block_delta"),
            };
            let _ = chunk(
                s,
                &frame(name, "{\"type\": \"content_block_delta\", \"delta\": {"),
            );
            let _ = s.write_all(b"0\r\n\r\n");
        }
    }
    let _ = s.flush();
}

/// Serve `plan` on `wire`. Request `n` gets `plan[n]`; past the end the last
/// fault repeats.
pub fn fault_provider(wire: Wire, plan: Vec<Fault>) -> FaultServer {
    assert!(!plan.is_empty(), "a plan needs at least one answer");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let received: Arc<Mutex<Vec<Received>>> = Arc::default();
    let rec = Arc::clone(&received);
    let plan = Arc::new(plan);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { break };
            let rec = Arc::clone(&rec);
            let plan = Arc::clone(&plan);
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 8192];
                let (head_end, len) = loop {
                    let n = s.read(&mut tmp).unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..pos]).to_string();
                        let len = head
                            .lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.eq_ignore_ascii_case("content-length")
                                    .then(|| v.trim().parse::<usize>().ok())
                                    .flatten()
                            })
                            .unwrap_or(0);
                        break (pos + 4, len);
                    }
                };
                while buf.len() < head_end + len {
                    let n = s.read(&mut tmp).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                }
                let body = serde_json::from_slice(&buf[head_end..(head_end + len).min(buf.len())])
                    .unwrap_or(serde_json::Value::Null);
                let nth = {
                    let mut r = rec.lock().unwrap();
                    r.push(Received {
                        at: Instant::now(),
                        body,
                    });
                    r.len() - 1
                };
                let fault = plan[nth.min(plan.len() - 1)].clone();
                answer_with(&mut s, wire, &fault);
            });
        }
    });
    FaultServer {
        base_url: format!("http://127.0.0.1:{port}"),
        received,
    }
}
