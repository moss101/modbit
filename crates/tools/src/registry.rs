//! Immutable versioned tool registry (docs/16 "Tool registry").

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::pipeline::InvokeContext;
use crate::{EffectClass, Error, Result};

/// Idempotency semantics of a tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Idempotency {
    /// Safe to repeat.
    Idempotent,
    /// Repeating may duplicate an effect; retries need reconciliation.
    NonIdempotent,
}

/// Immutable tool metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSpec {
    /// `namespace.name`.
    pub name: String,
    /// Version.
    pub version: String,
    /// Human description (shown in projections).
    pub description: String,
    /// JSON Schema for arguments.
    pub input_schema: Value,
    /// JSON Schema for the structured output.
    pub output_schema: Value,
    /// Effect class.
    pub effect_class: EffectClass,
    /// Capability selectors required (docs/23 examples).
    pub required_capabilities: Vec<String>,
    /// Execution profiles the tool may run under.
    pub execution_profiles: Vec<String>,
    /// Default timeout.
    pub timeout_ms: u64,
    /// Default inline output budget in bytes.
    pub output_budget_bytes: u64,
    /// Idempotency.
    pub idempotency: Idempotency,
}

/// What a tool returns to the pipeline.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolOutcome {
    /// Application-level success.
    pub ok: bool,
    /// Structured output.
    pub structured_output: Value,
    /// Raw stdout-like bytes (spilled when large).
    pub stdout: Option<Vec<u8>>,
    /// Raw stderr-like bytes.
    pub stderr: Option<Vec<u8>>,
    /// Workspace revision after a write.
    pub workspace_revision_after: Option<u64>,
    /// Application failure code (when `!ok`).
    pub error_code: Option<String>,
    /// Message.
    pub error_message: Option<String>,
    /// Infrastructure failure (transport, effector unavailable) rather than application failure.
    pub infra_failure: bool,
    /// The effect may have happened but the outcome is unknown.
    pub unknown_outcome: Option<String>,
}

impl ToolOutcome {
    /// Successful outcome with structured output.
    pub fn ok(v: Value) -> Self {
        Self {
            ok: true,
            structured_output: v,
            ..Default::default()
        }
    }

    /// Application failure.
    pub fn fail(code: &str, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error_code: Some(code.into()),
            error_message: Some(message.into()),
            ..Default::default()
        }
    }

    /// Infrastructure failure.
    pub fn infra(code: &str, message: impl Into<String>) -> Self {
        Self {
            infra_failure: true,
            ..Self::fail(code, message)
        }
    }
}

/// Boxed future.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A registered tool.
pub trait Tool: Send + Sync {
    /// Spec.
    fn spec(&self) -> &ToolSpec;
    /// Execute against the real effector. Arguments are already schema-valid.
    fn invoke<'a>(&'a self, ctx: &'a InvokeContext, args: Value) -> BoxFuture<'a, ToolOutcome>;
}

/// The registry.
#[derive(Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.tools.keys()).finish()
    }
}

impl ToolRegistry {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool; the same name may not be registered twice (metadata is immutable).
    pub fn register(&mut self, tool: Arc<dyn Tool>) -> Result<()> {
        let spec = tool.spec();
        if spec.name.split('.').count() < 2 || spec.name.contains(' ') {
            return Err(Error::Registry(format!(
                "tool name `{}` must be namespace.name",
                spec.name
            )));
        }
        if spec.input_schema.get("type").is_none() && spec.input_schema.get("$ref").is_none() {
            return Err(Error::Registry(format!(
                "tool `{}` has no input schema",
                spec.name
            )));
        }
        jsonschema::validator_for(&spec.input_schema).map_err(|e| {
            Error::Registry(format!("tool `{}` input schema invalid: {e}", spec.name))
        })?;
        if self.tools.contains_key(&spec.name) {
            return Err(Error::Registry(format!(
                "tool `{}` already registered",
                spec.name
            )));
        }
        self.tools.insert(spec.name.clone(), tool);
        Ok(())
    }

    /// Look up.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// Every spec, sorted by name.
    #[must_use]
    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.values().map(|t| t.spec().clone()).collect()
    }

    /// Project a subset by name (docs/16 dynamic projection; the kernel remains the boundary).
    #[must_use]
    pub fn project(&self, names: &[&str]) -> Vec<ToolSpec> {
        names
            .iter()
            .filter_map(|n| self.tools.get(*n).map(|t| t.spec().clone()))
            .collect()
    }

    /// Number of tools.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Whether empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}
