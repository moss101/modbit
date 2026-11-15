/**
 * How tall More may be while an approval card is docked (REQ-PX-201's open item, closed with REQ-PX-202). The card stays on top
 * of More, so its answers can always be clicked; More therefore ends above the card and scrolls inside, so everything it holds
 * can still be reached in a short or narrow window. Null where the card is not under More: the style sheet's own limit stands.
 */
export interface Edges {
  top: number;
  left: number;
  right: number;
}

/** More is never squeezed under this: below it the card wins and More scrolls a few rows at a time. */
export const MORE_MIN_HEIGHT = 120;
const GAP = 8;

export function moreMaxHeight(more: Edges, card: Edges | null): number | null {
  if (!card) return null;
  const sideBySide = card.left >= more.right || card.right <= more.left;
  if (sideBySide || card.top <= more.top) return null;
  return Math.max(MORE_MIN_HEIGHT, Math.floor(card.top - more.top - GAP));
}
