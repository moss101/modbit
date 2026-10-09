//! PX-063: `projects` and `project_members` are projections of the `Project`
//! aggregate's events. They are kept in the transaction that appends the
//! events, rebuild from the log to the same rows, hold a task in at most one
//! project at the storage level, and refuse an event that names a project the
//! log never created.

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::project::ProjectEvent;
use modbit_domain::{ProjectId, SessionId, TaskId, TenantId};
use modbit_event_store::{AppendRequest, EventStore, NewEvent};

fn append(
    store: &mut EventStore,
    session: SessionId,
    e: &ProjectEvent,
    id: ProjectId,
) -> Result<(), String> {
    let mut ev = NewEvent::new(
        e.event_type(),
        serde_json::to_value(e).unwrap(),
        Actor::Core("test".into()),
    );
    ev.occurred_at = Some(modbit_domain::Timestamp::now());
    store
        .append(AppendRequest {
            tenant_id: TenantId::new(),
            session_id: session,
            task_id: None,
            run_id: None,
            turn_id: None,
            step_id: None,
            aggregate_type: AggregateType::Project,
            aggregate_id: *id.as_bytes(),
            expected_sequence: None,
            events: vec![ev],
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn created(id: ProjectId, name: &str, root: &str) -> ProjectEvent {
    ProjectEvent::ProjectCreated {
        project_id: id,
        name: name.into(),
        color: "accent".into(),
        icon: "folder".into(),
        workspace_root: root.into(),
    }
}

/// The projection as a comparable value (times and offsets included: a
/// rebuild must reproduce them too).
fn snapshot(store: &EventStore) -> Vec<String> {
    let mut out = Vec::new();
    for p in store.projects().unwrap() {
        let members: Vec<String> = store
            .project_members(&p.project_id)
            .unwrap()
            .iter()
            .map(|m| format!("{}@{}", m.task_id, m.added_offset))
            .collect();
        out.push(format!(
            "{} {} {} {} {} archived={} c={} u={} co={} lo={} members={members:?}",
            p.project_id,
            p.name,
            p.color,
            p.icon,
            p.workspace_root,
            p.archived,
            p.created_at_ms,
            p.updated_at_ms,
            p.created_offset,
            p.last_offset
        ));
    }
    out.sort();
    out
}

#[test]
fn project_events_project_into_the_tables_and_a_rebuild_replays_them_to_the_same_rows() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = EventStore::open(dir.path()).unwrap();
    let (s1, s2) = (SessionId::new(), SessionId::new());
    let (a, b) = (ProjectId::new(), ProjectId::new());
    let (t1, t2, t3) = (TaskId::new(), TaskId::new(), TaskId::new());
    // Projects are cross-session: the events land on whichever session acted.
    append(&mut store, s1, &created(a, "Alpha", "/w/a"), a).unwrap();
    append(&mut store, s2, &created(b, "Beta", "/w/a"), b).unwrap();
    for t in [t1, t2] {
        append(
            &mut store,
            s1,
            &ProjectEvent::ProjectMemberAdded {
                project_id: a,
                task_id: t,
            },
            a,
        )
        .unwrap();
    }
    append(
        &mut store,
        s2,
        &ProjectEvent::ProjectMemberAdded {
            project_id: b,
            task_id: t3,
        },
        b,
    )
    .unwrap();
    append(
        &mut store,
        s2,
        &ProjectEvent::ProjectMemberRemoved {
            project_id: a,
            task_id: t2,
        },
        a,
    )
    .unwrap();
    append(
        &mut store,
        s1,
        &ProjectEvent::ProjectRenamed {
            project_id: a,
            name: "Alpha 2".into(),
            color: "warn".into(),
            icon: "rocket".into(),
        },
        a,
    )
    .unwrap();
    append(
        &mut store,
        s1,
        &ProjectEvent::ProjectArchived { project_id: b },
        b,
    )
    .unwrap();

    let before = snapshot(&store);
    assert_eq!(store.project_of_task(&t1).unwrap().unwrap().project_id, a);
    assert!(store.project_of_task(&t2).unwrap().is_none(), "removed");
    assert_eq!(store.project_of_task(&t3).unwrap().unwrap().project_id, b);
    assert!(store.project(&b).unwrap().unwrap().archived);
    assert_eq!(store.project(&a).unwrap().unwrap().name, "Alpha 2");

    // A rebuild replays to exactly the same rows.
    store.rebuild_projections().unwrap();
    assert_eq!(snapshot(&store), before);

    // The rows survive a reopen (the store reads them from its own tables).
    drop(store);
    let reopened = EventStore::open(dir.path()).unwrap();
    assert_eq!(snapshot(&reopened), before, "a reopen reads the same state");
}

#[test]
fn a_task_cannot_be_in_two_projects_even_if_a_check_were_missed() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = EventStore::open(dir.path()).unwrap();
    let s = SessionId::new();
    let (a, b) = (ProjectId::new(), ProjectId::new());
    let t = TaskId::new();
    append(&mut store, s, &created(a, "A", "/w"), a).unwrap();
    append(&mut store, s, &created(b, "B", "/w"), b).unwrap();
    append(
        &mut store,
        s,
        &ProjectEvent::ProjectMemberAdded {
            project_id: a,
            task_id: t,
        },
        a,
    )
    .unwrap();
    // A second add of the same task to the same project changes nothing.
    append(
        &mut store,
        s,
        &ProjectEvent::ProjectMemberAdded {
            project_id: a,
            task_id: t,
        },
        a,
    )
    .unwrap();
    assert_eq!(store.project_members(&a).unwrap().len(), 1);
    // The other project cannot take it: the fold refuses and nothing is written.
    let err = append(
        &mut store,
        s,
        &ProjectEvent::ProjectMemberAdded {
            project_id: b,
            task_id: t,
        },
        b,
    )
    .unwrap_err();
    assert!(err.contains("another project holds"), "{err}");
    assert!(store.project_members(&b).unwrap().is_empty());
    assert_eq!(store.project_of_task(&t).unwrap().unwrap().project_id, a);
}

#[test]
fn an_event_for_a_project_the_log_never_created_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = EventStore::open(dir.path()).unwrap();
    let ghost = ProjectId::new();
    let err = append(
        &mut store,
        SessionId::new(),
        &ProjectEvent::ProjectArchived { project_id: ghost },
        ghost,
    )
    .unwrap_err();
    assert!(err.contains("does not exist"), "{err}");
    assert!(store.projects().unwrap().is_empty());
}

#[test]
fn a_membership_map_that_disagrees_with_the_log_is_found_at_recovery_and_rebuilt_from_the_log() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (ProjectId::new(), ProjectId::new());
    let (member, stranger) = (TaskId::new(), TaskId::new());
    let want = {
        let mut store = EventStore::open(dir.path()).unwrap();
        let s = SessionId::new();
        append(&mut store, s, &created(a, "A", "/w"), a).unwrap();
        append(&mut store, s, &created(b, "B", "/w"), b).unwrap();
        append(
            &mut store,
            s,
            &ProjectEvent::ProjectMemberAdded {
                project_id: a,
                task_id: member,
            },
            a,
        )
        .unwrap();
        assert!(store.project_tables_agree_with_log().unwrap());
        // The check changes nothing.
        let first = snapshot(&store);
        assert!(store.project_tables_agree_with_log().unwrap());
        assert_eq!(snapshot(&store), first);
        // An untouched store recovers with no complaint.
        let out = store.recover_on_start().unwrap();
        assert!(!out.projections_rebuilt, "{:?}", out.notes);
        first
    };
    // Something outside the log changes the map and leaves the cursor alone:
    // a task the log never added, and a moved member.
    {
        let conn = rusqlite::Connection::open(dir.path().join("core.db")).unwrap();
        conn.execute(
            "INSERT INTO project_members (task_id, project_id, added_at_ms, added_offset) VALUES (?1, ?2, 1, 1)",
            rusqlite::params![stranger.as_bytes().as_slice(), b.as_bytes().as_slice()],
        )
        .unwrap();
        conn.execute(
            "UPDATE project_members SET project_id = ?1 WHERE task_id = ?2",
            rusqlite::params![b.as_bytes().as_slice(), member.as_bytes().as_slice()],
        )
        .unwrap();
    }
    let mut store = EventStore::open(dir.path()).unwrap();
    assert!(
        !store.project_tables_agree_with_log().unwrap(),
        "the invariant fails on a map the log never recorded"
    );
    let out = store.recover_on_start().unwrap();
    assert!(out.projections_rebuilt, "{:?}", out.notes);
    assert!(
        out.notes.iter().any(|n| n.contains("disagreed")),
        "the disagreement is reported: {:?}",
        out.notes
    );
    assert_eq!(
        snapshot(&store),
        want,
        "what the log says, and nothing else"
    );
    assert!(store.project_of_task(&stranger).unwrap().is_none());
    assert!(store.project_tables_agree_with_log().unwrap());
}
