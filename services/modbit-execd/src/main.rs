//! `modbit-execd` — durable PTY/process broker (docs/11, docs/21 "Durable
//! modbit-execd"). Owns local processes and PTYs, keeps a durable per-session
//! output log with cursor replay, spills complete output to a content-addressed
//! OutputRef, and lets an authenticated Core attach, detach, write stdin and
//! cancel. It is not authorized to create capabilities or decide policy.
//!
//! Usage: `modbit-execd --data-dir <dir>`

use std::path::PathBuf;
use std::process::ExitCode;

mod broker;

fn main() -> ExitCode {
    let mut data_dir: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--data-dir" => data_dir = args.next().map(PathBuf::from),
            "-h" | "--help" => {
                println!("usage: modbit-execd --data-dir <dir>");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("modbit-execd: unknown argument `{other}`");
                return ExitCode::from(2);
            }
        }
    }
    let Some(data_dir) = data_dir else {
        eprintln!("modbit-execd: --data-dir is required");
        return ExitCode::from(2);
    };
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("modbit-execd: runtime: {e}");
            return ExitCode::from(1);
        }
    };
    match rt.block_on(broker::run(data_dir)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("modbit-execd: {e:#}");
            ExitCode::from(1)
        }
    }
}
