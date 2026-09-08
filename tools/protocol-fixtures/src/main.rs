//! Writes the Rust-encoded fixtures to `tests/fixtures/protocol/v1/rust/`:
//! `<name>.bin` (canonical bytes) and `<name>.json` (type name + expected
//! field values). Exit 1 if `--check` finds the files on disk differ.

use std::process::ExitCode;

use protocol_fixtures::{fixture_dir, samples};

fn main() -> ExitCode {
    let check = std::env::args().any(|a| a == "--check");
    let dir = fixture_dir("rust");
    std::fs::create_dir_all(&dir).expect("create fixture dir");
    let mut stale = 0;
    for s in samples() {
        let json = serde_json::to_string_pretty(&serde_json::json!({
            "type": s.type_name,
            "expected": s.expected,
        }))
        .expect("json")
            + "\n";
        for (ext, content) in [("bin", s.bytes.clone()), ("json", json.into_bytes())] {
            let path = dir.join(format!("{}.{ext}", s.name));
            let current = std::fs::read(&path).ok();
            if current.as_deref() != Some(content.as_slice()) {
                if check {
                    eprintln!("stale fixture: {}", path.display());
                    stale += 1;
                } else {
                    std::fs::write(&path, &content).expect("write fixture");
                    println!("wrote {}", path.display());
                }
            }
        }
    }
    if stale > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
