//! QUAL-PX-148 for the real `modbit-execd` binary: the core-dump limit is zero,
//! the process is not attachable, an injection variable does not reach the
//! serving image, and the broker still serves.

#![cfg(unix)]
#![allow(unsafe_code)]

#[path = "../../../crates/process-hardening/tests/support/mod.rs"]
mod support;

#[test]
fn qual_px_148_the_broker_hardens_itself_before_it_serves() {
    let bin = env!("CARGO_BIN_EXE_modbit-execd");
    let data = tempfile::tempdir().expect("data dir");
    let data_dir = data.path().to_owned();
    support::run_qualification(bin, move || {
        let mut c = std::process::Command::new(bin);
        c.arg("--data-dir").arg(&data_dir);
        c
    });
}
