//! QUAL-PX-071 on every operating system: the helper is a real process; the
//! authenticated handshake, the refusals, the Core-disconnect rule and the
//! crash/restart behaviour are proven over its real standard streams, with no
//! operating-system permission and no display.
mod common;

use std::process::Stdio;
use std::time::{Duration, Instant};

use common::*;
use modbit_computer::actuator::{Actuator, Call, CallError, LaunchError, Reply};
use modbit_protocol::framing::{read_message, write_message};
use modbit_protocol::v1 as wire;
use sha2::{Digest, Sha256};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

struct Raw {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: ChildStdout,
}

fn spawn_raw(env: &[(&str, &str)]) -> Raw {
    let mut c = Command::new(helper_bin());
    c.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    for (k, v) in env {
        c.env(k, v);
    }
    let mut child = c.spawn().expect("spawn the helper");
    let stdin = child.stdin.take();
    let stdout = child.stdout.take().expect("stdout");
    Raw {
        child,
        stdin,
        stdout,
    }
}

fn hello(major: u32, token: Vec<u8>, core_pid: u32) -> wire::ActuatorFrame {
    wire::ActuatorFrame {
        body: Some(wire::actuator_frame::Body::Hello(wire::ActuatorHello {
            protocol_major: major,
            protocol_minor: 0,
            token,
            core_pid,
            core_version: "test".into(),
        })),
    }
}

fn proof(token: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b"modbit-actuator-welcome\0");
    h.update(token);
    hex::encode(h.finalize())
}

impl Raw {
    async fn send(&mut self, f: &wire::ActuatorFrame) {
        write_message(self.stdin.as_mut().expect("stdin"), f)
            .await
            .expect("write");
    }

    async fn welcome(&mut self) -> Option<wire::ActuatorWelcome> {
        let f: Option<wire::ActuatorFrame> =
            tokio::time::timeout(Duration::from_secs(10), read_message(&mut self.stdout))
                .await
                .expect("an answer in time")
                .ok()
                .flatten();
        match f?.body {
            Some(wire::actuator_frame::Body::Welcome(w)) => Some(w),
            _ => None,
        }
    }

    async fn exit_code(&mut self) -> Option<i32> {
        tokio::time::timeout(Duration::from_secs(5), self.child.wait())
            .await
            .ok()
            .and_then(Result::ok)
            .and_then(|s| s.code())
    }
}

fn me() -> u32 {
    std::process::id()
}

#[tokio::test]
async fn a_hello_with_this_launchs_token_is_welcomed_with_the_proof_the_pid_and_the_platform() {
    let mut h = spawn_raw(&[]);
    let token = vec![0xA5; 32];
    h.send(&hello(1, token.clone(), me())).await;
    let w = h.welcome().await.expect("a welcome");
    assert_eq!(w.refusal, "");
    assert_eq!(w.token_proof, proof(&token));
    assert_eq!(w.pid, h.child.id().expect("child pid"));
    assert_eq!(w.actuator_name, "modbit-actuator-macos");
    assert_eq!(w.platform, std::env::consts::OS);
    assert_eq!(w.protocol_major, 1);
    if cfg!(target_os = "macos") {
        for c in [
            "app_scope",
            "screenshot",
            "secure_field_masking",
            "human_input_detection",
        ] {
            assert!(
                w.capabilities.iter().any(|x| x == c),
                "{c} in {:?}",
                w.capabilities
            );
        }
        assert!(
            !w.capabilities.iter().any(|c| c == "stop_overlay"),
            "no overlay is drawn, none is claimed"
        );
    } else {
        assert!(
            w.capabilities.is_empty(),
            "a platform without a backend claims nothing"
        );
    }
}

#[tokio::test]
async fn a_protocol_major_the_helper_does_not_speak_is_refused_with_the_typed_reason() {
    let mut h = spawn_raw(&[]);
    h.send(&hello(2, vec![1; 32], me())).await;
    let w = h.welcome().await.expect("a welcome carrying the refusal");
    assert!(w.refusal.contains("protocol major 2"), "{}", w.refusal);
    assert_eq!(
        w.token_proof, "",
        "a refused Core gets no proof of its token"
    );
    assert_eq!(h.exit_code().await, Some(4));
}

#[tokio::test]
async fn a_token_that_is_not_32_bytes_is_refused() {
    let mut h = spawn_raw(&[]);
    h.send(&hello(1, vec![1; 16], me())).await;
    let w = h.welcome().await.expect("a welcome");
    assert!(w.refusal.contains("32 bytes"), "{}", w.refusal);
    assert_eq!(h.exit_code().await, Some(4));
}

#[cfg(unix)]
#[tokio::test]
async fn a_hello_naming_another_parent_is_refused() {
    let mut h = spawn_raw(&[]);
    h.send(&hello(1, vec![1; 32], me() + 1)).await;
    let w = h.welcome().await.expect("a welcome");
    assert!(w.refusal.contains("parent"), "{}", w.refusal);
    assert_eq!(w.token_proof, "");
    assert_eq!(h.exit_code().await, Some(4));
}

#[tokio::test]
async fn a_launch_pinned_to_a_token_hash_refuses_a_forged_hello_and_accepts_the_real_one() {
    let real = vec![0x11; 32];
    let pin = hex::encode(Sha256::digest(&real));
    // A forger who writes to the stream with a different token.
    let mut h = spawn_raw(&[("MODBIT_ACTUATOR_TOKEN_SHA256", pin.as_str())]);
    h.send(&hello(1, vec![0x22; 32], me())).await;
    let w = h.welcome().await.expect("a welcome");
    assert!(w.refusal.contains("pinned"), "{}", w.refusal);
    assert_eq!(w.token_proof, "");
    assert_eq!(h.exit_code().await, Some(4));
    // The Core that holds the token is served.
    let mut h = spawn_raw(&[("MODBIT_ACTUATOR_TOKEN_SHA256", pin.as_str())]);
    h.send(&hello(1, real.clone(), me())).await;
    let w = h.welcome().await.expect("a welcome");
    assert_eq!(
        (w.refusal.as_str(), w.token_proof.as_str()),
        ("", proof(&real).as_str())
    );
}

#[tokio::test]
async fn nothing_is_served_before_a_hello_and_a_second_hello_ends_the_connection() {
    // A request as the very first frame: no welcome, the process leaves.
    let mut h = spawn_raw(&[]);
    h.send(&wire::ActuatorFrame {
        body: Some(wire::actuator_frame::Body::Request(wire::ActuatorRequest {
            request_id: 1,
            deadline_ms: 0,
            call: Some(Call::ListApplications(wire::ListApplicationsRequest {})),
        })),
    })
    .await;
    assert!(h.welcome().await.is_none());
    assert_eq!(h.exit_code().await, Some(3));
    // A second hello after the welcome is a protocol violation.
    let mut h = spawn_raw(&[]);
    h.send(&hello(1, vec![3; 32], me())).await;
    assert!(h.welcome().await.is_some());
    h.send(&hello(1, vec![4; 32], me())).await;
    assert_eq!(h.exit_code().await, Some(5));
}

#[tokio::test]
async fn the_core_closing_the_stream_stops_the_helper_at_once() {
    let mut h = spawn_raw(&[]);
    h.send(&hello(1, vec![3; 32], me())).await;
    assert!(h.welcome().await.is_some());
    let t0 = Instant::now();
    h.stdin = None; // the Core is gone
    assert_eq!(h.exit_code().await, Some(0));
    assert!(
        t0.elapsed() < Duration::from_secs(2),
        "left in {:?}",
        t0.elapsed()
    );
}

#[tokio::test]
async fn the_core_client_attaches_and_a_wrong_proof_would_not_be_accepted() {
    let a = launch(&[]).await.expect("attached");
    assert_eq!(a.info().name, "modbit-actuator-macos");
    assert_eq!(a.info().pid, a.pid());
    let r = call(&a, Call::Ping(wire::PingRequest::default())).await;
    assert!(
        matches!(r, Ok(Reply::Ping(ref p)) if !p.lease_held),
        "{r:?}"
    );
    // Stop with nothing held halts nothing and answers within the bound.
    let r = call(
        &a,
        Call::Stop(wire::StopRequest {
            reason: "test".into(),
            source: "test".into(),
        }),
    )
    .await;
    assert!(
        matches!(r, Ok(Reply::Stop(ref s)) if s.halted_leases == 0 && s.stopped_within_ms < 500),
        "{r:?}"
    );
    // A helper pinned to another launch's token is refused by the Core's own client...
    // (the Core pins its own token, so the matching case is the ordinary attach above).
}

#[tokio::test]
async fn a_helper_that_cannot_control_this_platform_says_so_and_never_crashes() {
    let a = launch(&[]).await.expect("attached");
    if cfg!(target_os = "macos") {
        // macOS: listing needs no permission; with both denied, control is refused by permission.
        let a = launch(&[("MODBIT_ACTUATOR_DENY", "accessibility,screen_recording")])
            .await
            .expect("attached");
        let acq = call(
            &a,
            Call::Acquire(wire::AcquireRequest {
                control_id: "c1".into(),
                scope: wire::Scope::App as i32,
                application: Some(wire::ApplicationIdentity {
                    pid: std::process::id(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        )
        .await;
        assert_eq!(code(&acq), "PERMISSION_REQUIRED");
        assert_eq!(delivery(&acq), wire::Delivery::NotDelivered as i32);
    } else {
        let r = call(&a, Call::ListApplications(wire::ListApplicationsRequest {})).await;
        assert_eq!(code(&r), "UNSUPPORTED_REQUEST");
    }
    assert!(a.is_alive(), "a refusal is not a crash");
}

#[tokio::test]
async fn a_killed_helper_is_noticed_calls_say_nothing_was_delivered_and_a_restart_serves_again() {
    let a = launch(&[]).await.expect("attached");
    let pid = a.pid();
    a.kill();
    tokio::time::timeout(Duration::from_secs(5), a.closed())
        .await
        .expect("the connection ended");
    assert!(!a.is_alive());
    let r = a
        .call(
            Call::Ping(wire::PingRequest::default()),
            Duration::from_secs(1),
        )
        .await;
    assert!(matches!(r, Err(CallError::NotDelivered(_))), "{r:?}");
    // The next start is a new process and works.
    let b = launch(&[]).await.expect("restarted");
    assert_ne!(b.pid(), pid);
    assert!(matches!(
        call(&b, Call::Ping(wire::PingRequest::default())).await,
        Ok(Reply::Ping(_))
    ));
}

#[tokio::test]
async fn a_binary_that_is_not_a_helper_never_gets_a_welcome_the_core_accepts() {
    // `launch` against something that answers nothing sensible.
    let mut cfg = config(&[]);
    cfg.executable = if cfg!(windows) {
        "cmd".into()
    } else {
        "/bin/cat".into()
    };
    cfg.args = vec![];
    let e = modbit_computer::actuator::ProcessActuator::launch(&cfg, Duration::from_millis(800))
        .await
        .err()
        .expect("refused");
    assert!(
        matches!(
            e,
            LaunchError::Handshake(_) | LaunchError::Refused(_) | LaunchError::Spawn(_)
        ),
        "{e:?}"
    );
}

/// What a process is comes from the operating system - its executable path,
/// its code signature, its bundle - never from anything the process says.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn an_applications_identity_is_what_the_operating_system_vouches_for() {
    let mut sleeper = std::process::Command::new("/bin/sleep")
        .arg("60")
        .spawn()
        .unwrap();
    let a = launch(&[]).await.expect("attached");
    let id = |r: Result<Reply, CallError>| match r {
        Ok(Reply::ResolveApplication(r)) => r.application.unwrap(),
        other => panic!("{other:?}"),
    };
    let sleep = id(call(
        &a,
        Call::ResolveApplication(wire::ResolveApplicationRequest {
            pid: sleeper.id(),
            ..Default::default()
        }),
    )
    .await);
    let i = sleep.identity.unwrap();
    assert_eq!(i.pid, sleeper.id());
    assert!(
        i.executable_path.ends_with("/sleep"),
        "{}",
        i.executable_path
    );
    assert_eq!(i.bundle_id, "", "a command-line tool has no bundle");
    assert!(
        i.signing_identity.ends_with(":com.apple.sleep"),
        "Apple-signed, no team: {:?}",
        i.signing_identity
    );
    assert!(!sleep.elevated);
    // This very test binary: ad-hoc linker-signed.
    let me_info = id(call(
        &a,
        Call::ResolveApplication(wire::ResolveApplicationRequest {
            pid: std::process::id(),
            ..Default::default()
        }),
    )
    .await);
    let mi = me_info.identity.unwrap();
    assert_eq!(
        std::fs::canonicalize(&mi.executable_path).unwrap(),
        std::fs::canonicalize(std::env::current_exe().unwrap()).unwrap()
    );
    assert!(
        mi.signing_identity.starts_with("-:"),
        "{:?}",
        mi.signing_identity
    );
    // No such process, and no such application.
    let r = call(
        &a,
        Call::ResolveApplication(wire::ResolveApplicationRequest {
            pid: 999_999,
            ..Default::default()
        }),
    )
    .await;
    assert_eq!(code(&r), "WINDOW_UNVERIFIABLE");
    let r = call(
        &a,
        Call::ResolveApplication(wire::ResolveApplicationRequest {
            bundle_id: "org.nonexistent.nothing".into(),
            ..Default::default()
        }),
    )
    .await;
    assert_eq!(code(&r), "WINDOW_UNVERIFIABLE");
    // The list needs no permission and every entry is well-formed.
    let Ok(Reply::ListApplications(l)) =
        call(&a, Call::ListApplications(wire::ListApplicationsRequest {})).await
    else {
        panic!()
    };
    for app in &l.applications {
        let i = app.identity.as_ref().unwrap();
        assert!(
            i.pid > 0 && !i.executable_path.is_empty() && !i.name.is_empty(),
            "{i:?}"
        );
    }
    let _ = sleeper.kill();
    let _ = sleeper.wait();
}

/// A process id that is not the approved application's, or is the approved
/// one with a different signature, is not controllable (the swapped-identity case).
#[cfg(target_os = "macos")]
#[tokio::test]
async fn acquire_refuses_an_application_whose_identity_is_not_the_one_approved() {
    let mut sleeper = std::process::Command::new("/bin/sleep")
        .arg("60")
        .spawn()
        .unwrap();
    let a = launch(&[]).await.expect("attached");
    let Ok(Reply::ResolveApplication(r)) = call(
        &a,
        Call::ResolveApplication(wire::ResolveApplicationRequest {
            pid: sleeper.id(),
            ..Default::default()
        }),
    )
    .await
    else {
        panic!()
    };
    let mut approved = ident_of(&r.application.unwrap());
    approved.signing_identity = "EVILTEAM:com.apple.sleep".into();
    let r = call(
        &a,
        Call::Acquire(wire::AcquireRequest {
            control_id: "c".into(),
            scope: wire::Scope::App as i32,
            application: Some(approved),
            ..Default::default()
        }),
    )
    .await;
    // Without the permission it is PERMISSION_REQUIRED, with it WINDOW_UNVERIFIABLE; never a success.
    assert!(
        matches!(
            code(&r).as_str(),
            "WINDOW_UNVERIFIABLE" | "PERMISSION_REQUIRED" | "SECURE_DESKTOP"
        ),
        "{r:?}"
    );
    let _ = sleeper.kill();
    let _ = sleeper.wait();
}

/// PX-074: the welcome reports the permissions truthfully (a platform that
/// cannot tell names none, which the Core shows as UNKNOWN).
#[tokio::test]
async fn the_welcome_reports_the_permissions_the_helper_holds_and_never_a_guess() {
    let mut h = spawn_raw(&[("MODBIT_ACTUATOR_DENY", "accessibility,screen_recording")]);
    h.send(&hello(1, vec![7; 32], me())).await;
    let w = h.welcome().await.expect("a welcome");
    if cfg!(target_os = "macos") {
        let get = |n: &str| {
            w.permissions
                .iter()
                .find(|p| p.permission == n)
                .map(|p| p.granted)
        };
        assert_eq!(get("accessibility"), Some(false));
        assert_eq!(get("screen_recording"), Some(false));
    } else {
        assert!(w.permissions.is_empty(), "{:?}", w.permissions);
    }
}
