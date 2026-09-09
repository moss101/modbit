//! `modbit-core` — local Core daemon (docs/11 "Deployment units", docs/33
//! startup sequence). M1.3 scope: open the stores, generate a boot-scoped
//! secret, bind the authenticated local SurfaceProtocol, print the ready line
//! for the process that spawned it, and serve session/task commands and event
//! subscriptions. Scheduler, providers and tools arrive with later tasks.
//!
//! Usage: `modbit-core --data-dir <dir>`

use std::path::PathBuf;
use std::process::ExitCode;

mod probe;
mod runtime;
mod server;
mod tools;
mod verify;

fn usage() -> &'static str {
    "usage: modbit-core --data-dir <dir>"
}

fn main() -> ExitCode {
    let mut data_dir: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--data-dir" => data_dir = args.next().map(PathBuf::from),
            "-h" | "--help" => {
                println!("{}", usage());
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("modbit-core: unknown argument `{other}`\n{}", usage());
                return ExitCode::from(2);
            }
        }
    }
    let Some(data_dir) = data_dir else {
        eprintln!("modbit-core: --data-dir is required\n{}", usage());
        return ExitCode::from(2);
    };
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("modbit-core: runtime: {e}");
            return ExitCode::from(1);
        }
    };
    match rt.block_on(server::run(data_dir)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("modbit-core: {e:#}");
            ExitCode::from(1)
        }
    }
}
