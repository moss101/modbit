//! `modbit-execd` — durable PTY/process broker (docs/11, docs/21 "Durable
//! modbit-execd"). Owns local processes and PTYs, keeps a durable per-session
//! output log with cursor replay, spills complete output to a content-addressed
//! OutputRef, and lets an authenticated Core attach, detach, write stdin and
//! cancel. It is not authorized to create capabilities or decide policy.
//!
//! Usage: `modbit-execd --data-dir <dir> [--object-dir <dir>]
//! [--replay-window-bytes <n>] [--segment-bytes <n>] [--retain-sessions <n>]
//! [--retain-secs <n>] [--tether-stdin] [--orphan-grace-secs <n>]`
//!
//! Bounds (flag, else the `MODBIT_EXECD_*` variable, else the default): a
//! session keeps its last `--replay-window-bytes` of output replayable
//! (default 64 MiB; the log rotates in `--segment-bytes` segments, default
//! 4 MiB, so disk per session is at most window + segment); a cursor older
//! than the window is answered `CURSOR_EXPIRED`. Finished sessions are kept
//! up to `--retain-sessions` (256) and `--retain-secs` (7 days; 0 = no age
//! limit). `output_ref` objects are sealed into `--object-dir`, by default
//! `<data-dir>/objects`; the Core starts the broker with its own object
//! store there, so one content-addressed store serves both.
//!
//! On Linux it is also its own review-sandbox launcher (EPR-018):
//! `modbit-execd --review-sandbox-exec -- <argv...>` runs `argv` with no
//! network, and `--review-sandbox-selfcheck` reports whether that holds.

use std::path::PathBuf;
use std::process::ExitCode;

mod broker;
mod listeners;
#[cfg(target_os = "linux")]
mod seccomp_net;
mod seglog;

fn main() -> ExitCode {
    let mut data_dir: Option<PathBuf> = None;
    let mut tether_stdin = false;
    let mut orphan_grace: Option<std::time::Duration> = None;
    let mut object_dir: Option<PathBuf> = None;
    let mut limits = broker::Limits::default();
    let env_u64 = |name: &str| std::env::var(name).ok().and_then(|v| v.parse::<u64>().ok());
    if let Some(v) = env_u64("MODBIT_EXECD_REPLAY_WINDOW_BYTES") {
        limits.replay_window_bytes = v;
    }
    if let Some(v) = env_u64("MODBIT_EXECD_SEGMENT_BYTES") {
        limits.segment_bytes = v;
    }
    if let Some(v) = env_u64("MODBIT_EXECD_RETAIN_SESSIONS") {
        limits.retain_sessions = usize::try_from(v).unwrap_or(usize::MAX);
    }
    if let Some(v) = env_u64("MODBIT_EXECD_RETAIN_SECS") {
        limits.retain_age = (v > 0).then(|| std::time::Duration::from_secs(v));
    }
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            #[cfg(target_os = "linux")]
            "--review-sandbox-exec" => {
                let argv: Vec<std::ffi::OsString> = std::env::args_os()
                    .skip_while(|a| a != "--")
                    .skip(1)
                    .collect();
                let e = seccomp_net::launch(argv);
                eprintln!("modbit-execd: review sandbox: {e}");
                return ExitCode::from(126);
            }
            #[cfg(target_os = "linux")]
            "--review-sandbox-selfcheck" => {
                return if seccomp_net::selfcheck() {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(1)
                };
            }
            "--data-dir" => data_dir = args.next().map(PathBuf::from),
            "--tether-stdin" => tether_stdin = true,
            "--orphan-grace-secs" => {
                orphan_grace = args
                    .next()
                    .and_then(|v| v.parse::<u64>().ok())
                    .filter(|s| *s > 0)
                    .map(std::time::Duration::from_secs);
            }
            "--object-dir" => object_dir = args.next().map(PathBuf::from),
            "--replay-window-bytes" | "--segment-bytes" | "--retain-sessions" | "--retain-secs" => {
                let Some(v) = args.next().and_then(|v| v.parse::<u64>().ok()) else {
                    eprintln!("modbit-execd: `{a}` needs a number");
                    return ExitCode::from(2);
                };
                match a.as_str() {
                    "--replay-window-bytes" => limits.replay_window_bytes = v,
                    "--segment-bytes" => limits.segment_bytes = v,
                    "--retain-sessions" => {
                        limits.retain_sessions = usize::try_from(v).unwrap_or(usize::MAX);
                    }
                    _ => limits.retain_age = (v > 0).then(|| std::time::Duration::from_secs(v)),
                }
            }
            "-h" | "--help" => {
                println!(
                    "usage: modbit-execd --data-dir <dir> [--object-dir <dir>] [--replay-window-bytes <n>] [--segment-bytes <n>] [--retain-sessions <n>] [--retain-secs <n>] [--tether-stdin] [--orphan-grace-secs <n>]"
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
    // A segment below one page is a configuration mistake, not a policy.
    limits.segment_bytes = limits.segment_bytes.max(4096);
    let config = broker::Config {
        object_dir: object_dir.unwrap_or_else(|| data_dir.join("objects")),
        data_dir,
        orphan_grace,
        limits,
    };
    match rt.block_on(broker::run(config)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("modbit-execd: {e:#}");
            ExitCode::from(1)
        }
    }
}
