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
    let mut tether_stdin = false;
    let mut orphan_grace: Option<std::time::Duration> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--data-dir" => data_dir = args.next().map(PathBuf::from),
            "--tether-stdin" => tether_stdin = true,
            "--orphan-grace-secs" => {
                orphan_grace = args
                    .next()
                    .and_then(|v| v.parse::<u64>().ok())
                    .filter(|s| *s > 0)
                    .map(std::time::Duration::from_secs);
            }
            "-h" | "--help" => {
                println!(
                    "usage: modbit-execd --data-dir <dir> [--tether-stdin] [--orphan-grace-secs <n>]"
                );
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
    // Parent tether (docs/17): the Core holds our stdin; EOF means the Core is
    // gone and the broker must not outlive it.
    if tether_stdin {
        std::thread::spawn(|| {
            use std::io::Read;
            let mut sink = [0u8; 64];
            let mut stdin = std::io::stdin();
            while matches!(stdin.read(&mut sink), Ok(n) if n > 0) {}
            std::process::exit(0);
        });
    }
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
    match rt.block_on(broker::run(data_dir, orphan_grace)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("modbit-execd: {e:#}");
            ExitCode::from(1)
        }
    }
}
