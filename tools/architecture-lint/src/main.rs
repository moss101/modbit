//! `architecture-lint` command line: exit 0 when the workspace respects
//! `rules.toml`, 1 when violations exist, 2 on operational error.

use std::path::PathBuf;
use std::process::ExitCode;

use architecture_lint::{Graph, Rules, check};

fn usage() -> &'static str {
    "usage: architecture-lint [--manifest-path Cargo.toml] [--rules tools/architecture-lint/rules.toml] [--include-dev]"
}

fn run() -> anyhow::Result<bool> {
    let mut manifest = PathBuf::from("Cargo.toml");
    let mut rules_path = PathBuf::from("tools/architecture-lint/rules.toml");
    let mut include_dev = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--manifest-path" => {
                manifest = PathBuf::from(args.next().ok_or_else(|| anyhow::anyhow!(usage()))?);
            }
            "--rules" => {
                rules_path = PathBuf::from(args.next().ok_or_else(|| anyhow::anyhow!(usage()))?);
            }
            "--include-dev" => include_dev = true,
            "-h" | "--help" => {
                println!("{}", usage());
                return Ok(true);
            }
            other => anyhow::bail!("unknown argument `{other}`\n{}", usage()),
        }
    }
    let rules = Rules::load(&rules_path)?;
    let graph = Graph::from_cargo_metadata(&manifest, include_dev)?;
    let violations = check(&graph, &rules);
    for v in &violations {
        eprintln!("{v}");
    }
    println!(
        "architecture-lint: {} workspace members, {} forbid rules, {} confine rules, {} violation(s)",
        graph.members.len(),
        rules.forbid.len(),
        rules.confine.len(),
        violations.len()
    );
    Ok(violations.is_empty())
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("architecture-lint: error: {e:#}");
            ExitCode::from(2)
        }
    }
}
