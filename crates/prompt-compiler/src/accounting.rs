//! Context accounting by category (REQ-PX-059; docs/65 AFW-H04, AFW-H05).
//!
//! A model request is made of a handful of parts the person can reason about:
//! the system instructions, the tool definitions, the rules, the skills, the
//! external (MCP) tool definitions, the engineering memory, the summary a
//! compaction left, the subagent definitions and the conversation. The
//! compiler knows which text it put in which part, so the breakdown is derived
//! from the compiled envelope, not guessed afterwards.
//!
//! The estimator counts the parts; the **provider counts the request**. When
//! the provider reported what it counted, the parts are scaled to that total
//! and rounded so that they sum to it exactly — [`apportion`], the largest
//! remainder method, ties broken by category order. When it did not, the
//! estimator's own counts stand and the total says it is an estimate.

use serde::{Deserialize, Serialize};

/// A part of the request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Category {
    /// The system segment and the runtime's own notes.
    System,
    /// The Core's tool definitions.
    Tools,
    /// Workspace rules and instructions (AGENTS.md and the like).
    Rules,
    /// Selected skills and the skill index.
    Skills,
    /// External (MCP) tool definitions.
    Mcp,
    /// Curated engineering memory.
    Memory,
    /// The summary a compaction epoch left.
    Summary,
    /// Subagent definitions and the delegation rule.
    Subagents,
    /// Everything else the model reads: the goal, the transcript and the run
    /// state.
    Conversation,
}

impl Category {
    /// Every category, in the order the breakdown lists them.
    pub const ALL: [Category; 9] = [
        Category::System,
        Category::Tools,
        Category::Rules,
        Category::Skills,
        Category::Mcp,
        Category::Memory,
        Category::Summary,
        Category::Subagents,
        Category::Conversation,
    ];

    /// The stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::System => "SYSTEM",
            Self::Tools => "TOOLS",
            Self::Rules => "RULES",
            Self::Skills => "SKILLS",
            Self::Mcp => "MCP",
            Self::Memory => "MEMORY",
            Self::Summary => "SUMMARY",
            Self::Subagents => "SUBAGENTS",
            Self::Conversation => "CONVERSATION",
        }
    }

    /// Parse a stable name.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name() == s)
    }
}

/// The text the compiler put in each part except the conversation (which is
/// the rest of the request). The Core estimates each with its own estimator.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CategoryText {
    /// System segment and surface note.
    pub system: String,
    /// Tool definitions other than the external and subagent ones.
    pub tools: String,
    /// The rules message.
    pub rules: String,
    /// Skill bodies and the skill index.
    pub skills: String,
    /// External tool definitions.
    pub mcp: String,
    /// The memory segment of the task turn.
    pub memory: String,
    /// The compaction epoch and its narrative.
    pub summary: String,
    /// The delegation rule and the `agent.*` tool definitions.
    pub subagents: String,
}

impl CategoryText {
    /// The text of one category; the conversation has none of its own.
    #[must_use]
    pub fn of(&self, c: Category) -> &str {
        match c {
            Category::System => &self.system,
            Category::Tools => &self.tools,
            Category::Rules => &self.rules,
            Category::Skills => &self.skills,
            Category::Mcp => &self.mcp,
            Category::Memory => &self.memory,
            Category::Summary => &self.summary,
            Category::Subagents => &self.subagents,
            Category::Conversation => "",
        }
    }
}

/// Which part a tool definition belongs to, by name.
#[must_use]
pub fn tool_category(name: &str) -> Category {
    if name.starts_with("external.") || name.starts_with("mcp.") {
        Category::Mcp
    } else if name.starts_with("agent.") {
        Category::Subagents
    } else {
        Category::Tools
    }
}

/// The rounding rule, by name, as the view states it.
pub const ROUNDING_RULE: &str = "LARGEST_REMAINDER";

/// Scale `estimates` so they sum to exactly `total`: each is multiplied by
/// `total / sum`, floored, and the tokens left over go one each to the parts
/// with the largest fractional remainder (ties to the earlier category). With
/// every estimate zero the whole total is the last part's (the conversation).
#[must_use]
pub fn apportion(estimates: &[u64; 9], total: u64) -> [u64; 9] {
    let sum: u128 = estimates.iter().map(|e| u128::from(*e)).sum();
    let mut out = [0u64; 9];
    if sum == 0 {
        out[8] = total;
        return out;
    }
    let mut remainders: Vec<(u128, usize)> = Vec::with_capacity(9);
    let mut given: u64 = 0;
    for (i, e) in estimates.iter().enumerate() {
        let scaled = u128::from(*e) * u128::from(total);
        let floor = u64::try_from(scaled / sum).unwrap_or(u64::MAX);
        out[i] = floor;
        given = given.saturating_add(floor);
        remainders.push((scaled % sum, i));
    }
    let mut left = total.saturating_sub(given);
    // Largest remainder first; equal remainders go to the earlier category.
    remainders.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (_, i) in remainders {
        if left == 0 {
            break;
        }
        out[i] += 1;
        left -= 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_parts_always_sum_to_the_total() {
        let cases: [([u64; 9], u64); 6] = [
            ([10, 20, 30, 0, 0, 5, 0, 0, 35], 100),
            ([10, 20, 30, 0, 0, 5, 0, 0, 35], 101),
            ([1, 1, 1, 1, 1, 1, 1, 1, 1], 10),
            ([7, 0, 0, 0, 0, 0, 0, 0, 3], 1),
            ([0; 9], 42),
            ([123_456, 7_890, 1, 2, 3, 4, 5, 6, 987_654], 4_321_098),
        ];
        for (e, total) in cases {
            let out = apportion(&e, total);
            assert_eq!(out.iter().sum::<u64>(), total, "{e:?} -> {out:?}");
        }
    }

    #[test]
    fn a_part_is_never_off_by_more_than_one_token_from_its_exact_share() {
        let e = [13u64, 29, 31, 3, 0, 7, 0, 11, 906];
        let total = 1_000_003u64;
        let sum: u64 = e.iter().sum();
        let out = apportion(&e, total);
        for (i, part) in e.iter().enumerate() {
            let exact = (u128::from(*part) * u128::from(total)) as f64 / f64::from(sum as u32);
            assert!(
                (out[i] as f64 - exact).abs() < 1.0,
                "{i}: {} vs {exact}",
                out[i]
            );
        }
    }

    #[test]
    fn equal_remainders_go_to_the_earlier_category_and_zero_stays_zero() {
        let out = apportion(&[1, 1, 1, 0, 0, 0, 0, 0, 0], 4);
        assert_eq!(out, [2, 1, 1, 0, 0, 0, 0, 0, 0]);
        let out = apportion(&[0, 5, 0, 0, 0, 0, 0, 0, 5], 9);
        assert_eq!(out[0], 0);
        assert_eq!(out.iter().sum::<u64>(), 9);
    }

    #[test]
    fn tools_are_sorted_into_their_part_by_name() {
        assert_eq!(tool_category("fs.read"), Category::Tools);
        assert_eq!(tool_category("external.call"), Category::Mcp);
        assert_eq!(tool_category("agent.spawn"), Category::Subagents);
        assert_eq!(Category::parse("MCP"), Some(Category::Mcp));
        assert_eq!(Category::ALL.len(), 9);
    }
}
