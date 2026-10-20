/** The agent list's own small line icons (16 px, currentColor, decorative: each sits inside a control with a text name). */
import type { ReactNode } from "react";

const svg = (children: ReactNode) => (
  <svg viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">
    {children}
  </svg>
);

export const IconPin = () => svg(<path d="M9.5 2.5 13.5 6.5 11 7l-1.5 3-1-1L4 13.5 2.5 12 7 7.5l-1-1 3-1.5Z" />);
export const IconArchive = () => svg(<><rect x="2" y="3" width="12" height="3" rx="1" /><path d="M3 6v6.5a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V6M6.5 9h3" /></>);
export const IconFilter = () => svg(<path d="M2.5 3.5h11L9.5 8.5V13l-3-1.5V8.5Z" />);
export const IconProject = () => svg(<path d="M2 4.5a1 1 0 0 1 1-1h3l1.5 1.5H13a1 1 0 0 1 1 1V12a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1ZM8 7.5v3M6.5 9h3" />);
export const IconRename = () => svg(<path d="M3 13h2.5L13 5.5 10.5 3 3 10.5ZM9 4.5 11.5 7" />);
export const IconRestore = () => svg(<path d="M3 8a5 5 0 1 0 1.6-3.7M3 2.5v3h3" />);
export const IconViews = () => svg(<path d="M2.5 4h11M2.5 8h11M2.5 12h7M12 11l1.5 1.5L12 14" />);
