#!/usr/bin/env python3
"""Multi-level integration qualification gate (REQ-EV-0211, QUAL-EV-0211).

"Qualification includes direct component, registry/integration and real
external API test when integration requires it. Staging integration uses real
test credential and recorded safe fixture; mock-only cannot pass."

`tools/integrations.json` declares every integration the product has: each
place its production code opens a connection to something outside its own
process. Each entry names its boundary and the tests that qualify it at each
level. This gate holds the declaration to the code:

* coverage — every workspace member (and TypeScript package) whose production
  source opens an outbound connection is claimed by an entry; an entry that
  claims a member with no connection is stale;
* levels — every named test exists as a test. An `external-api` integration
  needs all three levels: component (the adapter called directly), integration
  (through the Core or the registry the product uses) and external (a live
  test the live workflow actually runs, against the real API with the real
  test credential) plus a recorded safe fixture replayed offline. The live
  level can be deferred only by an accepted Decision Record naming the input
  that is missing. A `real-service` or `in-tree` integration needs its
  integration level against the real counterpart and has no external level;
* recorded fixtures — each is a recording (schema, the CI run that made it,
  the endpoint it talked to), carries no credential header, and its sha256
  is in a retained live result record of that run under `evidence/`, so the
  file in the tree is the one the live run wrote;
* `--live DIR` — the result records a live run left (`MODBIT_LIVE_RESULTS_DIR`):
  every live test of every non-deferred external integration must have
  `passed` and proved that integration. A `skipped` record, or none, fails:
  a live test that skipped exits 0 exactly like one that passed;
* `--release` — a deferred live level fails too: a release cannot stand on
  mock-only proof of an external integration.

Standard library only, Python 3.9+.

    python3 tools/integration_gate.py               # structural checks
    python3 tools/integration_gate.py --live DIR    # plus a live run's records
    python3 tools/integration_gate.py --release     # plus: no deferrals
    python3 tools/integration_gate.py --list        # the inventory and its state
"""

import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
INVENTORY = ROOT / "tools" / "integrations.json"

BOUNDARIES = ("external-api", "real-service", "in-tree")
LEVELS = ("component", "integration", "external")
FIXTURE_SCHEMA = "modbit.recorded-exchange/1"
RESULT_SCHEMA = "modbit.live-result/1"
CREDENTIAL_HEADERS = {"authorization", "x-api-key", "api-key", "cookie", "set-cookie",
                      "proxy-authorization"}

# What opening a connection looks like in production source. Deliberately
# about clients: a server that only listens is not an integration.
RUST_CONNECTORS = [
    (re.compile(r"\breqwest::"), "reqwest"),
    (re.compile(r"\buse\s+reqwest\b"), "reqwest"),
    (re.compile(r"\b(ureq|isahc|surf|octocrab|aws_sdk_\w+)::"), "an HTTP client crate"),
    (re.compile(r"\bobject_store::"), "object_store"),
    (re.compile(r"\bhyper(_util)?::client\b"), "a hyper client"),
    (re.compile(r"\btokio_tungstenite::(connect_async|client_async)"), "a websocket client"),
    (re.compile(r"\bTcpStream::connect\b"), "TcpStream::connect"),
]
TS_CONNECTORS = [
    (re.compile(r"(?<![\w.])fetch\("), "fetch"),
    (re.compile(r"\bhttps?\.request\("), "http.request"),
    (re.compile(r"\bnet\.request\("), "electron net.request"),
    (re.compile(r"\bnew WebSocket\("), "WebSocket"),
]

TEST_ATTR = re.compile(r"#\[(tokio::)?test\b")


def rel(p):
    return str(Path(p).relative_to(ROOT))


def rust_members():
    """`{package name: member dir}` from the workspace manifest."""
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8") if (ROOT / "Cargo.toml").is_file() else ""
    m = re.search(r"^members\s*=\s*\[(.*?)^\]", text, re.M | re.S)
    out = {}
    for path in re.findall(r'"([^"]+)"', m.group(1) if m else ""):
        manifest = ROOT / path / "Cargo.toml"
        if manifest.is_file():
            n = re.search(r'^name\s*=\s*"([^"]+)"', manifest.read_text(encoding="utf-8"), re.M)
            if n:
                out[n.group(1)] = ROOT / path
    return out


def ts_packages():
    """`{package name: dir}` for the pnpm workspace's own packages."""
    ws = ROOT / "pnpm-workspace.yaml"
    out = {}
    if not ws.is_file():
        return out
    for pattern in re.findall(r'^\s*-\s*"([^"]+)"', ws.read_text(encoding="utf-8"), re.M):
        if pattern.startswith("tests/"):
            continue  # fixture target software, not the product
        for d in sorted(ROOT.glob(pattern)):
            pkg = d / "package.json"
            if pkg.is_file():
                out[json.loads(pkg.read_text(encoding="utf-8")).get("name", d.name)] = d
    return out


def connections(name, root, rust):
    """The first outbound connection in a member's production source, as
    `(file, line, what)`, or None."""
    src = root / "src"
    if not src.is_dir():
        return None
    files = sorted(src.rglob("*.rs")) if rust else sorted(
        p for p in list(src.rglob("*.ts")) + list(src.rglob("*.tsx"))
        if "/gen/" not in str(p) and not re.search(r"\.(spec|test)\.tsx?$", p.name))
    patterns = RUST_CONNECTORS if rust else TS_CONNECTORS
    for f in files:
        for i, line in enumerate(f.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
            if line.lstrip().startswith(("//", "*", "/*")):
                continue
            for rx, what in patterns:
                if rx.search(line):
                    return rel(f), i, what
    return None


def test_exists(ref):
    """None when `path::name` names a test, else why not."""
    if "::" not in ref:
        return "%r is not `path::test`" % ref
    path, name = ref.split("::", 1)
    f = ROOT / path
    if not f.is_file():
        return "%s: no such file" % path
    lines = f.read_text(encoding="utf-8", errors="replace").splitlines()
    if path.endswith(".rs"):
        for i, line in enumerate(lines):
            if re.search(r"\bfn\s+%s\s*[(<]" % re.escape(name), line):
                # Only the attributes and comments directly on this function.
                j = i - 1
                while j >= 0 and lines[j].strip().startswith(("#[", "//")):
                    if TEST_ATTR.search(lines[j]):
                        return None
                    j -= 1
                return "%s: `%s` is a function, not a test" % (path, name)
        return "%s: no test `%s`" % (path, name)
    if re.search(r"\.(spec|test)\.tsx?$", path):
        if any(re.search(r"\btest(\.\w+)?\(\s*[\"'`]%s[\"'`]" % re.escape(name), l) for l in lines):
            return None
        return "%s: no test titled %r" % (path, name)
    return "%s: not a Rust or TypeScript test file" % path


def member_of(path, members):
    """The package a repository path belongs to."""
    p = (ROOT / path).resolve()
    best = None
    for name, d in members.items():
        try:
            p.relative_to(d.resolve())
        except ValueError:
            continue
        if best is None or len(str(d)) > len(str(members[best])):
            best = name
    return best


def live_crates():
    """Packages whose `live_` tests a workflow runs, from `.github/workflows`."""
    out = set()
    wf = ROOT / ".github" / "workflows"
    for f in sorted(wf.glob("*.yml")) if wf.is_dir() else []:
        for line in f.read_text(encoding="utf-8").splitlines():
            if re.search(r"\bcargo test\b", line) and re.search(r"\blive_", line):
                out.update(re.findall(r"-p\s+([\w-]+)", line))
    return out


def accepted_decision(dr):
    d = ROOT / "docs" / "decisions"
    for f in sorted(d.glob("%s-*.md" % dr)) if d.is_dir() else []:
        head = f.read_text(encoding="utf-8").split("\n---", 1)[0]
        if re.search(r"^status:\s*accepted\s*$", head, re.M):
            return None
        return "%s is not accepted" % rel(f)
    return "no Decision Record %s under docs/decisions/" % dr


def retained_records():
    """Every retained live result record under evidence/."""
    out = []
    ev = ROOT / "evidence"
    for f in sorted(ev.rglob("*.json")) if ev.is_dir() else []:
        try:
            v = json.loads(f.read_text(encoding="utf-8"))
        except (ValueError, UnicodeDecodeError):
            continue
        if isinstance(v, dict) and v.get("schema") == RESULT_SCHEMA:
            out.append((f, v))
    return out


def check_fixture(entry, rec, records, fail):
    iid = entry["id"]
    path = rec.get("fixture", "")
    f = ROOT / path
    if not f.is_file():
        fail(iid, "recorded fixture %s does not exist (record it with the live workflow)" % path)
        return
    raw = f.read_bytes()
    try:
        fx = json.loads(raw.decode("utf-8"))
    except (ValueError, UnicodeDecodeError) as e:
        fail(iid, "%s is not JSON: %s" % (path, e))
        return
    if fx.get("schema") != FIXTURE_SCHEMA:
        fail(iid, "%s: schema %r, not %s" % (path, fx.get("schema"), FIXTURE_SCHEMA))
    if fx.get("integration") != iid:
        fail(iid, "%s records %r, not this integration" % (path, fx.get("integration")))
    prov = fx.get("provenance") or {}
    run = str(prov.get("run", ""))
    if not run.isdigit():
        fail(iid, "%s: provenance.run %r is not a CI run: a fixture is recorded by the live "
                  "workflow, never locally or by hand" % (path, run))
    for key in ("endpoint_host", "model"):
        if not prov.get(key):
            fail(iid, "%s: provenance.%s is missing" % (path, key))
    exchanges = fx.get("exchanges") or []
    if not exchanges:
        fail(iid, "%s holds no exchange" % path)
    for i, ex in enumerate(exchanges):
        for side in ("request", "response"):
            for h in ((ex.get(side) or {}).get("headers") or {}):
                if h.lower() in CREDENTIAL_HEADERS:
                    fail(iid, "%s: exchange %d %s carries the credential header %r" % (path, i, side, h))
    why = test_exists(rec.get("replay", ""))
    if why:
        fail(iid, "replay test: " + why)
    digest = hashlib.sha256(raw).hexdigest()
    for rf, rv in records:
        if (str(rv.get("run")) == run and rv.get("outcome") == "passed"
                and any(r.get("sha256") == digest and r.get("integration") == iid
                        for r in rv.get("recordings") or [])):
            return
    fail(iid, "%s (sha256 %s) is not the recording of a retained live run: keep run %s's "
              "result record under evidence/" % (path, digest[:12], run or "?"))


def main(argv):
    live_dir = None
    if "--live" in argv:
        i = argv.index("--live")
        if i + 1 >= len(argv):
            print("--live needs the directory of a live run's result records", file=sys.stderr)
            return 2
        live_dir = Path(argv[i + 1])
    release = "--release" in argv
    listing = "--list" in argv

    failures = []
    failing = set()

    def fail(iid, msg):
        failing.add(iid)
        failures.append("%s: %s" % (iid, msg))

    if not INVENTORY.is_file():
        print("integration gate: %s is missing" % rel(INVENTORY), file=sys.stderr)
        return 1
    inv = json.loads(INVENTORY.read_text(encoding="utf-8"))
    entries = inv.get("integrations") or []
    rust = rust_members()
    ts = ts_packages()
    members = dict(rust)
    members.update(ts)
    records = retained_records()
    run_live = live_crates()

    # Coverage: code with a connection is declared, and a declaration has code.
    claimed = {}
    ids = set()
    for e in entries:
        if e.get("id") in ids:
            fail(e.get("id"), "declared twice")
        ids.add(e.get("id"))
        for c in e.get("crates") or []:
            claimed.setdefault(c, []).append(e.get("id"))
    found = {}
    for name, d in sorted(rust.items()):
        hit = connections(name, d, True)
        if hit:
            found[name] = hit
    for name, d in sorted(ts.items()):
        hit = connections(name, d, False)
        if hit:
            found[name] = hit
    for name, (f, line, what) in sorted(found.items()):
        if name not in claimed:
            failures.append("undeclared integration: %s opens a connection (%s at %s:%d) and no "
                            "entry in tools/integrations.json claims it" % (name, what, f, line))
    for name, owners in sorted(claimed.items()):
        if name not in members:
            fail(owners[0], "claims %s, which is not a workspace member or package" % name)
        elif name not in found:
            fail(owners[0], "claims %s, whose source opens no outbound connection" % name)

    states = []
    for e in entries:
        iid = e.get("id", "?")
        boundary = e.get("boundary")
        levels = e.get("levels") or {}
        deferred = e.get("deferred")
        if boundary not in BOUNDARIES:
            fail(iid, "boundary %r is not one of %s" % (boundary, ", ".join(BOUNDARIES)))
            continue
        if not e.get("counterparty"):
            fail(iid, "names no counterparty (what is on the other end)")
        for level in LEVELS:
            for ref in levels.get(level) or []:
                why = test_exists(ref)
                if why:
                    fail(iid, "%s level: %s" % (level, why))
        for level in levels:
            if level not in LEVELS:
                fail(iid, "unknown level %r" % level)
        if boundary == "external-api":
            if not e.get("credentials"):
                fail(iid, "an external API is reached with a credential; none is named")
            for level in ("component", "integration"):
                if not levels.get(level):
                    fail(iid, "no %s-level test" % level)
            if deferred:
                why = accepted_decision(deferred.get("decision", ""))
                if why:
                    fail(iid, "deferral: " + why)
                if not deferred.get("needs"):
                    fail(iid, "deferral names no missing input")
            else:
                if not levels.get("external"):
                    fail(iid, "no external-level test against the real API (mock-only cannot pass)")
                if not e.get("recorded"):
                    fail(iid, "no recorded safe fixture of the real API")
            for ref in levels.get("external") or []:
                path, _, name = ref.partition("::")
                if not name.startswith("live_"):
                    fail(iid, "external test %s is not a `live_` test the live workflow selects" % ref)
                crate = member_of(path, rust)
                if crate not in run_live:
                    fail(iid, "external test %s is in %s, whose live tests no workflow runs"
                         % (ref, crate))
            for rec in e.get("recorded") or []:
                check_fixture(e, rec, records, fail)
        else:
            if not levels.get("integration"):
                fail(iid, "no integration-level test against the real %s" % e.get("counterparty", "counterpart"))
            if levels.get("external") or e.get("recorded") or deferred:
                fail(iid, "a %s integration has no external API level" % boundary)
        states.append((iid, boundary, levels, e))

    # A live run's records: every live test of every non-deferred external
    # integration passed and proved it.
    if live_dir is not None:
        for iid, boundary, levels, e in states:
            if boundary != "external-api" or e.get("deferred"):
                continue
            for ref in levels.get("external") or []:
                name = ref.split("::", 1)[-1]
                f = live_dir / ("%s.json" % name)
                if not f.is_file():
                    fail(iid, "%s left no result record in %s: it did not run, or it failed"
                         % (name, live_dir))
                    continue
                rv = json.loads(f.read_text(encoding="utf-8"))
                if rv.get("outcome") != "passed":
                    fail(iid, "%s %s (%s): a live test that did not pass is not proof, "
                              "whatever its exit status" % (name, rv.get("outcome"), rv.get("reason", "")))
                elif not any(p.get("integration") == iid for p in rv.get("proven") or []):
                    fail(iid, "%s passed without proving this integration (no credential for it?)"
                         % name)
    if release:
        for iid, boundary, levels, e in states:
            if e.get("deferred"):
                fail(iid, "qualified mock-only: %s defers its real-API level until %s"
                     % (e["deferred"].get("decision"), e["deferred"].get("needs")))

    if listing or failures:
        for iid, boundary, levels, e in states:
            counts = " · ".join("%s %d" % (l, len(levels.get(l) or [])) for l in LEVELS)
            state = ("FAILING" if iid in failing else
                     "DEFERRED (%s)" % e["deferred"].get("decision") if e.get("deferred") else
                     "QUALIFIED")
            print("%-26s %-13s %s · recorded %d  %s" % (
                iid, boundary, counts, len(e.get("recorded") or []), state))
    for f in failures:
        print("FAIL " + f)
    if failures:
        print("integration gate: %d failure(s) (REQ-EV-0211)" % len(failures))
        return 1
    mode = "live" if live_dir is not None else "structural"
    if release:
        mode += ", release"
    print("integration gate: %d integrations, %d members with outbound connections, all declared "
          "and qualified (%s)" % (len(entries), len(found), mode))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
