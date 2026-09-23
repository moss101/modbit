//! M9.6 (docs/52 "Security gates": property tests for lease fencing; fuzzer
//! for event migration/replay). On the real SQLite store, for arbitrary
//! interleavings of lease acquisitions and fenced appends, an append lands
//! only under the session's current lease generation and a stale one writes
//! nothing; the store's migrations are idempotent across reopen; and a
//! rebuild of the projections from arbitrary appended history reproduces the
//! session it projected while appending.

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::session::SessionEvent;
use modbit_domain::{SessionId, SpaceId, TenantId, UserId};
use modbit_event_store::{AppendRequest, Error, EventStore, NewEvent};
use proptest::prelude::*;

fn typed<E: serde::Serialize>(event_type: &str, e: &E) -> NewEvent {
    NewEvent::new(
        event_type,
        serde_json::to_value(e).unwrap(),
        Actor::Core("fuzz".into()),
    )
}

fn req(tenant: TenantId, session: SessionId, events: Vec<NewEvent>) -> AppendRequest {
    AppendRequest {
        tenant_id: tenant,
        session_id: session,
        task_id: None,
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::Session,
        aggregate_id: *session.as_bytes(),
        expected_sequence: None,
        events,
    }
}

#[derive(Clone, Debug)]
enum Op {
    /// A client acquires the lease: the generation becomes current + 1.
    Acquire,
    /// A fenced append presenting the current generation minus `stale`
    /// (0 = current, so it must land; otherwise it must not).
    Append { stale: u64 },
}

fn ops() -> impl Strategy<Value = Vec<Op>> {
    prop::collection::vec(
        prop_oneof![
            Just(Op::Acquire),
            (0u64..4).prop_map(|stale| Op::Append { stale }),
        ],
        1..24,
    )
}

fn open_with_session() -> (tempfile::TempDir, EventStore, TenantId, SessionId) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = EventStore::open(dir.path()).unwrap();
    let tenant = TenantId::new();
    let session = SessionId::new();
    store
        .append(req(
            tenant,
            session,
            vec![typed(
                "SessionCreated",
                &SessionEvent::SessionCreated {
                    tenant_id: tenant,
                    user_id: UserId::new(),
                    space_id: SpaceId::new(),
                },
            )],
        ))
        .unwrap();
    (dir, store, tenant, session)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn only_the_current_lease_generation_can_advance_state(ops in ops()) {
        let (_dir, mut store, tenant, session) = open_with_session();
        let mut generation = 0u64;
        let mut landed = 0u64;
        for op in ops {
            match op {
                Op::Acquire => {
                    generation += 1;
                    store
                        .append(req(
                            tenant,
                            session,
                            vec![typed(
                                "SessionLeaseAcquired",
                                &SessionEvent::SessionLeaseAcquired {
                                    lease_generation: generation,
                                    owner: format!("client-{generation}"),
                                },
                            )],
                        ))
                        .unwrap();
                    prop_assert_eq!(store.session(&session).unwrap().unwrap().lease_generation, generation);
                }
                Op::Append { stale } => {
                    let presented = generation.saturating_sub(stale);
                    let before = store.session(&session).unwrap().unwrap().generation;
                    let r = store.append_fenced(
                        req(
                            tenant,
                            session,
                            vec![typed(
                                "SessionFocusChanged",
                                &SessionEvent::SessionFocusChanged { task_id: None },
                            )],
                        ),
                        presented,
                    );
                    let after = store.session(&session).unwrap().unwrap().generation;
                    if presented == generation {
                        prop_assert!(r.is_ok(), "current generation {generation} refused: {r:?}");
                        prop_assert_eq!(after, before + 1, "the fenced append advanced the aggregate");
                        landed += 1;
                    } else {
                        prop_assert!(
                            matches!(r, Err(Error::StaleLease { presented: p, current: c, .. }) if p == presented && c == generation),
                            "stale generation {presented} (current {generation}) was not refused: {r:?}"
                        );
                        prop_assert_eq!(after, before, "a stale append wrote nothing");
                    }
                }
            }
        }
        let _ = landed;
    }

    #[test]
    fn migrations_are_idempotent_and_replay_reproduces_the_projection(
        focus in prop::collection::vec(any::<bool>(), 0..12),
        acquisitions in 0u64..5,
    ) {
        let (dir, mut store, tenant, session) = open_with_session();
        for g in 1..=acquisitions {
            store
                .append(req(
                    tenant,
                    session,
                    vec![typed(
                        "SessionLeaseAcquired",
                        &SessionEvent::SessionLeaseAcquired { lease_generation: g, owner: format!("c{g}") },
                    )],
                ))
                .unwrap();
        }
        for f in &focus {
            store
                .append(req(
                    tenant,
                    session,
                    vec![typed(
                        "SessionFocusChanged",
                        &SessionEvent::SessionFocusChanged { task_id: if *f { Some(modbit_domain::TaskId::new()) } else { None } },
                    )],
                ))
                .unwrap();
        }
        let projected = store.session(&session).unwrap().unwrap();
        drop(store);
        // Reopen: the migration ledger is already at the current version and
        // opening again changes nothing; the projection reads the same.
        let mut reopened = EventStore::open(dir.path()).unwrap();
        prop_assert_eq!(reopened.session(&session).unwrap().unwrap(), projected.clone());
        // Rebuild from the log alone: identical.
        reopened.rebuild_projections().unwrap();
        let rebuilt = reopened.session(&session).unwrap().unwrap();
        prop_assert_eq!(rebuilt, projected);
    }
}
