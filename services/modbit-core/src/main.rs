//! `modbit-core` — local Core daemon (docs/11 "Deployment units", docs/33
//! startup sequence). M1.3 scope: open the stores, generate a boot-scoped
//! secret, bind the authenticated local SurfaceProtocol, print the ready line
//! for the process that spawned it, and serve session/task commands and event
//! subscriptions. Scheduler, providers and tools arrive with later tasks.
//!
//! Usage: `modbit-core --data-dir <dir>`

use std::path::PathBuf;
use std::process::ExitCode;

mod baseline;
mod economics;
mod inspector;
mod languages;
mod model_registry;
mod probe;
mod review;
mod routing;
mod runtime;
mod server;
mod side;
mod subagent;
mod tools;
mod undo;
mod verify;

fn usage() -> &'static str {
    "usage: modbit-core --data-dir <dir> [--tether-stdin] [--idle-exit-secs N]"
}

fn main() -> ExitCode {
    let mut data_dir: Option<PathBuf> = None;
    let mut tether_stdin = false;
    let mut idle_exit_secs: Option<u64> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--data-dir" => data_dir = args.next().map(PathBuf::from),
            "--tether-stdin" => tether_stdin = true,
            "--idle-exit-secs" => idle_exit_secs = args.next().and_then(|v| v.parse().ok()),
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
    // Parent tether (docs/33 one Core per profile): a supervising client holds
    // our stdin; EOF means the client is gone and this Core must not outlive
    // it, or its singleton lock would refuse the client's next Core.
    if tether_stdin {
        std::thread::spawn(|| {
            use std::io::Read;
            let mut sink = [0u8; 64];
            let mut stdin = std::io::stdin();
            while matches!(stdin.read(&mut sink), Ok(n) if n > 0) {}
            eprintln!("modbit-core: supervising client closed its pipe; exiting");
            std::process::exit(0);
        });
        // Second tether, independent of pipes: when the supervising parent
        // dies this process is re-parented, and it must not hold the profile.
        #[cfg(unix)]
        {
            let parent = std::os::unix::process::parent_id();
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    if std::os::unix::process::parent_id() != parent {
                        eprintln!("modbit-core: supervising parent is gone; exiting");
                        std::process::exit(0);
                    }
                }
            });
        }
    }
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
    let code = match rt.block_on(server::run(data_dir, idle_exit_secs)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("modbit-core: {e:#}");
            ExitCode::from(1)
        }
    };
    // A stopping Core must release the profile promptly (docs/33 one Core per
    // profile): every accepted command is durable before its ack, so nothing
    // in flight is worth waiting for once the serve loop has ended.
    rt.shutdown_timeout(std::time::Duration::from_millis(500));
    code
}
