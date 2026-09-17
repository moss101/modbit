//! `modbit-cloud-api` — Rust HTTP/WSS API service (M8.1; docs/24, docs/30).

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let cfg = match modbit_cloud_api::Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("modbit-cloud-api: {e}");
            return ExitCode::FAILURE;
        }
    };
    match modbit_cloud_api::serve(cfg).await {
        Ok(served) => {
            eprintln!("modbit-cloud-api: listening on {}", served.addr);
            let _ = tokio::signal::ctrl_c().await;
            served.stop();
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("modbit-cloud-api: {e:#}");
            ExitCode::FAILURE
        }
    }
}
