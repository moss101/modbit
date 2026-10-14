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

static LOG: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// What the most recently launched helper wrote to its standard error.
pub fn last_helper_log() -> String {
    let p = LOG.lock().unwrap().clone();
    p.and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default()
}

pub async fn launch(env: &[(&str, &str)]) -> Result<ProcessActuator, LaunchError> {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let mut cfg = config(env);
    let log = std::env::temp_dir().join(format!(
        "modbit-helper-{}-{}.log",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    cfg.log_path = Some(log.clone());
    *LOG.lock().unwrap() = Some(log);
    ProcessActuator::launch(&cfg, Duration::from_secs(10)).await
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
