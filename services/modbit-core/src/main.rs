//! `modbit-core` — local Core daemon (docs/11 "Deployment units", docs/33
//! startup sequence). M1.3 scope: open the stores, generate a boot-scoped
//! secret, bind the authenticated local SurfaceProtocol, print the ready line
//! for the process that spawned it, and serve session/task commands and event
//! subscriptions. Scheduler, providers and tools arrive with later tasks.
//!
//! Usage: `modbit-core --data-dir <dir>`

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

mod accounting;
mod agent_profiles;
mod agent_tools;
mod agents;
mod apply_back;
mod assurance;
mod attention;
mod background_process;
mod baseline;
mod branch;
mod browser;
mod browser_cloud;
mod browser_observer;
mod budgets;
mod capacity;
mod checkpoint;
mod checkpoint_gc;
mod ci_evidence;
mod compaction_model;
mod compensation;
mod config;
mod conversation_search;
mod credentials;
mod critique;
mod dashboard;
mod doctor;
mod economics;
mod environment;
mod epoch;
mod escalation;
mod extensions;
mod external_diagnostics;
mod forge;
mod gate;
mod git_state;
mod handoff;
mod hooks;
mod index_host;
mod inspector;
mod knowledge;
mod languages;
mod mcp;
mod media_bridge;
mod memory;
mod memory_commands;
mod merge_tx;
mod model_registry;
mod onboarding;
mod pause;
mod plans;
mod preturn;
mod probe;
mod procedural;
mod process_services;
mod promotion;
mod protocol;
mod pull_request;
mod replay;
mod review;
mod review_comments;
mod review_env;
mod routing;
mod rules;
mod run_control;
mod runtime;
mod sandboxes;
mod server;
mod side;
mod skills;
mod slo;
mod spawn;
mod statistics;
mod stream;
mod subagent;
mod tasking;
mod telemetry;
mod terminal_stream;
mod tool_projection;
mod tools;
mod transcript;
mod undo;
mod usage;
mod user_patch;
mod verify;
mod workspace_files;
mod worktree_cleanup;
mod worktrees;

/// Waiting for the supervising parent process to end (Windows). The stdin tether covers a client
/// that closes its pipe; this covers one that is killed without the pipe reaching this process.
#[cfg(windows)]
mod parent_exit {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };

    /// The pid of this process's parent, from a process snapshot.
    #[allow(unsafe_code)]
    pub fn parent_pid() -> Option<u32> {
        let me = std::process::id();
        // SAFETY: the snapshot handle is owned here and closed before returning; the entry is a
        // plain struct whose `dwSize` is set as the API requires, and only read after a success.
        unsafe {
            let snap: HANDLE = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return None;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut found = None;
            let mut ok = Process32FirstW(snap, &mut entry);
            while ok != 0 {
                if entry.th32ProcessID == me {
                    found = Some(entry.th32ParentProcessID);
                    break;
                }
                ok = Process32NextW(snap, &mut entry);
            }
            CloseHandle(snap);
            found
        }
    }

    /// Blocks until the parent process ends; false when there is no parent to watch (it is
    /// already gone, or it cannot be opened), so the caller must not treat that as an exit.
    #[allow(unsafe_code)]
    pub fn wait() -> bool {
        let Some(parent) = parent_pid().filter(|p| *p != 0) else {
            return false;
        };
        // SAFETY: OpenProcess returns an owned handle or null; WaitForSingleObject only waits on
        // it, and it is closed afterwards.
        unsafe {
            let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, parent);
            if h.is_null() {
                return false;
            }
            let r = WaitForSingleObject(h, INFINITE);
            CloseHandle(h);
            r == 0
        }
    }
}

fn usage() -> &'static str {
    "usage: modbit-core --data-dir <dir> [--tether-stdin] [--idle-exit-secs N] [--tenant-id <uuid>]"
}

fn main() -> ExitCode {
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
        let trace_dir = data_dir.clone();
        let trace = move |what: &str| {
            if std::env::var_os("MODBIT_TETHER_TRACE").is_some()
                && let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(trace_dir.join("tether-trace.log"))
            {
                let _ = writeln!(f, "pid {} {what}", std::process::id());
            }
        };
        trace("tether armed");
        let t = trace.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut sink = [0u8; 64];
            let mut stdin = std::io::stdin();
            while matches!(stdin.read(&mut sink), Ok(n) if n > 0) {}
            t("stdin closed");
            // Windows: the client's pipes are gone with it, and `eprintln!` panics on a dead stderr, which
            // would end this thread without the exit. (Unix keeps `eprintln!`: there the orphaned Core is
            // reclaimed by the next Core, `orphaned_tethered_core`.)
            #[cfg(windows)]
            let _ = writeln!(
                std::io::stderr(),
                "modbit-core: supervising client closed its pipe; exiting"
            );
            #[cfg(not(windows))]
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
        // Windows: a killed client does not always close the pipe this Core reads (a killed Electron
        // main left its Core, and the profile lock, behind), so the Core also waits on the parent
        // process itself.
        #[cfg(windows)]
        std::thread::spawn(move || {
            trace(&format!(
                "parent wait begins; parent pid {:?}",
                parent_exit::parent_pid()
            ));
            let gone = parent_exit::wait();
            trace(&format!("parent wait returned {gone}"));
            if gone {
                let _ = writeln!(
                    std::io::stderr(),
                    "modbit-core: supervising parent is gone; exiting"
                );
                std::process::exit(0);
            }
        });
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
