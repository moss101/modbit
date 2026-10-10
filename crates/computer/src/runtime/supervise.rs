//! Keeping the actuator and its sessions alive and honest: the event pump
//! (a physical key parks the controller, the actuator's own Stop control
//! stops the task), the loss handler (a dead actuator ends its sessions and
//! is relaunched with backoff), the watchdog (an idle session or an ended
//! grant closes) and the heartbeat that keeps the actuator's lease alive.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use modbit_protocol::v1 as wire;

use super::{ComputerRuntime, Controller, now_ms};
use crate::actuator::{Actuator, Call};

/// Watch one actuator connection: pump its events, notice its end.
pub(super) fn watch(rt: &ComputerRuntime, actuator: Arc<dyn Actuator>, epoch: u64) {
    let mut events = actuator.events();
    let pump = rt.clone();
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(e) => pump.on_event(epoch, e).await,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(_) => break,
            }
        }
    });
    let lost = rt.clone();
    tokio::spawn(async move {
        actuator.closed().await;
        lost.actuator_lost(epoch).await;
    });
}

impl ComputerRuntime {
    async fn on_event(&self, epoch: u64, e: wire::ActuatorEvent) {
        let Some(body) = e.event else {
            return;
        };
        match body {
            wire::actuator_event::Event::HumanInput(_) => {
                // The person is using the machine: the controller parks
                // (CUC-D04) and the agent's next input waits out a cooldown.
                let until = now_ms()
                    + i64::try_from(self.shared.cfg.human_cooldown.as_millis()).unwrap_or(i64::MAX);
                let until = u64::try_from(until).unwrap_or(0);
                let mut st = self.lock();
                st.human_until_ms = st.human_until_ms.max(until);
                for s in st.sessions.values_mut().filter(|s| s.epoch == epoch) {
                    if s.controller == Controller::Agent {
                        s.controller = Controller::Parked { until_ms: until };
                    }
                }
            }
            wire::actuator_event::Event::StopRequested(stop) => {
                // The actuator's own Stop control: the same as the desktop's.
                let tasks: Vec<_> = {
                    let st = self.lock();
                    st.sessions
                        .values()
                        .filter(|s| {
                            s.epoch == epoch
                                && (stop.control_id.is_empty() || s.control_id == stop.control_id)
                        })
                        .map(|s| (s.task_id, s.session_id, s.run_id.clone()))
                        .collect()
                };
                for (t, sid, run) in tasks {
                    let _ = self
                        .stop_task(t, sid, &run, &format!("Stop control ({})", stop.source))
                        .await;
                }
            }
            wire::actuator_event::Event::WindowGone(w) => {
                let ids: Vec<String> = {
                    let st = self.lock();
                    st.sessions
                        .values()
                        .filter(|s| {
                            s.epoch == epoch
                                && (w.control_id.is_empty() || s.control_id == w.control_id)
                        })
                        .map(|s| s.control_id.clone())
                        .collect()
                };
                for id in ids {
                    self.close_session(&id, "WINDOW_GONE", true).await;
                }
            }
            wire::actuator_event::Event::PermissionChanged(_) => {}
        }
    }

    /// One tick of the watchdog: close what ran out. Returns the sessions closed.
    pub async fn watchdog_tick(&self) -> usize {
        let now = now_ms();
        let idle = self.shared.cfg.idle_ttl;
        let due: Vec<(String, &'static str)> = {
            let st = self.lock();
            st.sessions
                .values()
                .filter_map(|s| {
                    if s.expires_ms <= now {
                        Some((s.control_id.clone(), "GRANT_EXPIRED"))
                    } else if s.last_activity.elapsed() >= idle {
                        Some((s.control_id.clone(), "WATCHDOG"))
                    } else {
                        None
                    }
                })
                .collect()
        };
        let n = due.len();
        for (id, why) in due {
            self.close_session(&id, why, true).await;
        }
        n
    }

    async fn heartbeat(&self) {
        let controller = {
            let st = self.lock();
            st.controller.clone()
        };
        let Some(control_id) = controller else {
            return;
        };
        let Some(a) = self.actuator() else {
            return;
        };
        let due = {
            let mut last = self
                .shared
                .last_ping
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if last.elapsed() >= self.shared.cfg.lease_ttl / 3 {
                *last = Instant::now();
                true
            } else {
                false
            }
        };
        if due {
            let _ = a
                .call(
                    Call::Ping(wire::PingRequest { control_id }),
                    self.shared.cfg.observe_deadline,
                )
                .await;
        }
    }
}

pub(super) fn run(rt: ComputerRuntime) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(rt.shared.cfg.tick).await;
            rt.watchdog_tick().await;
            rt.heartbeat().await;
            // Relaunch a lost actuator, with backoff.
            if rt.actuator().is_none()
                && rt.configured()
                && !rt.shared.fatal.load(Ordering::SeqCst)
                && Instant::now()
                    >= *rt
                        .shared
                        .next_attempt
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                && rt
                    .shared
                    .config
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .is_some()
            {
                match rt.attach().await {
                    Ok(()) => {}
                    Err(_) => {
                        let mut b = rt.shared.backoff.lock().unwrap_or_else(|e| e.into_inner());
                        *rt.shared
                            .next_attempt
                            .lock()
                            .unwrap_or_else(|e| e.into_inner()) = Instant::now() + *b;
                        *b = (*b * 2).min(Duration::from_secs(10));
                    }
                }
            }
        }
    });
}
