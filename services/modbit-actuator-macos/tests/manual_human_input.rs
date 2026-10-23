//! QUAL-PX-071, the one check no program can make: a REAL person on the
//! keyboard or pointer parks the controller. Hardware events cannot be
//! synthesised (events posted at the HID tap do not reach the counter the
//! helper watches - measured), so this test waits for you.
//!
//!   cargo test -p modbit-actuator-macos --test manual_human_input -- --ignored --nocapture
//!
//! When it says GO, touch the keyboard or the trackpad within 30 seconds.
//! Needs Accessibility + Screen Recording and an unlocked screen. The script
//! `scripts/ax-tests.sh` does not run it.
#![cfg(target_os = "macos")]

mod common;

use std::time::{Duration, Instant};

use common::fixture_app::Fixture;
use common::*;
use modbit_computer::actuator::{Actuator, Call, Reply};
use modbit_protocol::v1 as wire;

#[tokio::test]
#[ignore = "manual: a person must touch the keyboard or pointer when asked"]
async fn a_real_person_on_the_keyboard_parks_the_controller_within_one_event() {
    let fx = Fixture::launch();
    let a = launch(&[]).await.expect("the helper attaches");
    let Ok(Reply::ResolveApplication(r)) = call(
        &a,
        Call::ResolveApplication(wire::ResolveApplicationRequest {
            pid: fx.pid(),
            ..Default::default()
        }),
    )
    .await
    else {
        panic!("resolve")
    };
    let ident = ident_of(&r.application.unwrap());
    let acq = call(
        &a,
        Call::Acquire(wire::AcquireRequest {
            control_id: "c".into(),
            scope: wire::Scope::App as i32,
            application: Some(ident.clone()),
            window_id: String::new(),
            lease_ttl_ms: 120_000,
        }),
    )
    .await;
    assert!(matches!(acq, Ok(Reply::Acquire(_))), "{acq:?}");
    let mut events = a.events();
    eprintln!("\n>>> GO: touch the keyboard or the trackpad now (30 s) <<<\n");
    let t0 = Instant::now();
    let ev = tokio::time::timeout(Duration::from_secs(30), events.recv())
        .await
        .expect("nobody touched anything")
        .unwrap();
    assert!(matches!(
        ev.event,
        Some(wire::actuator_event::Event::HumanInput(_))
    ));
    eprintln!("HumanInput event {:?} after GO", t0.elapsed());
    let r = call(
        &a,
        Call::Perform(wire::PerformRequest {
            control_id: "c".into(),
            application: Some(ident),
            scope: wire::Scope::App as i32,
            action: Some(wire::perform_request::Action::Key(wire::KeyAction {
                keys: vec!["a".into()],
            })),
            ..Default::default()
        }),
    )
    .await;
    assert_eq!(code(&r), "HUMAN_ACTIVE");
    assert_eq!(delivery(&r), wire::Delivery::NotDelivered as i32);
}
