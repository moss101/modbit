//! Run modes and durable allowlist rules (PX-057; docs/65 AFW-F06 to F12).
//!
//! A [`RunMode`] is a preset of **who approves effects inside the Capability
//! Kernel's envelope**. It has no field that widens a capability, a path, an
//! egress or an effect class: the kernel still runs every one of its checks
//! first and a mode only ever answers the last question ("may this effect run
//! without asking a person?"). The [`AskClass`]es are the effects that ask in
//! every mode.
//!
//! An [`AllowRule`] is a durable policy record: an argv-prefix pattern, a
//! scope, who made it and when, and an optional expiry. It is created by a
//! person through the Core (never by the renderer, a model or a tool result)
//! and is revocable. This file holds the types; the matcher is
//! `modbit_policy::runmode`.

use serde::{Deserialize, Serialize};

/// Who approves protected effects for a task (docs/65 AFW-F06).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunMode {
    /// The default: every protected effect asks.
    #[default]
    Ask,
    /// An effect matching a durable rule runs without asking.
    Allowlist,
    /// As `Allowlist`; additionally an effect fully contained in a sandbox
    /// (no network, writes confined to the sandbox) runs without asking.
    AllowlistSandbox,
    /// Every in-envelope effect outside the always-ask classes runs without
    /// asking. Per task and per Core process: never persisted across a
    /// restart, acknowledged every time it is set.
    RunEverything,
}

impl RunMode {
    /// Every mode, from the one that approves least to the one that approves
    /// most.
    pub const ALL: [RunMode; 4] = [
        RunMode::Ask,
        RunMode::Allowlist,
        RunMode::AllowlistSandbox,
        RunMode::RunEverything,
    ];

    /// How much the mode approves without a person; a move to a higher rank
    /// "approves more" and needs the recorded acknowledgement (AFW-F07).
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Ask => 0,
            Self::Allowlist => 1,
            Self::AllowlistSandbox => 2,
            Self::RunEverything => 3,
        }
    }

    /// The stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ask => "ASK",
            Self::Allowlist => "ALLOWLIST",
            Self::AllowlistSandbox => "ALLOWLIST_SANDBOX",
            Self::RunEverything => "RUN_EVERYTHING",
        }
    }

    /// Parse a stable name.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == s)
    }

    /// Whether the mode survives a Core restart. `RunEverything` does not.
    #[must_use]
    pub const fn durable(self) -> bool {
        !matches!(self, Self::RunEverything)
    }
}

/// The effects that ask in every mode (docs/65 AFW-F08).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AskClass {
    /// A write outside the workspace.
    OutsideWorkspaceWrite,
    /// A network fetch or connection.
    Network,
    /// Any use of a secret.
    Secret,
    /// A protected or configuration path.
    ProtectedPath,
    /// A deletion.
    Deletion,
    /// A push.
    Push,
    /// An escalation beyond the declared capability.
    Escalation,
}

impl AskClass {
    /// The stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::OutsideWorkspaceWrite => "OUTSIDE_WORKSPACE_WRITE",
            Self::Network => "NETWORK",
            Self::Secret => "SECRET",
            Self::ProtectedPath => "PROTECTED_PATH",
            Self::Deletion => "DELETION",
            Self::Push => "PUSH",
            Self::Escalation => "ESCALATION",
        }
    }
}

/// Where a rule applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuleScope {
    /// One task.
    Task,
    /// Every task whose workspace root is the rule's repository.
    Repo,
    /// Every task of this user on this Core.
    User,
}

impl RuleScope {
    /// The stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Task => "TASK",
            Self::Repo => "REPO",
            Self::User => "USER",
        }
    }

    /// Parse a stable name.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        [Self::Task, Self::Repo, Self::User]
            .into_iter()
            .find(|m| m.name() == s)
    }
}

/// One durable allowlist rule (docs/65 AFW-F10).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowRule {
    /// Rule id (the `AddAllowRule` command's own).
    pub rule_id: String,
    /// The argv prefix, token by token.
    pub pattern: Vec<String>,
    /// Where it applies.
    pub scope: RuleScope,
    /// What the scope names: the task id (`TASK`), the canonical workspace
    /// root (`REPO`), empty (`USER`).
    #[serde(default)]
    pub scope_key: String,
    /// Who created it (`user:<id>`).
    pub created_by: String,
    /// When (ms since the epoch).
    pub created_at_ms: i64,
    /// When it stops applying, if it does.
    #[serde(default)]
    pub expires_at_ms: Option<i64>,
    /// Whether the rule may also stand for an always-ask class. Explicit,
    /// narrow (task or repository scope only) and recorded: a rule without it
    /// never auto-runs an always-ask effect (AFW-F08).
    #[serde(default)]
    pub covers_always_ask: bool,
    /// The task whose log holds the record (revocation is appended there).
    #[serde(default)]
    pub origin_task: String,
}
