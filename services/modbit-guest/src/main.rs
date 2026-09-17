//! `modbit-guest` — the sandbox guest agent (M8.3; docs/21 "`modbit-guest`",
//! docs/11: it implements typed capability-bound RPC for process and
//! filesystem operations and cannot mint capabilities or fetch secrets).
//!
//! Two ways to run. As `/init` of a MicroVM (no arguments; PID 1): it mounts
//! `/proc`, `/sys`, `/dev`, `/tmp`, `/run` and the workspace block device
//! the kernel command line names at `/workspace`, sets the hostname and
//! listens on the vsock port the command line names. As a child process
//! of the reference backend (`--listen 127.0.0.1:0 --workspace-host <dir>`):
//! it listens on loopback, announces `ready listen=<addr>` on stdout and
//! maps the guest's `/workspace` onto `<dir>`.
//!
//! Whatever the substrate, the guest speaks first (`GuestHello`), is
//! admitted or refused by the gateway (`GuestAdmit` with the ephemeral
//! credential and the policy), and thereafter answers only calls whose
//! HMAC verifies under that credential, each call id once; it enforces the
//! admitted policy — writes only under the workspace and never on a
//! protected path, reads only under the workspace and the readable roots,
//! a bounded, timed, environment-clean process per exec — and refuses the
//! rest with a typed refusal.

use std::process::ExitCode;

mod procs;
mod proxy;
mod serve;

#[cfg(target_os = "linux")]
mod init;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut listen: Option<String> = None;
    let mut workspace_host: Option<String> = None;
    let mut egress_host: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--listen" => {
                listen = args.get(i + 1).cloned();
                i += 2;
            }
            "--workspace-host" => {
                workspace_host = args.get(i + 1).cloned();
                i += 2;
            }
            "--egress-host" => {
                egress_host = args.get(i + 1).cloned();
                i += 2;
            }
            other => {
                eprintln!("modbit-guest: unknown argument `{other}`");
                return ExitCode::FAILURE;
            }
        }
    }
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("modbit-guest: runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    match (listen, workspace_host) {
        (Some(addr), ws) => {
            let mapping = ws.map(std::path::PathBuf::from);
            if let Some(e) = egress_host {
                serve::set_broker(proxy::BrokerAddr::Tcp(e));
            }
            match rt.block_on(serve::serve_tcp(&addr, mapping)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("modbit-guest: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        (None, _) => {
            #[cfg(target_os = "linux")]
            {
                match rt.block_on(init::run_as_init()) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(e) => {
                        eprintln!("modbit-guest: init: {e}");
                        // PID 1 must not exit; the console shows why.
                        std::thread::sleep(std::time::Duration::from_secs(3600));
                        ExitCode::FAILURE
                    }
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                eprintln!(
                    "modbit-guest: without --listen this binary is a MicroVM init (Linux only)"
                );
                ExitCode::FAILURE
            }
        }
    }
}
