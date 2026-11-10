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

/// The fixture application's bundle identifier (the form application).
pub const BUNDLE_ID: &str = "org.modbit.axfixture";

/// The applications of the fixture suite (QUAL-PX-069): one Swift source, one
/// bundle each, so each has its own real bundle identifier and process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A counter, a Name field, a password field and a checkbox.
    Form,
    /// A multi-line text document with a Save button.
    Document,
    /// A Delete... button that opens a modal sheet.
    Dialog,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Form, Kind::Document, Kind::Dialog];

    pub fn bundle_id(self) -> &'static str {
        match self {
            Kind::Form => BUNDLE_ID,
            Kind::Document => "org.modbit.axfixture.document",
            Kind::Dialog => "org.modbit.axfixture.dialog",
        }
    }

    fn mode(self) -> &'static str {
        match self {
            Kind::Form => "form",
            Kind::Document => "document",
            Kind::Dialog => "dialog",
        }
    }

    fn bundle_name(self) -> &'static str {
        match self {
            Kind::Form => "AxFixture",
            Kind::Document => "AxFixtureDocument",
            Kind::Dialog => "AxFixtureDialog",
        }
    }
}

/// The form application's bundle (built with the rest of the suite).
pub fn fixture_app() -> &'static PathBuf {
    &fixture_apps()[0]
}

/// The bundle of one application of the suite.
pub fn fixture_app_of(kind: Kind) -> &'static PathBuf {
    let i = Kind::ALL.iter().position(|k| *k == kind).unwrap();
    &fixture_apps()[i]
}

fn fixture_apps() -> &'static Vec<PathBuf> {
    static APPS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    APPS.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("modbit-axfixture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
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
        let exe = dir.join("axfixture");
        let st = Command::new("swiftc")
            .args(["-O", "-o"])
            .arg(&exe)
            .arg(&src)
            .status()
            .expect("swiftc is part of the macOS developer tools");
        assert!(st.success(), "building the fixture application");
        Kind::ALL
            .iter()
            .map(|k| {
                let name = k.bundle_name();
                let app = dir.join(format!("{name}.app"));
                let exe_dir = app.join("Contents/MacOS");
                std::fs::create_dir_all(&exe_dir).unwrap();
                std::fs::copy(&exe, exe_dir.join(name)).unwrap();
                std::fs::write(
                    app.join("Contents/Info.plist"),
                    format!(
                        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>{id}</string>
<key>CFBundleName</key><string>{name}</string>
<key>CFBundleExecutable</key><string>{name}</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSUIElement</key><true/>
</dict></plist>"#,
                        id = k.bundle_id()
                    ),
                )
                .unwrap();
                let st = Command::new("codesign")
                    .args(["--force", "--sign", "-"])
                    .arg(&app)
                    .status()
                    .expect("codesign");
                assert!(st.success());
                app
            })
            .collect()
    })
}

pub struct Fixture {
    child: Child,
    pub state: PathBuf,
    _dir: tempfile::TempDir,
}

impl Fixture {
    pub fn launch() -> Self {
        Self::launch_kind(Kind::Form)
    }

    /// Launch one application of the suite and wait until it says it is ready.
    pub fn launch_kind(kind: Kind) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state");
        let exe = fixture_app_of(kind)
            .join("Contents/MacOS")
            .join(kind.bundle_name());
        let child = Command::new(exe)
            .arg(&state)
            .arg(kind.mode())
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
            .filter_map(|l| l.split(' ').next()?.parse().ok())
            .collect()
    }

    /// The first character of every keyDown the application received, in order.
    pub fn key_chars(&self) -> Vec<char> {
        let p = format!("{}.keys", self.state.display());
        std::fs::read_to_string(p)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split_once(' ')?.1.chars().next())
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
