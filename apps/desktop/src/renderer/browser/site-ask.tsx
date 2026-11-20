/**
 * The question the browser pane asks when the agent wants to open an origin nobody has decided (PX-216, MWS-19): Allow once,
 * Always allow or Deny. The answer is sent for the origin the card shows; main refuses any other. The card draws nothing when
 * there is no question, and it shows where the person already is: the Browser tab and the full browser view.
 */
import { Button } from "@modbit/ui";

export interface SiteAskView {
  id: string;
  origin: string;
  url: string;
  atMs: number;
}

export function SiteAskCard({ browserSessionId, ask }: { browserSessionId: string; ask: SiteAskView | null | undefined }) {
  if (!ask) return null;
  const decide = (decision: "allow_once" | "allow_always" | "deny") => void window.modbit.decideBrowserSite(browserSessionId, ask.id, ask.origin, decision);
  return (
    <div className="site-ask" role="alertdialog" aria-label="The agent wants to open a site" data-testid="browser-site" data-origin={ask.origin}>
      <strong className="site-ask-title">The agent wants to open {ask.origin}.</strong>
      <span className="meta">Allow it this once, allow it from now on, or refuse it. You can change this later in Settings.</span>
      <div className="site-ask-actions">
        <Button size="sm" variant="outline" onClick={() => decide("deny")} data-testid="browser-site-deny">
          Deny
        </Button>
        <Button size="sm" variant="outline" onClick={() => decide("allow_once")} data-testid="browser-site-once">
          Allow once
        </Button>
        <Button size="sm" variant="primary" onClick={() => decide("allow_always")} data-testid="browser-site-always">
          Always allow
        </Button>
      </div>
    </div>
  );
}
