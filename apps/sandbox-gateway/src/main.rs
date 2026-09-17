//! `modbit-sandbox-gateway` — tenant-bound MicroVM substrate boundary (M8.3;
//! docs/24, docs/33 "Sandbox Gateway").

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let cfg = match modbit_sandbox_gateway::Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("modbit-sandbox-gateway: {e}");
            return ExitCode::FAILURE;
        }
    };
    match modbit_sandbox_gateway::serve(cfg).await {
        Ok(served) => {
            eprintln!(
                "modbit-sandbox-gateway: listening on {} ({} backend)",
                served.addr,
                served.state.backend.kind()
            );
            let _ = tokio::signal::ctrl_c().await;
            served.stop();
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("modbit-sandbox-gateway: {e:#}");
            ExitCode::FAILURE
        }
    }
}
