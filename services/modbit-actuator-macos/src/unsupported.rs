//! The backend of every platform that has no actuator yet (docs/66 CUC-A03,
//! V06): the helper still speaks the authenticated RPC - so the Core's
//! handshake, version and supply-chain checks are exercised on every
//! operating system - and answers every operation `UNSUPPORTED_REQUEST`.

use std::time::Duration;

use modbit_protocol::v1 as wire;

use crate::backend::{
    Backend, CaptureArgs, Ctl, Fail, PerformArgs, Permissions, RawFrame, ReadArgs, Res,
};

/// No operating-system control.
pub struct Unsupported;

fn no<T>() -> Res<T> {
    Err(Fail::nd(
        "UNSUPPORTED_REQUEST",
        "this platform has no actuator backend (macOS only)",
    ))
}

impl Backend for Unsupported {
    fn name(&self) -> &'static str {
        "modbit-actuator-macos"
    }
    fn platform(&self) -> &'static str {
        std::env::consts::OS
    }
    fn capabilities(&self) -> Vec<String> {
        vec![]
    }
    fn permissions(&self) -> Permissions {
        // Not a permission problem: the operations say what is wrong.
        Permissions {
            accessibility: true,
            screen_recording: true,
        }
    }
    fn secure_desktop(&self) -> bool {
        false
    }
    fn list_apps(&self) -> Res<Vec<wire::ApplicationInfo>> {
        no()
    }
    fn resolve(&self, _: &str, _: &str, _: u32) -> Res<wire::ApplicationInfo> {
        no()
    }
    fn verify(&self, _: &wire::ApplicationIdentity) -> Res<wire::ApplicationInfo> {
        no()
    }
    fn read_tree(&self, _: &ReadArgs<'_>, _: &Ctl) -> Res<wire::ReadStateResponse> {
        no()
    }
    fn tree_digest(
        &self,
        _: &wire::ApplicationIdentity,
        _: &str,
        _: &Ctl,
    ) -> Res<(String, wire::WindowInfo)> {
        no()
    }
    fn capture(&self, _: &CaptureArgs<'_>, _: &Ctl) -> Res<RawFrame> {
        no()
    }
    fn perform(&self, _: &PerformArgs<'_>, _: &Ctl) -> Res<wire::PerformResponse> {
        no()
    }
    fn human_idle(&self) -> Option<Duration> {
        None
    }
    fn window_exists(&self, _: u32, _: &str) -> bool {
        false
    }
}
