//! Standing computer access per application (PX-215, REQ-PX-215,
//! DR-PX-2026-11-11-021 MWS-19).
//!
//! A person can say, once, how far the agent may go in one application: look
//! only, click, do everything the grant allows, or never touch it. The level
//! is held by the Core (this store is the runtime's, hydrated from the log at
//! boot and changed only through a recorded event) and the computer runtime
//! enforces it beside the observation grant and the per-call approval.
//!
//! Three rules make it safe, and each is a test:
//!
//! * **It only narrows.** Nothing here grants anything. A level is one more
//!   refusal a call can meet after the checks that already exist (a live
//!   grant, the approval, administrative policy, the list of applications
//!   no approval lifts, the scope); `full` means "no standing restriction",
//!   not "no approval". An application that is never driven stays never
//!   driven at any level, and a level cannot be set for it above `never`.
//! * **Nothing is set by default.** An application with no level is decided
//!   by the grant alone, exactly as before this existed.
//! * **A change is audited and applies from the next action.** The store is
//!   read at every call (never cached in a session), and a change is an
//!   event on the log before it is in force.
//!
//! The level is keyed by the application's identity as the operating system
//! vouches for it - the bundle identifier, else the executable's file name -
//! never by a window title or a pattern: a level names one application.

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::model::AppIdentity;
use crate::taxonomy::{Code, Refusal};

/// How far the agent may go in one application. Ordered: each level allows
/// everything the one before it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// No session may be opened for the application, and an open one is
    /// refused from its next call.
    Never,
    /// Observation only: the interface tree, screenshots, waiting. No input.
    ViewOnly,
    /// Observation and pointer-style input: pressing an element, clicking,
    /// moving, dragging, scrolling. No text and no keys.
    Click,
    /// Everything the grant allows, including typing, setting a value and
    /// key chords. Every input is still approved one by one.
    Full,
}

impl Level {
    /// Every level, narrowest first.
    pub const ALL: [Level; 4] = [Level::Never, Level::ViewOnly, Level::Click, Level::Full];

    /// The stable wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Level::Never => "never",
            Level::ViewOnly => "view_only",
            Level::Click => "click",
            Level::Full => "full",
        }
    }

    /// The name as a person says it.
    #[must_use]
    pub const fn words(self) -> &'static str {
        match self {
            Level::Never => "never",
            Level::ViewOnly => "view only",
            Level::Click => "click",
            Level::Full => "full",
        }
    }

    /// The level a wire name denotes.
    #[must_use]
    pub fn parse(name: &str) -> Option<Level> {
        Level::ALL.into_iter().find(|l| l.as_str() == name)
    }

    /// Whether this level lets a call of kind `need` through.
    #[must_use]
    pub fn allows(self, need: Need) -> bool {
        self >= need.least()
    }
}

/// What a call needs of the application: the least level that lets it run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// Opening a session, reading the tree, a screenshot, waiting.
    Observe,
    /// Pointer-style input.
    Click,
    /// Text and keys.
    Keyboard,
}

impl Need {
    /// The stable wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Need::Observe => "observe",
            Need::Click => "click",
            Need::Keyboard => "keyboard",
        }
    }

    /// The least level that allows it.
    #[must_use]
    pub const fn least(self) -> Level {
        match self {
            Need::Observe => Level::ViewOnly,
            Need::Click => Level::Click,
            Need::Keyboard => Level::Full,
        }
    }

    /// What the call does, in words.
    #[must_use]
    pub const fn words(self) -> &'static str {
        match self {
            Need::Observe => "looking",
            Need::Click => "clicking",
            Need::Keyboard => "typing",
        }
    }
}

/// The key an application is held under: its bundle identifier, else
/// `exe:<file name>`, lower case. Empty when the identity names neither.
#[must_use]
pub fn key_of(app: &AppIdentity) -> String {
    if !app.bundle_id.is_empty() {
        return app.bundle_id.to_ascii_lowercase();
    }
    let exe = app
        .executable_path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default();
    if exe.is_empty() {
        String::new()
    } else {
        format!("exe:{}", exe.to_ascii_lowercase())
    }
}

/// The longest key a level can be held under.
pub const MAX_KEY: usize = 200;

/// A key as a client may send it: trimmed, lower case, one application.
///
/// # Errors
/// Why it names no single application: empty, too long, holding whitespace
/// or control characters, or a pattern (`*`) - a level is never set for more
/// than one application at a time.
pub fn normalize_key(raw: &str) -> Result<String, String> {
    let key = raw.trim().to_ascii_lowercase();
    if key.is_empty() {
        return Err("an application's bundle identifier is required".into());
    }
    if key.chars().count() > MAX_KEY {
        return Err(format!(
            "the identifier is longer than {MAX_KEY} characters"
        ));
    }
    if key.contains('*') {
        return Err("a level is set for one application, never for a pattern".into());
    }
    if key.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("an identifier holds no spaces or control characters".into());
    }
    if key == "exe:" {
        return Err("`exe:` needs the executable's file name".into());
    }
    Ok(key)
}

/// One application's standing level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Standing {
    /// The key ([`key_of`]).
    pub key: String,
    /// The application's name when the level was set (an untrusted label).
    pub application: String,
    /// The level.
    pub level: Level,
    /// When it was set (ms since the epoch).
    pub set_at_ms: i64,
}

/// A change to the standing levels, as it is recorded on the log. The
/// payload carries the whole fact (`previous` included), so the log reads as
/// an audit without the store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum AccessEvent {
    /// `ComputerAccessSet`: the application's level is now `level`.
    ComputerAccessSet {
        /// The key.
        key: String,
        /// The application's name (an untrusted label).
        application: String,
        /// The new level.
        level: Level,
        /// What it was; `None` when none was set.
        previous: Option<Level>,
    },
    /// `ComputerAccessCleared`: the application has no standing level; the
    /// grant alone decides again.
    ComputerAccessCleared {
        /// The key.
        key: String,
        /// The application's name (an untrusted label).
        application: String,
        /// What it was.
        previous: Level,
    },
}

impl AccessEvent {
    /// The event type on the log.
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            AccessEvent::ComputerAccessSet { .. } => "ComputerAccessSet",
            AccessEvent::ComputerAccessCleared { .. } => "ComputerAccessCleared",
        }
    }

    /// Both event types, for a fold of the log.
    pub const TYPES: [&'static str; 2] = ["ComputerAccessSet", "ComputerAccessCleared"];
}

/// The standing levels in force. One per Core, shared with the runtime.
#[derive(Debug, Default)]
pub struct StandingAccess {
    inner: Mutex<BTreeMap<String, Standing>>,
}

impl StandingAccess {
    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Standing>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The level held for `key`, if one is.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<Level> {
        self.lock().get(key).map(|s| s.level)
    }

    /// The level held for `app`, if one is. `None` is "no standing level":
    /// the grant alone decides.
    #[must_use]
    pub fn level_of(&self, app: &AppIdentity) -> Option<Level> {
        let key = key_of(app);
        if key.is_empty() {
            return None;
        }
        self.get(&key)
    }

    /// Every level held, by key.
    #[must_use]
    pub fn list(&self) -> Vec<Standing> {
        self.lock().values().cloned().collect()
    }

    /// Fold one recorded change. The log is the authority: the Core calls
    /// this after the event is appended, and at boot for the whole log.
    pub fn apply(&self, event: &AccessEvent, at_ms: i64) {
        let mut m = self.lock();
        match event {
            AccessEvent::ComputerAccessSet {
                key,
                application,
                level,
                ..
            } => {
                m.insert(
                    key.clone(),
                    Standing {
                        key: key.clone(),
                        application: application.clone(),
                        level: *level,
                        set_at_ms: at_ms,
                    },
                );
            }
            AccessEvent::ComputerAccessCleared { key, .. } => {
                m.remove(key);
            }
        }
    }
}

/// The refusal for a call the application's level does not allow, with what
/// the card and the model need: the application, its level, what the call
/// needed and where the person changes it. Nothing was actuated.
#[must_use]
pub fn refusal(app: &AppIdentity, level: Level, need: Need) -> Refusal {
    let who = if app.bundle_id.is_empty() {
        key_of(app)
    } else {
        app.bundle_id.clone()
    };
    let message = match level {
        Level::Never => format!(
            "your standing access for `{who}` is never: no session may be opened for it, and an open one is refused from its next call"
        ),
        _ => format!(
            "your standing access for `{who}` is {}: {} needs {} or full",
            level.words(),
            need.words(),
            need.least().words()
        ),
    };
    Refusal::new(Code::ActionUnsafe, message).with_facts(serde_json::json!({
        "standing_access": {
            "application": who,
            "level": level.as_str(),
            "needed": need.as_str(),
            "changed_in": "Settings > Computer control",
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(bundle: &str, exe: &str) -> AppIdentity {
        AppIdentity {
            bundle_id: bundle.into(),
            executable_path: exe.into(),
            signing_identity: "TEAM".into(),
            pid: 7,
            name: "x".into(),
        }
    }

    #[test]
    fn levels_are_ordered_and_each_allows_what_the_one_before_it_does() {
        assert!(Level::Never < Level::ViewOnly);
        assert!(Level::ViewOnly < Level::Click);
        assert!(Level::Click < Level::Full);
        for need in [Need::Observe, Need::Click, Need::Keyboard] {
            let allowed: Vec<bool> = Level::ALL.iter().map(|l| l.allows(need)).collect();
            // Once a level allows a call, every wider level does.
            let first = allowed.iter().position(|a| *a);
            assert!(first.is_some(), "{need:?}");
            assert!(allowed[first.unwrap()..].iter().all(|a| *a), "{need:?}");
        }
        assert!(!Level::Never.allows(Need::Observe));
        assert!(Level::ViewOnly.allows(Need::Observe));
        assert!(!Level::ViewOnly.allows(Need::Click));
        assert!(!Level::ViewOnly.allows(Need::Keyboard));
        assert!(Level::Click.allows(Need::Click));
        assert!(!Level::Click.allows(Need::Keyboard));
        assert!(Level::Full.allows(Need::Keyboard));
        for l in Level::ALL {
            assert_eq!(Level::parse(l.as_str()), Some(l));
        }
        assert_eq!(Level::parse("admin"), None);
        assert_eq!(Level::parse(""), None);
    }

    #[test]
    fn an_application_is_held_by_bundle_identity_else_executable_never_by_a_pattern() {
        assert_eq!(
            key_of(&app("Com.Example.Notes", "/x/Notes")),
            "com.example.notes"
        );
        assert_eq!(
            key_of(&app("", "C:\\Windows\\System32\\Notepad.exe")),
            "exe:notepad.exe"
        );
        assert_eq!(key_of(&app("", "")), "");
        assert_eq!(
            normalize_key("  Com.Example.Notes ").unwrap(),
            "com.example.notes"
        );
        assert!(normalize_key("").is_err());
        assert!(normalize_key("com.example.*").is_err());
        assert!(normalize_key("*").is_err());
        assert!(normalize_key("com example").is_err());
        assert!(normalize_key("exe:").is_err());
        assert!(normalize_key(&"a".repeat(MAX_KEY + 1)).is_err());
    }

    #[test]
    fn the_store_folds_changes_and_an_unset_application_has_no_level() {
        let s = StandingAccess::default();
        let notes = app("com.example.notes", "/x/notes");
        assert_eq!(s.level_of(&notes), None, "nothing is set by default");
        s.apply(
            &AccessEvent::ComputerAccessSet {
                key: "com.example.notes".into(),
                application: "Notes".into(),
                level: Level::ViewOnly,
                previous: None,
            },
            10,
        );
        assert_eq!(s.level_of(&notes), Some(Level::ViewOnly));
        assert_eq!(s.level_of(&app("com.example.other", "/x/o")), None);
        // A look-alike executable with another bundle does not inherit it.
        assert_eq!(s.level_of(&app("com.example.notes2", "/x/notes")), None);
        s.apply(
            &AccessEvent::ComputerAccessSet {
                key: "com.example.notes".into(),
                application: "Notes".into(),
                level: Level::Full,
                previous: Some(Level::ViewOnly),
            },
            20,
        );
        assert_eq!(s.level_of(&notes), Some(Level::Full));
        assert_eq!(s.list()[0].set_at_ms, 20);
        s.apply(
            &AccessEvent::ComputerAccessCleared {
                key: "com.example.notes".into(),
                application: "Notes".into(),
                previous: Level::Full,
            },
            30,
        );
        assert_eq!(s.level_of(&notes), None);
        assert!(s.list().is_empty());
    }

    #[test]
    fn an_event_round_trips_through_the_log_payload() {
        let e = AccessEvent::ComputerAccessSet {
            key: "k".into(),
            application: "A".into(),
            level: Level::ViewOnly,
            previous: Some(Level::Full),
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["event_type"], "ComputerAccessSet");
        assert_eq!(v["level"], "view_only");
        assert_eq!(v["previous"], "full");
        assert_eq!(serde_json::from_value::<AccessEvent>(v).unwrap(), e);
        assert_eq!(e.event_type(), "ComputerAccessSet");
        assert!(AccessEvent::TYPES.contains(&e.event_type()));
    }

    #[test]
    fn a_refusal_says_the_level_what_was_needed_and_where_to_change_it() {
        let notes = app("com.example.notes", "/x/notes");
        let r = refusal(&notes, Level::ViewOnly, Need::Click);
        assert_eq!(r.code, Code::ActionUnsafe);
        assert!(r.message.contains("view only"), "{}", r.message);
        assert!(r.message.contains("clicking"), "{}", r.message);
        let v = r.to_json();
        assert_eq!(v["standing_access"]["level"], "view_only");
        assert_eq!(v["standing_access"]["needed"], "click");
        assert_eq!(
            v["standing_access"]["changed_in"],
            "Settings > Computer control"
        );
        assert_eq!(v["actuated"], false);
        let n = refusal(&notes, Level::Never, Need::Observe);
        assert!(n.message.contains("never"), "{}", n.message);
        let t = refusal(&notes, Level::Click, Need::Keyboard);
        assert!(t.message.contains("typing"), "{}", t.message);
    }
}
