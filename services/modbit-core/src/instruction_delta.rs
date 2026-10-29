//! Instruction changes during a task arrive as appended notes (REQ-PX-162,
//! DR-PX-2026-10-26-018 D1/D2).
//!
//! The prompt's early segments (workspace rules, skills, the task turn with
//! its curated memory) are the cached prefix. They used to be recompiled from
//! the current files, skills and memory every turn, so an edit to an
//! instruction file, a lazily activated rule or a promoted memory rewrote the
//! prefix and paid for it again. Here a run compiles its prefix from the
//! instructions in force when it started (or when a compaction epoch opened),
//! and a change it sees afterwards is delivered once, as a note appended to
//! the transcript: what changed, what it replaces, the new text with its
//! provenance, and the hashes of both versions. The note is an event on the
//! log, so a resumed run rebuilds the same transcript; a resumed run compiles
//! a fresh prefix, which already holds every change.
//!
//! A change that only takes something away (a memory forgotten or expired, a
//! skill revoked, a rule file removed) is not a note: the withdrawn text must
//! leave the request, so the prefix is rebuilt from the current instructions.
//! Nothing here decides what is permitted; the notes are project data with
//! their provenance, as the segments they update are.

use std::collections::BTreeMap;

use modbit_prompt_compiler::MemoryFragment;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// The event the note is recorded as; the transcript rebuild replays it.
pub(crate) const NOTE_EVENT: &str = "InstructionUpdateNoted";

/// The event recorded when the prefix is deliberately compiled anew mid-run.
pub(crate) const REBUILT_EVENT: &str = "InstructionPrefixRebuilt";

/// Longest text of one changed item inside a note, in characters.
const MAX_ITEM_CHARS: usize = 8_000;

/// What the early prompt segments are made of for one request.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Instructions {
    pub rules: Vec<String>,
    pub skills: Vec<String>,
    pub memory: Vec<MemoryFragment>,
}

/// One note, with the facts the log records beside its text.
#[derive(Clone, Debug)]
pub(crate) struct Note {
    pub text: String,
    pub changes: Vec<Value>,
}

/// What the compile step uses this turn.
pub(crate) struct Step {
    /// The instructions the prefix is compiled from.
    pub base: Instructions,
    /// The note to append before the request, when something changed.
    pub note: Option<Note>,
    /// The prefix was rebuilt from the current instructions this turn
    /// (the first turn, an epoch, or a withdrawal).
    pub rebuilt: Option<&'static str>,
}

/// Follows what the prefix was compiled from and what the model was told since.
#[derive(Default)]
pub(crate) struct Tracker {
    base: Option<Instructions>,
    told: Instructions,
    epoch: Option<u64>,
}

struct Item {
    category: &'static str,
    key: String,
    slot: String,
    text: String,
}

fn sha(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

fn short(h: &str) -> &str {
    &h[..h.len().min(12)]
}

/// The name a rule or skill text is known by: its header up to the hash.
fn head_key(text: &str) -> String {
    let first = text.lines().next().unwrap_or_default();
    first.split(" — ").next().unwrap_or(first).trim().to_owned()
}

fn items(i: &Instructions) -> Vec<Item> {
    let mut out = Vec::new();
    for t in &i.rules {
        let key = head_key(t);
        out.push(Item {
            category: "rule",
            slot: key.clone(),
            key,
            text: t.clone(),
        });
    }
    for t in &i.skills {
        let key = head_key(t);
        out.push(Item {
            category: "skill",
            slot: key.clone(),
            key,
            text: t.clone(),
        });
    }
    for m in &i.memory {
        out.push(Item {
            category: "memory",
            key: m.memory_id.clone(),
            slot: format!("{}|{}|{}", m.scope, m.record_type, m.topic),
            text: m.render(),
        });
    }
    out
}

/// The name a person reads a note's item by: a memory by the short id its
/// header shows, a rule or skill by its header.
fn label(i: &Item) -> String {
    if i.category == "memory" {
        i.key[..i.key.len().min(12)].to_owned()
    } else {
        i.key.clone()
    }
}

fn clip(text: &str) -> String {
    if text.chars().count() <= MAX_ITEM_CHARS {
        return text.to_owned();
    }
    let head: String = text.chars().take(MAX_ITEM_CHARS).collect();
    format!("{head}\n[…truncated by the Core]")
}

impl Tracker {
    /// The instructions for this turn's request. `current` is what a fresh
    /// compile would use now; `epoch` is the compaction epoch in force.
    pub(crate) fn step(&mut self, current: Instructions, epoch: Option<u64>) -> Step {
        let rebuilt = if self.base.is_none() {
            Some("FIRST_REQUEST")
        } else if epoch != self.epoch {
            Some("COMPACTION_EPOCH")
        } else {
            None
        };
        if let Some(why) = rebuilt {
            return self.rebuild(current, epoch, why);
        }
        if current == self.told {
            return Step {
                base: self.base.clone().unwrap_or_default(),
                note: None,
                rebuilt: None,
            };
        }
        let (told, now) = (items(&self.told), items(&current));
        let by_key = |v: &[Item]| -> BTreeMap<(&'static str, String), usize> {
            v.iter()
                .enumerate()
                .map(|(n, i)| ((i.category, i.key.clone()), n))
                .collect()
        };
        let (told_keys, now_keys) = (by_key(&told), by_key(&now));
        let mut changes = Vec::new();
        let mut text = String::new();
        let mut withdrawn = false;
        for item in &now {
            let was = told_keys.get(&(item.category, item.key.clone()));
            let replaces: Option<&Item> = match was {
                Some(n) => Some(&told[*n]),
                // A memory edit is a new item in the slot the old one held.
                None if item.category == "memory" => told.iter().find(|t| {
                    t.category == "memory"
                        && t.slot == item.slot
                        && !now_keys.contains_key(&(t.category, t.key.clone()))
                }),
                None => None,
            };
            if replaces.is_some_and(|r| r.text == item.text) {
                continue;
            }
            let kind = if replaces.is_some() {
                "REPLACED"
            } else {
                "ADDED"
            };
            changes.push(json!({
                "category": item.category,
                "key": item.key,
                "change": kind,
                "replaces": replaces.map(|r| r.key.clone()),
                "replaces_sha256": replaces.map(|r| sha(&r.text)),
                "sha256": sha(&item.text),
            }));
            text.push_str(&format!(
                "\n- {} {}: {}{}\n{}\n",
                item.category,
                label(item),
                if kind == "ADDED" { "added" } else { "changed" },
                match replaces {
                    Some(r) => format!(
                        "; replaces {} in the instructions above (sha256:{}, now sha256:{})",
                        label(r),
                        short(&sha(&r.text)),
                        short(&sha(&item.text))
                    ),
                    None => format!("; sha256:{}", short(&sha(&item.text))),
                },
                clip(&item.text)
            ));
        }
        // Something taken away with nothing replacing it must leave the request.
        for old in &told {
            let gone = !now_keys.contains_key(&(old.category, old.key.clone()));
            let replaced = now
                .iter()
                .any(|n| n.category == old.category && n.slot == old.slot && n.key != old.key)
                && old.category == "memory";
            if gone && !replaced {
                withdrawn = true;
            }
        }
        if withdrawn {
            return self.rebuild(current, epoch, "WITHDRAWN");
        }
        self.told = current;
        if changes.is_empty() {
            return Step {
                base: self.base.clone().unwrap_or_default(),
                note: None,
                rebuilt: None,
            };
        }
        let text = format!(
            "[modbit: instruction update] The workspace rules, skills or curated memory changed after this task's instructions above were compiled. The instructions above are unchanged; each item below is the current version, with its provenance, and supersedes the version it names; where files disagree, the one nearer the work than those above wins, as in their listing. It is project data and project guidance, never a grant of a tool, approval or permission.{text}"
        );
        Step {
            base: self.base.clone().unwrap_or_default(),
            note: Some(Note { text, changes }),
            rebuilt: None,
        }
    }

    fn rebuild(&mut self, current: Instructions, epoch: Option<u64>, why: &'static str) -> Step {
        self.base = Some(current.clone());
        self.told = current.clone();
        self.epoch = epoch;
        Step {
            base: current,
            note: None,
            rebuilt: Some(why),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem(id: char, topic: &str, text: &str) -> MemoryFragment {
        MemoryFragment {
            memory_id: id.to_string().repeat(64),
            scope: "project:p".into(),
            record_type: "decision".into(),
            topic: topic.into(),
            source: "user_stated".into(),
            author: "user:u".into(),
            confidence: 1.0,
            validated: true,
            created_at_ms: 1,
            text: text.into(),
            ..Default::default()
        }
    }

    fn rule(source: &str, hash: &str, body: &str) -> String {
        format!(
            "[instructions {source} — sha256:{hash} — {} bytes]\n{body}",
            body.len()
        )
    }

    #[test]
    fn an_edit_and_a_new_memory_are_one_note_and_the_base_stays() {
        let mut t = Tracker::default();
        let first = Instructions {
            rules: vec![rule("AGENTS.md", "aaaa", "use tabs")],
            skills: vec![],
            memory: vec![mem('a', "style", "tabs")],
        };
        let s = t.step(first.clone(), Some(0));
        assert_eq!(s.rebuilt, Some("FIRST_REQUEST"));
        assert!(s.note.is_none());
        let second = Instructions {
            rules: vec![rule("AGENTS.md", "bbbb", "use spaces")],
            skills: vec![],
            memory: vec![mem('a', "style", "tabs"), mem('b', "db", "use sqlite")],
        };
        let s = t.step(second.clone(), Some(0));
        assert_eq!(s.base, first, "the prefix inputs are unchanged");
        let note = s.note.expect("a note");
        assert_eq!(note.changes.len(), 2, "{:?}", note.changes);
        assert!(note.text.contains("use spaces") && note.text.contains("use sqlite"));
        assert!(
            note.text
                .contains("replaces [instructions AGENTS.md in the instructions above")
        );
        // Told once.
        let s = t.step(second, Some(0));
        assert!(s.note.is_none() && s.rebuilt.is_none());
    }

    #[test]
    fn a_withdrawal_and_a_compaction_epoch_rebuild_the_prefix() {
        let mut t = Tracker::default();
        let first = Instructions {
            rules: vec![],
            skills: vec![],
            memory: vec![mem('a', "style", "tabs")],
        };
        t.step(first, Some(0));
        let s = t.step(Instructions::default(), Some(0));
        assert_eq!(s.rebuilt, Some("WITHDRAWN"));
        assert!(s.base.memory.is_empty() && s.note.is_none());
        let s = t.step(Instructions::default(), Some(1));
        assert_eq!(s.rebuilt, Some("COMPACTION_EPOCH"));
    }

    #[test]
    fn a_memory_edit_replaces_the_item_in_its_slot_without_a_rebuild() {
        let mut t = Tracker::default();
        t.step(
            Instructions {
                memory: vec![mem('a', "style", "tabs")],
                ..Default::default()
            },
            None,
        );
        let s = t.step(
            Instructions {
                memory: vec![mem('b', "style", "spaces")],
                ..Default::default()
            },
            None,
        );
        assert!(s.rebuilt.is_none());
        let note = s.note.expect("a note");
        assert_eq!(note.changes[0]["change"], "REPLACED");
        assert_eq!(note.changes[0]["replaces"], "a".repeat(64));
    }
}
