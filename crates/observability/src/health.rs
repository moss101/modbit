//! Persisted component health (REQ-PX-139, docs/34).
//!
//! Component health — provider reachability, the terminal broker, the
//! sandbox gateway, the indexes, the exporter itself — used to live in memory
//! and reset to "unknown" at every restart. Each observation now lands in a
//! small file beside the Core's store; a restarted Core opens it and reports
//! the last known state *with its age* until a fresh observation replaces it,
//! marking the old one as not observed in this run. It is an operational
//! record, not a source of truth for any task: nothing decides anything from
//! it.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// The file's schema.
pub const SCHEMA: &str = "modbit.component-health/1";

/// How a component is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HealthState {
    /// Working as observed.
    Ok,
    /// Working with failures.
    Degraded,
    /// Not working as observed.
    Down,
    /// Nothing has been observed.
    Unknown,
    /// Not configured, deliberately off.
    Disabled,
}

impl HealthState {
    /// The wire name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Degraded => "DEGRADED",
            Self::Down => "DOWN",
            Self::Unknown => "UNKNOWN",
            Self::Disabled => "DISABLED",
        }
    }
}

/// One observation of one component.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    /// The component (`provider:openai`, `execd`, `sandbox_gateway`,
    /// `indexes`, `telemetry`).
    pub name: String,
    /// Its state.
    pub state: HealthState,
    /// Why, redacted and bounded by the caller.
    pub detail: String,
    /// When it was observed (ms since the epoch).
    pub observed_at_ms: i64,
}

/// What a client sees of one component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentView {
    /// The observation.
    pub observation: Observation,
    /// How old it is now.
    pub age_ms: i64,
    /// Whether it was observed by this process (false: read back from the
    /// previous run's file and not yet replaced).
    pub observed_this_run: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    schema: String,
    components: Vec<Observation>,
}

#[derive(Default)]
struct Inner {
    current: BTreeMap<String, Observation>,
    previous: BTreeMap<String, Observation>,
    last_written_ms: i64,
}

/// The persisted health of a Core's components.
pub struct HealthStore {
    path: Option<PathBuf>,
    inner: Mutex<Inner>,
}

/// An unchanged observation is written again at most this often.
const REWRITE_MS: i64 = 30_000;

impl HealthStore {
    /// A store persisted at `path`, loading what the previous run left (an
    /// unreadable or foreign file is ignored: health is advisory).
    #[must_use]
    pub fn open(path: PathBuf) -> Self {
        let previous = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<File>(&b).ok())
            .filter(|f| f.schema == SCHEMA)
            .map(|f| {
                f.components
                    .into_iter()
                    .map(|o| (o.name.clone(), o))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            path: Some(path),
            inner: Mutex::new(Inner {
                previous,
                ..Inner::default()
            }),
        }
    }

    /// A store that keeps nothing on disk.
    #[must_use]
    pub fn in_memory() -> Self {
        Self {
            path: None,
            inner: Mutex::new(Inner::default()),
        }
    }

    /// Record an observation made `now_ms`. The file is rewritten when the
    /// state or the detail changed, and otherwise every [`REWRITE_MS`].
    pub fn observe(&self, name: &str, state: HealthState, detail: &str, now_ms: i64) {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let detail = crate::otlp::bounded(detail, 300);
        let changed = g
            .current
            .get(name)
            .is_none_or(|o| o.state != state || o.detail != detail);
        g.current.insert(
            name.to_owned(),
            Observation {
                name: name.to_owned(),
                state,
                detail,
                observed_at_ms: now_ms,
            },
        );
        if changed || now_ms - g.last_written_ms >= REWRITE_MS {
            g.last_written_ms = now_ms;
            let file = File {
                schema: SCHEMA.into(),
                components: Self::merged(&g).into_values().collect(),
            };
            drop(g);
            self.write(&file);
        }
    }

    fn merged(g: &Inner) -> BTreeMap<String, Observation> {
        let mut all = g.previous.clone();
        all.extend(g.current.clone());
        all
    }

    fn write(&self, file: &File) {
        let Some(path) = &self.path else { return };
        let Ok(bytes) = serde_json::to_vec(file) else {
            return;
        };
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }

    /// Every component known to this run or the previous one, with ages.
    #[must_use]
    pub fn view(&self, now_ms: i64) -> Vec<ComponentView> {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut out: Vec<ComponentView> = Self::merged(&g)
            .into_values()
            .map(|o| ComponentView {
                observed_this_run: g.current.contains_key(&o.name),
                age_ms: (now_ms - o.observed_at_ms).max(0),
                observation: o,
            })
            .collect();
        out.sort_by(|a, b| a.observation.name.cmp(&b.observation.name));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_restart_reports_the_last_known_state_with_its_age_until_it_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("health.json");
        {
            let h = HealthStore::open(path.clone());
            h.observe("execd", HealthState::Ok, "connected", 1_000);
            h.observe("provider:openai", HealthState::Down, "refused", 2_000);
        }
        let h = HealthStore::open(path);
        let v = h.view(10_000);
        let execd = v.iter().find(|c| c.observation.name == "execd").unwrap();
        assert_eq!(execd.observation.state, HealthState::Ok);
        assert_eq!(execd.age_ms, 9_000);
        assert!(!execd.observed_this_run, "read back, not observed now");
        h.observe("execd", HealthState::Degraded, "slow", 10_500);
        let v = h.view(11_000);
        let execd = v.iter().find(|c| c.observation.name == "execd").unwrap();
        assert_eq!(execd.observation.state, HealthState::Degraded);
        assert!(execd.observed_this_run);
        assert_eq!(execd.age_ms, 500);
        let p = v
            .iter()
            .find(|c| c.observation.name == "provider:openai")
            .unwrap();
        assert!(!p.observed_this_run && p.observation.state == HealthState::Down);
    }

    #[test]
    fn a_foreign_or_corrupt_file_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("health.json");
        std::fs::write(&path, b"{\"schema\":\"other/1\",\"components\":[]}").unwrap();
        assert!(HealthStore::open(path.clone()).view(0).is_empty());
        std::fs::write(&path, b"not json").unwrap();
        assert!(HealthStore::open(path).view(0).is_empty());
    }
}
