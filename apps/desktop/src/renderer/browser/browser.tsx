import { useCallback, useEffect, useRef, useState } from "react";
import type { TaskCard } from "../model.ts";
import { browserState } from "../screens.ts";
import { StateLine } from "../shell/state-line.tsx";
import type { BrowserSessionSummary } from "../../preload/preload.ts";
import "../state/types.ts";

/** Browser panel (M7.1, docs/22): the task's live Chromium session — main's
 *  sandboxed view placed over the placeholder below; the URL, title and
 *  state version are what the host reports, the lease what the Core
 *  records. The page's content never reaches this renderer. */
export function Browser({ browsing, card, sessionId, onReopen, onClose }: { browsing: { taskId: string; browserSessionId: string | null; error: string | null }; card: TaskCard | null; sessionId: string | null; onReopen: () => void; onClose: () => void }) {
  const [host, setHost] = useState<{ attached: boolean; shown: boolean; url: string; title: string; stateVersion: number; leaseGeneration: number; controller: "AGENT" | "USER"; stopped?: string | null; humanInputAt?: number; reclaimed?: boolean; certPending?: { id: string; hostPort: string; error: string; issuer: string; subject: string; validStart: number; validExpiry: number; fingerprint: string } | null; refusals?: number } | null>(null);
  const [controlBusy, setControlBusy] = useState(false);
  // PX-073: the certificates the person trusted (per workspace), and the last note about the view (reset, cleared).
  const [trusts, setTrusts] = useState<{ workspace: string; hostPort: string; fingerprint: string; issuer: string }[]>([]);
  const [viewNote, setViewNote] = useState<string | null>(null);
  const loadTrusts = () => void window.modbit.browserCertificateTrusts().then(setTrusts).catch(() => {});
  const decideCert = async (decision: "trust" | "reject") => {
    const pending = host?.certPending;
    if (!bsid || !pending) return;
    await window.modbit.decideBrowserCertificate(bsid, pending.id, decision);
    loadTrusts();
  };
  const clearData = async () => {
    if (!bsid) return;
    const r = await window.modbit.clearBrowserData(bsid);
    setViewNote(`signed out of the sites this session visited: cookies, storage, service workers and caches cleared (${r.cleared.length} session)`);
  };
  const clearTrusts = async () => {
    const n = await window.modbit.clearBrowserCertificateTrusts();
    setViewNote(`${n} trusted certificate(s) forgotten`);
    loadTrusts();
  };
  const [stopped, setStopped] = useState<string | null>(null);
  // IMP-EV-0085: the emergency stop — the host's input halts at once, the
  // Core blocks every new effect; the reason is on the log.
  const emergencyStop = async () => {
    if (!sessionId || controlBusy) return;
    setControlBusy(true);
    try {
      await window.modbit.emergencyStop(sessionId, "stopped from the Browser panel");
      setStopped("stopped from the Browser panel");
    } finally {
      setControlBusy(false);
    }
  };
  // M7.6: take or return control — the lease moves, the same session stays.
  const setControl = async (controller: "AGENT" | "USER") => {
    if (!bsid || controlBusy) return;
    setControlBusy(true);
    try {
      await window.modbit.setBrowserControl(bsid, controller);
    } finally {
      setControlBusy(false);
    }
  };
  const [core, setCore] = useState<BrowserSessionSummary | null>(null);
  const [gone, setGone] = useState<string | null>(null);
  const [probe, setProbe] = useState<{ node_reachable: boolean; partition: string; sandboxed: boolean; context_isolated: boolean } | null>(null);
  const placeholder = useRef<HTMLDivElement>(null);
  const bsid = browsing.browserSessionId;
  const taskId = card?.taskId ?? null;
  // Place the view over the placeholder and keep it there through resizes.
  useEffect(() => {
    if (!bsid) return;
    const place = () => {
      const r = placeholder.current?.getBoundingClientRect();
      if (!r) return;
      void window.modbit.showBrowser(bsid, { x: r.left, y: r.top, width: r.width, height: r.height }).catch(() => {});
    };
    place();
    const ro = new ResizeObserver(place);
    if (placeholder.current) ro.observe(placeholder.current);
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    const refresh = () => {
      void window.modbit
        .describeBrowser(bsid)
        .then((h) => {
          setHost(h);
          // FIX-19: the host is the authority on its own fence: once a new session lease lifted it, the panel stops saying it stands.
          if (h && !h.stopped) setStopped(null);
        })
        .catch(() => setHost(null));
      if (taskId) void window.modbit.browserSession(bsid, taskId).then(setCore).catch(() => {});
    };
    refresh();
    loadTrusts();
    void window.modbit.probeBrowser(bsid).then((p) => setProbe(p ? { node_reachable: p.node_reachable, partition: p.partition, sandboxed: p.sandboxed, context_isolated: p.context_isolated } : null)).catch(() => {});
    const off = window.modbit.onBrowserState((raw) => {
      const s = raw as { browserSessionId: string; gone?: string; reset?: string | null; certDecision?: string };
      if (s.browserSessionId !== bsid) return;
      if (s.gone) setGone(s.gone);
      if (s.reset) setViewNote(s.reset);
      if (s.certDecision) loadTrusts();
      refresh();
    });
    return () => {
      off();
      ro.disconnect();
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
      void window.modbit.hideBrowser(bsid).catch(() => {});
    };
  }, [bsid, taskId]);
  const state = browserState({ opening: !bsid && !browsing.error, error: browsing.error, host, gone, controller: host?.controller ?? core?.controller ?? "AGENT" });
  return (
    <section className="review browser" data-testid="browser" aria-label="Browser" data-browser-session-id={bsid ?? ""}>
      <div className="review-head">
        <h2 style={{ margin: 0 }}>Browser</h2>
        <span className="meta" data-testid="browser-meta">
          {card ? `task ${card.taskId.slice(0, 8)} · ${card.goalText}` : ""}
          {core ? ` · ${core.controller.toLowerCase()} holds control (generation ${core.leaseGeneration})` : ""}
        </span>
        <button type="button" data-testid="browser-close" onClick={onClose} aria-keyshortcuts="Escape">
          Back to fleet
        </button>
      </div>
      <StateLine state={state} testid="browser-state" />
      <div className="actions" data-testid="browser-control" data-controller={host?.controller ?? "AGENT"} data-lease-generation={host?.leaseGeneration ?? 0}>
        {host?.controller === "USER" ? (
          <button type="button" data-testid="browser-return-control" onClick={() => void setControl("AGENT")} disabled={controlBusy || !bsid}>
            Return control to the agent
          </button>
        ) : (
          <button type="button" data-testid="browser-take-control" onClick={() => void setControl("USER")} disabled={controlBusy || !bsid}>
            Take control
          </button>
        )}
        <span className="meta">
          {host?.controller === "USER" ? "you hold control: the agent's input is blocked, it can still observe" : "the agent holds control: your typing into the page is yours to do, its actions are its own"} · lease generation {host?.leaseGeneration ?? 0}
        </span>
        <button type="button" data-testid="browser-emergency-stop" onClick={() => void emergencyStop()} disabled={controlBusy || !sessionId || !!(stopped ?? host?.stopped)}>
          Emergency stop
        </button>
        {(stopped ?? host?.stopped) && (
          <span className="meta" role="status" data-testid="browser-stopped">
            ⚠ emergency stop: {stopped ?? host?.stopped} — the host runs no agent input until a new session lease is taken (the Core blocks new effects for the session's life)
          </span>
        )}
      </div>
      {(gone || (host && !host.attached)) && (
        <div className="actions">
          <button type="button" data-testid="browser-reopen" onClick={onReopen}>
            Reopen session
          </button>
        </div>
      )}
      {host?.certPending && (
        <div className="actions" role="alertdialog" aria-label="Certificate not trusted" data-testid="browser-cert" data-host-port={host.certPending.hostPort}>
          <strong>{host.certPending.hostPort} presented a certificate this machine does not trust ({host.certPending.error}).</strong>
          <span className="meta" data-testid="browser-cert-details">
            issuer {host.certPending.issuer} · subject {host.certPending.subject} · valid {new Date(host.certPending.validStart * 1000).toISOString().slice(0, 10)} to {new Date(host.certPending.validExpiry * 1000).toISOString().slice(0, 10)} · fingerprint {host.certPending.fingerprint}
          </span>
          <span className="meta">the request waits up to 60 seconds for your decision; no decision is a rejection</span>
          <button type="button" data-testid="browser-cert-reject" onClick={() => void decideCert("reject")}>
            Reject
          </button>
          <button type="button" data-testid="browser-cert-trust" onClick={() => void decideCert("trust")}>
            Trust this certificate
          </button>
        </div>
      )}
      <div className="actions" data-testid="browser-data">
        <button type="button" data-testid="browser-clear-data" onClick={() => void clearData()} disabled={!bsid}>
          Sign out of sites (clear browser data)
        </button>
        <button type="button" data-testid="browser-cert-clear" onClick={() => void clearTrusts()} disabled={trusts.length === 0}>
          Forget trusted certificates ({trusts.length})
        </button>
        <ul data-testid="browser-cert-trusts" className="meta">
          {trusts.map((t) => (
            <li key={`${t.workspace}|${t.hostPort}`} data-host-port={t.hostPort}>
              {t.hostPort} · {t.issuer} · {t.fingerprint}
            </li>
          ))}
        </ul>
        {(viewNote || host?.reclaimed) && (
          <span className="meta" role="status" data-testid="browser-view-note">
            {host?.reclaimed ? "this view was reclaimed to free memory; it reloads when it is used again. " : ""}
            {viewNote ?? ""}
          </span>
        )}
      </div>
      <div className="meta" data-testid="browser-page">
        {"url: "}
        <span data-testid="browser-url">{host?.url ?? ""}</span>
        {" · title: "}
        <span data-testid="browser-title">{host?.title ?? ""}</span>
        {" · state "}
        <span data-testid="browser-version">{host?.stateVersion ?? 0}</span>
        {core?.url ? ` · recorded on the Core: ${core.url} (state ${core.stateVersion}, ${core.fingerprint.slice(0, 12)})` : ""}
        {" · page content is untrusted and never enters this window"}
      </div>
      {probe && (
        <div className="meta" data-testid="browser-isolation" data-node-reachable={probe.node_reachable ? "true" : "false"}>
          isolation: partition {probe.partition} · sandbox {probe.sandboxed ? "on" : "off"} · context isolation {probe.context_isolated ? "on" : "off"} · Node reachable from the page: {probe.node_reachable ? "YES" : "no"}
        </div>
      )}
      <div ref={placeholder} className="browser-view" data-testid="browser-view" aria-label="the live page (rendered by the host's sandboxed view)" />
    </section>
  );
}
