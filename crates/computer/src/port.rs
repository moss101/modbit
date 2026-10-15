//! The port the tool registry calls (PX-069). The tools crate defines the
//! `computer.*` tools and their schemas; the Core implements this trait over
//! the [`ComputerRuntime`](crate::ComputerRuntime), binding each call to the
//! task, the run, the mode and the policy it is made under.

use serde_json::Value;

use crate::actuator::BoxFuture;
use crate::ops::Op;
use crate::runtime::{Answer, Failure};
use crate::taxonomy::Refusal;

/// What the pipeline knows of one call that the port's owner does not.
#[derive(Clone, Debug, Default)]
pub struct CallInfo {
    /// The tool call.
    pub tool_call_id: String,
    /// The approval that authorised it, once there is one.
    pub approval_id: String,
    /// The intent hash that approval bound.
    pub intent_hash: String,
}

/// Native control as the tools see it.
pub trait ComputerPort: Send + Sync {
    /// Whether an actuator is attached or restarting: the tools are offered
    /// to a model only while this holds.
    fn offered(&self) -> bool;

    /// The refusals a call can get without asking a person, and the intent an
    /// approval would bind (`None` for a call that needs no approval).
    fn prepare<'a>(
        &'a self,
        info: &'a CallInfo,
        tool: &'a str,
        op: &'a Op,
    ) -> BoxFuture<'a, Result<Option<Value>, Refusal>>;

    /// Run an approved (or approval-free) call.
    fn invoke<'a>(
        &'a self,
        info: &'a CallInfo,
        tool: &'a str,
        op: Op,
    ) -> BoxFuture<'a, Result<Answer, Failure>>;
}
