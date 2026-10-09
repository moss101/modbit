/**
 * What may be attached to a message (REQ-PX-054, docs/65 AFW-D09): an image, a
 * PDF or a plain-text file, up to a size cap. The type is decided from the
 * bytes (a name or a declared MIME is only a hint, and a mismatch is
 * refused), here once, by both sides: the renderer to explain a refusal
 * before anything is sent, main as the authority before anything reaches the
 * Core. Pure; no Node, no DOM.
 */

/** The Core's frame ceiling is 4 MiB; an attachment leaves room for the envelope. */
export const MAX_ATTACHMENT_BYTES = 3 * 1024 * 1024;
export const MAX_ATTACHMENTS_PER_MESSAGE = 8;

export type AttachmentKind = "image" | "pdf" | "text";

export interface AttachmentCheck {
  ok: boolean;
  kind?: AttachmentKind;
  mime?: string;
  /** Why not, in words for the person. */
  reason?: string;
}

const TEXT_EXTENSIONS = new Set(["txt", "md", "markdown", "json", "csv", "tsv", "log", "yml", "yaml", "toml", "ini", "xml", "html", "css", "js", "mjs", "cjs", "ts", "tsx", "jsx", "py", "rs", "go", "java", "c", "h", "cc", "cpp", "hpp", "sh", "sql", "diff", "patch"]);

const startsWith = (b: Uint8Array, sig: number[], at = 0): boolean => b.length >= at + sig.length && sig.every((v, i) => b[at + i] === v);

/** The image or document type the bytes really are, or null. */
export function sniff(bytes: Uint8Array): { kind: AttachmentKind; mime: string } | null {
  if (startsWith(bytes, [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])) return { kind: "image", mime: "image/png" };
  if (startsWith(bytes, [0xff, 0xd8, 0xff])) return { kind: "image", mime: "image/jpeg" };
  if (startsWith(bytes, [0x47, 0x49, 0x46, 0x38]) && (bytes[4] === 0x37 || bytes[4] === 0x39) && bytes[5] === 0x61) return { kind: "image", mime: "image/gif" };
  if (startsWith(bytes, [0x52, 0x49, 0x46, 0x46]) && startsWith(bytes, [0x57, 0x45, 0x42, 0x50], 8)) return { kind: "image", mime: "image/webp" };
  if (startsWith(bytes, [0x25, 0x50, 0x44, 0x46, 0x2d])) return { kind: "pdf", mime: "application/pdf" };
  return null;
}

/** Whether the bytes are text: no NUL in the first 8 KiB and valid UTF-8 throughout. */
export function isText(bytes: Uint8Array): boolean {
  const head = bytes.subarray(0, 8192);
  if (head.includes(0)) return false;
  try {
    new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    return true;
  } catch {
    return false;
  }
}

/** The extension of a file name, lower case, without the dot ("" when none). */
export function extensionOf(name: string): string {
  const base = name.split(/[\\/]/).pop() ?? "";
  const dot = base.lastIndexOf(".");
  return dot > 0 ? base.slice(dot + 1).toLowerCase() : "";
}

/**
 * The verdict for one candidate. `declaredMime` is what the browser said (may
 * be empty); a declared image or PDF type the bytes contradict is refused, so
 * a renamed executable never passes for a picture.
 */
export function checkAttachment(name: string, bytes: Uint8Array, declaredMime = ""): AttachmentCheck {
  if (bytes.byteLength === 0) return { ok: false, reason: "The file is empty." };
  if (bytes.byteLength > MAX_ATTACHMENT_BYTES) return { ok: false, reason: `The file is larger than ${Math.round(MAX_ATTACHMENT_BYTES / (1024 * 1024))} MB, the most one attachment may be.` };
  const found = sniff(bytes);
  if (found) {
    const declared = declaredMime.toLowerCase();
    if (declared && /^(image|application\/pdf)/.test(declared) && declared !== found.mime && !(declared === "image/jpg" && found.mime === "image/jpeg")) return { ok: false, reason: `The file says it is ${declaredMime} but its contents are ${found.mime}.` };
    return { ok: true, kind: found.kind, mime: found.mime };
  }
  const ext = extensionOf(name);
  if (TEXT_EXTENSIONS.has(ext) && isText(bytes)) return { ok: true, kind: "text", mime: ext === "json" ? "application/json" : ext === "md" || ext === "markdown" ? "text/markdown" : "text/plain" };
  if (ext && !TEXT_EXTENSIONS.has(ext)) return { ok: false, reason: `A .${ext} file is not a type that can be attached. Images, PDFs and plain-text files can.` };
  return { ok: false, reason: "That file is not an image, a PDF or plain text, so it cannot be attached." };
}

/** A file name safe to show and to send as a label: no directories, no control characters, bounded. */
export function labelOf(name: string): string {
  // eslint-disable-next-line no-control-regex
  const base = (name.split(/[\\/]/).pop() ?? "").replace(/[\u0000-\u001f\u007f]/g, "").trim();
  return (base || "attachment").slice(0, 120);
}
