//! Shared helpers of the helper's integration tests: launching the real
//! helper binary the way the Core does, and building requests.
#![allow(dead_code)]

#[cfg(target_os = "macos")]
pub mod fixture_app;

use std::path::PathBuf;
use std::time::Duration;

use modbit_computer::actuator::{
    Actuator, ActuatorConfig, Call, CallError, LaunchError, ProcessActuator, Reply,
};
use modbit_protocol::v1 as wire;

pub fn helper_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_modbit-actuator-macos"))
}

pub fn config(env: &[(&str, &str)]) -> ActuatorConfig {
    ActuatorConfig {
        executable: helper_bin(),
        args: vec![],
        env: env
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect(),
        log_path: None,
        supply: None,
    }
}

pub async fn launch(env: &[(&str, &str)]) -> Result<ProcessActuator, LaunchError> {
    ProcessActuator::launch(&config(env), Duration::from_secs(10)).await
}

pub async fn call(a: &ProcessActuator, c: Call) -> Result<Reply, CallError> {
    a.call(c, Duration::from_secs(20)).await
}

/// The error code of a failed call ("OK" for a success).
pub fn code(r: &Result<Reply, CallError>) -> String {
    match r {
        Ok(_) => "OK".into(),
        Err(CallError::Remote(e)) => e.code.clone(),
        Err(other) => format!("{other:?}"),
    }
}

pub fn delivery(r: &Result<Reply, CallError>) -> i32 {
    match r {
        Err(CallError::Remote(e)) => e.delivery,
        _ => -1,
    }
}

pub fn ident_of(info: &wire::ApplicationInfo) -> wire::ApplicationIdentity {
    info.identity.clone().expect("an identity")
}
