//! Diagnosability dump of the raw server streams (ignored; run by hand).
use modbit_diagnostics::{LanguageServer, resolve_server_in};
use std::path::Path;
use std::time::Duration;
#[test]
#[ignore]
fn dump() {
    let nm = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../node_modules")
        .canonicalize()
        .unwrap();
    for (lang, fixture, file, text) in [
        (
            "rust",
            "rust-cli",
            "src/seeded.rs",
            "pub fn seeded() -> u32 { \"x\" }\n",
        ),
        (
            "python",
            "python-service",
            "seeded.py",
            "x: int = \"text\"\n",
        ),
        (
            "typescript",
            "ts-webapp",
            "test/seeded.ts",
            "const wrong: number = \"text\";\n",
        ),
    ] {
        let src = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/repos")
            .join(fixture)
            .canonicalize()
            .unwrap();
        let spec = match resolve_server_in(lang, &src, Some(&nm)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{lang}: {e}");
                continue;
            }
        };
        eprintln!(
            "== {lang}: {:?} {:?} init={}",
            spec.command, spec.args, spec.init_options
        );
        let mut s = match LanguageServer::spawn(
            &spec.name,
            &spec.command,
            &spec.args,
            &src,
            spec.init_options,
            Duration::from_secs(60),
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("spawn: {e}");
                continue;
            }
        };
        eprintln!(
            "  caps: {}",
            s.capabilities()
                .to_string()
                .chars()
                .take(400)
                .collect::<String>()
        );
        let backup = std::fs::read(src.join(file)).ok();
        std::fs::write(src.join(file), text).ok();
        s.open(file, &spec.language_id, text).unwrap();
        eprintln!(
            "  diagnostics(): {:?}",
            s.diagnostics(file, Duration::from_secs(90)).map(|v| v
                .iter()
                .map(|d| format!(
                    "{}:{}:{}",
                    d.severity,
                    d.range.start.line,
                    d.message.chars().take(60).collect::<String>()
                ))
                .collect::<Vec<_>>())
        );
        for m in s.drain(Duration::from_secs(if lang == "rust" { 75 } else { 25 })) {
            if !m.contains("$/progress") {
                eprintln!("  {m}");
            }
        }
        if s.supports_pull_diagnostics() {
            eprintln!(
                "  pull: {:?}",
                s.pull_diagnostics(file, Duration::from_secs(30)).map(|v| v
                    .iter()
                    .map(|d| d["message"].to_string())
                    .collect::<Vec<_>>())
            );
        }
        match backup {
            Some(b) => {
                std::fs::write(src.join(file), b).ok();
            }
            None => {
                let _ = std::fs::remove_file(src.join(file));
            }
        }
        s.shutdown();
    }
}
