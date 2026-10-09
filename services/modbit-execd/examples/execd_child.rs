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
        // PX-099: a child that reports the size of the terminal it runs on,
        // as the kernel tells it (`stty size` reads the tty it inherits): at
        // start, then on every input line `size`; `quit` ends it. Unix only
        // (the test that uses it is gated, with the reason).
        "stty-size" => {
            let report = |out: &mut std::io::StdoutLock<'_>| {
                let text = std::process::Command::new("stty")
                    .arg("size")
                    .stdin(std::process::Stdio::inherit())
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
                    .unwrap_or_else(|e| format!("stty failed: {e}"));
                writeln!(out, "size={text}").unwrap();
                out.flush().unwrap();
            };
            report(&mut out);
            let stdin = std::io::stdin();
            for line in stdin.lock().lines() {
                match line.unwrap().trim() {
                    "size" => report(&mut out),
                    "quit" => break,
                    _ => {}
                }
            }
        }
        // PX-132: a real listening server in a process of its own. It binds
        // `MODBIT_EXECD_TEST_BIND` (default 127.0.0.1) on an ephemeral port,
        // prints `listening <port>`, and answers every connection with an
        // HTTP response whose status is `MODBIT_EXECD_TEST_STATUS` (default
        // 200) until it is killed.
        "listen" => {
            let bind = std::env::var("MODBIT_EXECD_TEST_BIND").unwrap_or("127.0.0.1".into());
            let status: u16 = std::env::var("MODBIT_EXECD_TEST_STATUS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(200);
            let listener = std::net::TcpListener::bind((bind.as_str(), 0)).unwrap();
            let port = listener.local_addr().unwrap().port();
            writeln!(out, "listening {port}").unwrap();
            out.flush().unwrap();
            drop(out);
            for stream in listener.incoming().flatten() {
                use std::io::Read;
                let mut stream = stream;
                let mut buf = [0u8; 1024];
                let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                let _ = stream.read(&mut buf);
                let body = "ok";
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        }
        other => {
            eprintln!("unknown role {other}");
            std::process::exit(2);
        }
    }
}
