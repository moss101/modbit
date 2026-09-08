//! `modbit-guest` — sandbox guest RPC agent (never depends on cloud-api business logic).
//!
//! Created by milestone task M0.1. No runtime is wired in this build, so the
//! binary refuses to run instead of simulating success (docs/82 no-placeholder
//! production evidence gate).

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!(
        "{} {}: no runtime is wired in this build (milestone M0); refusing to run",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );
    ExitCode::FAILURE
}
