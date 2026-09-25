//! Diagnostics export (IMP-EV-0142, REQ-EV-0142; docs/71 "Desktop
//! diagnostics package"): the Core's side of
//! [`modbit_observability::diagnostics`]. It reads its own log, store,
//! gateway and leases to fill a package, redacts the whole package with the
//! one redactor (REQ-EV-0017) and seals it; and it replays a package against
//! its log. Read-only: nothing here appends.

use modbit_domain::event::PayloadRef;
use modbit_domain::ids::{SessionId, TaskId};
use modbit_observability::diagnostics as d;

use crate::server::Core;

/// The most trace lines a package carries (the latest are kept).
const TRACE_BOUND: usize = 5_000;
/// The most recent failures a package carries.
const ERRORS_BOUND: usize = 200;

/// Event types that are failures: their code is read from the payload.
fn is_failure(event_type: &str) -> bool {
    event_type.ends_with("Failed")
        || event_type.ends_with("NeedsAttention")
        || event_type.ends_with("Denied")
        || event_type.ends_with("UnknownOutcome")
        || event_type == "SecurityEventRecorded"
}

/// A failure's class, code and retryability, from its payload.
fn failure_of(payload: &serde_json::Value) -> (String, String, Option<bool>) {
    let diag = &payload["diagnostic"];
    if let Some(code) = diag["code"].as_str() {
        return (
            diag["class"].as_str().unwrap_or_default().to_owned(),
            code.to_owned(),
            diag["retryable"].as_bool(),
        );
    }
    let code = ["failure_code", "code", "kind"]
        .into_iter()
        .find_map(|k| payload[k].as_str())
        .unwrap_or_default()
        .to_owned();
    (String::new(), code, None)
}

/// An endpoint's host, without scheme, user info, path or query.
fn host_of(base_url: &str) -> String {
    let rest = base_url.split_once("://").map_or(base_url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    authority
        .rsplit_once('@')
        .map_or(authority, |(_, h)| h)
        .to_owned()
}

fn hex_id(b: &[u8; 16]) -> String {
    hex::encode(b)
}

fn id_of(hex: &str) -> Option<[u8; 16]> {
    let v = hex::decode(hex).ok()?;
    v.try_into().ok()
}

/// Build, redact and seal a package for `session` (narrowed to `task`).
///
/// # Errors
/// `(code, detail)`: `UNKNOWN_SESSION` when the log holds nothing of it;
/// `STORE` when the store cannot be read.
pub(crate) async fn export(
    core: &Core,
    session: SessionId,
    task: Option<TaskId>,
    include_content: bool,
) -> Result<d::Package, (String, String)> {
    let store_err = |e: modbit_event_store::Error| ("STORE".to_owned(), e.to_string());
    let mut pkg = d::Package {
        schema: d::SCHEMA.into(),
        generated_at_ms: modbit_domain::Timestamp::now().0,
        ..d::Package::default()
    };
    {
        let store = core.store.lock().await;
        let events = store
            .read_session(&session, 0, usize::MAX)
            .map_err(store_err)?;
        if events.is_empty() {
            return Err((
                "UNKNOWN_SESSION".into(),
                format!("the log holds no events of session {session}"),
            ));
        }
        let events: Vec<_> = events
            .into_iter()
            .filter(|e| task.is_none_or(|t| e.envelope.task_id == Some(t)))
            .collect();
        if events.is_empty() {
            return Err((
                "UNKNOWN_TASK".into(),
                format!("the log holds no events of that task in session {session}"),
            ));
        }
        pkg.scope = d::Scope {
            session_id: session.to_string(),
            task_id: task.map(|t| t.to_string()),
            task_ids: {
                let mut ids: Vec<String> = events
                    .iter()
                    .filter_map(|e| e.envelope.task_id.map(|t| t.to_string()))
                    .collect();
                ids.sort();
                ids.dedup();
                ids
            },
        };
        // Ranges and chain heads, per aggregate, in first-seen order.
        let mut ranges: Vec<d::AggregateRange> = Vec::new();
        let mut index: std::collections::HashMap<[u8; 16], usize> =
            std::collections::HashMap::new();
        let mut objects = std::collections::BTreeSet::new();
        let mut trace = Vec::new();
        let mut errors = Vec::new();
        let mut content = Vec::new();
        for e in &events {
            let env = &e.envelope;
            let i = *index.entry(env.aggregate_id).or_insert_with(|| {
                ranges.push(d::AggregateRange {
                    aggregate_type: format!("{:?}", env.aggregate_type),
                    aggregate_id: hex_id(&env.aggregate_id),
                    first_sequence: env.sequence,
                    first_offset: e.offset,
                    ..d::AggregateRange::default()
                });
                ranges.len() - 1
            });
            let r = &mut ranges[i];
            r.last_sequence = env.sequence;
            r.last_offset = e.offset;
            r.count += 1;
            r.head_hash.clone_from(&env.integrity_hash);
            if let PayloadRef::Object { object_hash, .. } = &env.payload {
                objects.insert(object_hash.clone());
            }
            let failure = is_failure(&env.event_type);
            let payload = if failure || include_content {
                store.payload(env).ok()
            } else {
                None
            };
            let mut code = None;
            if failure && let Some(p) = &payload {
                let (class, c, retryable) = failure_of(p);
                if !c.is_empty() {
                    errors.push(d::ErrorCode {
                        offset: e.offset,
                        event_type: env.event_type.clone(),
                        class,
                        code: c.clone(),
                        retryable,
                        task_id: env.task_id.map(|t| t.to_string()),
                    });
                    code = Some(c);
                }
            }
            trace.push(d::TraceLine {
                offset: e.offset,
                aggregate_type: format!("{:?}", env.aggregate_type),
                sequence: env.sequence,
                event_type: env.event_type.clone(),
                task_id: env.task_id.map(|t| t.to_string()),
                run_id: env.run_id.map(|r| r.to_string()),
                code,
            });
            if include_content {
                content.push(d::ContentEvent {
                    offset: e.offset,
                    event_type: env.event_type.clone(),
                    payload: payload.unwrap_or(serde_json::Value::Null),
                });
            }
        }
        if trace.len() > TRACE_BOUND {
            pkg.trace_truncated = true;
            trace.drain(..trace.len() - TRACE_BOUND);
        }
        if errors.len() > ERRORS_BOUND {
            errors.drain(..errors.len() - ERRORS_BOUND);
        }
        // Integrity now: the database, every chain in scope, the receipts.
        pkg.integrity.database = match store.integrity_check() {
            Ok(()) => "ok".into(),
            Err(e) => e.to_string(),
        };
        for r in &ranges {
            let verified = id_of(&r.aggregate_id)
                .ok_or_else(|| "not an aggregate id".to_owned())
                .and_then(|id| store.verify_aggregate(&id).map_err(|e| e.to_string()));
            match verified {
                Ok(_) => pkg.integrity.chains_verified += 1,
                Err(e) => pkg
                    .integrity
                    .chain_failures
                    .push(format!("{} {}: {e}", r.aggregate_type, r.aggregate_id)),
            }
        }
        pkg.integrity.receipts = match store.receipts(None) {
            Ok(all) => match modbit_policy::ledger::verify_chain(&all) {
                Ok(()) => "valid".into(),
                Err(e) => format!("invalid: {e}"),
            },
            Err(e) => format!("unreadable: {e}"),
        };
        pkg.health.last_offset = store.last_offset().unwrap_or(0);
        pkg.aggregates = ranges;
        pkg.object_refs = objects.into_iter().collect();
        pkg.trace = trace;
        pkg.recent_errors = errors;
        pkg.content = include_content.then_some(content);
    }
    let recovery = core.recovery();
    pkg.build = d::Build {
        core_version: env!("CARGO_PKG_VERSION").into(),
        protocol_version: format!(
            "{}.{}",
            modbit_protocol::PROTOCOL_VERSION.major,
            modbit_protocol::PROTOCOL_VERSION.minor
        ),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        boot_generation: recovery.boot_generation,
    };
    let started = core.started_at().0;
    pkg.health.started_at_ms = started;
    pkg.health.uptime_ms = u64::try_from(pkg.generated_at_ms.saturating_sub(started)).unwrap_or(0);
    pkg.health.recovery = {
        let mut lines = vec![format!(
            "verified {} events in {} aggregates in {} ms; projections rebuilt: {}",
            recovery.events_verified,
            recovery.aggregates_verified,
            recovery.recovery_ms,
            recovery.projections_rebuilt
        )];
        lines.extend(recovery.notes.iter().cloned());
        lines
    };
    pkg.providers = core
        .gateway
        .endpoints()
        .into_iter()
        .map(|ep| {
            let h = core.gateway.health(&ep.name);
            d::ProviderHealth {
                host: host_of(&ep.base_url),
                kind: format!("{:?}", ep.kind),
                credential_configured: ep.credential.resolve().is_some(),
                models: ep.models.iter().map(|m| m.model.clone()).collect(),
                requests: h.requests,
                successes: h.successes,
                failures: h.failures,
                interruptions: h.interruptions,
                rate_limited: h.rate_limited,
                last_first_token_ms: h.last_first_token_ms,
                name: ep.name,
            }
        })
        .collect();
    pkg.leases.capacity = core
        .capacity
        .view()
        .tickets
        .iter()
        .map(|t| {
            format!(
                "{} ticket {} until {}",
                t.holder, t.ticket_id, t.expires_at_ms
            )
        })
        .collect();
    pkg.leases.sandboxes = {
        let mut s: Vec<String> = core
            .tools
            .sandboxes
            .lock()
            .await
            .keys()
            .map(|t| t.to_string())
            .collect();
        s.sort();
        s
    };
    // Whatever the package holds leaves redacted: every value in custody,
    // every credential shape (REQ-EV-0017).
    let mut v = serde_json::to_value(&pkg).map_err(|e| ("STORE".to_owned(), e.to_string()))?;
    let redactions = core.tools.redactor().error_json(&mut v);
    let mut pkg: d::Package =
        serde_json::from_value(v).map_err(|e| ("STORE".to_owned(), e.to_string()))?;
    pkg.redactions = redactions as u64;
    Ok(pkg.sealed())
}

/// Replay a package against this Core's log.
///
/// # Errors
/// `(BAD_PACKAGE, detail)` when it is not a diagnostics package.
pub(crate) async fn verify(
    core: &Core,
    package_json: &str,
) -> Result<d::Verification, (String, String)> {
    let pkg: d::Package = serde_json::from_str(package_json).map_err(|e| {
        (
            "BAD_PACKAGE".to_owned(),
            format!("not a diagnostics package: {e}"),
        )
    })?;
    let store = core.store.lock().await;
    let head_of = |agg: &str, seq: u64| -> Option<String> {
        let id = id_of(agg)?;
        let e = store
            .read_aggregate(&id, seq.checked_sub(1)?, 1)
            .ok()?
            .into_iter()
            .next()?;
        (e.envelope.sequence == seq).then_some(e.envelope.integrity_hash)
    };
    let chain_ok = |agg: &str| -> Result<(), String> {
        let id = id_of(agg).ok_or_else(|| "not an aggregate id".to_owned())?;
        store
            .verify_aggregate(&id)
            .map(|_| ())
            .map_err(|e| e.to_string())
    };
    Ok(d::verify(&pkg, head_of, chain_ok))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_is_named_without_what_could_carry_a_secret() {
        assert_eq!(host_of("https://api.openai.com"), "api.openai.com");
        assert_eq!(
            host_of("https://user:pass@gw.example.com:8443/v1?key=abc"),
            "gw.example.com:8443"
        );
        assert_eq!(host_of("127.0.0.1:9000/x"), "127.0.0.1:9000");
    }

    #[test]
    fn a_failure_reads_its_code_from_its_diagnosis_or_its_own_field() {
        let (class, code, r) = failure_of(&serde_json::json!({
            "reason": "x", "diagnostic": {"class": "PROVIDER", "code": "AUTH_REJECTED", "retryable": false}
        }));
        assert_eq!(
            (class.as_str(), code.as_str(), r),
            ("PROVIDER", "AUTH_REJECTED", Some(false))
        );
        assert_eq!(
            failure_of(&serde_json::json!({"failure_code": "TIMEOUT"})).1,
            "TIMEOUT"
        );
        assert_eq!(
            failure_of(&serde_json::json!({"kind": "SECRET_IN_TOOL_RESULT"})).1,
            "SECRET_IN_TOOL_RESULT"
        );
    }
}
