//! Child process roles for the execd tests (a real, portable process with
//! deterministic output; no test harness noise). Role from
//! `MODBIT_EXECD_TEST_ROLE`; extra args after `--`.

// PX-168 lint policy: this file spawns processes and baselined expect/unwrap sites; new ones need review.
#![allow(clippy::disallowed_methods, clippy::unwrap_used)]
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
        // PX-099: 4000 numbered lines, each written and flushed on its own
        // with a pause between, so the broker's reader stores a record of a
        // few bytes per line (what a PTY gives a process that prints a line
        // at a time).
        "tiny-lines" => {
            for i in 0..4000u32 {
                writeln!(out, "{i}").unwrap();
                out.flush().unwrap();
                std::thread::sleep(Duration::from_micros(300));
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
                #[cfg(unix)]
                let text = std::process::Command::new("stty")
                    .arg("size")
                    .stdin(std::process::Stdio::inherit())
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
                    .unwrap_or_else(|e| format!("stty failed: {e}"));
                #[cfg(windows)]
                // A ConPTY child has no `stty`; the window rectangle of the
                // console the pseudoconsole gave it is the size.
                #[allow(unsafe_code)]
                let text = unsafe {
                    use windows_sys::Win32::System::Console::{
                        CONSOLE_SCREEN_BUFFER_INFO, GetConsoleScreenBufferInfo, GetStdHandle,
                        STD_OUTPUT_HANDLE,
                    };
                    let h = GetStdHandle(STD_OUTPUT_HANDLE);
                    let mut info: CONSOLE_SCREEN_BUFFER_INFO = std::mem::zeroed();
                    if GetConsoleScreenBufferInfo(h, &mut info) != 0 {
                        let rows = info.srWindow.Bottom - info.srWindow.Top + 1;
                        let cols = info.srWindow.Right - info.srWindow.Left + 1;
                        format!("{rows} {cols}")
                    } else {
                        format!("size-probe failed: {}", std::io::Error::last_os_error())
                    }
                };
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
        // WIN-04: ask the terminal where the cursor is (DSR `ESC[6n`). The
        // ConPTY broker answers that itself with `ESC[1;1R`; the verdict
        // says which answer arrived, so a silent terminal is named, not hung.
        "dsr-probe" => {
            use std::io::Read;
            out.write_all(&[0x1b, b'[', b'6', b'n']).unwrap();
            out.flush().unwrap();
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let mut stdin = std::io::stdin();
                let mut buf = [0u8; 32];
                loop {
                    match stdin.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if tx.send(buf[..n].to_vec()).is_err() {
                                break;
                            }
                        }
                    }
                }
            });
            // A cooked-mode line never completes without a carriage
            // return, and the reply has none: read the console in raw mode.
            #[cfg(windows)]
            {
                #[allow(unsafe_code)]
                unsafe {
                    use windows_sys::Win32::System::Console::{
                        ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
                        GetConsoleMode, GetStdHandle, STD_INPUT_HANDLE, SetConsoleMode,
                    };
                    let h = GetStdHandle(STD_INPUT_HANDLE);
                    let mut mode = 0u32;
                    if GetConsoleMode(h, &mut mode) != 0 {
                        let raw = mode
                            & !(ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT);
                        let _ = SetConsoleMode(h, raw);
                    }
                }
            }
            let mut got = Vec::new();
            while let Ok(chunk) = rx.recv_timeout(Duration::from_secs(5)) {
                got.extend(chunk);
                if got.contains(&b'R') {
                    break;
                }
            }
            let answered = got.starts_with(&[0x1b, b'[', b'1', b';', b'1', b'R']);
            let verdict = if answered { "REPLY-1;1R" } else { "OTHER" };
            writeln!(out, "DSR:{verdict} got={got:?}").unwrap();
        }
        // WIN-04: a grandchild of this grandchild-spawner's own choosing:
        // it detaches (Windows DETACHED_PROCESS) so only the session's job
        // object — not pipe closure — can end it, writes its pid to
        // `MODBIT_EXECD_TEST_PID_FILE`, and sleeps until killed.
        // The grandchild is detached by design (that is what the WIN-04
        // tests prove the broker can end); it is never waited on.
        #[allow(clippy::zombie_processes)]
        "spawns-grandchild-and-exits" | "spawns-grandchild-and-runs" => {
            let marker = std::env::var("MODBIT_EXECD_TEST_PID_FILE").unwrap_or_default();
            let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
            cmd.env("MODBIT_EXECD_TEST_ROLE", "sleep-until-killed")
                .env("MODBIT_EXECD_TEST_PID_FILE", &marker)
                // Null stdio: the grandchild must not hold the session's
                // output pipes, or the exit observation waits for its death.
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                // DETACHED_PROCESS: no console, no pipe inheritance — a
                // grandchild only the job object reaches.
                cmd.creation_flags(0x0000_0008);
            }
            let g = cmd.spawn().unwrap();
            writeln!(out, "grandchild {}", g.id()).unwrap();
            out.flush().unwrap();
            if role == "spawns-grandchild-and-exits" {
                std::process::exit(0);
            }
            loop {
                writeln!(out, "parent-alive").unwrap();
                out.flush().unwrap();
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        "sleep-until-killed" => {
            if let Ok(f) = std::env::var("MODBIT_EXECD_TEST_PID_FILE")
                && !f.is_empty()
            {
                let _ = std::fs::write(f, std::process::id().to_string());
            }
            // Bounded (two minutes): long enough to prove a survivor when a
            // kill path leaks, short enough that a red run leaks nothing.
            for _ in 0..120 {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        other => {
            eprintln!("unknown role {other}");
            std::process::exit(2);
        }
    }
}
