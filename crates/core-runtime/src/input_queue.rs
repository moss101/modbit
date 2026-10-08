//! The task's input queue as its log says it (PX-050; docs/65 AFW-E01 to
//! AFW-E07, docs/14 contract 9).
//!
//! There is one queue and one fold. A person's inputs are `TaskInputQueued`
//! events on the task's own log; the queue is managed by further events on the
//! same log (`TaskInputEdited`, `TaskInputRemoved`, `TaskInputReordered`,
//! `TaskInterruptRequested`), and the run loop consumes it with `TaskSteered`
//! naming the inputs it dispatched. [`InputQueue`] folds those records in log
//! order and nothing else: no timer, no clock, no in-memory copy is a second
//! source of truth, so a Core killed at any point rebuilds exactly the queue
//! it had, and the loop, the list command and the edit commands all read the
//! same one.
//!
//! The fold is lenient about what it cannot apply (an edit of an input that
//! was already dispatched is ignored) because the log may hold a record that
//! lost a race; the *commands* are strict and refuse such a request before it
//! is written.

use modbit_domain::task::{InputMode, TaskEvent};

/// Where one input stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemState {
    /// Waiting for a safe boundary.
    Queued,
    /// The run loop applied it (`TaskSteered`).
    Dispatched,
    /// The person deleted it before it was dispatched.
    Removed,
}

impl ItemState {
    /// The stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Queued => "QUEUED",
            Self::Dispatched => "DISPATCHED",
            Self::Removed => "REMOVED",
        }
    }
}

/// One input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueItem {
    /// The client-chosen id.
    pub input_id: String,
    /// How it is dispatched.
    pub mode: InputMode,
    /// What it says (after any edit).
    pub text: String,
    /// Where it came from when not the person (`forge_review_comment`).
    pub provenance: String,
    /// Whether the text is untrusted data.
    pub untrusted: bool,
    /// The per-item model selection recorded by an edit.
    pub model: String,
    /// Where it stands.
    pub state: ItemState,
    /// Whether an edit changed it.
    pub edited: bool,
    /// Whether the person asked to send it now.
    pub sent_now: bool,
    /// The log offset of the `TaskInputQueued` record.
    pub queued_offset: u64,
    /// The log offset of the record that last changed it.
    pub changed_offset: u64,
}

/// One interrupt request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterruptRecord {
    /// The interrupt's id.
    pub interrupt_id: String,
    /// `SEND_NOW` | `STOP`.
    pub kind: String,
    /// The input a `SEND_NOW` promoted.
    pub input_id: String,
    /// The log offset of the request.
    pub requested_offset: u64,
    /// Whether the run recorded that it applied it.
    pub applied: bool,
}

/// What an input with no mode of its own does while a turn runs, and what
/// Send now does (docs/65 AFW-E05). The Core applies it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendBehavior {
    /// `QUEUE` | `COLLECT` | `STEER` | `STOP_AND_SEND`.
    pub while_running: String,
    /// `INTERRUPT` | `STEER`.
    pub send_now: String,
}

impl Default for SendBehavior {
    fn default() -> Self {
        Self {
            while_running: "QUEUE".into(),
            send_now: "INTERRUPT".into(),
        }
    }
}

/// The values `while_running` takes.
pub const WHILE_RUNNING: [&str; 4] = ["QUEUE", "COLLECT", "STEER", "STOP_AND_SEND"];
/// The values `send_now` takes.
pub const SEND_NOW: [&str; 2] = ["INTERRUPT", "STEER"];

/// The queue.
#[derive(Clone, Debug, Default)]
pub struct InputQueue {
    items: Vec<QueueItem>,
    interrupts: Vec<InterruptRecord>,
    behavior: SendBehavior,
    behavior_offset: u64,
}

/// The event types the fold reads; a reader asks the store for exactly these.
pub const EVENT_TYPES: &[&str] = &[
    "SendBehaviorSet",
    "TaskInputQueued",
    "TaskInputEdited",
    "TaskInputRemoved",
    "TaskInputReordered",
    "TaskInterruptRequested",
    "TaskInterruptApplied",
    "TaskSteered",
];

impl InputQueue {
    /// An empty queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one record. Records are applied in log order.
    pub fn apply(&mut self, offset: u64, event: &TaskEvent) {
        match event {
            TaskEvent::SendBehaviorSet {
                while_running,
                send_now,
            } => {
                if WHILE_RUNNING.contains(&while_running.as_str()) {
                    self.behavior.while_running.clone_from(while_running);
                }
                if SEND_NOW.contains(&send_now.as_str()) {
                    self.behavior.send_now.clone_from(send_now);
                }
                self.behavior_offset = offset;
            }
            TaskEvent::TaskInputQueued {
                input_id,
                mode,
                text,
                provenance,
                untrusted,
            } => {
                // A retry that names an input already held is the same input.
                if self.items.iter().any(|i| i.input_id == *input_id) {
                    return;
                }
                self.items.push(QueueItem {
                    input_id: input_id.clone(),
                    mode: *mode,
                    text: text.clone(),
                    provenance: provenance.clone(),
                    untrusted: *untrusted,
                    model: String::new(),
                    state: ItemState::Queued,
                    edited: false,
                    sent_now: false,
                    queued_offset: offset,
                    changed_offset: offset,
                });
            }
            TaskEvent::TaskInputEdited {
                input_id,
                text,
                mode,
                model,
            } => {
                if let Some(i) = self.queued_mut(input_id) {
                    if !text.is_empty() {
                        i.text.clone_from(text);
                    }
                    if let Some(m) = mode {
                        i.mode = *m;
                    }
                    if !model.is_empty() {
                        i.model.clone_from(model);
                    }
                    i.edited = true;
                    i.changed_offset = offset;
                }
            }
            TaskEvent::TaskInputRemoved { input_id, .. } => {
                if let Some(i) = self.queued_mut(input_id) {
                    i.state = ItemState::Removed;
                    i.changed_offset = offset;
                }
            }
            TaskEvent::TaskInputReordered {
                input_id,
                before_input_id,
            } => {
                if self.queued_mut(input_id).is_some() {
                    self.move_before(input_id, before_input_id);
                    if let Some(i) = self.queued_mut(input_id) {
                        i.changed_offset = offset;
                    }
                }
            }
            TaskEvent::TaskInterruptRequested {
                interrupt_id,
                kind,
                input_id,
                ..
            } => {
                if self
                    .interrupts
                    .iter()
                    .any(|r| r.interrupt_id == *interrupt_id)
                {
                    return;
                }
                self.interrupts.push(InterruptRecord {
                    interrupt_id: interrupt_id.clone(),
                    kind: kind.clone(),
                    input_id: input_id.clone(),
                    requested_offset: offset,
                    applied: false,
                });
                if kind == "SEND_NOW" && self.queued_mut(input_id).is_some() {
                    self.promote(input_id);
                    if let Some(i) = self.queued_mut(input_id) {
                        i.sent_now = true;
                        i.changed_offset = offset;
                    }
                }
            }
            TaskEvent::TaskInterruptApplied { interrupt_id, .. } => {
                if let Some(r) = self
                    .interrupts
                    .iter_mut()
                    .find(|r| r.interrupt_id == *interrupt_id)
                {
                    r.applied = true;
                }
            }
            TaskEvent::TaskSteered { input_ids, .. } => {
                if input_ids.is_empty() {
                    // A record from before the queue was managed: it consumed
                    // the oldest input still waiting.
                    if let Some(i) = self
                        .items
                        .iter_mut()
                        .filter(|i| i.state == ItemState::Queued)
                        .min_by_key(|i| i.queued_offset)
                    {
                        i.state = ItemState::Dispatched;
                        i.changed_offset = offset;
                    }
                }
                for id in input_ids {
                    if let Some(i) = self.queued_mut(id) {
                        i.state = ItemState::Dispatched;
                        i.changed_offset = offset;
                    }
                }
            }
            _ => {}
        }
    }

    fn queued_mut(&mut self, id: &str) -> Option<&mut QueueItem> {
        self.items
            .iter_mut()
            .find(|i| i.input_id == id && i.state == ItemState::Queued)
    }

    /// Move `id` to precede `before` (a queued input), or to the end.
    fn move_before(&mut self, id: &str, before: &str) {
        let Some(from) = self.items.iter().position(|i| i.input_id == id) else {
            return;
        };
        let item = self.items.remove(from);
        let at = if before.is_empty() || before == id {
            None
        } else {
            self.items
                .iter()
                .position(|i| i.input_id == before && i.state == ItemState::Queued)
        };
        match at {
            Some(at) => self.items.insert(at, item),
            None => self.items.push(item),
        }
    }

    /// A sent-now input runs before every input that is not, in the order it
    /// was sent.
    fn promote(&mut self, id: &str) {
        let Some(from) = self.items.iter().position(|i| i.input_id == id) else {
            return;
        };
        let item = self.items.remove(from);
        let at = self
            .items
            .iter()
            .position(|i| i.state == ItemState::Queued && !i.sent_now)
            .unwrap_or(self.items.len());
        self.items.insert(at, item);
    }

    /// What an input with no mode of its own does, and what Send now does.
    #[must_use]
    pub fn behavior(&self) -> &SendBehavior {
        &self.behavior
    }

    /// The log offset of the record that last set the behavior (0 = never).
    #[must_use]
    pub fn behavior_offset(&self) -> u64 {
        self.behavior_offset
    }

    /// The inputs still waiting, in the order they will be dispatched.
    #[must_use]
    pub fn pending(&self) -> Vec<&QueueItem> {
        self.items
            .iter()
            .filter(|i| i.state == ItemState::Queued)
            .collect()
    }

    /// Every input the log holds, queued ones first in dispatch order and then
    /// the settled ones (dispatched, removed) in the order they were queued.
    #[must_use]
    pub fn all(&self) -> Vec<&QueueItem> {
        let mut out = self.pending();
        let mut settled: Vec<&QueueItem> = self
            .items
            .iter()
            .filter(|i| i.state != ItemState::Queued)
            .collect();
        settled.sort_by_key(|i| i.queued_offset);
        out.extend(settled);
        out
    }

    /// One input by id, in any state.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&QueueItem> {
        self.items.iter().find(|i| i.input_id == id)
    }

    /// The interrupts the run has not yet recorded as applied, oldest first.
    #[must_use]
    pub fn unapplied_interrupts(&self) -> Vec<&InterruptRecord> {
        self.interrupts.iter().filter(|r| !r.applied).collect()
    }

    /// One interrupt by id.
    #[must_use]
    pub fn interrupt(&self, id: &str) -> Option<&InterruptRecord> {
        self.interrupts.iter().find(|r| r.interrupt_id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queued(id: &str, mode: InputMode) -> TaskEvent {
        TaskEvent::TaskInputQueued {
            input_id: id.into(),
            mode,
            text: format!("text {id}"),
            provenance: String::new(),
            untrusted: false,
        }
    }

    fn ids(q: &InputQueue) -> Vec<String> {
        q.pending().iter().map(|i| i.input_id.clone()).collect()
    }

    fn fold(events: &[TaskEvent]) -> InputQueue {
        let mut q = InputQueue::new();
        for (n, e) in events.iter().enumerate() {
            q.apply(n as u64 + 1, e);
        }
        q
    }

    #[test]
    fn inputs_drain_in_the_order_they_were_queued() {
        let q = fold(&[
            queued("a", InputMode::FollowUp),
            queued("b", InputMode::FollowUp),
            queued("c", InputMode::Collect),
        ]);
        assert_eq!(ids(&q), ["a", "b", "c"]);
    }

    #[test]
    fn edit_changes_text_and_mode_only_while_queued() {
        let q = fold(&[
            queued("a", InputMode::FollowUp),
            queued("b", InputMode::FollowUp),
            TaskEvent::TaskInputEdited {
                input_id: "a".into(),
                text: "edited".into(),
                mode: Some(InputMode::Steer),
                model: "m".into(),
            },
            TaskEvent::TaskSteered {
                text: "edited".into(),
                provenance: String::new(),
                untrusted: false,
                input_ids: vec!["a".into()],
            },
            // Too late: a was dispatched.
            TaskEvent::TaskInputEdited {
                input_id: "a".into(),
                text: "too late".into(),
                mode: None,
                model: String::new(),
            },
        ]);
        let a = q.get("a").unwrap();
        assert_eq!(a.state, ItemState::Dispatched);
        assert_eq!(a.text, "edited");
        assert_eq!(a.mode, InputMode::Steer);
        assert_eq!(a.model, "m");
        assert!(a.edited);
        assert_eq!(ids(&q), ["b"]);
    }

    #[test]
    fn a_removed_input_is_never_pending_and_cannot_come_back() {
        let q = fold(&[
            queued("a", InputMode::FollowUp),
            queued("b", InputMode::FollowUp),
            TaskEvent::TaskInputRemoved {
                input_id: "a".into(),
                reason: String::new(),
            },
            TaskEvent::TaskInputEdited {
                input_id: "a".into(),
                text: "again".into(),
                mode: None,
                model: String::new(),
            },
        ]);
        assert_eq!(ids(&q), ["b"]);
        assert_eq!(q.get("a").unwrap().state, ItemState::Removed);
        assert_eq!(q.get("a").unwrap().text, "text a");
    }

    #[test]
    fn reorder_moves_a_queued_input_and_never_a_settled_one() {
        let q = fold(&[
            queued("a", InputMode::FollowUp),
            queued("b", InputMode::FollowUp),
            queued("c", InputMode::FollowUp),
            TaskEvent::TaskInputReordered {
                input_id: "c".into(),
                before_input_id: "a".into(),
            },
        ]);
        assert_eq!(ids(&q), ["c", "a", "b"]);
        let q = fold(&[
            queued("a", InputMode::FollowUp),
            queued("b", InputMode::FollowUp),
            queued("c", InputMode::FollowUp),
            TaskEvent::TaskInputReordered {
                input_id: "a".into(),
                before_input_id: String::new(),
            },
            TaskEvent::TaskSteered {
                text: String::new(),
                provenance: String::new(),
                untrusted: false,
                input_ids: vec!["b".into()],
            },
            TaskEvent::TaskInputReordered {
                input_id: "b".into(),
                before_input_id: "c".into(),
            },
        ]);
        assert_eq!(ids(&q), ["c", "a"]);
        assert_eq!(q.get("b").unwrap().state, ItemState::Dispatched);
    }

    #[test]
    fn send_now_promotes_the_input_ahead_of_the_rest_in_the_order_sent() {
        let q = fold(&[
            queued("a", InputMode::FollowUp),
            queued("b", InputMode::FollowUp),
            queued("c", InputMode::FollowUp),
            TaskEvent::TaskInterruptRequested {
                interrupt_id: "i1".into(),
                kind: "SEND_NOW".into(),
                input_id: "c".into(),
                reason: String::new(),
            },
            TaskEvent::TaskInterruptRequested {
                interrupt_id: "i2".into(),
                kind: "SEND_NOW".into(),
                input_id: "b".into(),
                reason: String::new(),
            },
            // The same interrupt id twice is one interrupt.
            TaskEvent::TaskInterruptRequested {
                interrupt_id: "i2".into(),
                kind: "SEND_NOW".into(),
                input_id: "a".into(),
                reason: String::new(),
            },
        ]);
        assert_eq!(ids(&q), ["c", "b", "a"]);
        assert!(q.get("c").unwrap().sent_now);
        assert!(!q.get("a").unwrap().sent_now);
        assert_eq!(q.unapplied_interrupts().len(), 2);
    }

    #[test]
    fn a_steered_record_without_ids_consumes_the_oldest_input() {
        let q = fold(&[
            queued("a", InputMode::FollowUp),
            queued("b", InputMode::FollowUp),
            TaskEvent::TaskSteered {
                text: "text a".into(),
                provenance: String::new(),
                untrusted: false,
                input_ids: vec![],
            },
        ]);
        assert_eq!(ids(&q), ["b"]);
    }

    #[test]
    fn a_retried_queue_command_is_the_same_input() {
        let q = fold(&[queued("a", InputMode::Steer), queued("a", InputMode::Steer)]);
        assert_eq!(ids(&q), ["a"]);
    }

    #[test]
    fn the_same_log_always_folds_to_the_same_queue() {
        let log = [
            queued("a", InputMode::FollowUp),
            queued("b", InputMode::Collect),
            TaskEvent::TaskInputReordered {
                input_id: "b".into(),
                before_input_id: "a".into(),
            },
            TaskEvent::TaskInputRemoved {
                input_id: "a".into(),
                reason: String::new(),
            },
            queued("c", InputMode::Steer),
        ];
        assert_eq!(ids(&fold(&log)), ids(&fold(&log)));
        assert_eq!(ids(&fold(&log)), ["b", "c"]);
    }
}
