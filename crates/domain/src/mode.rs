//! Task mode and execution preference (PX-051, PX-053; docs/65 AFW-D03,
//! AFW-D05, AFW-D13, AFW-D14).
//!
//! A task carries a typed [`TaskMode`]. The mode selects a [`ModePosture`],
//! and the posture can only *narrow* what the task's execution profile
//! already allows: it has no field that widens a capability, an effect class
//! or a path. A client names the mode; the Core derives the posture from this
//! file and the Capability Kernel enforces it.
//!
//! An [`ExecutionPreference`] is the user's intent about how a task runs. It
//! is data the router reads; nothing here decides a route.

use serde::{Deserialize, Serialize};

use crate::toolcall::EffectClass;

/// The capabilities a read-only mode keeps: exactly what the `plan` execution
/// profile's lease grants (REQ-EV-0117), so a read-only mode and the read-only
/// profile admit the same tools.
pub const READ_ONLY_CAPABILITIES: &[&str] =
    &["fs.read", "git.read", "memory.query", "external.list"];

/// A task's mode (docs/65 AFW-D03).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskMode {
    /// The default: no narrowing beyond the profile.
    #[default]
    Agent,
    /// Reads and a plan; nothing is written until the user accepts the plan.
    Plan,
    /// Reproduction before a fix is mandatory (PX-039).
    Debug,
    /// Subagent admission with capacity tickets (the profile's own posture).
    Multitask,
    /// Read-only: no write and no execution effect.
    Ask,
}

impl TaskMode {
    /// Every mode, in the order docs/65 lists them.
    pub const ALL: [TaskMode; 5] = [
        TaskMode::Agent,
        TaskMode::Plan,
        TaskMode::Debug,
        TaskMode::Multitask,
        TaskMode::Ask,
    ];

    /// The stable name (as events and the CLI spell it).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            TaskMode::Agent => "AGENT",
            TaskMode::Plan => "PLAN",
            TaskMode::Debug => "DEBUG",
            TaskMode::Multitask => "MULTITASK",
            TaskMode::Ask => "ASK",
        }
    }

    /// The mode a name denotes (case-insensitive).
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let name = name.trim();
        Self::ALL
            .into_iter()
            .find(|m| m.name().eq_ignore_ascii_case(name))
    }

    /// The posture this mode enforces.
    #[must_use]
    pub fn posture(self) -> ModePosture {
        match self {
            TaskMode::Agent | TaskMode::Multitask => ModePosture {
                effect_ceiling: None,
                allowed_capabilities: None,
                subagents: true,
                reproduction_first: false,
            },
            TaskMode::Debug => ModePosture {
                effect_ceiling: None,
                allowed_capabilities: None,
                subagents: true,
                reproduction_first: true,
            },
            TaskMode::Plan | TaskMode::Ask => ModePosture {
                effect_ceiling: Some(EffectClass::ReadOnly),
                allowed_capabilities: Some(READ_ONLY_CAPABILITIES),
                subagents: false,
                reproduction_first: false,
            },
        }
    }
}

/// What a mode enforces. Every field is a restriction: `None` means the
/// profile's own envelope stands, never "more".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModePosture {
    /// The highest effect class the mode admits; `None` = the profile's own.
    pub effect_ceiling: Option<EffectClass>,
    /// The only capabilities the mode admits; `None` = the profile's own.
    pub allowed_capabilities: Option<&'static [&'static str]>,
    /// Whether `agent.spawn` is admitted (a child could write).
    pub subagents: bool,
    /// Whether a fix needs a reproduction first (PX-039).
    pub reproduction_first: bool,
}

impl ModePosture {
    /// Whether any write or execution effect can be admitted at all.
    #[must_use]
    pub fn writes(&self) -> bool {
        self.effect_ceiling != Some(EffectClass::ReadOnly)
    }

    /// Why a call is outside the posture, or `None` when the posture admits
    /// it (the profile and the lease still judge it afterwards).
    #[must_use]
    pub fn refusal(&self, effect_class: EffectClass, capabilities: &[String]) -> Option<String> {
        if let Some(ceiling) = self.effect_ceiling
            && effect_class > ceiling
        {
            return Some(format!(
                "{effect_class:?} exceeds the mode's {ceiling:?} ceiling"
            ));
        }
        if let Some(allowed) = self.allowed_capabilities
            && let Some(c) = capabilities.iter().find(|c| !allowed.contains(&c.as_str()))
        {
            return Some(format!(
                "`{c}` is outside the mode's read-only capabilities"
            ));
        }
        None
    }
}

/// What the router optimises (docs/27 `objective_mode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Objective {
    /// The cheapest plan that clears the quality floor.
    Cost,
    /// The default balance (the registry's `auto` floor).
    Balance,
    /// The strongest plan.
    Intelligence,
}

impl Objective {
    /// The stable name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Objective::Cost => "COST",
            Objective::Balance => "BALANCE",
            Objective::Intelligence => "INTELLIGENCE",
        }
    }

    /// The objective a name denotes (case-insensitive).
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        [Objective::Cost, Objective::Balance, Objective::Intelligence]
            .into_iter()
            .find(|o| o.name().eq_ignore_ascii_case(name.trim()))
    }

    /// The registry quality-floor row the objective selects: the modes the
    /// registry document defines (`auto`, `quality`, `economy`).
    #[must_use]
    pub fn floor_mode(self) -> &'static str {
        match self {
            Objective::Cost => "economy",
            Objective::Balance => "auto",
            Objective::Intelligence => "quality",
        }
    }
}

/// The reasoning efforts a request may carry.
pub const EFFORTS: [&str; 3] = ["low", "medium", "high"];

/// Whether `effort` is a value a request may carry.
#[must_use]
pub fn valid_effort(effort: &str) -> bool {
    EFFORTS.contains(&effort)
}

/// Whether `tier` is a service-tier name (letters, digits, underscore).
#[must_use]
pub fn valid_tier(tier: &str) -> bool {
    !tier.is_empty()
        && tier.len() <= 32
        && tier.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The user's execution preference for a task, as recorded.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionPreference {
    /// What the router optimises; `None` = nothing recorded.
    #[serde(default)]
    pub objective: Option<Objective>,
    /// Reasoning effort; `None` = the catalog entry's own.
    #[serde(default)]
    pub effort: Option<String>,
    /// Service tier; `None` = the catalog entry's own.
    #[serde(default)]
    pub service_tier: Option<String>,
    /// A manual model pin (endpoint, model); applied from the next run.
    #[serde(default)]
    pub pin: Option<(String, String)>,
}

impl ExecutionPreference {
    /// Whether nothing is recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}
