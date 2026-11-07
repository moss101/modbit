/**
 * The icon primitive: draws a shape of the generated Lucide table (`icons.generated.ts`, ISC, third-party/zcode-ui/LUCIDE-LICENSE) at
 * 24 x 24, stroke 2, round caps and joins, as ZCode's interface does (DR-PX-2026-10-24-016, ZDL-07). Decorative by default
 * (`aria-hidden`): a control that is only an icon carries its accessible name on the control (`IconButton` requires `label`).
 */
import { createElement, type SVGProps } from "react";
import { ICON_NODES, type IconName } from "./icons.generated.ts";

export type { IconName } from "./icons.generated.ts";
export const ICON_NAMES = Object.keys(ICON_NODES) as IconName[];

export interface IconProps extends Omit<SVGProps<SVGSVGElement>, "name" | "ref"> {
  name: IconName;
  /** Pixel size of both sides; 16 by default (ZCode's `size-4`). */
  size?: number | undefined;
}

export function Icon({ name, size = 16, className, ...rest }: IconProps) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`lucide lucide-${name}${className ? ` ${className}` : ""}`}
      aria-hidden="true"
      {...rest}
    >
      {ICON_NODES[name].map(([tag, attrs], i) => createElement(tag, { key: i, ...attrs }))}
    </svg>
  );
}
