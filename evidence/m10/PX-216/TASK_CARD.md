# Task Card — PX-216 Browser pane site decisions: allow once and deny beside always allow, kept by the desktop's browser policy

## Identity

- Task ID: PX-216 (REQ-PX-216; owner desktop); qualification QUAL-PX-216 / PX-E2E-216; Decision Record DR-PX-2026-11-11-021 (MWS-19); AFW-A12, A13, A14 unchanged.
- Evidence tier: release-critical (the browser pane's policy store gains decision kinds that gate the agent's navigation).
- Prerequisites: PX-202 (COMPLETE), DOC-PX-020 (COMPLETE).
- Branch `work/px-216-site-decisions`, from `main` at c73af335 (PX-214 merged as #236).

## Existing-code audit (before any edit)

Traced from the agent's `browser.navigate` through the Core's tool to the desktop host and the page:

- The agent's call reaches `BrowserHost.request` -> `navigate` in `apps/desktop/src/main/browser.ts`. Before the load it checks the destination (`checkUrlLiteral`, then the proxy's pin on the address connected to) against the person's target policy (`browser-policy.ts`, `PolicyStore`: `allowTargets`, `allowOrigins`). PRODUCTION-WORKING.
- `allowOrigins` is a limit, not a question: empty means any http(s) page, non-empty refuses a page outside it. It never asks, and it is checked against the page the view is on, not against the destination of a navigation. No per-origin decision of any kind existed. NOT-FOUND.
- The only decisions the pane asks the person for today are certificate trust (`browser-cert.ts`: Reject or Trust, remembered per workspace, listed and cleared in the pane). That is the precedent for a hold that answers the agent with a typed pending error, a card in the pane, and a store in the desktop's own file; it is a different question (a certificate, not an origin) and was left alone.
- Nothing in the pane, the preload, main or Settings named Allow once, Always allow or Deny for a site. The card's phrase "beside always allow" refers to the approval deck's buttons for commands, which are the Core's; the browser has no such control. NOT-FOUND.
- Settings ("Browser permissions") listed the origin and target policy and a count of trusted certificates, with nothing to remove. IMPLEMENTED-PARTIAL.

## First missing link

No decision about an origin exists between the destination policy and the load. Everything else (the pending-hold pattern, the pane's cards, the desktop's own files, the typed IPC) exists and is owned by the desktop's browser host, so the work extends it. No protocol change, no Core change, no second approval system: the question is about where the person's own browser view may go, answered in the desktop (the same owner as certificate trust), and it never reaches the Core's policy kernel.

## Plan

1. `browser-sites.ts`: a store of the person's decisions (origin to allow or deny, in `browser-sites.json`), startup allows from the environment, and the origin canon.
2. The host: `siteGate` between the destination policy and the load; an Allow once is an in-memory grant for the session, used up by the navigation; the ask is raised once per origin and answered with `SITE_PENDING`; a Deny is answered with `SITE_DENIED` and never asks again; `siteDecide` accepts only the origin the card showed.
3. Main and preload: `browser:siteDecide`, `browser:sites`, `browser:siteRemove` (validated; the two deciding channels refused to pane windows).
4. The pane: one shared card (Deny, Allow once, Always allow) in the Browser tab and in the full browser view; Settings lists the decisions with Remove.
5. Proof: unit tests of the store; a real Electron app with a real Core, a scripted model, and two local origins, over three launches of one data directory; screenshots; retained mutation checks.

## What was built

- `apps/desktop/src/main/browser-sites.ts` (new): `SiteStore` over `browser-sites.json` (read tolerantly: a file that is not the person's list decides nothing; an entry that is not an origin as a URL prints it is dropped), `siteOrigin`, `canonicalOrigin`, startup allows (`MODBIT_BROWSER_ALLOW_SITES`, comma-separated; `scheme://host:*` names every port of a host). A person's Deny wins over a startup allow; startup entries cannot be removed from Settings. A decision belongs to one origin: another port, scheme or subdomain is undecided.
- `apps/desktop/src/main/browser.ts`: `siteGate` runs after the destination checks and before the load. Decided Allow: through. Decided Deny: `SITE_DENIED`, with its reason ("the person denied <origin>; ... asking again changes nothing ... can remove the entry in Settings"), no ask. An Allow once for the origin: used up and through. An origin the person's own `allowOrigins` policy names: through (it is theirs already). Otherwise the ask is raised once per origin (`siteAsk` on the session, `browser:state` notified, `site-asked` on the host log) and the agent gets `SITE_PENDING` and tries again after the person answers. `siteDecide(session, ask id, origin, decision)` refuses any ask id or origin other than the one the card showed (nothing recorded). An Allow once is held in memory for 10 minutes and never written down. `siteRemove` forgets a decision and withdraws any once-grant for it.
- `apps/desktop/src/main/main.ts`, `src/preload/preload.ts`: `decideBrowserSite`, `browserSites`, `removeBrowserSite` over `browser:siteDecide`, `browser:sites`, `browser:siteRemove`; `siteAsk` on the session's description. `hardening-guard.test.ts`'s preload pin is updated with a note; `pane-windows.test.ts` lists the two deciding channels as refused to a pane window; `workers.spec.ts`'s pinned bridge list gains the three names.
- Renderer: `browser/site-ask.tsx` (one card, titled "The agent wants to open <origin>.", with Deny, Allow once and Always allow) shown in the Browser tab (`apps/browser-app.tsx`) and the full view (`browser/browser.tsx`); Settings, Browser permissions, gains "Sites you decided for the agent" with one row per decision ("Always allowed" or "Denied") and Remove.
- Existing browser specs: each already names its local test site through `MODBIT_BROWSER_ALLOW_TARGETS`; each now also names it as a standing allow through `MODBIT_BROWSER_ALLOW_SITES` (`http://127.0.0.1:*,http://localhost:*`), so none of them is asked. Only the PX-216 spec launches without it. The environment value is the person's startup allow, not a switch: nothing disables the ask.

## Verification

macOS arm64, rustc 1.97.1, node 26, Electron 44, Playwright 1.63. The model is the repository's scripted stand-in; Electron main, its `WebContentsView`, the preload, the renderer and the Core (its socket, store and tool runtime) are real, and the pages are served over real HTTP on two ports of 127.0.0.1.

- `pnpm --filter @modbit/desktop test`: 478 passed, 0 failed (six new tests of the store: origin canon, one decision per origin, replace/restart/remove, tolerant file, startup allows, only http(s)).
- `px-216-site-decisions.spec.ts` (QUAL-PX-216), three launches of one data directory, 27 s:
  - first run: an undecided origin is refused `SITE_PENDING`, the pane's card names it and shows the three answers; Allow once lets that navigation load and writes nothing to the file; the next navigation to it asks again; an answer for another origin and an answer with another ask id are refused and record nothing; Always allow is written to the file and the next navigation loads with no ask; a target the destination policy forbids (127.0.0.2) is `TARGET_NOT_ALLOWED` with no ask on the host's log; the second origin asks and its card names it; Deny is written, the retry is `SITE_DENIED` with its reason and does not ask again;
  - after a restart: the Always allow loads without an ask and the Deny is refused without an ask; Settings lists both ("Always allowed", "Denied"); Remove on the allowed one removes it from the file; the agent's next navigation to it asks;
  - after a second restart: the Deny still refuses, and the removed origin asks.
- Mutation checks, `mutation-check-2026-11-20.txt`: seven mutations, each caught by the spec: Allow once written down as Always allow; a denied origin not refused; a decision for another origin accepted; an Allow once not used up; the ask raised before the destination policy; Settings removal forgetting nothing; decisions not kept across a restart. Each was reverted and the sources touched before the next build.
- Neighbouring desktop specs on the same tree (`neighbours-e2e.log`): browser, browser-hardening, browser-semantic, browser-observer, browser-feedback, apps-browser, workers (the pinned bridge list), px-202 pane dock: 27 passed and 1 failed. The failure was the certificate spec in `browser-hardening.spec.ts`, which opens an `https://127.0.0.1` site that my startup allow did not name (so it was asked and answered `SITE_PENDING`); the allow now names the https forms too, and `browser-hardening.spec.ts` plus the PX-216 spec were re-run: 7 of 7 passed. Not re-run in full after that one-line value change.
- `pnpm typecheck` exits 0; `node --test` on `hardening-guard`, `pane-windows` and the new store tests pass.
- Real screenshots of the running app, light and dark, in `after/`: the card in the Browser pane (`browser-site-ask-*`), Settings listing the two decisions (`settings-sites-*`) and after Remove (`settings-sites-removed-*`). I looked at them: the first draft of the card put its text and buttons in one row and cut "Always allow" off at the pane's edge, so the card was given its own stacked layout and the screenshots re-taken; the final ones show the card inside the pane with the origin, one sentence, and the three buttons wrapped under it.

## Limitations

- The ask covers the agent's `browser.navigate` tool. A link the agent clicks that leads to another origin, and a redirect, are not asked about: they remain under the destination policy only. A cross-origin hop inside a page is a separate decision (it needs the host to know the click was the agent's) and is not part of this row's acceptance text.
- The ask is answered with `SITE_PENDING` and the agent tries again, as a certificate hold is; the task is not parked. Three turns in a row with no progress park a task as Needs attention (the Core's own guard), which is the right outcome for an agent that keeps asking while no one answers.
- Decisions are the person's across all of their tasks and workspaces (one file in the desktop's data directory); certificate trust is per workspace. A decision per workspace is a different row.
- An Allow once is lost when the app restarts (it is only in memory), and a grant the agent does not come back for expires after 10 minutes.
- No protocol or Core change, so the Core suite was not re-run for this row; the Rust and protocol gates are unchanged and CI runs them.
- macOS only so far; Linux and Windows are CI's.
- CI run 44703829557 on the PR's final head 02358388: all 19 checks green on Linux, macOS and Windows (`ci-run-44703829557.json`); merged as fe049bae (#237). The owner reviewed the screenshots and asked for the seal on 2026-11-20.

## Status

COMPLETE (sealed 2026-11-20).
