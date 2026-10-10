//! `modbit-actuator-fixture` - a real actuator process over an in-memory
//! desktop (PX-069; docs/66).
//!
//! It speaks the same authenticated local RPC as the macOS helper of PX-071
//! (`crates/protocol/proto/modbit/v1/computer.proto`) over its standard input
//! and output, so every path of the Core's Computer Runtime is proven over a
//! real process boundary on macOS, Linux and Windows with no display and no
//! operating-system permission. The desktop is a JSON document (`--desktop`)
//! of applications, windows and elements, with a few behaviours an element
//! can have - including the ones tests need: a crash after an input was
//! applied and before any answer, a stalled answer, a physical key, a swapped
//! application identity, the secure desktop.
//!
//! This is a test fixture. It makes no claim to control a real machine.

#![forbid(unsafe_code)]

mod desktop;
mod render;
mod rpc;

use std::sync::{Arc, Mutex};

use modbit_protocol::framing::{read_message, write_message};
use modbit_protocol::v1 as wire;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

const PROTOCOL_MAJOR: u32 = 1;

fn proof(token: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b"modbit-actuator-welcome\0");
    h.update(token);
    hex::encode(h.finalize())
}

#[tokio::main]
async fn main() {
    let mut desktop_path: Option<String> = None;
    let mut pid_file: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--desktop" => desktop_path = args.next(),
            "--pid-file" => pid_file = args.next(),
            other => {
                eprintln!("modbit-actuator-fixture: unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    let desktop = match desktop_path {
        Some(p) => match std::fs::read_to_string(&p)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str::<desktop::Desktop>(&t).map_err(|e| e.to_string()))
        {
            Ok(d) => d,
            Err(e) => {
                eprintln!("modbit-actuator-fixture: desktop {p}: {e}");
                std::process::exit(2);
            }
        },
        None => desktop::default_desktop(),
    };
    if let Some(p) = pid_file {
        let _ = std::fs::write(p, std::process::id().to_string());
    }

    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();

    // The first frame is the Core's hello; the answer proves its token.
    let hello: wire::ActuatorFrame = match read_message(&mut stdin).await {
        Ok(Some(f)) => f,
        _ => std::process::exit(3),
    };
    let Some(wire::actuator_frame::Body::Hello(h)) = hello.body else {
        std::process::exit(3);
    };
    let mut refusal = String::new();
    if h.protocol_major != PROTOCOL_MAJOR {
        refusal = format!(
            "protocol major {} is not {PROTOCOL_MAJOR}",
            h.protocol_major
        );
    }
    if h.token.len() != 32 {
        refusal = "the launch token is not 32 bytes".into();
    }
    #[cfg(unix)]
    if std::os::unix::process::parent_id() != h.core_pid {
        refusal = format!(
            "the parent process is {} and the hello names {}",
            std::os::unix::process::parent_id(),
            h.core_pid
        );
    }
    let welcome = wire::ActuatorFrame {
        body: Some(wire::actuator_frame::Body::Welcome(wire::ActuatorWelcome {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: 0,
            actuator_name: "modbit-actuator-fixture".into(),
            actuator_version: env!("CARGO_PKG_VERSION").into(),
            pid: std::process::id(),
            platform: "fixture".into(),
            token_proof: proof(&h.token),
            capabilities: vec![
                "app_scope".into(),
                "screen_scope".into(),
                "screenshot".into(),
                "secure_field_masking".into(),
                "human_input_detection".into(),
                "stop_overlay".into(),
            ],
            refusal: refusal.clone(),
        })),
    };
    if write_message(&mut stdout, &welcome).await.is_err() || !refusal.is_empty() {
        std::process::exit(4);
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel::<wire::ActuatorFrame>(64);
    let writer = tokio::spawn(async move {
        while let Some(f) = rx.recv().await {
            if write_message(&mut stdout, &f).await.is_err() {
                break;
            }
            let _ = stdout.flush().await;
        }
    });

    let state = Arc::new(Mutex::new(rpc::State::new(desktop)));
    rpc::spawn_lease_watchdog(Arc::clone(&state));

    loop {
        match read_message::<_, wire::ActuatorFrame>(&mut stdin).await {
            Ok(Some(frame)) => {
                if let Some(wire::actuator_frame::Body::Request(req)) = frame.body {
                    let st = Arc::clone(&state);
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        let resp = rpc::handle(&st, &tx, req).await;
                        let _ = tx
                            .send(wire::ActuatorFrame {
                                body: Some(wire::actuator_frame::Body::Response(resp)),
                            })
                            .await;
                    });
                }
            }
            // The Core is gone: stop injecting and leave (CUC-D05).
            _ => break,
        }
    }
    // Handlers may still be stalled on purpose; the process ends now.
    writer.abort();
    std::process::exit(0);
}
