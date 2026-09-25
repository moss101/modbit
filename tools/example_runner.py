#!/usr/bin/env python3
"""Declared-example runner (REQ-EV-0212, QUAL-EV-0212).

Every ```bash block in a governing file or numbered doc is an instruction this
package gives an agent. This runner refuses to let one be a placeholder:

- every block must be declared in `tools/examples.json`, pinned by the sha256 of
  its body, so a block that is added, edited or removed fails until the
  declaration is updated (drift);
- every command inside a block carries its own policy, so one illustrative line
  does not excuse the eleven runnable ones beside it;
- a `run` command is executed for real, in a throwaway copy of the package, and
  must exit with the declared status — which is how a documented non-zero
  contract (`graph.py goal` exits 1 while work remains) is pinned rather than
  hidden;
- a `shape` command is not executed — it names why it cannot be — but it is
  still checked against the real thing it invokes: `graph.py`'s own argparse
  parser, the flag literals the other tools parse, the workspace members
  `cargo -p` names, the scripts `pnpm` runs;
- a `synopsis` is a usage template rather than a command (`graph.py set <id>
  <STATE> [--evidence <ref> ...]`); it is not executed, but its subcommand and
  every flag it names are checked against the tool's own parser, so a renamed
  subcommand or a dropped flag fails;
- a program with no shape checker cannot be declared `shape`. There is no
  policy that means "trust me".

Usage:
    python3 tools/example_runner.py              # check every declared example
    python3 tools/example_runner.py --list       # print blocks and their policy
    python3 tools/example_runner.py --declare    # rewrite declarations' hashes
                                                 # (policies and reasons are kept)
"""

import argparse
import contextlib
import hashlib
import importlib.util
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DECLARATIONS = ROOT / "tools" / "examples.json"

# Where an example may live: the governing root files and the numbered dossier,
# plus the graph's human view. A block anywhere else is not an instruction.
SOURCES = ["AGENTS.md", "README.md", "SKILLS.md", "graph/PROJECT_GRAPH.md"]
SOURCE_GLOBS = ["docs/*.md"]

BLOCK = re.compile(r"^```bash[ \t]*\n(.*?)^```[ \t]*$", re.M | re.S)
# Any fenced block, so a command cannot be moved out of ```bash to escape the gate.
ANY_BLOCK = re.compile(r"^```([a-z]*)[ \t]*\n(.*?)^```[ \t]*$", re.M | re.S)
SMUGGLED = re.compile(r"^\s*(python3|python|cargo|pnpm|npm|node|jq|git)\s+\S", re.M)


def sources():
    """Every file that may declare an example, in a stable order."""
    out = [ROOT / s for s in SOURCES]
    for g in SOURCE_GLOBS:
        out.extend(sorted(ROOT.glob(g)))
    return [p for p in out if p.is_file()]


def blocks_of(path):
    """(index, body) for each bash block in `path`, in document order."""
    text = path.read_text(encoding="utf-8")
    return [(i, m.group(1)) for i, m in enumerate(BLOCK.finditer(text))]


def smuggled():
    """Command lines sitting in a block that is not ```bash: the gate would not
    see them, so they are refused rather than silently exempt."""
    out = []
    for path in sources():
        rel = path.relative_to(ROOT).as_posix()
        for m in ANY_BLOCK.finditer(path.read_text(encoding="utf-8")):
            lang, body = m.group(1), m.group(2)
            if lang == "bash":
                continue
            for line in SMUGGLED.findall(body):
                out.append((rel, lang or "(none)", line))
    return out


def digest(body):
    return hashlib.sha256(body.encode("utf-8")).hexdigest()


def commands_of(body):
    """The command lines of a block: comments and blank lines dropped, a
    trailing `\\` continued, and `&&` / `;` chains split into their parts."""
    lines, joined = [], ""
    for raw in body.splitlines():
        line = raw.split(" #", 1)[0].rstrip() if " #" in raw else raw.rstrip()
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        if line.endswith("\\"):
            joined += line[:-1] + " "
            continue
        joined += line
        lines.append(joined)
        joined = ""
    if joined:
        lines.append(joined)
    out = []
    for line in lines:
        for part in re.split(r"&&|\|\||;", line):
            part = part.strip()
            if part:
                out.append(part)
    return out


# ---------------------------------------------------------------- shape checks


def graph_parser():
    """graph.py's own parser, so a shape check is the tool's own opinion."""
    spec = importlib.util.spec_from_file_location("_graph", ROOT / "tools" / "graph.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod.build_parser()


def graph_subcommands(state):
    """{subcommand: {flags}} as graph.py's own parser defines them."""
    if "graph_subs" in state:
        return state["graph_subs"]
    parser = state.setdefault("graph_parser", graph_parser())
    subs = {}
    for action in parser._actions:
        if isinstance(action, argparse._SubParsersAction):
            for name, sub in action.choices.items():
                subs[name] = {opt for a in sub._actions for opt in a.option_strings}
    state["graph_subs"] = subs
    return subs


# A template's placeholder, once the brackets and ellipsis a synopsis uses are
# stripped: `<id>`, `[--evidence`, `<ref>`, `...]`, `<STATE>`.
METAVAR = re.compile(r"^[a-z_]+$|^[A-Z_]+$")


def synopsis_error(command, state):
    """A usage template: check the subcommand and the flags it names, not a run."""
    try:
        argv = shlex.split(command)
    except ValueError as e:
        return "is not a shell template (%s)" % e
    if len(argv) < 3 or argv[0] not in ("python3", "python") or not argv[1].endswith("graph.py"):
        return ("is only supported for `python3 tools/graph.py …` templates; "
                "declare it `shape` or `run`")
    subs = graph_subcommands(state)
    sub = argv[2]
    if sub not in subs:
        return "names no graph.py subcommand (`%s`)" % sub
    for token in argv[3:]:
        flags = re.findall(r"--[a-z][a-z-]*", token)
        for f in flags:
            if f not in subs[sub]:
                return "names %s, which `graph.py %s` does not accept" % (f, sub)
        if flags:
            continue
        core = token.strip("[]()<>.")
        if not core or METAVAR.match(core):
            continue
        return ("names the literal %r, which a synopsis should not carry; "
                "declare it `shape` or `run`" % token)
    return None


def workspace_members():
    """Package names of the workspace members, read from their manifests."""
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    m = re.search(r"^members\s*=\s*\[(.*?)^\]", text, re.M | re.S)
    names = set()
    for path in re.findall(r'"([^"]+)"', m.group(1) if m else ""):
        manifest = ROOT / path / "Cargo.toml"
        if manifest.is_file():
            n = re.search(r'^name\s*=\s*"([^"]+)"', manifest.read_text(encoding="utf-8"), re.M)
            if n:
                names.add(n.group(1))
    return names


def pnpm_scripts():
    pkg = ROOT / "package.json"
    if not pkg.is_file():
        return set()
    return set(json.loads(pkg.read_text(encoding="utf-8")).get("scripts", {}))


def cargo_subcommands():
    try:
        out = subprocess.run(["cargo", "--list"], capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.SubprocessError):
        return None
    if out.returncode != 0:
        return None
    return {l.split()[0] for l in out.stdout.splitlines()[1:] if l.strip()}


def shape_python(argv, state):
    """`python3 tools/<tool>.py …`."""
    tool = Path(argv[1])
    if not (ROOT / tool).is_file():
        return "names %s, which is not in this package" % tool
    if tool.name == "graph.py":
        parser = state.setdefault("graph_parser", graph_parser())
        try:
            with open(os.devnull, "w", encoding="utf-8") as quiet, \
                    contextlib.redirect_stdout(quiet), contextlib.redirect_stderr(quiet):
                parser.parse_args(argv[2:])
        except SystemExit:
            return "graph.py's own parser rejects `%s`" % " ".join(argv[2:])
        return None
    # The other tools read sys.argv directly, so every flag they honour is a
    # literal in their source.
    source = (ROOT / tool).read_text(encoding="utf-8")
    for flag in [a for a in argv[2:] if a.startswith("--")]:
        if '"%s"' % flag not in source and "'%s'" % flag not in source:
            return "%s does not parse %s any more" % (tool.name, flag)
    return None


def shape_cargo(argv, state):
    if len(argv) < 2:
        return "names no cargo subcommand"
    subs = state.setdefault("cargo_subs", cargo_subcommands())
    if subs is not None and argv[1] not in subs:
        return "cargo has no `%s` subcommand" % argv[1]
    members = state.setdefault("members", workspace_members())
    for i, a in enumerate(argv):
        if a == "-p" or a == "--package":
            if i + 1 >= len(argv):
                return "names -p with no package"
            if argv[i + 1] not in members:
                return "`%s` is not a workspace member" % argv[i + 1]
    return None


def shape_pnpm(argv, state):
    scripts = state.setdefault("pnpm", pnpm_scripts())
    words = [a for a in argv[1:] if not a.startswith("-")]
    if words and words[0] in ("install", "exec", "run", "dlx", "add"):
        words = words[1:] if words[0] == "run" else []
    for w in words:
        if w not in scripts:
            return "package.json has no `%s` script" % w
    return None


SHAPE = {"python3": shape_python, "python": shape_python, "cargo": shape_cargo, "pnpm": shape_pnpm}


def shape_error(command, state):
    try:
        argv = shlex.split(command)
    except ValueError as e:
        return "is not a shell command (%s)" % e
    if not argv:
        return "is empty"
    checker = SHAPE.get(argv[0])
    if checker is None:
        return ("invokes `%s`, which has no shape checker: declare the example `run`, "
                "or add a checker for that program" % argv[0])
    return checker(argv, state)


# ------------------------------------------------------------------ run checks


def copied_package():
    """A throwaway copy of the package: a `run` example's effects are real but
    never touch the package under review."""
    tmp = tempfile.mkdtemp(prefix="modbit-examples-")
    dst = Path(tmp) / "package"
    shutil.copytree(ROOT, dst, ignore=shutil.ignore_patterns(
        ".git", "__pycache__", ".DS_Store", "node_modules", "target"))
    return tmp, dst


NESTED = "MODBIT_EXAMPLE_RUNNER"


def run_error(command, cwd, expect):
    # A declared example that invoked this gate would run it inside itself, and
    # each level copies the package: the guard turns that into one legible
    # failure instead of a timeout.
    env = dict(os.environ, **{NESTED: "1"})
    try:
        out = subprocess.run(command, shell=True, cwd=cwd, capture_output=True,
                             text=True, timeout=600, env=env)
    except subprocess.SubprocessError as e:
        return "did not complete (%s)" % e
    if out.returncode != expect:
        tail = (out.stdout + out.stderr).strip().splitlines()
        return "exited %d, not the declared %d: %s" % (
            out.returncode, expect, tail[-1] if tail else "(no output)")
    return None


# ---------------------------------------------------------------------- driver


def load_declarations():
    if not DECLARATIONS.is_file():
        return {}
    data = json.loads(DECLARATIONS.read_text(encoding="utf-8"))
    return {(d["file"], d["index"]): d for d in data.get("examples", [])}


def found_blocks():
    out = {}
    for path in sources():
        rel = path.relative_to(ROOT).as_posix()
        for index, body in blocks_of(path):
            out[(rel, index)] = body
    return out


def check():
    declared = load_declarations()
    found = found_blocks()
    problems, state, ran, shaped = [], {}, 0, 0

    for rel, lang, program in smuggled():
        problems.append("%s: a `%s` command sits in a ```%s block, where this gate cannot "
                        "see it; put it in a ```bash block and declare it"
                        % (rel, program, lang))

    for key in sorted(set(declared) - set(found)):
        problems.append("%s block %d: declared but no longer in the file "
                        "(remove the declaration)" % key)

    tmp = dst = None
    try:
        for key in sorted(found):
            rel, index = key
            body = found[key]
            d = declared.get(key)
            if d is None:
                problems.append("%s block %d: undeclared example; declare it in "
                                "tools/examples.json as `run` or `shape`" % key)
                continue
            if d.get("sha256") != digest(body):
                problems.append("%s block %d: drifted from its declaration; re-declare it "
                                "(python3 tools/example_runner.py --declare)" % key)
                continue
            commands = commands_of(body)
            if not commands:
                problems.append("%s block %d: declares no command" % key)
                continue
            decls = d.get("commands") or []
            if len(decls) != len(commands):
                problems.append("%s block %d: declares %d command(s), the block has %d; "
                                "re-declare it" % (rel, index, len(decls), len(commands)))
                continue
            for c, cd in zip(commands, decls):
                policy = cd.get("policy")
                if policy in ("shape", "synopsis"):
                    if not (cd.get("reason") or "").strip():
                        problems.append("%s block %d: `%s` is declared `%s` without "
                                        "saying why it cannot be executed"
                                        % (rel, index, c, policy))
                        continue
                    e = synopsis_error(c, state) if policy == "synopsis" else shape_error(c, state)
                    if e:
                        problems.append("%s block %d: `%s` %s" % (rel, index, c, e))
                    else:
                        shaped += 1
                elif policy == "run":
                    if int(cd.get("exit", 0)) and not (cd.get("reason") or "").strip():
                        problems.append("%s block %d: `%s` expects a non-zero exit without "
                                        "saying why that is the contract" % (rel, index, c))
                        continue
                    if dst is None:
                        tmp, dst = copied_package()
                    e = run_error(c, dst, int(cd.get("exit", 0)))
                    if e:
                        problems.append("%s block %d: `%s` %s" % (rel, index, c, e))
                    else:
                        ran += 1
                else:
                    problems.append("%s block %d: `%s` has policy %r; it must be `run`, "
                                    "`shape` or `synopsis`" % (rel, index, c, policy))
    finally:
        if tmp:
            shutil.rmtree(tmp, ignore_errors=True)

    return problems, len(found), ran, shaped


def declare():
    """Rewrite the hashes of the declarations, keeping policy and reason, and
    add a stub for a new block so the author must choose a policy."""
    declared = load_declarations()
    found = found_blocks()
    out = []
    added = 0
    for key in sorted(found):
        rel, index = key
        old = declared.get(key) or {}
        commands = commands_of(found[key])
        # Keep a command's policy only while its text is unchanged, so a rewritten
        # line comes back undeclared instead of inheriting a stale decision.
        by_text = {c.get("command"): c for c in (old.get("commands") or []) if c.get("command")}
        decls = []
        for c in commands:
            prev = by_text.get(c) or {}
            if not prev:
                added += 1
            e = {"command": c, "policy": prev.get("policy", ""), "reason": prev.get("reason", "")}
            if int(prev.get("exit", 0)):
                e["exit"] = int(prev["exit"])
            decls.append(e)
        out.append({"file": rel, "index": index, "sha256": digest(found[key]), "commands": decls})
    DECLARATIONS.write_text(
        json.dumps({"examples": out}, indent=1) + "\n", encoding="utf-8")
    print("tools/examples.json: %d block(s), %d command(s) new and undeclared"
          % (len(out), added))
    return 0


def main(argv):
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--list", action="store_true", help="print every block and its policy")
    p.add_argument("--declare", action="store_true", help="rewrite declaration hashes")
    args = p.parse_args(argv)

    if os.environ.get(NESTED):
        print("[example] this gate is running inside one of its own `run` examples; "
              "a declared example must not invoke tools/example_runner.py")
        return 1

    if args.declare:
        return declare()

    if args.list:
        declared = load_declarations()
        for key, body in sorted(found_blocks().items()):
            d = declared.get(key) or {}
            decls = {c.get("command"): c.get("policy") for c in (d.get("commands") or [])}
            print("%s block %d:" % key)
            for c in commands_of(body):
                print("    %-6s %s" % (decls.get(c) or "NONE", c))
        return 0

    problems, total, ran, shaped = check()
    for pr in problems:
        print("[example] " + pr)
    if problems:
        print("FAIL: %d finding(s) over %d declared example(s)" % (len(problems), total))
        return 1
    print("OK: %d example block(s); %d command(s) executed in a copied package, "
          "%d checked against the tool they invoke" % (total, ran, shaped))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
