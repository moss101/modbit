//! The guest's browser (M8.8, docs/22 "Cloud browser"): one headless
//! Chromium per guest, started on the policy's say-so, its DevTools
//! endpoint bound to the guest's loopback and reached only through the
//! gateway (a link the gateway turns into a raw relay — `browser.forward`).
//! Its traffic leaves like any process's: through the local egress proxy,
//! never by an interface of its own (the loopback is bypassed nowhere, so
//! even `127.0.0.1` goes to the broker, which decides).
//!
//! Where Chromium is: `--chromium <path>` (the reference backend names the
//! host's), else the image's `/opt/chromium/chrome-headless-shell` or
//! `/opt/chromium/chrome`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static CHROMIUM: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
static RUNNING: Mutex<Option<(Running, tokio::process::Child)>> = Mutex::new(None);

/// Name the Chromium binary (once, at start).
pub fn set_chromium(path: PathBuf) {
    let _ = CHROMIUM.set(path);
}

/// The Chromium this guest can run, if any.
fn chromium() -> Option<PathBuf> {
    if let Some(p) = CHROMIUM.get() {
        return Some(p.clone());
    }
    [
        "/opt/chromium/chrome-headless-shell",
        "/opt/chromium/chrome",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.is_file())
}

/// What is running.
#[derive(Clone, Debug)]
pub struct Running {
    /// The DevTools port on the guest's loopback.
    pub port: u16,
    /// `/devtools/browser/<id>`.
    pub ws_path: String,
    /// The browser process.
    pub pid: u32,
}

/// The running browser, if any (and still alive).
pub fn running() -> Option<Running> {
    let mut g = RUNNING.lock().expect("browser");
    let alive = match g.as_mut() {
        Some((_, child)) => matches!(child.try_wait(), Ok(None)),
        None => false,
    };
    if !alive {
        *g = None;
        return None;
    }
    g.as_ref().map(|(r, _)| r.clone())
}

/// Start the browser (or report the one running). `proxy`: the local
/// egress proxy every request goes through, when egress is granted; with
/// none, the browser is pointed at a dead proxy so nothing leaves.
pub async fn start(
    width: u32,
    height: u32,
    proxy: Option<&str>,
    data_dir: &Path,
) -> Result<(Running, bool), (&'static str, String)> {
    if let Some(r) = running() {
        return Ok((r, true));
    }
    let Some(bin) = chromium() else {
        return Err((
            "BROWSER_UNAVAILABLE",
            "this guest has no Chromium (no --chromium and none in the image)".into(),
        ));
    };
    let width = if width == 0 { 1024 } else { width.min(4096) };
    let height = if height == 0 { 768 } else { height.min(4096) };
    let _ = std::fs::remove_dir_all(data_dir);
    std::fs::create_dir_all(data_dir).map_err(|e| ("BAD_CALL", format!("data dir: {e}")))?;
    let mut cmd = tokio::process::Command::new(&bin);
    cmd.arg("--headless=new")
        .arg("--no-sandbox")
        .arg("--disable-gpu")
        .arg("--disable-dev-shm-usage")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-extensions")
        .arg("--disable-background-networking")
        .arg("--disable-sync")
        .arg("--disable-crash-reporter")
        .arg("--no-zygote")
        .arg("--remote-debugging-port=0")
        .arg("--remote-allow-origins=*")
        .arg(format!("--user-data-dir={}", data_dir.display()))
        .arg(format!("--window-size={width},{height}"))
        // Everything goes through the proxy — the loopback too (`<-loopback>`
        // removes Chromium's implicit bypass), so `127.0.0.1` means the
        // host's, through the broker, exactly as it does for wget.
        .arg(format!(
            "--proxy-server=http://{}",
            proxy.unwrap_or("127.0.0.1:1")
        ))
        .arg("--proxy-bypass-list=<-loopback>")
        .arg("about:blank")
        .env_clear()
        .env("HOME", data_dir)
        .env("TMPDIR", "/tmp")
        .env("PATH", "/usr/bin:/bin")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::from(
            std::fs::File::create(data_dir.join("chromium.log"))
                .map_err(|e| ("BAD_CALL", format!("log: {e}")))?,
        ));
    #[cfg(windows)]
    {
        for k in ["SystemRoot", "TEMP", "TMP", "LOCALAPPDATA"] {
            if let Ok(v) = std::env::var(k) {
                cmd.env(k, v);
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        cmd.env("XDG_RUNTIME_DIR", "/tmp");
        cmd.env("FONTCONFIG_PATH", "/etc/fonts");
    }
    let mut child = cmd.spawn().map_err(|e| {
        (
            "BROWSER_FAILED",
            format!("spawning `{}`: {e}", bin.display()),
        )
    })?;
    let pid = child.id().unwrap_or(0);
    // Chromium writes `DevToolsActivePort` (the port, then the browser
    // target's path) into the data dir once its DevTools server listens.
    let marker = data_dir.join("DevToolsActivePort");
    let started = Instant::now();
    let parsed = loop {
        if let Ok(text) = std::fs::read_to_string(&marker) {
            let mut lines = text.lines();
            if let (Some(port), Some(path)) = (lines.next(), lines.next())
                && let Ok(port) = port.trim().parse::<u16>()
                && port != 0
            {
                break Some((port, path.trim().to_owned()));
            }
        }
        if let Ok(Some(status)) = child.try_wait() {
            let log = std::fs::read_to_string(data_dir.join("chromium.log")).unwrap_or_default();
            let tail: String = log
                .chars()
                .rev()
                .take(1500)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            return Err((
                "BROWSER_FAILED",
                format!("chromium exited ({status}) before its DevTools listened: {tail}"),
            ));
        }
        if started.elapsed() > Duration::from_secs(30) {
            let _ = child.start_kill();
            break None;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let Some((port, ws_path)) = parsed else {
        let log = std::fs::read_to_string(data_dir.join("chromium.log")).unwrap_or_default();
        let tail: String = log
            .chars()
            .rev()
            .take(1500)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return Err((
            "BROWSER_FAILED",
            format!("chromium's DevTools did not listen within 30 s: {tail}"),
        ));
    };
    let running = Running { port, ws_path, pid };
    *RUNNING.lock().expect("browser") = Some((running.clone(), child));
    Ok((running, false))
}

/// Stop the browser, if one runs.
pub fn stop() -> bool {
    let mut g = RUNNING.lock().expect("browser");
    let Some((_, mut child)) = g.take() else {
        return false;
    };
    let _ = child.start_kill();
    true
}
