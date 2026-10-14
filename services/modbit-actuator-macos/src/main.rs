//! `modbit-actuator-macos` - the macOS actuator helper (PX-071; docs/66,
//! DR-PX-2026-10-03-008).
//!
//! A separate process that holds the operating-system permissions
//! (Accessibility, Screen Recording) and nothing else: no model, no policy,
//! no credential, no filesystem authority. The Core launches it and talks to
//! it over its standard input and output in length-prefixed `ActuatorFrame`s
//! (`crates/protocol/proto/modbit/v1/computer.proto`): the descriptors are
//! the Core's own, so there is no endpoint another process could squat
//! (CUC-E02). Before serving anything it
//!
//! * requires a `Hello` as the very first frame, at this protocol's major
//!   version, with a 32-byte per-launch token;
//! * verifies its parent process is the `core_pid` the hello names;
//! * when the Core pinned the launch (`MODBIT_ACTUATOR_TOKEN_SHA256`),
//!   requires the token to hash to it;
//!
//! and otherwise answers a `Welcome` carrying a `refusal` and exits. It stops
//! injecting and exits the moment the Core's stream ends (CUC-D05: "a Core
//! disconnect") or its parent changes.
//!
//! The same RPC, error codes and masking contract as the test fixture
//! `modbit-actuator-fixture`; the difference is that this one touches the
//! real machine (macOS only; other platforms answer `UNSUPPORTED_REQUEST`).

// Off macOS the backend is the unsupported stub: the parts of the portable
// layer that only the macOS backend calls (cancellation, the secure-region
// fields, ...) are legitimately unused there.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

mod backend;
mod canvas;
#[cfg(target_os = "macos")]
mod mac;
mod session;
#[cfg(not(target_os = "macos"))]
mod unsupported;

use std::sync::Arc;
use std::time::Duration;

use modbit_protocol::framing::{read_message, write_message};
use modbit_protocol::v1 as wire;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::backend::Backend;
use crate::session::{Denied, Hub};

/// The protocol major version this helper speaks.
const PROTOCOL_MAJOR: u32 = 1;
/// The minor version.
const PROTOCOL_MINOR: u32 = 0;
/// Version string for the welcome; the supply-chain manifest pins the file.
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn proof(token: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b"modbit-actuator-welcome\0");
    h.update(token);
    hex::encode(h.finalize())
}

/// Why this helper must refuse the Core that said hello ("" = accept).
fn vet_hello(h: &wire::ActuatorHello, parent: Option<u32>, pinned_hash: Option<&str>) -> String {
    if h.protocol_major != PROTOCOL_MAJOR {
        return format!(
            "protocol major {} is not {PROTOCOL_MAJOR}",
            h.protocol_major
        );
    }
    if h.token.len() != 32 {
        return "the launch token is not 32 bytes".into();
    }
    if let Some(want) = pinned_hash
        && hex::encode(Sha256::digest(&h.token)) != want.to_ascii_lowercase()
    {
        return "the launch token is not the one this launch was pinned to".into();
    }
    if let Some(p) = parent
        && p != h.core_pid
    {
        return format!(
            "the parent process is {p} and the hello names {}",
            h.core_pid
        );
    }
    String::new()
}

#[cfg(unix)]
fn parent_pid() -> Option<u32> {
    Some(std::os::unix::process::parent_id())
}

#[cfg(not(unix))]
fn parent_pid() -> Option<u32> {
    None
}

fn make_backend() -> Arc<dyn Backend> {
    #[cfg(target_os = "macos")]
    {
        Arc::new(mac::Mac::new())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Arc::new(unsupported::Unsupported)
    }
}

fn denied_from_env() -> Denied {
    std::env::var("MODBIT_ACTUATOR_DENY")
        .map(|v| Denied::parse(&v))
        .unwrap_or_default()
}

#[tokio::main]
async fn main() {
    // One command-line flag at most; the process is otherwise driven over its
    // standard streams by the Core that launched it.
    if let Some(a) = std::env::args().nth(1) {
        match a.as_str() {
            "--version" => {
                println!(
                    "modbit-actuator-macos {VERSION} protocol {PROTOCOL_MAJOR}.{PROTOCOL_MINOR}"
                );
            }
            // Print the permissions held and exit: the first-run window of
            // PX-074 and the test runner use it. Asks for nothing.
            "--check-permissions" => {
                let b = make_backend();
                let p = b.permissions();
                let d = denied_from_env();
                println!(
                    "{{\"platform\":\"{}\",\"accessibility\":{},\"screen_recording\":{},\"secure_desktop\":{}}}",
                    std::env::consts::OS,
                    p.accessibility && !d.accessibility,
                    p.screen_recording && !d.screen_recording,
                    b.secure_desktop()
                );
            }
            // Ask the OS to show the permission prompts, from THIS process
            // (QUAL-PX-071: the prompt comes from the helper, never from
            // another process), then exit.
            "--request-permissions" => {
                #[cfg(target_os = "macos")]
                mac::request_permissions();
            }
            other => {
                eprintln!("modbit-actuator-macos: unknown argument {other}");
                std::process::exit(2);
            }
        }
        return;
    }

    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();

    // The first frame must be the Core's hello; nothing is served before it.
    let hello: wire::ActuatorFrame = match read_message(&mut stdin).await {
        Ok(Some(f)) => f,
        _ => std::process::exit(3),
    };
    let Some(wire::actuator_frame::Body::Hello(h)) = hello.body else {
        std::process::exit(3);
    };
    let pinned = std::env::var("MODBIT_ACTUATOR_TOKEN_SHA256").ok();
    let refusal = vet_hello(&h, parent_pid(), pinned.as_deref());

    let backend = make_backend();
    let welcome = wire::ActuatorFrame {
        body: Some(wire::actuator_frame::Body::Welcome(wire::ActuatorWelcome {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            actuator_name: backend.name().into(),
            actuator_version: VERSION.into(),
            pid: std::process::id(),
            platform: backend.platform().into(),
            // A refused Core gets no proof of its token.
            token_proof: if refusal.is_empty() {
                proof(&h.token)
            } else {
                String::new()
            },
            capabilities: backend.capabilities(),
            refusal: refusal.clone(),
        })),
    };
    if write_message(&mut stdout, &welcome).await.is_err() || !refusal.is_empty() {
        std::process::exit(4);
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel::<wire::ActuatorFrame>(64);
    tokio::spawn(async move {
        while let Some(f) = rx.recv().await {
            if write_message(&mut stdout, &f).await.is_err() {
                std::process::exit(0);
            }
            let _ = stdout.flush().await;
        }
    });

    let hub = Hub::new(backend, tx.clone(), denied_from_env());
    hub.spawn_monitors();

    // The Core's parent changing means the Core is gone even if a stray
    // descriptor keeps the pipe open: stop and leave.
    let core_pid = h.core_pid;
    let hub_w = Arc::clone(&hub);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if let Some(p) = parent_pid()
                && p != core_pid
            {
                let _ = hub_w.stop().await;
                std::process::exit(0);
            }
        }
    });

    while let Ok(Some(frame)) = read_message::<_, wire::ActuatorFrame>(&mut stdin).await {
        match frame.body {
            Some(wire::actuator_frame::Body::Request(req)) => {
                let hub = Arc::clone(&hub);
                let tx = tx.clone();
                tokio::spawn(async move {
                    let resp = hub.handle(req).await;
                    let _ = tx
                        .send(wire::ActuatorFrame {
                            body: Some(wire::actuator_frame::Body::Response(resp)),
                        })
                        .await;
                });
            }
            // A second hello, or any frame only the actuator sends, is a
            // protocol violation by whoever is on the other end.
            Some(
                wire::actuator_frame::Body::Hello(_)
                | wire::actuator_frame::Body::Welcome(_)
                | wire::actuator_frame::Body::Response(_)
                | wire::actuator_frame::Body::Event(_),
            ) => {
                let _ = hub.stop().await;
                std::process::exit(5);
            }
            None => {}
        }
    }
    // The Core's stream ended: stop injecting at once and leave (CUC-D05).
    // Dropping the process ends any blocking injection thread with it.
    let _ = tokio::time::timeout(Duration::from_millis(200), hub.stop()).await;
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello(major: u32, token: Vec<u8>, pid: u32) -> wire::ActuatorHello {
        wire::ActuatorHello {
            protocol_major: major,
            protocol_minor: 0,
            token,
            core_pid: pid,
            core_version: "t".into(),
        }
    }

    #[test]
    fn a_hello_is_accepted_only_with_the_right_major_token_length_parent_and_pin() {
        let t = vec![9u8; 32];
        assert_eq!(vet_hello(&hello(1, t.clone(), 7), Some(7), None), "");
        assert!(vet_hello(&hello(2, t.clone(), 7), Some(7), None).contains("protocol major"));
        assert!(vet_hello(&hello(1, vec![1; 31], 7), Some(7), None).contains("32 bytes"));
        assert!(vet_hello(&hello(1, t.clone(), 7), Some(8), None).contains("parent"));
        let good = hex::encode(Sha256::digest(&t));
        assert_eq!(vet_hello(&hello(1, t.clone(), 7), Some(7), Some(&good)), "");
        assert!(vet_hello(&hello(1, t, 7), Some(7), Some(&"0".repeat(64))).contains("pinned"));
    }
}
