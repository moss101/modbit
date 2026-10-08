import { useEffect, useState } from "react";
import { Badge, Button } from "@modbit/ui";
import type { ContextInspectorSummary, ReceiptView, ReviewBundleView } from "../../preload/preload.ts";
import { categoryLabel, percent, windowLine } from "./evidence-model.ts";

interface Loaded {
  bundle: ReviewBundleView | null;
  bundleError: string | null;
  receipts: { chainValid: boolean; detail: string; receipts: ReceiptView[] } | null;
  receiptsError: string | null;
  inspector: ContextInspectorSummary | null;
  inspectorNote: string | null;
}

/**
 * The Evidence app (REQ-PX-048): everything the Core holds as proof for the
 * task, read-only. Receipts are the effect ledger's (with whether the chain
 * verifies), checks and CI results are the review bundle's (CI results are
 * external evidence with their provenance, never a verification result), and
 * the context numbers are the Core's own accounting of the last request by
 * category (REQ-PX-059) beside the inspector's pack.
 */
export function EvidenceApp({ taskId, refreshKey }: { taskId: string; refreshKey: string }) {
  const [data, setData] = useState<Loaded | null>(null);
  const [tick, setTick] = useState(0);
  useEffect(() => {
    let live = true;
    void Promise.allSettled([window.modbit.reviewBundle(taskId), window.modbit.effectReceipts(taskId), window.modbit.contextInspector(taskId)]).then(([b, r, i]) => {
      if (!live) return;
      setData({
        bundle: b.status === "fulfilled" ? b.value : null,
        bundleError: b.status === "rejected" ? (b.reason as Error).message : null,
        receipts: r.status === "fulfilled" ? r.value : null,
        receiptsError: r.status === "rejected" ? (r.reason as Error).message : null,
        inspector: i.status === "fulfilled" ? i.value : null,
        inspectorNote: i.status === "rejected" ? (i.reason as Error).message : null,
      });
    });
    return () => {
      live = false;
    };
  }, [taskId, refreshKey, tick]);

  if (!data) {
    return (
      <p className="meta" role="status" data-testid="evidence-loading">
        Reading the evidence…
      </p>
    );
  }
  const { bundle, receipts, inspector } = data;
  const acc = inspector?.accounting ?? null;
  return (
    <div className="evidence-app" data-testid="app-evidence">
      <div className="changes-head">
        <span className="meta">Read-only: what the Core recorded for this task.</span>
        <Button size="sm" onClick={() => setTick((t) => t + 1)} data-testid="evidence-refresh">
          Refresh
        </Button>
      </div>

      <section aria-labelledby="ev-receipts" data-testid="evidence-receipts">
        <h3 id="ev-receipts">Effect receipts</h3>
        {data.receiptsError ? (
          <p className="meta" role="status">{`Receipts could not be read: ${data.receiptsError}`}</p>
        ) : receipts && receipts.receipts.length > 0 ? (
          <>
            <p className="meta" data-testid="evidence-chain" data-valid={receipts.chainValid ? "true" : "false"}>
              {receipts.receipts.length} {receipts.receipts.length === 1 ? "receipt" : "receipts"} · the receipt chain {receipts.chainValid ? "verifies" : `does not verify: ${receipts.detail}`}
            </p>
            <ul className="evidence-list">
              {receipts.receipts.map((r) => (
                <li key={r.receiptHash} data-testid="evidence-receipt" data-status={r.status} data-receipt-hash={r.receiptHash}>
                  <Badge tone={r.status === "AUTHORIZED" ? "info" : "neutral"}>{r.status}</Badge> {r.executionTarget || "effect"} · {r.policyDecision || "decided"} · {r.reversibility ? r.reversibility.toLowerCase() : "reversibility not stated"}
                  <span className="meta"> · {r.receiptHash.slice(0, 12)}</span>
                </li>
              ))}
            </ul>
          </>
        ) : (
          <p className="meta empty" data-testid="evidence-no-receipts">
            No protected effect has run for this task yet, so there is no receipt.
          </p>
        )}
      </section>

      <section aria-labelledby="ev-checks" data-testid="evidence-checks">
        <h3 id="ev-checks">Checks and CI</h3>
        {data.bundleError ? (
          <p className="meta" role="status">{`The checks could not be read: ${data.bundleError}`}</p>
        ) : bundle ? (
          <>
            {bundle.verificationRuns.length === 0 ? (
              <p className="meta empty">No verification run has been recorded yet.</p>
            ) : (
              <ul className="evidence-list">
                {bundle.verificationRuns.map((v) => (
                  <li key={v.id} data-testid="evidence-run">
                    {v.stage}: <strong>{v.status}</strong>
                    <span className="meta"> · {v.checks.map((c) => `${c.id} ${c.status}`).join(", ") || "no checks"}</span>
                  </li>
                ))}
              </ul>
            )}
            {bundle.ciEvidence.length > 0 && (
              <ul className="evidence-list" aria-label="CI results">
                {bundle.ciEvidence.map((c) => (
                  <li key={`${c.runId}-${c.name}`} data-testid="evidence-ci">
                    {c.name}: {c.conclusion || c.status} <span className="meta">· external evidence ({c.provenance}) for {c.commit.slice(0, 12)}</span>
                  </li>
                ))}
              </ul>
            )}
            {bundle.ciRejected.length > 0 && (
              <p className="meta">{bundle.ciRejected.length} CI result(s) were refused because they belong to another commit.</p>
            )}
            <p className="meta">{bundle.receipts} receipt(s) and {bundle.evidenceLinks.length} evidence link(s) are attached to the change set.</p>
          </>
        ) : null}
      </section>

      <section aria-labelledby="ev-context" data-testid="evidence-context">
        <h3 id="ev-context">Context</h3>
        {data.inspectorNote ? (
          <p className="meta" role="status">{`The context could not be read: ${data.inspectorNote}`}</p>
        ) : acc ? (
          <>
            <p className="meta" data-testid="evidence-window">
              {windowLine(acc)}
            </p>
            <table className="evidence-table" data-testid="evidence-categories">
              <caption className="mb-sr-only">The last request, by category</caption>
              <thead>
                <tr>
                  <th scope="col">Category</th>
                  <th scope="col">Tokens</th>
                  <th scope="col">Share</th>
                </tr>
              </thead>
              <tbody>
                {acc.categories.map((c) => (
                  <tr key={c.category} data-testid="evidence-category" data-category={c.category}>
                    <th scope="row">{categoryLabel(c.category)}</th>
                    <td>{c.tokens}</td>
                    <td>{percent(c.shareBp)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </>
        ) : (
          <p className="meta empty" data-testid="evidence-no-accounting">
            No request has been compiled for this task yet, so there is no breakdown by category.
          </p>
        )}
        {inspector && (
          <p className="meta" data-testid="evidence-pack">
            Context pack {inspector.packId.slice(0, 12) || "none"}: {inspector.tokenUsed} of {inspector.tokenBudget} budget tokens, {inspector.entries.length} entries, {inspector.injectedRefs.length} injected, {inspector.rejectedRefs.length} rejected{inspector.complete ? "" : ", incomplete"}; {inspector.omittedCount} omitted.
          </p>
        )}
      </section>
    </div>
  );
}
