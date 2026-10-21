//! Append latency of one streamed delta at full or relaxed durability, with
//! `noise` other writers committing (and so fsyncing) on the same filesystem
//! (DR-PX-2026-10-21-015, the variant table). Not a test: it prints one line.
//!
//! usage: delta_bench <full|relaxed> <noise-threads> [deltas=230] [gap_ms=60]
//! `BENCH_DIR` names the directory the stores are made in (default: the temp dir).
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::stream::{Retention, StreamEvent, StreamKind};
use modbit_domain::{RunId, RunStepId, SessionId, StreamId, TaskId, TenantId, TurnId};
use modbit_event_store::{AppendRequest, Durability, EventStore, NewEvent};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

fn request(session: SessionId, stream: StreamId, sequence: u64, text: &str) -> AppendRequest {
    let e = StreamEvent::AssistantTextDelta {
        stream_id: stream,
        kind: StreamKind::Text,
        sequence,
        run_id: RunId::new(),
        turn_id: TurnId::new(),
        step_id: RunStepId::new(),
        text: text.into(),
        provenance: "MODEL_OUTPUT".into(),
        retention: Retention::CollapsibleOnComplete,
    };
    AppendRequest {
        tenant_id: TenantId::from_bytes([7; 16]),
        session_id: session,
        task_id: Some(TaskId::new()),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::AssistantStream,
        aggregate_id: *stream.as_bytes(),
        expected_sequence: None,
        events: vec![NewEvent::new(
            e.event_type(),
            serde_json::to_value(&e).expect("a delta serializes"),
            Actor::Core("bench".into()),
        )],
    }
}

fn store_dir(base: &Option<String>) -> tempfile::TempDir {
    match base {
        Some(b) => tempfile::tempdir_in(b),
        None => tempfile::tempdir(),
    }
    .expect("a directory for the store")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map_or("full", String::as_str);
    let relaxed = mode == "relaxed";
    let noise: usize = args.get(2).map_or(0, |v| v.parse().expect("noise threads"));
    let deltas: u64 = args.get(3).map_or(230, |v| v.parse().expect("deltas"));
    let gap: u64 = args.get(4).map_or(60, |v| v.parse().expect("gap_ms"));
    let base = std::env::var("BENCH_DIR").ok();
    let stop = Arc::new(AtomicBool::new(false));
    let neighbours: Vec<_> = (0..noise)
        .map(|_| {
            let (stop, base) = (Arc::clone(&stop), base.clone());
            std::thread::spawn(move || {
                let dir = store_dir(&base);
                let mut store = EventStore::open(dir.path()).expect("a neighbour store");
                let (session, stream) = (SessionId::new(), StreamId::new());
                let mut n = 0;
                while !stop.load(Ordering::Relaxed) {
                    n += 1;
                    let _ = store.append(request(session, stream, n, "noise noise noise "));
                    std::thread::sleep(Duration::from_millis(2));
                }
            })
        })
        .collect();
    let dir = store_dir(&base);
    let mut store = EventStore::open(dir.path()).expect("the store");
    let (session, stream) = (SessionId::new(), StreamId::new());
    let durability = if relaxed {
        Durability::Relaxed
    } else {
        Durability::Full
    };
    std::thread::sleep(Duration::from_millis(300));
    let mut ms = Vec::new();
    for n in 1..=deltas {
        std::thread::sleep(Duration::from_millis(gap));
        let req = request(session, stream, n, "alpha beta gamma delta epsilon ");
        let started = Instant::now();
        store.append_with(req, durability).expect("an append");
        ms.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    stop.store(true, Ordering::Relaxed);
    for n in neighbours {
        n.join().expect("a neighbour");
    }
    ms.sort_by(f64::total_cmp);
    let at = |q: f64| ms[((ms.len() as f64 * q) as usize).min(ms.len() - 1)];
    println!(
        "{mode} noise={noise} n={} append_ms p50={:.1} p95={:.1} p99={:.1} max={:.1} over100ms={}",
        ms.len(),
        at(0.5),
        at(0.95),
        at(0.99),
        ms[ms.len() - 1],
        ms.iter().filter(|v| **v > 100.0).count()
    );
}
