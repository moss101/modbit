//! `modbit-computer` - the Computer Runtime for native applications
//! (PX-069, PX-070, PX-075, PX-076; docs/66, DR-PX-2026-10-03-008).
//!
//! The Browser and Computer Runtime owner's second runtime: the model sees
//! typed `computer.*` tools, the Core owns control sessions, handles,
//! refusals, the unknown-outcome latch and the audit, and a *separate
//! actuator process* - behind the authenticated local RPC of
//! `computer.proto` - is the only thing that touches the screen or the
//! input devices. Native control is semantic first: an element action on the
//! application's accessibility tree, then a coordinate action bound to a
//! fresh screenshot, then raw input (CUC-A04).
//!
//! * [`taxonomy`] - the 25 typed refusal codes and their escalations.
//! * [`canvas`] - the fixed 1280 x 800 coordinate canvas and its one scaler.
//! * [`model`], [`tree`] - what an actuator reports and how a tree reads as text.
//! * [`policy`] - the versioned list of applications no approval lifts, and
//!   the administrative restrictions.
//! * [`frame`] - screenshot artifacts: masking, the content-aware codec, memo.
//! * [`actuator`] - the Core's client of the actuator RPC.
//! * [`ops`] - the tool calls, parsed strictly.
//! * [`runtime`] - sessions, grants, handles, the latch, the watchdog, stop.
//!
//! Dependency direction: this crate depends on the browser crate's shared
//! escalation type, the domain events and the protocol; it never depends on
//! the tool registry or the Core, which depend on it.

#![forbid(unsafe_code)]

pub mod actuator;
pub mod canvas;
pub mod frame;
pub mod model;
pub mod ops;
pub mod policy;
pub mod port;
pub mod runtime;
pub mod taxonomy;
pub mod tree;

pub use port::{CallInfo, ComputerPort};
pub use runtime::{
    Answer, CallCtx, ComputerRuntime, Failure, FrameArtifact, PendingEvent, RuntimeConfig,
    RuntimeView,
};
pub use taxonomy::{Code, Refusal};
