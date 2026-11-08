import { useEffect, useState } from "react";
import type { UpdateViewDto } from "../../preload/preload.ts";

/**
 * M10.2 (docs/70 "Desktop update"): what the updater is doing, said plainly.
 * It only offers an install when a verified update is staged, and a refused
 * or failed one names the reason. Nothing here decides anything: the main
 * process verifies, decides and installs.
 */
export function UpdateIndicator() {
  const [v, setV] = useState<UpdateViewDto | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let alive = true;
    const poll = () => window.modbit.updateView().then((x) => alive && setV(x)).catch(() => {});
    poll();
    const t = setInterval(poll, 2000);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, []);
  if (!v) return null;
  const act = (fn: () => Promise<UpdateViewDto>) => {
    setBusy(true);
    fn().then(setV).catch(() => {}).finally(() => setBusy(false));
  };
  const s = v.status;
  const text = !v.enabled
    ? "Updates off"
    : v.lastApply && !v.lastApply.ok
      ? `Update to ${v.lastApply.version} failed: ${v.lastApply.error ?? "unknown"}`
      : s.state === "staged"
        ? `Update ${s.version} ready`
        : s.state === "refused"
          ? `Update refused: ${s.code}`
          : s.state === "error"
            ? `Update check failed: ${s.code}`
            : s.state === "held"
              ? `Update ${s.version} is staged for a later rollout`
              : `Modbit ${v.version}${s.state === "up-to-date" ? ", up to date" : ""}`;
  return (
    <span className="meta" data-testid="update-state" data-state={v.enabled ? s.state : "disabled"} data-version={v.version} title={v.disabledReason ?? s.detail ?? ""}>
      {text}{" "}
      {v.enabled && (
        <button type="button" className="small" data-testid="update-check" disabled={busy} onClick={() => act(() => window.modbit.updateCheck())}>
          Check for updates
        </button>
      )}{" "}
      {v.enabled && s.state === "staged" && (
        <button type="button" className="small" data-testid="update-install" disabled={busy} onClick={() => act(() => window.modbit.updateInstall())}>
          Restart to update
        </button>
      )}{" "}
      {v.enabled && v.canRollback && (
        <button type="button" className="small" data-testid="update-rollback" disabled={busy} onClick={() => act(() => window.modbit.updateRollback())}>
          Roll back
        </button>
      )}
    </span>
  );
}
