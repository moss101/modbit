//! QUAL-PX-148 for the real `modbit-core` binary: the core-dump limit is zero,
//! the process is not attachable, an injection variable does not reach the
//! serving image, and the Core still serves.

#![cfg(unix)]
#![allow(unsafe_code)]

#[path = "../../../crates/process-hardening/tests/support/mod.rs"]
mod support;

#[test]
fn qual_px_148_the_core_hardens_itself_before_it_serves() {
    let bin = env!("CARGO_BIN_EXE_modbit-core");
    let data = tempfile::tempdir().expect("data dir");
    let data_dir = data.path().to_owned();
    support::run_qualification(bin, move || {
        let mut c = std::process::Command::new(bin);
        c.arg("--data-dir").arg(&data_dir).arg("--tether-stdin");
        c
    });
}
