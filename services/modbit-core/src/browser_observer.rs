//! The Core's half of the page observer (PX-122): the host's mutation
//! observer reports that the page changed on its own (`BrowserHostNotice`);
//! the Core reads the page again, compiles it, diffs it against the page it
//! held and journals the semantic delta (`BrowserPageChanged`) — with no
//! model call and nothing acted on. A page that mutates thousands of nodes
//! a second reaches the Core as a handful of notices (the host folds them)
//! and is read at most a few times a second (the Core folds those). The
//! known-state map the compile feeds is what a restart restores (the page is
//! persisted as an object the log points at), so the same element keeps its
//! reference and a delta against a fingerprint the model holds still works.

use std::sync::Arc;
use std::time::{Duration, Instant};

use modbit_browser::compiler::{PageEntities, compile_with_frames, diff, state_fingerprint};
use modbit_browser::{
    BrowserPort, BrowserSessionId, HostRequest, HostResponse, NoticeStats, PageNotice,
};
use modbit_domain::event::AggregateType;
use modbit_domain::task::TaskEvent;

use crate::server::Core;

/// Entities a compile keeps (the tools' bound).
const MAX_ENTITIES: usize = 200;
/// Nodes the host is asked for.
const MAX_NODES: u32 = 400;
/// The first read waits this long after a notice: the host has already
/// folded a burst of mutations, and a page usually finishes a render in it.
const FIRST_READ_AFTER: Duration = Duration::from_millis(120);
/// Reads are at least this far apart while notices keep coming.
const MIN_GAP: Duration = Duration::from_millis(400);

/// The page without what only one version of it knows (node ids, boxes):
/// what a restart restores.
fn slim(page: &PageEntities) -> PageEntities {
    let mut p = page.clone();
    for e in &mut p.entities {
        e.backend_dom_node_id = None;
        e.bounds = None;
    }
    for r in &mut p.visual_regions {
        r.backend_dom_node_id = None;
        r.bounds = None;
    }
    p
}

/// Put the page in the object store; the reference is what the log carries.
pub(crate) fn persist_page(
    store: &modbit_event_store::EventStore,
    page: &PageEntities,
) -> Option<String> {
    let bytes = serde_json::to_vec(&slim(page)).ok()?;
    store.objects().put(&bytes).ok()
}

/// The page a reference names.
pub(crate) fn load_page(
    store: &modbit_event_store::EventStore,
    reference: &str,
) -> Option<PageEntities> {
    let bytes = store.objects().get(reference).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// A host's notice: counted, and a read scheduled unless one is running.
pub(crate) async fn accept_notice(
    core: &Arc<Core>,
    bsid: BrowserSessionId,
    notice: PageNotice,
) -> Option<NoticeStats> {
    let (stats, spawn) = core.browser.note_notice(bsid, &notice).await?;
    if spawn {
        let core = Arc::clone(core);
        tokio::spawn(async move { observe(core, bsid).await });
    }
    Some(stats)
}

/// Read, compile, diff and journal until no notice is waiting.
async fn observe(core: Arc<Core>, bsid: BrowserSessionId) {
    let port: Arc<dyn BrowserPort> = crate::browser::port(&core.browser);
    let mut first = true;
    let mut last_read: Option<Instant> = None;
    loop {
        let wait = if first {
            FIRST_READ_AFTER
        } else {
            last_read
                .map(|t| MIN_GAP.saturating_sub(t.elapsed()))
                .unwrap_or_default()
        };
        first = false;
        tokio::time::sleep(wait).await;
        let Some((seq, first_notice)) = core.browser.begin_read(bsid).await else {
            return;
        };
        last_read = Some(Instant::now());
        let coalesced = core.browser.take_coalesced(bsid).await;
        let page = match port
            .request(
                bsid,
                HostRequest::Snapshot {
                    max_nodes: MAX_NODES,
                    observer: true,
                },
            )
            .await
        {
            Ok(HostResponse::Snapshot {
                state,
                nodes,
                truncated,
                frames,
                ..
            }) if !nodes.is_empty() => {
                compile_with_frames(&state, &nodes, &frames, truncated, MAX_ENTITIES)
            }
            // A page that cannot be read now (a dialog, a restart, no host)
            // is read again at the next notice, not retried in a loop.
            _ => {
                if core.browser.finish_read(bsid, seq, false).await {
                    continue;
                }
                return;
            }
        };
        let previous = port.last_page(bsid).await;
        port.remember_page(bsid, page.clone()).await;
        let to = state_fingerprint(&page);
        let changed = previous.as_ref().is_none_or(|p| state_fingerprint(p) != to);
        if changed && let Some(rec) = core.browser.get(bsid).await {
            let delta = previous.as_ref().map(|p| diff(p, &page));
            let from = delta
                .as_ref()
                .map(|d| d.from_fingerprint.clone())
                .unwrap_or_default();
            let mut store = core.store.lock().await;
            let page_ref = persist_page(&store, &page).unwrap_or_default();
            let (added, removed, changed_n, ta, tr) = match &delta {
                Some(d) => (
                    d.added.len() as u64,
                    d.removed.len() as u64,
                    d.changed.len() as u64,
                    d.text_added.len() as u64,
                    d.text_removed.len() as u64,
                ),
                None => (page.entities.len() as u64, 0, 0, page.text.len() as u64, 0),
            };
            let ev = crate::runtime::typed(
                "BrowserPageChanged",
                &TaskEvent::BrowserPageChanged {
                    browser_session_id: bsid.to_string(),
                    change_seq: seq,
                    from_fingerprint: from,
                    to_fingerprint: to.clone(),
                    state_version: page.state.state_version,
                    added,
                    removed,
                    changed: changed_n,
                    text_added: ta,
                    text_removed: tr,
                    coalesced: u64::from(coalesced),
                    latency_ms: first_notice
                        .map(|t| t.elapsed().as_millis() as u64)
                        .unwrap_or(0),
                    url: page.state.url.clone(),
                    page_ref: page_ref.clone(),
                },
                modbit_domain::event::Actor::Core("browser-observer".into()),
            );
            let _ = crate::runtime::append_batch(
                &mut store,
                &core,
                crate::runtime::Lineage::task(core.tenant_id, rec.session_id, rec.task_id),
                vec![(AggregateType::Task, *rec.task_id.as_bytes(), vec![ev])],
            );
            drop(store);
            core.browser.note_persisted(bsid, page_ref).await;
        }
        if !core.browser.finish_read(bsid, seq, true).await {
            return;
        }
    }
}
