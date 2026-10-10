//! `retrieval-live`: the live same-model retrieval evaluation (PX-137).
//! Refuses (exit 2, nothing written) unless `MODBIT_LIVE=1` and a real
//! provider are configured; exit 0 only when every required measurement is
//! present; exit 1 otherwise, with `status.json` listing what is missing.
//! `retrieval-live --rescore <dir>` recomputes the report offline.

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = modbit_bench_retrieval::live_run::cli_main(&args, &|k| std::env::var(k).ok());
    std::process::ExitCode::from(u8::try_from(code).unwrap_or(1))
}
