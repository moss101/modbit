//! Shared by the real-process qualification of PX-148 (QUAL-PX-148): the same
//! cases run against the real `modbit-core` and the real `modbit-execd`
//! binaries. Included with `#[path]` by `services/modbit-core/tests/px_148_hardening.rs`
//! and `services/modbit-execd/tests/px_148_hardening.rs`.
#![allow(dead_code, unsafe_code)]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The variable the platform's loader reads to load a library into a process.
pub const INJECT_VAR: &str = if cfg!(target_os = "macos") {
    "DYLD_INSERT_LIBRARIES"
} else {
    "LD_PRELOAD"
};

const PLANT_SOURCE: &str = r#"
#include <stdio.h>
#include <stdlib.h>
__attribute__((constructor)) static void planted(void) {
    const char *m = getenv("MODBIT_PLANT_MARKER");
    if (m) { FILE *f = fopen(m, "a"); if (f) { fputs("loaded\n", f); fclose(f); } }
}
"#;

/// Build a shared library whose constructor appends a line to the file named by
/// `MODBIT_PLANT_MARKER` when the loader runs it.
pub fn plant_library(dir: &Path) -> PathBuf {
    let src = dir.join("plant.c");
    std::fs::write(&src, PLANT_SOURCE).expect("write plant.c");
    let lib = dir.join(if cfg!(target_os = "macos") {
        "libplant.dylib"
    } else {
        "libplant.so"
    });
    let mut cc = Command::new(std::env::var("CC").unwrap_or_else(|_| "cc".into()));
    if cfg!(target_os = "macos") {
        cc.arg("-dynamiclib");
    } else {
        cc.args(["-shared", "-fPIC"]);
    }
    let st = cc.arg("-o").arg(&lib).arg(&src).status().expect("run cc");
    assert!(st.success(), "the C compiler built the planted library");
    lib
}

pub fn marker_lines(marker: &Path) -> usize {
    std::fs::read_to_string(marker)
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

/// Run `bin --hardening-report` with `env` added and return its JSON, its
/// stderr and its exit success.
pub fn report(bin: &str, env: &[(&str, &Path)]) -> (serde_json::Value, String) {
    let mut cmd = Command::new(bin);
    cmd.arg("--hardening-report");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run --hardening-report");
    assert!(out.status.success(), "report exits 0: {:?}", out.status);
    let json = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "report is JSON ({e}): {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    (json, String::from_utf8_lossy(&out.stderr).into_owned())
}

/// `bin --hardening-report` as a child that is given the planted library.
pub fn report_with_plant(bin: &str, lib: &Path, marker: &Path) -> (serde_json::Value, String) {
    let mut cmd = Command::new(bin);
    cmd.arg("--hardening-report")
        .env(INJECT_VAR, lib)
        .env("MODBIT_PLANT_MARKER", marker);
    let out = cmd
        .output()
        .expect("run --hardening-report with a planted library");
    assert!(out.status.success(), "report exits 0: {:?}", out.status);
    let json = serde_json::from_slice(&out.stdout).expect("report is JSON");
    (json, String::from_utf8_lossy(&out.stderr).into_owned())
}

/// Start `cmd` (a long-running server) and wait until it prints a line on
/// stdout (its ready line) or `timeout` passes; the child is returned running.
pub fn start_running(mut cmd: Command, timeout: Duration) -> Server {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn the server");
    let out = child.stdout.take().expect("stdout");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut lines = BufReader::new(out).lines();
        if let Some(Ok(l)) = lines.next() {
            let _ = tx.send(l);
        }
        for _ in lines {}
    });
    let server = Server(child);
    rx.recv_timeout(timeout)
        .expect("the server printed its ready line");
    server
}

/// A running server that is killed when the test ends, even by a panic.
pub struct Server(pub Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn wait_for<F: FnMut() -> bool>(timeout: Duration, mut f: F) -> bool {
    let t = Instant::now();
    while t.elapsed() < timeout {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    f()
}

/// Linux: the core-dump limit of the running process, read from `/proc`.
#[cfg(target_os = "linux")]
pub fn proc_core_limit(pid: u32) -> (String, String) {
    let limits = std::fs::read_to_string(format!("/proc/{pid}/limits")).expect("limits");
    let line = limits
        .lines()
        .find(|l| l.starts_with("Max core file size"))
        .expect("core line");
    let cols: Vec<&str> = line.split_whitespace().collect();
    // "Max core file size <soft> <hard> bytes"
    (cols[4].to_owned(), cols[5].to_owned())
}

/// The shared assertions, given the path of the binary and a way to start it
/// as a server.
pub fn run_qualification(bin: &str, server: impl FnOnce() -> Command) {
    let dir = tempfile::tempdir().expect("tempdir");

    // 1. The state in a fresh process, read from the process itself.
    let (r, stderr) = report(bin, &[]);
    assert_eq!(r["core_limit_soft"], 0, "{r}");
    assert_eq!(
        r["core_limit_hard"], 0,
        "the hard limit is zero too, so it cannot be raised: {r}"
    );
    #[cfg(target_os = "linux")]
    {
        assert_eq!(r["dumpable"], 0, "PR_SET_DUMPABLE is 0: {r}");
        assert!(
            r["applied"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a == "not_dumpable"),
            "{r}"
        );
    }
    #[cfg(target_os = "macos")]
    assert!(
        r["applied"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a == "attach_denied"),
        "PT_DENY_ATTACH took effect: {r}"
    );
    assert!(
        r["applied"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a == "core_dump_limit_zero"),
        "{r}"
    );
    assert_eq!(r["failed"], serde_json::json!([]), "{r}");
    assert!(
        !stderr.contains("hardening:"),
        "no protection failed on a supported platform, and none was skipped silently: {stderr}"
    );

    // 2. The running server (the process that holds the keys), read from outside.
    let child = start_running(server(), Duration::from_secs(60));
    #[cfg(target_os = "linux")]
    let pid = child.0.id();
    #[cfg(target_os = "linux")]
    {
        let (soft, hard) = proc_core_limit(pid);
        assert_eq!(
            (soft.as_str(), hard.as_str()),
            ("0", "0"),
            "/proc limits of the running process"
        );
        // A non-dumpable process's /proc entries are owned by root, so a
        // same-user reader cannot open its environ or maps (unless this test
        // runs as root, which can read everything).
        // SAFETY: `geteuid` has no preconditions.
        if unsafe { libc::geteuid() } != 0 {
            assert!(
                std::fs::read(format!("/proc/{pid}/environ")).is_err(),
                "a same-user process cannot read the hardened process's environment"
            );
        }
    }
    drop(child);

    // 3. A planted library. First the control: without the hardening, the
    //    platform loader does load it, so the assertions below prove something.
    let lib = plant_library(dir.path());
    let control_marker = dir.path().join("control.marker");
    let control = Command::new(std::env::current_exe().expect("test exe"))
        .arg("--list")
        .env(INJECT_VAR, &lib)
        .env("MODBIT_PLANT_MARKER", &control_marker)
        .output()
        .expect("control run");
    assert!(control.status.success());
    assert_eq!(
        marker_lines(&control_marker),
        1,
        "control: the loader runs the planted library in an unhardened process"
    );

    // 4. The hardened process given the same variable: the serving image has
    //    no injection variable and the library is not mapped in it. The
    //    library's constructor ran at most once, in the image that was replaced.
    let marker = dir.path().join("hardened.marker");
    let (r, _stderr) = report_with_plant(bin, &lib, &marker);
    assert_eq!(
        r["loader_env"],
        serde_json::json!([]),
        "no LD_*/DYLD_* left: {r}"
    );
    assert_eq!(
        r["reexec_mark"], false,
        "the re-execution mark is removed: {r}"
    );
    let images = r["images"].as_array().expect("images");
    assert!(
        !images
            .iter()
            .any(|i| i.as_str().is_some_and(|p| p.contains("libplant"))),
        "the planted library is not loaded in the serving process: {images:?}"
    );
    assert!(
        marker_lines(&marker) <= 1,
        "the library never ran in the serving image (at most the replaced one): {}",
        marker_lines(&marker)
    );
    assert_eq!(
        r["core_limit_soft"], 0,
        "and the hardening still applied after the re-execution: {r}"
    );

    // 5. An unrelated loader variable is also gone from the serving image.
    let other = if cfg!(target_os = "macos") {
        "DYLD_LIBRARY_PATH"
    } else {
        "LD_LIBRARY_PATH"
    };
    let (r, _stderr) = report(bin, &[(other, dir.path())]);
    assert_eq!(
        r["loader_env"],
        serde_json::json!([]),
        "{other} is removed: {r}"
    );
}
