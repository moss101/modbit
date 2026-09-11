//! `modbit-core-runtime` — scheduler, WorkGraph/AgentGraph/StateGraph and
//! conditional plan admission (docs/12, docs/14). THE doc 81 orchestration
//! owner.
//!
//! M2.7 ships the one-agent harness contracts (`harness`): budgets, the plan
//! gate on writes, the completion handshake, failure signatures and bounded
//! observations with declared truncation. The Core's runtime loop
//! (`services/modbit-core/src/runtime.rs`) drives them against the real
//! gateway, tool host and event store. Scheduler and WorkGraph arrive with
//! their own scheduled tasks.
//!
//! M3 EPR-014 ships `admission`: the one place a conditional plan is validated
//! before anything dispatches, and the one place a slot is allowed to
//! activate. EPR-004 compiles plans and EPR-005 executes them; both go through
//! this interface rather than repeating its rules.

#![forbid(unsafe_code)]

pub mod admission;
pub mod diagnostics;
pub mod harness;

pub use admission::{Activation, Admission, Refused, RunLedger, admit_activation, admit_plan};
pub use diagnostics::{FailureSource, classify};
pub use harness::{
    Budgets, Exhausted, HarnessRefusal, HarnessState, Observation, Plan, failure_signature, observe,
};
