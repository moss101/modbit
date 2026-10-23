//! Conversation curation events (PX-042, docs/65 AFW-B04, AFW-B08): what a
//! person did with a task's conversation, as events on the canonical log so
//! that unread and archived are facts every client agrees on and can undo.
//!
//! The conversation itself is never stored: transcript rows and agent headers
//! are projections of the log. These three events are the only things a
//! person adds to it — a read marker, an archive and its reversal — and each
//! is an ordinary event of the task's lineage on its own aggregate (an
//! aggregate id derived from the task id, so it never shares a sequence with
//! the task's own aggregate).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ids::TaskId;

/// `ConversationRead`.
pub const READ: &str = "ConversationRead";
/// `ConversationArchived`.
pub const ARCHIVED: &str = "ConversationArchived";
/// `ConversationUnarchived`.
pub const UNARCHIVED: &str = "ConversationUnarchived";

/// Events on the `Conversation` aggregate of a task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum ConversationEvent {
    /// The person has seen the conversation up to this log offset.
    ConversationRead {
        /// Highest log offset seen.
        up_to_offset: u64,
    },
    /// The task left the active list.
    ConversationArchived,
    /// The task came back (the undo of an archive).
    ConversationUnarchived,
}

impl ConversationEvent {
    /// Canonical event type name.
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::ConversationRead { .. } => READ,
            Self::ConversationArchived => ARCHIVED,
            Self::ConversationUnarchived => UNARCHIVED,
        }
    }
}

/// The aggregate id of a task's conversation: a digest of the task id, so it
/// is stable, never equal to the task's own aggregate id, and needs no
/// lookup.
#[must_use]
pub fn aggregate_id(task: TaskId) -> [u8; 16] {
    let mut h = Sha256::new();
    h.update(b"modbit.conversation.v1");
    h.update(task.as_bytes());
    let digest = h.finalize();
    let mut out = [0u8; 16];
    out.copy_from_slice(&digest[..16]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_aggregate_id_is_stable_and_distinct_from_the_task() {
        let t = TaskId::new();
        assert_eq!(aggregate_id(t), aggregate_id(t));
        assert_ne!(aggregate_id(t), *t.as_bytes());
        assert_ne!(aggregate_id(t), aggregate_id(TaskId::new()));
        let v =
            serde_json::to_value(ConversationEvent::ConversationRead { up_to_offset: 7 }).unwrap();
        assert_eq!(v["event_type"], READ);
        assert_eq!(v["up_to_offset"], 7);
    }
}
