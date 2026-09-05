# Browser and Computer-Use Architecture

> **Authority date:** 2026-09-03  
> **Product:** Modbit — clean-slate implementation dossier  
> **Status vocabulary:** **LOCKED**, **PROVISIONAL**, **EXPERIMENT**, **DEFERRED**, **REJECTED**  
> **Source-of-truth rule:** latest explicit Modbit decision > locked decisions > current dossier > older project documents. Older Code-OSS/Modbit Lite material is historical only when it conflicts with this dossier.


## Locked direction

Modbit does **not** build a new browser engine and does not operate primarily from screenshots. It builds an agent-native semantic runtime over Chromium while exposing the exact same live session to the user.

## Local browser

Electron main creates a sandboxed `WebContentsView` in a dedicated session partition with Node disabled and strict context isolation. A Browser Bridge controls that same `webContents` through Chrome DevTools Protocol. The renderer only hosts/choreographs the view; untrusted page content never gets Modbit privileged APIs.

## Cloud browser

Chromium runs inside the isolated MicroVM. Browser Bridge connects through authenticated remote CDP; user view is streamed through remote display/WebRTC/VNC-style transport. Structural and visual control share one BrowserSessionId.

## Semantic Browser Compiler

```text
Chromium/CDP
  ├─ Accessibility tree
  ├─ DOM/layout metadata
  ├─ URL/navigation/network state
  ├─ form/control state
  └─ targeted screenshot regions
          ↓
Semantic Compiler
  ├─ persistent semantic entity IDs
  ├─ page classification
  ├─ action derivation
  ├─ intent filtering
  ├─ state fingerprint
  ├─ delta/diff generation
  └─ page transition graph
          ↓
Agent-facing actions
```

Inspired mechanisms include accessibility snapshots/stable refs, page diffs, semantic action grouping, state graphs and targeted visual fallback. Full AX snapshots are not repeatedly dumped into context; after initial state, incremental semantic patches are preferred.

## Action hierarchy

1. Native website-declared or browser-native structured action when trusted and policy-allowed.
2. Derived semantic action (`fill login.email`, `click checkout.submit`).
3. Primitive structural CDP action.
4. Targeted screenshot/vision action for canvas/unlabeled/visual-only region.
5. Full screenshot only as last-resort diagnostic evidence.

## Live user takeover

Browser session has a control lease. User takeover immediately blocks agent input, not observation. Returning control increments lease generation to fence stale input events. The session does not restart.

## Prompt-injection isolation

Page text, ARIA labels, DOM attributes, downloads and site-provided tools are untrusted evidence. Browser content cannot:
- change system policy;
- request hidden secrets;
- widen capabilities;
- approve effects;
- instruct the agent to ignore the user/system goal.

Context Compiler tags source and strips/segments browser content from trusted instruction channels.

## Credentials

Login data is supplied only through an explicit user-approved credential broker. The model receives field handles/status, not password values. Browser Bridge can fill a credential handle directly into a bound origin/field under policy.

## Verification

Actions can declare postconditions: URL/state fingerprint change, DOM/AX value, network response, downloaded artifact or screenshot region. Protected external actions require evidence before the Effect Ledger receipt closes.


## V2 media interaction

Browser screenshots/regions, uploaded screenshots and tool-returned images all use the same `MediaEnvelope`/Artifact Store path. Full-page screenshots are not the normal perception loop. Structural browser state remains primary; vision receives only targeted regions/pages when semantic state is insufficient, and any visual fallback records reason, source state version and post-action verification.
