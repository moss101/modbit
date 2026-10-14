//! The AppKit fixture application of the macOS actuator's real-permission
//! tests: built with `swiftc` from `fixtures/axfixture/main.swift`, wrapped in
//! an `.app` bundle (so its bundle identifier is real), ad-hoc signed, and
//! launched as a background (accessory) process. Shared by the helper's own
//! tests and the Core's (`services/modbit-core/tests/px_computer.rs` includes
//! this file by path).
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// The fixture application's bundle identifier.
pub const BUNDLE_ID: &str = "org.modbit.axfixture";

pub fn fixture_app() -> &'static PathBuf {
    static APP: OnceLock<PathBuf> = OnceLock::new();
    APP.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("modbit-axfixture-{}", std::process::id()));
        let exe_dir = dir.join("AxFixture.app/Contents/MacOS");
        std::fs::create_dir_all(&exe_dir).unwrap();
        // The helper crate's own tests find it beside them; the Core's tests
        // (which include this file by path) find it in the helper crate.
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let src = [
            manifest.join("tests/fixtures/axfixture/main.swift"),
            manifest.join("../modbit-actuator-macos/tests/fixtures/axfixture/main.swift"),
        ]
        .into_iter()
        .find(|p| p.exists())
        .expect("fixtures/axfixture/main.swift");
        let st = Command::new("swiftc")
            .args(["-O", "-o"])
            .arg(exe_dir.join("AxFixture"))
            .arg(&src)
            .status()
            .expect("swiftc is part of the macOS developer tools");
        assert!(st.success(), "building the fixture application");
        std::fs::write(
            dir.join("AxFixture.app/Contents/Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>org.modbit.axfixture</string>
<key>CFBundleName</key><string>AxFixture</string>
<key>CFBundleExecutable</key><string>AxFixture</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSUIElement</key><true/>
</dict></plist>"#,
        )
        .unwrap();
        let st = Command::new("codesign")
            .args(["--force", "--sign", "-"])
            .arg(dir.join("AxFixture.app"))
            .status()
            .expect("codesign");
        assert!(st.success());
        dir.join("AxFixture.app")
    })
}

pub struct Fixture {
    child: Child,
    pub state: PathBuf,
    _dir: tempfile::TempDir,
}

impl Fixture {
    pub fn launch() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state");
        let child = Command::new(fixture_app().join("Contents/MacOS/AxFixture"))
            .arg(&state)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("launch the fixture application");
        let f = Self {
            child,
            state,
            _dir: dir,
        };
        let t0 = Instant::now();
        while f.get("ready").as_deref() != Some("1") {
            assert!(
                t0.elapsed() < Duration::from_secs(20),
                "the fixture application did not start"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        f
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub fn get(&self, key: &str) -> Option<String> {
        let t = std::fs::read_to_string(&self.state).ok()?;
        t.lines()
            .find_map(|l| l.strip_prefix(&format!("{key}=")).map(str::to_owned))
    }

    /// Epoch-ms timestamps of every keyDown the application received.
    pub fn keys(&self) -> Vec<f64> {
        let p = format!("{}.keys", self.state.display());
        std::fs::read_to_string(p)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.parse().ok())
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
