//! `architecture-lint` command line.
//!
//! Subcommands: `deps` (default) checks dependency direction over `cargo
//! metadata`; `locked` checks that commits changing locked paths carry an
//! accepted Decision Record trailer (DR-M0-002). Exit 0 when clean, 1 when
//! violations exist, 2 on operational error.

use std::path::PathBuf;
use std::process::ExitCode;

use architecture_lint::locked::check_locked;
use architecture_lint::{Graph, Rules, check};

const USAGE: &str = "usage:\n  architecture-lint [deps] [--manifest-path Cargo.toml] [--rules tools/architecture-lint/rules.toml] [--include-dev]\n  architecture-lint locked [--repo .] [--base <sha>] [--head HEAD] [--rules tools/architecture-lint/rules.toml]";

fn take(args: &mut impl Iterator<Item = String>) -> anyhow::Result<String> {
    args.next().ok_or_else(|| anyhow::anyhow!(USAGE))
}

fn run() -> anyhow::Result<bool> {
    let mut args = std::env::args().skip(1).peekable();
    let mode = match args.peek().map(String::as_str) {
        Some("deps") | Some("locked") => args.next().unwrap(),
        Some("-h") | Some("--help") => {
            println!("{USAGE}");
            return Ok(true);
        }
        _ => "deps".to_owned(),
    };
    let mut rules_path = PathBuf::from("tools/architecture-lint/rules.toml");
    let mut manifest = PathBuf::from("Cargo.toml");
    let mut include_dev = false;
    let mut repo = PathBuf::from(".");
    let mut base: Option<String> = None;
    let mut head = "HEAD".to_owned();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--rules" => rules_path = PathBuf::from(take(&mut args)?),
            "--manifest-path" => manifest = PathBuf::from(take(&mut args)?),
            "--include-dev" => include_dev = true,
            "--repo" => repo = PathBuf::from(take(&mut args)?),
            "--base" => base = Some(take(&mut args)?),
            "--head" => head = take(&mut args)?,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(true);
            }
            other => anyhow::bail!("unknown argument `{other}`\n{USAGE}"),
        }
    }
    let rules = Rules::load(&rules_path)?;
    match mode.as_str() {
        "deps" => {
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
        _ => {
            let violations = check_locked(&repo, base.as_deref(), &head, &rules)?;
            for v in &violations {
                eprintln!("{v}");
            }
            println!(
                "architecture-lint locked: {} locked rules, range {}..{}, {} violation(s)",
                rules.locked.len(),
                base.as_deref().unwrap_or("<parent>"),
                head,
                violations.len()
            );
            Ok(violations.is_empty())
        }
    }
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
