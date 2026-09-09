//! Wall-clock timestamps as milliseconds since the Unix epoch.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Milliseconds since 1970-01-01T00:00:00Z.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(pub i64);

impl Timestamp {
    /// The current wall-clock time.
    #[must_use]
    pub fn now() -> Self {
        let d = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        Self(i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
    }

    /// Milliseconds since the epoch.
    #[must_use]
    pub const fn millis(self) -> i64 {
        self.0
    }
}
