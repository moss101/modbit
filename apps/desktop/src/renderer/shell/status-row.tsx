import type { ReactNode } from "react";
import { Badge, StatusDot, type Status } from "@modbit/ui";

export interface StatusRowProps {
  coreLabel: string;
  coreStatus: Status;
  platform: { state: string; statement: string } | null;
  location: string;
  /** Room for the context ring and branch pill that PX-054 and PX-060 own; nothing is drawn for them until they exist. */
  extra?: ReactNode;
}

/** The status row (AFW-A01, AFW-D12): execution location, the Core's state, the platform statement. */
export function StatusRow({ coreLabel, coreStatus, platform, location, extra }: StatusRowProps) {
  return (
    <footer className="statusrow" data-testid="status-row">
      <Badge tone="neutral">{location}</Badge>
      <span className="status" data-testid="core-status" aria-live="polite">
        <StatusDot status={coreStatus} label={coreLabel} showLabel />
      </span>
      {platform && (
        <span className="meta" data-testid="platform-state" data-state={platform.state} title={platform.statement}>
          {platform.statement}
        </span>
      )}
      {extra}
    </footer>
  );
}
