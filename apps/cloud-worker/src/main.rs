//! `modbit-cloud-worker` — remote Core host (M8.2; docs/24, docs/33).

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let cfg = match modbit_cloud_worker::Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("modbit-cloud-worker: {e}");
            return ExitCode::FAILURE;
        }
    };
    match modbit_cloud_worker::start(cfg).await {
        Ok(worker) => {
            eprintln!("modbit-cloud-worker: {} hosting", worker.worker_id);
            let _ = tokio::signal::ctrl_c().await;
            worker.stop().await;
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("modbit-cloud-worker: {e:#}");
            ExitCode::FAILURE
        }
    }
}
