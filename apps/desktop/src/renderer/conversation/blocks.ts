/**
 * Splits message text into blocks for rendering (AFW-C04): paragraphs and
 * fenced code. Pure, so the streaming render's cost is testable: a delta only
 * changes the last block, and every earlier block compares equal. No markup is
 * produced or interpreted; a block is text and nothing else.
 */
export type Block = { kind: "text"; text: string } | { kind: "code"; text: string; lang: string; open: boolean };

const FENCE = /^ {0,3}```\s*([\w+#.-]*)\s*$/;

export function splitBlocks(text: string): Block[] {
  const blocks: Block[] = [];
  const lines = text.split("\n");
  let para: string[] = [];
  let code: { lang: string; lines: string[] } | null = null;
  const flush = () => {
    const t = para.join("\n").replace(/^\n+|\n+$/g, "");
    if (t) blocks.push({ kind: "text", text: t });
    para = [];
  };
  for (const line of lines) {
    const m = FENCE.exec(line);
    if (code) {
      if (m && m[1] === "") {
        blocks.push({ kind: "code", text: code.lines.join("\n"), lang: code.lang, open: false });
        code = null;
      } else code.lines.push(line);
      continue;
    }
    if (m) {
      flush();
      code = { lang: m[1] ?? "", lines: [] };
      continue;
    }
    if (line.trim() === "") flush();
    else para.push(line);
  }
  if (code) blocks.push({ kind: "code", text: code.lines.join("\n"), lang: code.lang, open: true });
  else flush();
  return blocks;
}

export function sameBlock(a: Block, b: Block): boolean {
  return a.kind === b.kind && a.text === b.text && (a.kind !== "code" || (b.kind === "code" && a.lang === b.lang && a.open === b.open));
}
