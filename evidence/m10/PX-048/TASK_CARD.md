# Task Card — PX-048 Typed apps panel (Terminal scroll-back and per-task state)

- Status set: REAL_TESTING (not COMPLETE). Merged in #132 (374abcb8).
- Tier: release-critical (touches the preload bridge, a new main-process IPC call).

## Built
- Older terminal output loads by OutputRef range through `terminal:history` (hash read from the Core's own listing, 256 KiB cap), 64 KiB ranges in cursor order.

## Proven (real Electron + real Core, PR run 40086967864)
- terminal-history.spec.ts: live stream of a paced build; scroll-back reads OutputRef ranges; pane + view equals the Core's live stream from the oldest retained cursor and the sealed object by cursor (all three OSes).
- apps-panel-state.spec.ts: per-task tab state across selection and restart. apps-browser.spec.ts: lease badge generations and takeover. Existing: apps-panel, terminal-renderer-restart, changes-app.

## Not proven (why this stays REAL_TESTING)
1. A user terminal accepting typing only with policy: the Core has only agent- and host-owned terminals; the panel cannot list a user terminal. Needs a spec/owner decision or a user-terminal feature.
2. QUAL-PX-043 wording says agent terminals are read-only to clients; the shipped lease model (PX-099) lets the person take the input lease. Tests cover the lease behaviour.
3. Windows: exact line order is not asserted; the Windows pseudo-console dropped line 18752 from the Core's own stream once (run 40072387738). Open question whether this is ConPTY or the execd Windows path.
4. History for a still-running terminal is available only after the process ends (no sealed object before exit).
5. The preload pin was re-pinned for the read-only terminalHistory call; the pin change itself has had no human review.
6. Prerequisite PX-043 is COMPLETE, but its px_043 flake (extra newline in the PTY log) was fixed by #133 and remains unproven until CI stays green.
