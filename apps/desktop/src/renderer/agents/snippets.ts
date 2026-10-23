/**
 * Search snippets (docs/65 AFW-B10): the Core returns a plain-text excerpt and
 * the byte ranges of it that matched. This splits the excerpt into runs for
 * highlighting. The ranges are UTF-8 byte offsets, so the text is encoded,
 * cut on those offsets and decoded again; a range that is out of order,
 * overlapping, beyond the text or that splits a character is dropped rather
 * than trusted. The text is data: it is only ever rendered as text.
 */
export interface Run {
  text: string;
  match: boolean;
}

export function highlightRuns(text: string, matches: readonly { start: number; end: number }[]): Run[] {
  const bytes = new TextEncoder().encode(text);
  const dec = new TextDecoder("utf-8", { fatal: true });
  const runs: Run[] = [];
  let at = 0;
  const slice = (from: number, to: number): string | null => {
    try {
      return dec.decode(bytes.subarray(from, to));
    } catch {
      return null;
    }
  };
  for (const m of matches) {
    if (!Number.isInteger(m.start) || !Number.isInteger(m.end) || m.start < at || m.end <= m.start || m.end > bytes.length) continue;
    const before = slice(at, m.start);
    const hit = slice(m.start, m.end);
    if (before === null || hit === null) continue;
    if (before) runs.push({ text: before, match: false });
    runs.push({ text: hit, match: true });
    at = m.end;
  }
  const tail = slice(at, bytes.length);
  if (tail) runs.push({ text: tail, match: false });
  return runs.length > 0 ? runs : [{ text, match: false }];
}
