import { test } from "node:test";
import assert from "node:assert/strict";
import { checkAttachment, extensionOf, labelOf, MAX_ATTACHMENT_BYTES, sniff } from "./attachments.ts";

const PNG = Uint8Array.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0]);
const JPEG = Uint8Array.from([0xff, 0xd8, 0xff, 0xe0, 0, 0]);
const PDF = new TextEncoder().encode("%PDF-1.7\n%…");
const text = (s: string) => new TextEncoder().encode(s);

test("the type is decided from the bytes, not from the name or the declared type", () => {
  assert.deepEqual(sniff(PNG), { kind: "image", mime: "image/png" });
  assert.deepEqual(sniff(JPEG), { kind: "image", mime: "image/jpeg" });
  assert.deepEqual(sniff(PDF), { kind: "pdf", mime: "application/pdf" });
  assert.equal(sniff(text("hello")), null);
  assert.deepEqual(checkAttachment("photo.png", PNG, "image/png"), { ok: true, kind: "image", mime: "image/png" });
  // A name that lies does not matter; the bytes are what they are.
  assert.deepEqual(checkAttachment("notes.txt", PNG, ""), { ok: true, kind: "image", mime: "image/png" });
  // A declared image that the bytes contradict is refused.
  const lie = checkAttachment("evil.png", PDF, "image/png");
  assert.equal(lie.ok, false);
  assert.match(lie.reason ?? "", /contents are application\/pdf/);
  assert.equal(checkAttachment("a.jpg", JPEG, "image/jpg").ok, true);
});

test("plain text passes only with a text extension and valid UTF-8; binary and unknown types are refused with a reason", () => {
  assert.deepEqual(checkAttachment("README.md", text("# hi\n"), "text/markdown"), { ok: true, kind: "text", mime: "text/markdown" });
  assert.equal(checkAttachment("data.json", text("{}"), "").mime, "application/json");
  const exe = checkAttachment("setup.exe", Uint8Array.from([0x4d, 0x5a, 0x90, 0, 3, 0]), "application/x-msdownload");
  assert.equal(exe.ok, false);
  assert.match(exe.reason ?? "", /\.exe/);
  assert.equal(checkAttachment("blob", Uint8Array.from([1, 2, 3, 0, 4]), "").ok, false);
  assert.equal(checkAttachment("notes.txt", Uint8Array.from([0xff, 0xfe, 0xfd, 0x41]), "").ok, false, "not UTF-8");
  assert.equal(checkAttachment("bin.txt", Uint8Array.from([0x41, 0, 0x42]), "").ok, false, "a NUL byte is not text");
});

test("an empty file and one over the cap are refused before anything is sent", () => {
  assert.match(checkAttachment("a.png", new Uint8Array(0)).reason ?? "", /empty/);
  const big = new Uint8Array(MAX_ATTACHMENT_BYTES + 1);
  big.set(PNG);
  assert.match(checkAttachment("a.png", big).reason ?? "", /larger than 3 MB/);
  const edge = new Uint8Array(MAX_ATTACHMENT_BYTES);
  edge.set(PNG);
  assert.equal(checkAttachment("a.png", edge).ok, true, "exactly the cap is allowed");
});

test("a name is only a label: directories and control characters never travel", () => {
  assert.equal(labelOf("../../etc/passwd"), "passwd");
  assert.equal(labelOf("C:\\Users\\me\\pic.png"), "pic.png");
  assert.equal(labelOf("a\nb\u0000c.txt"), "abc.txt");
  assert.equal(labelOf(""), "attachment");
  assert.equal(labelOf("x".repeat(500)).length, 120);
  assert.equal(extensionOf("dir.d/file"), "");
  assert.equal(extensionOf("archive.TAR.GZ"), "gz");
});
