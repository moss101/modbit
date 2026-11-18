/**
 * Renderer entry (docs/32): mounts the agent-first shell (REQ-PX-045) over the
 * renderer's controller hook. Runs sandboxed; all Core access goes through
 * `window.modbit` (the preload bridge) from the screens under home/, review/,
 * browser/ and dashboard/. Screen states follow docs/39 PX-023. The model-free
 * state gallery (PX-044) is reachable only in a build made with
 * MODBIT_GALLERY=1 and only at `#gallery`; a production bundle defines the flag
 * false and contains none of it.
 */
import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { Shell } from "./shell/shell.tsx";
import { useApp } from "./state/use-app.ts";
import { Gallery } from "./gallery/gallery.tsx";
import { PaneWindow } from "./pane-window.tsx";

declare const __MODBIT_GALLERY__: boolean;

function App() {
  const app = useApp();
  return <Shell app={app} />;
}

function Root() {
  const [hash, setHash] = useState(() => location.hash);
  useEffect(() => {
    const onHash = () => setHash(location.hash);
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);
  return __MODBIT_GALLERY__ && hash.startsWith("#gallery") ? <Gallery /> : <App />;
}

if (/Mac/.test(navigator.platform)) document.documentElement.dataset.chrome = "mac";

// REQ-PX-203: the main process opens a pane window at `#pane`; which pane it shows is main's answer, never the address.
const paneWindow = location.hash === "#pane";
createRoot(document.getElementById("root")!).render(<StrictMode>{paneWindow ? <PaneWindow /> : <Root />}</StrictMode>);
