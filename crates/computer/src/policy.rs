//! What no approval can lift, and what administrative policy restricts
//! (docs/66 CUC-C04, CUC-C05).
//!
//! The list of applications that are never driven is *data owned by policy*:
//! versioned, shown to the person (`ComputerRuntimeView.non_drivable`) and
//! consulted before any approval is asked, so a person is never invited to
//! approve what cannot be done. It names applications that hold secrets or
//! control the machine: credential stores, the system settings and security
//! panes, terminal and shell hosts, the operating system's own authentication
//! dialogs and Modbit itself. Identity is the bundle identifier and the
//! executable, never a window title.

use std::collections::BTreeSet;

use crate::model::{AppIdentity, Scope};

/// The version of the list below. A change to the list changes this.
pub const NON_DRIVABLE_VERSION: &str = "2026-10-10.1";

/// One entry: an identity pattern and why it is not drivable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    /// A bundle identifier (`com.apple.Terminal`), a prefix pattern ending in
    /// `*` (`com.1password.*`), or `exe:<file name>` for an executable's file
    /// name. Matching is case-insensitive.
    pub pattern: &'static str,
    /// Why.
    pub why: &'static str,
}

const fn e(pattern: &'static str, why: &'static str) -> Entry {
    Entry { pattern, why }
}

/// Applications that hold secrets or control the machine (CUC-C05).
pub const NON_DRIVABLE: &[Entry] = &[
    // Credential stores.
    e("com.apple.keychainaccess", "a credential store"),
    e("com.apple.passwords", "a credential store"),
    e("com.1password.*", "a credential store"),
    e("com.agilebits.*", "a credential store"),
    e("com.bitwarden.*", "a credential store"),
    e("org.keepassxc.*", "a credential store"),
    e("com.lastpass.*", "a credential store"),
    e("com.dashlane.*", "a credential store"),
    // System settings and security panes.
    e(
        "com.apple.systempreferences",
        "system settings and security panes",
    ),
    e("com.apple.settings.*", "system settings and security panes"),
    e(
        "com.apple.securityagent",
        "an operating system authentication dialog",
    ),
    e(
        "com.apple.coreservices.uiagent",
        "an operating system authentication dialog",
    ),
    e("com.apple.loginwindow", "the login window"),
    e("com.apple.screensaver.*", "the lock screen"),
    e("com.apple.activitymonitor", "process control"),
    e("com.apple.disk-utility", "disk control"),
    e(
        "exe:SystemSettings.exe",
        "system settings and security panes",
    ),
    e("exe:LogonUI.exe", "the logon screen"),
    e(
        "exe:consent.exe",
        "an operating system authentication dialog",
    ),
    e(
        "exe:gnome-control-center",
        "system settings and security panes",
    ),
    e(
        "exe:polkit-gnome-authentication-agent-1",
        "an operating system authentication dialog",
    ),
    // Terminal and shell hosts.
    e("com.apple.terminal", "a terminal and shell host"),
    e("com.googlecode.iterm2", "a terminal and shell host"),
    e("dev.warp.*", "a terminal and shell host"),
    e("com.mitchellh.ghostty", "a terminal and shell host"),
    e("net.kovidgoyal.kitty", "a terminal and shell host"),
    e("org.alacritty", "a terminal and shell host"),
    e("com.github.wez.wezterm", "a terminal and shell host"),
    e("org.wezfurlong.*", "a terminal and shell host"),
    e("exe:WindowsTerminal.exe", "a terminal and shell host"),
    e("exe:cmd.exe", "a terminal and shell host"),
    e("exe:powershell.exe", "a terminal and shell host"),
    e("exe:pwsh.exe", "a terminal and shell host"),
    e("exe:gnome-terminal", "a terminal and shell host"),
    e("exe:gnome-terminal-server", "a terminal and shell host"),
    e("exe:konsole", "a terminal and shell host"),
    e("exe:xterm", "a terminal and shell host"),
    // Modbit itself: an agent never drives the application that governs it.
    e("dev.modbit.*", "Modbit itself"),
    e("com.modbit.*", "Modbit itself"),
    e("io.modbit.*", "Modbit itself"),
    e("exe:modbit-core", "Modbit itself"),
    e("exe:modbit-desktop", "Modbit itself"),
];

/// Case-insensitive match of a pattern against a value: an exact name, or a
/// prefix when the pattern ends in `*`.
#[must_use]
pub fn matches(pattern: &str, value: &str) -> bool {
    if pattern.is_empty() || value.is_empty() {
        return false;
    }
    let (p, v) = (pattern.to_ascii_lowercase(), value.to_ascii_lowercase());
    match p.strip_suffix('*') {
        Some(prefix) => v.starts_with(prefix),
        None => v == p,
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Why `app` is never driven, when it is one of the listed applications or
/// is this very process.
#[must_use]
pub fn non_drivable(app: &AppIdentity, own_pid: u32) -> Option<&'static str> {
    if own_pid != 0 && app.pid == own_pid {
        return Some("Modbit itself");
    }
    let exe = file_name(&app.executable_path);
    NON_DRIVABLE.iter().find_map(|entry| {
        let hit = match entry.pattern.strip_prefix("exe:") {
            Some(name) => matches(name, exe),
            None => matches(entry.pattern, &app.bundle_id),
        };
        hit.then_some(entry.why)
    })
}

/// The list as the person sees it: `<pattern>: <why>`.
#[must_use]
pub fn listing() -> Vec<String> {
    NON_DRIVABLE
        .iter()
        .map(|entry| format!("{}: {}", entry.pattern, entry.why))
        .collect()
}

/// What administrative policy decided for computer control, as the Core
/// resolves it for one call from the configuration in force (CUC-C04). The
/// default admits everything the capabilities admit; a verdict the Core
/// could not read never reaches this type — the call is refused before it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComputerPolicy {
    /// Observation is denied.
    pub observe_denied: bool,
    /// Actions and control sessions are denied.
    pub act_denied: bool,
    /// SCREEN scope is denied.
    pub screen_denied: bool,
    /// Only these applications (bundle identifier patterns) may be driven;
    /// `None` = no restriction.
    pub apps_allow: Option<BTreeSet<String>>,
}

impl ComputerPolicy {
    /// Whether policy admits driving `app`. An allow-list with an application
    /// whose bundle identifier is unknown fails closed.
    ///
    /// # Errors
    /// Why not.
    pub fn admits_app(&self, app: &AppIdentity) -> Result<(), String> {
        let Some(allow) = &self.apps_allow else {
            return Ok(());
        };
        if app.bundle_id.is_empty() {
            return Err(
                "policy restricts computer control to listed applications and this one has no bundle identity to check".into(),
            );
        }
        if allow.iter().any(|p| matches(p, &app.bundle_id)) {
            Ok(())
        } else {
            Err(format!(
                "policy allows computer control only for: {}; `{}` is not among them",
                allow.iter().cloned().collect::<Vec<_>>().join(", "),
                app.bundle_id
            ))
        }
    }

    /// Whether policy admits `scope`.
    ///
    /// # Errors
    /// Why not.
    pub fn admits_scope(&self, scope: Scope) -> Result<(), String> {
        if scope == Scope::Screen && self.screen_denied {
            return Err(
                "policy forbids SCREEN scope (it takes the display and the real cursor)".into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(bundle: &str, exe: &str) -> AppIdentity {
        AppIdentity {
            bundle_id: bundle.into(),
            executable_path: exe.into(),
            signing_identity: "TEAM".into(),
            pid: 4242,
            name: "x".into(),
        }
    }

    #[test]
    fn listed_applications_are_never_drivable_by_identity_not_by_title() {
        assert_eq!(
            non_drivable(
                &app(
                    "com.apple.Terminal",
                    "/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal"
                ),
                1
            ),
            Some("a terminal and shell host")
        );
        assert_eq!(
            non_drivable(&app("com.1password.1password", "/x"), 1),
            Some("a credential store")
        );
        assert_eq!(
            non_drivable(&app("dev.modbit.desktop", "/x"), 1),
            Some("Modbit itself")
        );
        assert_eq!(
            non_drivable(&app("", "C:\\Windows\\System32\\cmd.exe"), 1),
            Some("a terminal and shell host")
        );
        assert_eq!(
            non_drivable(&app("org.example.editor", "/x/editor"), 1),
            None
        );
        // This process is never drivable, whatever it says it is.
        let mut me = app("org.example.editor", "/x/editor");
        me.pid = 77;
        assert_eq!(non_drivable(&me, 77), Some("Modbit itself"));
        assert!(
            listing()
                .iter()
                .any(|l| l.starts_with("com.apple.terminal:"))
        );
    }

    #[test]
    fn an_allow_list_restricts_by_bundle_and_an_unknown_identity_fails_closed() {
        let p = ComputerPolicy {
            apps_allow: Some(
                [
                    "org.example.form".to_owned(),
                    "org.example.docs.*".to_owned(),
                ]
                .into_iter()
                .collect(),
            ),
            screen_denied: true,
            ..ComputerPolicy::default()
        };
        assert!(p.admits_app(&app("org.example.form", "/x")).is_ok());
        assert!(p.admits_app(&app("org.example.docs.writer", "/x")).is_ok());
        assert!(
            p.admits_app(&app("org.example.other", "/x"))
                .unwrap_err()
                .contains("not among them")
        );
        assert!(
            p.admits_app(&app("", "/x"))
                .unwrap_err()
                .contains("no bundle identity")
        );
        assert!(p.admits_scope(Scope::Screen).is_err());
        assert!(p.admits_scope(Scope::App).is_ok());
        assert!(ComputerPolicy::default().admits_app(&app("", "/x")).is_ok());
    }
}
