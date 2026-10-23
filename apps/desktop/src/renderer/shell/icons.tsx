/** Modbit's own small line icons (16 px, currentColor, decorative: every use sits inside a control that has a text name). */
import type { ReactNode } from "react";

const svg = (children: ReactNode) => (
  <svg viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">
    {children}
  </svg>
);

export const IconSidebar = () => svg(<><rect x="2" y="3" width="12" height="10" rx="2" /><path d="M6 3v10" /></>);
export const IconBack = () => svg(<path d="M9.5 3.5 5 8l4.5 4.5" />);
export const IconPanel = () => svg(<><rect x="2" y="3" width="12" height="10" rx="2" /><path d="M10 3v10" /></>);
export const IconMore = () => svg(<><circle cx="3.5" cy="8" r="0.6" fill="currentColor" /><circle cx="8" cy="8" r="0.6" fill="currentColor" /><circle cx="12.5" cy="8" r="0.6" fill="currentColor" /></>);
export const IconPlus = () => svg(<path d="M8 3v10M3 8h10" />);
export const IconLocal = () => svg(<><rect x="2.5" y="3.5" width="11" height="7.5" rx="1.5" /><path d="M6 13.5h4" /></>);
