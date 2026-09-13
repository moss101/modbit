/**
 * @modbit/ide-adapter-core — the shared thin-client library and the
 * conformance suite every SurfaceProtocol client passes before exposure
 * (docs/29 "Thin-client conformance contract (PX-001)").
 *
 * Canonical owner: desktop (docs/12). The library speaks the protocol once —
 * connection, handshake, commands idempotent by command id, events by cursor
 * — for Electron main and for IDE adapters; the suite proves a client keeps
 * the contract against a real Core and links no provider, filesystem, Git
 * or policy code of its own.
 */
export * from "./client.ts";
export * from "./supervisor.ts";
export * from "./conformance.ts";
