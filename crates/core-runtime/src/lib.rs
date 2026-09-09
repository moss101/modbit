//! `modbit-core-runtime` — scheduler, WorkGraph/AgentGraph/StateGraph and
//! conditional plan admission (docs/12, docs/14). THE doc 81 orchestration
//! owner.
//!
//! M2.7 ships the one-agent harness contracts (`harness`): budgets, the plan
//! gate on writes, the completion handshake, failure signatures and bounded
//! observations with declared truncation. The Core's runtime loop
//! (`services/modbit-core/src/runtime.rs`) drives them against the real
//! gateway, tool host and event store. Scheduler, WorkGraph and plan
//! admission arrive with their own scheduled tasks.

#![forbid(unsafe_code)]

pub mod harness;

pub use harness::{
    Budgets, Exhausted, HarnessRefusal, HarnessState, Observation, Plan, failure_signature, observe,
};
