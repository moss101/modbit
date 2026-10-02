//! `modbit-core` — local Core daemon (docs/11 "Deployment units", docs/33
//! startup sequence). M1.3 scope: open the stores, generate a boot-scoped
//! secret, bind the authenticated local SurfaceProtocol, print the ready line
//! for the process that spawned it, and serve session/task commands and event
//! subscriptions. Scheduler, providers and tools arrive with later tasks.
//!
//! Usage: `modbit-core --data-dir <dir>`
//!
//! Offline subcommands for the desktop updater (M10.2, docs/70 "Desktop
//! update"); both open `core.db` read-only and never start the Core:
//! `modbit-core schema-info --data-dir <dir>` prints the schema this build
//! writes and the one on disk as one JSON line; `modbit-core backup --data-dir
//! <dir> --to <file>` writes a consistent copy of the database; `modbit-core
//! device-policy` prints the device constraints the updater must honor.

use std::path::PathBuf;
use std::process::ExitCode;

mod accounting;
mod agent_profiles;
mod agent_tools;
mod agents;
mod assurance;
mod attention;
mod baseline;
mod branch;
mod browser;
mod browser_cloud;
mod capacity;
mod checkpoint;
mod ci_evidence;
mod compensation;
mod config;
mod critique;
mod dashboard;
mod doctor;
mod economics;
mod environment;
mod escalation;
mod extensions;
mod external_diagnostics;
mod forge;
mod gate;
mod handoff;
mod hooks;
mod inspector;
mod languages;
mod mcp;
mod media_bridge;
mod memory;
mod model_registry;
mod onboarding;
mod plans;
mod probe;
mod procedural;
mod promotion;
mod protocol;
mod pull_request;
mod replay;
mod review;
mod review_comments;
mod review_env;
mod routing;
mod rules;
mod runtime;
mod sandboxes;
mod server;
mod side;
mod skills;
mod slo;
mod spawn;
mod statistics;
mod subagent;
mod tools;
mod undo;
mod usage;
mod user_patch;
mod verify;

fn usage() -> &'static str {
    "usage: modbit-core --data-dir <dir> [--tether-stdin] [--idle-exit-secs N] [--tenant-id <uuid>]"
}

/// The updater's offline subcommands. `None` means the arguments are the
/// daemon's own.
fn offline_command() -> Option<ExitCode> {
    let mut args = std::env::args().skip(1);
    let command = args.next()?;
    if command == "device-policy" {
        let d = config::device_constraints().unwrap_or_default();
        println!(
            "{}",
            serde_json::json!({ "update_channel": d.update_channel, "minimum_version": d.minimum_version })
        );
        return Some(ExitCode::SUCCESS);
    }
    if command != "schema-info" && command != "backup" {
        return None;
    }
    let (mut data_dir, mut to) = (None, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--data-dir" => data_dir = args.next().map(PathBuf::from),
            "--to" if command == "backup" => to = args.next().map(PathBuf::from),
            other => {
                eprintln!("modbit-core {command}: unknown argument `{other}`");
                return Some(ExitCode::from(2));
            }
        }
    }
    let Some(data_dir) = data_dir else {
        eprintln!("modbit-core {command}: --data-dir is required");
        return Some(ExitCode::from(2));
    };
    let outcome = if command == "schema-info" {
        modbit_event_store::schema_info(&data_dir).map(|i| {
            println!(
                "{}",
                serde_json::json!({
                    "build_schema_version": i.build_schema_version,
                    "on_disk_schema_version": i.on_disk_schema_version,
                })
            );
        })
    } else {
        let Some(to) = to else {
            eprintln!("modbit-core backup: --to is required");
            return Some(ExitCode::from(2));
        };
        modbit_event_store::backup_database(&data_dir, &to).map(|v| {
            println!(
                "{}",
                serde_json::json!({ "backup": to, "schema_version": v })
            );
        })
    };
    Some(match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("modbit-core {command}: {e}");
            ExitCode::from(1)
        }
    })
}

fn main() -> ExitCode {
    if let Some(code) = offline_command() {
        return code;
    }
    let mut data_dir: Option<PathBuf> = None;
    let mut tether_stdin = false;
    let mut idle_exit_secs: Option<u64> = None;
    let mut tenant: Option<modbit_domain::TenantId> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--data-dir" => data_dir = args.next().map(PathBuf::from),
            "--tether-stdin" => tether_stdin = true,
            "--idle-exit-secs" => idle_exit_secs = args.next().and_then(|v| v.parse().ok()),
            // M8.2: a Cloud Core Worker's Core serves the cloud tenant it mirrors.
            "--tenant-id" => {
                tenant = match args.next().map(|v| modbit_domain::TenantId::parse(&v)) {
                    Some(Ok(t)) => Some(t),
                    _ => {
                        eprintln!("modbit-core: --tenant-id needs a uuid\n{}", usage());
                        return ExitCode::from(2);
                    }
                }
            }
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
    let code = match rt.block_on(server::run_as(data_dir, idle_exit_secs, tenant)) {
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
