//! Child process roles for the execd tests (a real, portable process with
//! deterministic output; no test harness noise). Role from
//! `MODBIT_EXECD_TEST_ROLE`; extra args after `--`.

use std::io::{BufRead, Write};
use std::time::Duration;

fn main() {
    let role = std::env::var("MODBIT_EXECD_TEST_ROLE").unwrap_or_default();
    let mut out = std::io::stdout().lock();
    match role.as_str() {
        "echo-args" => {
            let args: Vec<String> = std::env::args().skip_while(|a| a != "--").skip(1).collect();
            let cwd = std::env::current_dir().unwrap();
            writeln!(
                out,
                "args={args:?}\ncwd={}\nMODBIT_T1={}\nHOME_SET={}",
                cwd.display(),
                std::env::var("MODBIT_T1").unwrap_or_default(),
                std::env::var("HOME").is_ok() || std::env::var("USERPROFILE").is_ok()
            )
            .unwrap();
            eprintln!("to-stderr");
            std::process::exit(3);
        }
        "big" => {
            for i in 0..(10 * 1024 * 1024 / 64) {
                out.write_all(format!("{i:063}\n").as_bytes()).unwrap();
            }
            out.flush().unwrap();
        }
        // FIX-20: `MODBIT_EXECD_TEST_MIB` MiB of deterministic 64-byte lines
        // (line i is `{i:063}\n`, so the byte at any offset is known), then
        // exit (`noisy`) or keep running (`noisy-live`).
        "noisy" | "noisy-live" => {
            let mib: usize = std::env::var("MODBIT_EXECD_TEST_MIB")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1);
            let mut block = Vec::with_capacity(64 * 1024);
            let mut line = 0usize;
            let total_lines = mib * 1024 * 1024 / 64;
            while line < total_lines {
                block.clear();
                while block.len() < 64 * 1024 && line < total_lines {
                    block.extend_from_slice(format!("{line:063}\n").as_bytes());
                    line += 1;
                }
                out.write_all(&block).unwrap();
            }
            out.flush().unwrap();
            if role == "noisy-live" {
                // Long enough to outlive the test's broker kill, short enough
                // that nothing is left running afterwards.
                let until = std::time::Instant::now() + Duration::from_secs(25);
                while std::time::Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
        // FIX-20: many tiny records (one write each), the shape that made the
        // index re-read quadratic: line i is `{i:07}\n`.
        "chatty" => {
            let n: usize = std::env::var("MODBIT_EXECD_TEST_LINES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1000);
            for i in 0..n {
                writeln!(out, "{i:07}").unwrap();
                out.flush().unwrap();
            }
        }
        "ticker" => {
            for i in 0.. {
                writeln!(out, "tick {i}").unwrap();
                out.flush().unwrap();
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        "cat" => {
            let stdin = std::io::stdin();
            for line in stdin.lock().lines() {
                let line = line.unwrap();
                writeln!(out, "got:{line}").unwrap();
                out.flush().unwrap();
                if line == "quit" {
                    break;
                }
            }
        }
        other => {
            eprintln!("unknown role {other}");
            std::process::exit(2);
        }
    }
}
