/** Palette ranking: prefix beats word-prefix beats substring beats subsequence; keywords rank below the title. Pure. */
export interface Rankable {
  id: string;
  title: string;
  keywords?: readonly string[] | undefined;
}

function subsequenceScore(q: string, text: string): number {
  let ti = 0;
  let gaps = 0;
  let last = -1;
  for (const ch of q) {
    const at = text.indexOf(ch, ti);
    if (at < 0) return 0;
    if (last >= 0 && at !== last + 1) gaps++;
    last = at;
    ti = at + 1;
  }
  return Math.max(1, 30 - gaps * 3);
}

export function scoreOf(query: string, item: Rankable): number {
  const q = query.trim().toLowerCase();
  if (!q) return 1;
  const title = item.title.toLowerCase();
  if (title === q) return 120;
  if (title.startsWith(q)) return 100;
  if (title.split(/[\s/_.:-]+/).some((w) => w.startsWith(q))) return 80;
  if (title.includes(q)) return 60;
  const kw = (item.keywords ?? []).map((k) => k.toLowerCase());
  if (kw.some((k) => k.startsWith(q))) return 45;
  if (kw.some((k) => k.includes(q))) return 40;
  return subsequenceScore(q, title);
}

/** The items that match, best first; ties keep the input order. An empty query keeps everything in order. */
export function rank<T extends Rankable>(query: string, items: readonly T[]): T[] {
  return items
    .map((item, index) => ({ item, index, score: scoreOf(query, item) }))
    .filter((r) => r.score > 0)
    .sort((a, b) => b.score - a.score || a.index - b.index)
    .map((r) => r.item);
}
