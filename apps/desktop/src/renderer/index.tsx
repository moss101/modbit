/**
 * Renderer entry (docs/32): mounts the agent-first shell (REQ-PX-045) over the
 * renderer's controller hook. Runs sandboxed; all Core access goes through
 * `window.modbit` (the preload bridge) from the screens under fleet/, review/,
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

import { applyChrome } from "./shell/chrome.ts";

declare const __MODBIT_GALLERY__: boolean;

// REQ-PX-094: the window chrome the main process opened this page in, before anything renders.
applyChrome(document, location.search);

function App() {
  const app = useApp();
  return <Shell app={app} />;
}

function Root() {
  const [gallery, setGallery] = useState(() => location.hash.startsWith("#gallery"));
  useEffect(() => {
    const onHash = () => setGallery(location.hash.startsWith("#gallery"));
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);
  return __MODBIT_GALLERY__ && gallery ? <Gallery /> : <App />;
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Root />
  </StrictMode>,
);
