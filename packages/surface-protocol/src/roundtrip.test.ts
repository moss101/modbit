/**
 * Cross-language round trip, TypeScript side (M0.3 acceptance).
 *
 * For every Rust fixture in tests/fixtures/protocol/v1/rust:
 *  1. decode the Rust bytes and compare every field with the expected values;
 *  2. build the same message independently from those values, encode it, and
 *     require byte equality with the Rust encoding;
 *  3. write the TypeScript encoding to tests/fixtures/protocol/v1/ts for the
 *     Rust test (tools/protocol-fixtures) to decode.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync, writeFileSync, mkdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import {
  create,
  fromBinary,
  toBinary,
  type DescField,
  type DescMessage,
  type Message,
} from "@bufbuild/protobuf";
import { file_modbit_v1_domain } from "./gen/modbit/v1/domain_pb.js";
import { file_modbit_v1_envelope } from "./gen/modbit/v1/envelope_pb.js";
import { file_modbit_v1_tool } from "./gen/modbit/v1/tool_pb.js";
import { file_modbit_v1_output_ref } from "./gen/modbit/v1/output_ref_pb.js";
import { file_modbit_v1_negotiation } from "./gen/modbit/v1/negotiation_pb.js";

const here = dirname(fileURLToPath(import.meta.url));
const fixtures = join(here, "../../../tests/fixtures/protocol/v1");
const messagesByType = new Map<string, DescMessage>(
  [file_modbit_v1_domain, file_modbit_v1_envelope, file_modbit_v1_tool, file_modbit_v1_output_ref, file_modbit_v1_negotiation]
    .flatMap((f) => f.messages)
    .map((m) => [m.typeName, m]),
);

type Plain = string | number | boolean | null | Plain[] | { [k: string]: Plain };

const hex = (b: Uint8Array): string => Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
const unhex = (s: string): Uint8Array => Uint8Array.from(s.match(/../g) ?? [], (h) => parseInt(h, 16));

/** Field value -> the plain representation used by the Rust fixture JSON. */
function normalizeValue(f: DescField, v: unknown): Plain {
  if (v === undefined) return null;
  switch (f.fieldKind) {
    case "list":
      return (v as unknown[]).map((item) => normalizeItem(f, item));
    default:
      return normalizeItem(f, v);
  }
}

function normalizeItem(f: DescField, v: unknown): Plain {
  if (f.fieldKind === "message" || (f.fieldKind === "list" && f.listKind === "message")) {
    const m = v as Record<string, unknown>;
    const typeName = f.message!.typeName;
    if (typeName === "modbit.v1.Id") return hex(m["value"] as Uint8Array);
    if (typeName === "google.protobuf.Timestamp") return { seconds: String(m["seconds"]), nanos: Number(m["nanos"]) };
    if (typeName === "modbit.v1.ProtocolVersion") return { major: Number(m["major"]), minor: Number(m["minor"]) };
    throw new Error(`unhandled message type ${typeName}`);
  }
  if (f.fieldKind === "enum" || (f.fieldKind === "list" && f.listKind === "enum")) {
    const name = f.enum!.values.find((e) => e.number === v)?.name;
    assert.ok(name, `enum value ${String(v)} has no name`);
    return name;
  }
  if (v instanceof Uint8Array) return hex(v);
  if (typeof v === "bigint") return v.toString();
  return v as Plain;
}

function normalize(desc: DescMessage, msg: Message): Record<string, Plain> {
  const out: Record<string, Plain> = {};
  for (const f of desc.fields) out[f.localName] = normalizeValue(f, (msg as unknown as Record<string, unknown>)[f.localName]);
  return out;
}

/** Plain representation -> message, built independently of the Rust bytes. */
function denormalizeItem(f: DescField, v: Plain): unknown {
  if (f.fieldKind === "message" || (f.fieldKind === "list" && f.listKind === "message")) {
    const desc = f.message!;
    if (desc.typeName === "modbit.v1.Id") return create(desc, { value: unhex(v as string) });
    if (desc.typeName === "google.protobuf.Timestamp") {
      const t = v as { seconds: string; nanos: number };
      return create(desc, { seconds: BigInt(t.seconds), nanos: t.nanos });
    }
    if (desc.typeName === "modbit.v1.ProtocolVersion") return create(desc, v as { major: number; minor: number });
    throw new Error(`unhandled message type ${desc.typeName}`);
  }
  if (f.fieldKind === "enum" || (f.fieldKind === "list" && f.listKind === "enum")) {
    const e = f.enum!.values.find((x) => x.name === v);
    assert.ok(e, `enum name ${String(v)} unknown`);
    return e.number;
  }
  const scalar = f.scalar;
  // ScalarType: BYTES = 12, 64-bit integers are represented as bigint by default.
  if (scalar === 12) return unhex(v as string);
  if (scalar === 3 || scalar === 4 || scalar === 6 || scalar === 16 || scalar === 18) return BigInt(v as string);
  return v;
}

function denormalize(desc: DescMessage, plain: Record<string, Plain>): Message {
  const init: Record<string, unknown> = {};
  for (const f of desc.fields) {
    const v = plain[f.localName];
    if (v === null || v === undefined) continue;
    init[f.localName] = f.fieldKind === "list" ? (v as Plain[]).map((i) => denormalizeItem(f, i)) : denormalizeItem(f, v);
  }
  return create(desc, init);
}

const names = readdirSync(join(fixtures, "rust")).filter((n) => n.endsWith(".json")).map((n) => n.slice(0, -5)).sort();
assert.ok(names.length >= 6, "Rust fixtures missing; run `cargo run -p protocol-fixtures`");
mkdirSync(join(fixtures, "ts"), { recursive: true });

for (const name of names) {
  test(`round trip ${name}: Rust bytes -> TS fields -> TS bytes`, () => {
    const spec = JSON.parse(readFileSync(join(fixtures, "rust", `${name}.json`), "utf8")) as { type: string; expected: Record<string, Plain> };
    const rustBytes = new Uint8Array(readFileSync(join(fixtures, "rust", `${name}.bin`)));
    const desc = messagesByType.get(spec.type);
    assert.ok(desc, `unknown type ${spec.type}`);

    const decoded = fromBinary(desc, rustBytes);
    assert.deepEqual(normalize(desc, decoded), spec.expected, "decoded fields differ from Rust expectation");
    assert.deepEqual(toBinary(desc, decoded), rustBytes, "re-encoding decoded message changed bytes");

    const rebuilt = denormalize(desc, spec.expected);
    const tsBytes = toBinary(desc, rebuilt);
    assert.deepEqual(tsBytes, rustBytes, "independently built TS message encodes differently from Rust");
    writeFileSync(join(fixtures, "ts", `${name}.bin`), tsBytes);
  });
}
