/**
 * A project's icon in its palette colour (PX-063/064). The icon is drawn from
 * the small set the Core accepts; the colour is a design-token role, so it is
 * right in every theme. The glyph is decorative: the project's name always
 * sits beside it, and the colour is never the only signal.
 */
import type { ReactNode } from "react";
import { colorVar, iconName } from "./project-model.ts";

const PATHS: Record<string, ReactNode> = {
  folder: <path d="M2 4.5a1 1 0 0 1 1-1h3l1.5 1.5H13a1 1 0 0 1 1 1V12a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1Z" />,
  star: <path d="m8 2 1.8 3.7 4 .6-2.9 2.8.7 4L8 11.2l-3.6 1.9.7-4L2.2 6.3l4-.6Z" />,
  flag: <path d="M4 14V2.5M4 3h8l-1.5 2.5L12 8H4" />,
  bolt: <path d="M9 1.5 3.5 9H8l-1 5.5L12.5 7H8Z" />,
  book: <path d="M3 3.5A1.5 1.5 0 0 1 4.5 2H13v10H4.5A1.5 1.5 0 0 0 3 13.5ZM3 13.5A1.5 1.5 0 0 0 4.5 15H13v-3" />,
  bug: <path d="M5.5 5a2.5 2.5 0 0 1 5 0v4.5a2.5 2.5 0 0 1-5 0ZM2.5 7.5h3M10.5 7.5h3M3 4l2.5 1.5M13 4l-2.5 1.5M3 12l2.5-1.5M13 12l-2.5-1.5" />,
  rocket: <path d="M8 1.5c2.5 1.5 3.5 4 3 7l-1 2.5H6L5 8.5c-.5-3 .5-5.5 3-7ZM6 11l-1.5 2.5M10 11l1.5 2.5M8 6.5v.01" />,
  leaf: <path d="M3 13c0-6 3-10 10-10 0 7-4 10-10 10ZM3 13l5-5" />,
};

export function ProjectGlyph({ icon, color, size = 16 }: { icon: string; color: string; size?: number }) {
  return (
    <svg viewBox="0 0 16 16" width={size} height={size} fill="none" stroke={colorVar(color)} strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false" data-testid="project-glyph" data-icon={iconName(icon)} data-color={color}>
      {PATHS[iconName(icon)]}
    </svg>
  );
}
