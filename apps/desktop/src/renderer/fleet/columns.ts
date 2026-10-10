import type { FleetColumn } from "../model.ts";

export const COLUMNS: { key: FleetColumn; title: string }[] = [
  { key: "needsAttention", title: "Needs Attention" },
  { key: "readyForReview", title: "Ready for Review" },
  { key: "running", title: "Running" },
  { key: "waiting", title: "Waiting" },
  { key: "completed", title: "Completed" },
  { key: "failed", title: "Failed" },
];
