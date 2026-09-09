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
