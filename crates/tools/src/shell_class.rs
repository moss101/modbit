//! Effect classification of what a shell-backed call is asked to run
//! (FIX-02, docs/16 "Effect classes", docs/23 "Approval policy").
//!
//! `shell.exec`, `shell.start` and `test.run` are registered as
//! `ReversibleWrite`, but what one call does depends entirely on its argv:
//! `git push --force`, `rm -rf`, `curl -X POST` and `npm install` are not
//! reversible workspace writes. This module is the single owner of the
//! argv → [`EffectClass`] mapping. The tools call it through
//! [`Tool::effect_in_profile`](crate::registry::Tool::effect_in_profile), so
//! the pipeline and the Core's proposal both judge the class it returns, the
//! kernel decides on that class with its existing lattice (no class is
//! invented here), and the effect receipt (class ≥ `ProtectedWrite`) follows.
//!
//! What it understands, deliberately conservatively:
//!
//! - wrappers: `env`, `sudo`/`doas`/`su -c`, `nice`, `nohup`, `time`,
//!   `timeout`, `command`, `exec`, `stdbuf`, `xargs`, `find -exec`;
//! - shell strings: `sh|bash|zsh|… -c '<string>'` (and `-lc`, `-ec`, …),
//!   `eval`, `trap`, a here-document or `stdin` given to a bare shell. The
//!   string is parsed (quotes, `;` `&&` `||` `|` `&`, subshells, `$(…)`,
//!   backticks, here-documents, redirections, loops and `if`) and every
//!   command in it is classified; what cannot be parsed or resolved
//!   (a variable in command position, `case`, an unterminated quote) is
//!   *unclassified*, never assumed safe;
//! - programs: a table of git sub-commands, package managers, build
//!   toolchains, network clients, cloud CLIs, system administration and
//!   destructive file tools.
//!
//! This is a deterministic tripwire in front of the kernel, not a sandbox:
//! an interpreter running a script file (`sh check.sh`, `python x.py`,
//! `./gradlew test`) runs project code the classifier cannot see, exactly as
//! `cargo test` runs `build.rs` and tests. What such code writes in the
//! workspace is the change barrier's business (FIX-06, [`crate::direct`]);
//! real confinement is `review_isolated` / `cloud_isolated`.
//!
//! Unclassified programs raise to `ProtectedWrite` under `local_trusted`
//! only (an approval can be asked there): `local_autonomous` cannot wait for
//! one and `review_isolated`/`cloud_isolated` confine the process.

use serde_json::Value;

use crate::EffectClass;

const MAX_DEPTH: usize = 6;
const MAX_REASONS: usize = 4;

/// What a shell-backed call does, from its arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellEffect {
    /// The class of what the command is known to do (never below
    /// `ReversibleWrite`, the registered floor of the shell tools).
    pub class: EffectClass,
    /// What inline interpreter code (`python -c`, `node -e`) appears to do,
    /// by marker scan. Weaker evidence than the argv itself, so it counts
    /// only under `local_trusted`, the profile where an approval can be
    /// asked and nothing confines the process (the sandboxed profiles
    /// enforce no-network and the rest at the kernel boundary instead).
    pub inline_class: EffectClass,
    /// Part of the command could not be resolved or recognized.
    pub unclassified: bool,
    /// The command knowingly writes outside the workspace (a path that
    /// climbs out, a system location, a home-directory dotfile, a device, or
    /// a system-administration program). PX-057: an always-ask class.
    pub outside_workspace: bool,
    /// The command knowingly writes repository or tool configuration (git
    /// configuration, or a setting that names programs git runs). PX-057: an
    /// always-ask class.
    pub protected_path: bool,
    /// Why the class was raised (and what was not understood), short.
    pub reasons: Vec<String>,
}

impl ShellEffect {
    /// The class the kernel is asked about under `profile`: an
    /// unclassified command is `ProtectedWrite` under `local_trusted` (the
    /// profile that can ask a person) and its known class elsewhere.
    #[must_use]
    pub fn class_in(&self, profile: &str) -> EffectClass {
        if profile != "local_trusted" {
            return self.class;
        }
        let known = self.class.max(self.inline_class);
        if self.unclassified {
            known.max(EffectClass::ProtectedWrite)
        } else {
            known
        }
    }

    /// The human-readable reason for the class under `profile`, when it was
    /// raised above a plain workspace write.
    #[must_use]
    pub fn reason_in(&self, profile: &str) -> Option<String> {
        (self.class_in(profile) > EffectClass::ReversibleWrite && !self.reasons.is_empty())
            .then(|| self.reasons.join("; "))
    }
}

/// Classify the `argv` / `stdin` / `env` of one shell-backed call.
#[must_use]
pub fn classify_args(args: &Value) -> ShellEffect {
    let argv: Vec<String> = args
        .get("argv")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let stdin = args.get("stdin").and_then(Value::as_str);
    let mut acc = Acc::new();
    if let Some(env) = args.get("env").and_then(Value::as_object) {
        for k in env.keys() {
            if dangerous_env(k) {
                acc.raise(
                    EffectClass::ProtectedWrite,
                    format!("`{k}` in env changes what programs load or run"),
                );
            }
        }
    }
    classify_cmd(&argv, false, stdin, 0, &mut acc);
    acc.finish()
}

/// The argv a durable run-mode rule may cover for one command-shaped call
/// (PX-057, docs/65 AFW-F10): the call's argv, and nothing at all when the
/// call overrides the environment. A rule is a literal argv prefix; what the
/// environment changes (`PATH`, a wrapper, a loader) changes what the same argv
/// runs, so such a call is not covered by a rule and asks.
#[must_use]
pub fn rule_argv(args: &Value) -> Option<Vec<String>> {
    if args
        .get("env")
        .and_then(Value::as_object)
        .is_some_and(|e| !e.is_empty())
    {
        return None;
    }
    args.get("argv").and_then(Value::as_array).map(|a| {
        a.iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    })
}

/// Whether a typed line is only an answer to a prompt (`y`, `no`, `3`, a bare
/// Enter) rather than something a shell would run.
fn is_prompt_answer(line: &str) -> bool {
    let t = line.trim();
    t.is_empty()
        || matches!(
            t.to_ascii_lowercase().as_str(),
            "y" | "yes" | "n" | "no" | "q" | "a"
        )
        || t.chars().all(|c| c.is_ascii_digit())
}

/// Classify what a `shell.input` call types into a terminal (PX-099).
///
/// Typing into a shell is running commands in it: stdin to an interactive
/// shell must not be a way round the classification `shell.exec` gets. The
/// decision (docs/21, docs/62 PX-099): **the typed text is classified as the
/// shell script it would be if the session's program is a shell** — the same
/// parser, tables and rules as `shell.exec -c <text>`, every command in it,
/// raised to `ProtectedWrite` under `local_trusted` when something in it
/// cannot be resolved — **and, because the classifier cannot see which
/// program reads the text, it is also scanned as inline interpreter code**
/// (what a REPL would run). The class is never below the registered
/// `ReversibleWrite` floor. Plain answers to a prompt (`y`, `no`, `3`, Enter)
/// and key presses (`ctrl-c`, `ctrl-d`) are not commands; a line that is one
/// bare word nothing resolves (a name typed at a `read`) is an answer too,
/// while a word the tables know to be dangerous still raises the class and a
/// word with arguments (`some-tool --flag`) is unclassified like any
/// unknown program. What this cannot
/// see is a program that interprets the text in a way no marker names; that
/// is bounded the way `shell.exec` of an interpreter is: by the owner check
/// (the task's own session only), the person's input lease and, under
/// `review_isolated`, the sandbox the process runs in.
#[must_use]
pub fn classify_input(args: &Value) -> ShellEffect {
    let mut acc = Acc::new();
    let text = args.get("text").and_then(Value::as_str).unwrap_or_default();
    let mut script: Vec<&str> = Vec::new();
    for line in text.lines().filter(|l| !is_prompt_answer(l)) {
        if is_bare_word(line) {
            // One word and nothing else: it runs only as a program with no
            // arguments, and one the tables know to be dangerous (`reboot`,
            // `shutdown`) still raises the class. A word nothing resolves is
            // an answer to a prompt (a name, a choice) as often as a command
            // and cannot do more than a program run with no arguments, so it
            // is not held for an approval the way `some-tool --flag` is.
            let mut word = Acc::new();
            classify_string(line, 0, &mut word);
            acc.class = acc.class.max(word.class);
            acc.inline = acc.inline.max(word.inline);
            for why in word
                .reasons
                .into_iter()
                .filter(|w| !w.starts_with("unclassified"))
            {
                acc.note(why);
            }
        } else {
            script.push(line);
        }
    }
    if !script.is_empty() {
        let script = script.join("\n");
        classify_string(&script, 0, &mut acc);
        scan_inline_code(&script, &mut acc);
    }
    acc.finish()
}

/// A line that is a single plain word.
fn is_bare_word(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty()
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '@' | ':' | '-'))
        && !t.starts_with('-')
}

/// Classify one argv (tests and callers that have no JSON arguments).
#[must_use]
pub fn classify_argv(argv: &[String], stdin: Option<&str>) -> ShellEffect {
    let mut acc = Acc::new();
    classify_cmd(argv, false, stdin, 0, &mut acc);
    acc.finish()
}

struct Acc {
    class: EffectClass,
    inline: EffectClass,
    unknown: bool,
    outside_workspace: bool,
    protected_path: bool,
    reasons: Vec<String>,
}

impl Acc {
    fn new() -> Self {
        Self {
            class: EffectClass::ReversibleWrite,
            inline: EffectClass::ReversibleWrite,
            unknown: false,
            outside_workspace: false,
            protected_path: false,
            reasons: vec![],
        }
    }

    fn note(&mut self, why: String) {
        if self.reasons.len() < MAX_REASONS && !self.reasons.contains(&why) {
            self.reasons.push(why);
        }
    }

    fn raise(&mut self, class: EffectClass, why: impl Into<String>) {
        self.note(why.into());
        self.class = self.class.max(class);
    }

    fn raise_inline(&mut self, class: EffectClass, why: impl Into<String>) {
        self.note(why.into());
        self.inline = self.inline.max(class);
    }

    fn unknown(&mut self, why: impl Into<String>) {
        self.note(format!("unclassified: {}", why.into()));
        self.unknown = true;
    }

    fn finish(self) -> ShellEffect {
        ShellEffect {
            class: self.class,
            inline_class: self.inline,
            unclassified: self.unknown,
            outside_workspace: self.outside_workspace,
            protected_path: self.protected_path,
            reasons: self.reasons,
        }
    }
}

fn dangerous_env(k: &str) -> bool {
    let k = k.to_ascii_uppercase();
    k.starts_with("LD_")
        || k.starts_with("DYLD_")
        || matches!(
            k.as_str(),
            "BASH_ENV"
                | "ENV"
                | "PROMPT_COMMAND"
                | "PATH"
                | "GIT_SSH"
                | "GIT_SSH_COMMAND"
                | "GIT_EXTERNAL_DIFF"
                | "GIT_ASKPASS"
                | "GIT_PAGER"
                | "GIT_DIR"
                | "GIT_WORK_TREE"
                | "GIT_CONFIG_COUNT"
                | "GIT_CONFIG_GLOBAL"
                | "PAGER"
                | "EDITOR"
                | "VISUAL"
                | "NODE_OPTIONS"
                | "PYTHONSTARTUP"
                | "PYTHONPATH"
                | "RUSTC_WRAPPER"
                | "RUSTC_WORKSPACE_WRAPPER"
                | "RUSTFLAGS"
                | "CARGO_BUILD_RUSTC_WRAPPER"
        )
}

// ---------------------------------------------------------------------------
// program tables
// ---------------------------------------------------------------------------

/// Local programs whose effect is reading, printing or a write in the
/// workspace; shell builtins.
const SAFE: &[&str] = &[
    "ls",
    "cat",
    "head",
    "tail",
    "wc",
    "grep",
    "egrep",
    "fgrep",
    "rg",
    "ag",
    "ack",
    "fd",
    "fdfind",
    "tree",
    "stat",
    "file",
    "du",
    "df",
    "pwd",
    "echo",
    "printf",
    "true",
    "false",
    "test",
    "[",
    "[[",
    "sleep",
    "date",
    "which",
    "whereis",
    "type",
    "whoami",
    "id",
    "uname",
    "hostname",
    "basename",
    "dirname",
    "realpath",
    "readlink",
    "sort",
    "uniq",
    "cut",
    "tr",
    "diff",
    "cmp",
    "md5",
    "md5sum",
    "shasum",
    "sha1sum",
    "sha256sum",
    "sha512sum",
    "cksum",
    "xxd",
    "od",
    "hexdump",
    "strings",
    "jq",
    "yq",
    "less",
    "more",
    "column",
    "tac",
    "rev",
    "nl",
    "paste",
    "comm",
    "join",
    "fold",
    "expand",
    "unexpand",
    "seq",
    "yes",
    "printenv",
    "mktemp",
    "cd",
    "pushd",
    "popd",
    "export",
    "set",
    "unset",
    "read",
    "wait",
    "exit",
    "return",
    ":",
    "local",
    "declare",
    "typeset",
    "readonly",
    "alias",
    "unalias",
    "shift",
    "getopts",
    "umask",
    "ulimit",
    "hash",
    "let",
    "break",
    "continue",
    "fg",
    "bg",
    "jobs",
    "history",
    "tput",
    "clear",
    "reset",
    "stty",
    "tty",
    "locale",
    "nproc",
    "getconf",
    "arch",
    "sw_vers",
    "lscpu",
    "free",
    "uptime",
    "ps",
    "pgrep",
    "lsof",
    "pidof",
    "expr",
    "bc",
    "dc",
    "awk",
    "gawk",
    "mawk",
    "sed",
    "gsed",
    "tee",
    "cp",
    "mv",
    "ln",
    "rsync",
    "mkdir",
    "touch",
    "install",
    "chmod",
    "truncate",
    "patch",
    "tar",
    "zip",
    "unzip",
    "gzip",
    "gunzip",
    "zcat",
    "bzip2",
    "xz",
    "7z",
    "7za",
    "iconv",
    "base64",
    "openssl",
    "shuf",
    "split",
    "csplit",
    "numfmt",
    "units",
    "bat",
    "delta",
    "eza",
    "exa",
    "lsd",
    "tokei",
    "cloc",
    "scc",
    "ctags",
    "shellcheck",
    "shfmt",
    "hadolint",
    "actionlint",
    "yamllint",
    "markdownlint",
    "codespell",
    "typos",
    "diff3",
    "sdiff",
    "colordiff",
    "cargo-nextest",
    "nextest",
    "wasm-opt",
    "sqlite3",
    "rustc-demangle",
    "c++filt",
    "nm",
    "objdump",
    "strip",
    "ranlib",
    "ar",
    "as",
    "ld",
    "otool",
    "lipo",
    "codesign",
    "dsymutil",
];

/// Build and test toolchains: they run project code (build scripts, tests,
/// recipes) by design; what they write in the workspace is attributed by the
/// change barrier. Their sub-commands that reach the network are listed in
/// [`network_verbs`].
const BUILD: &[&str] = &[
    "cargo",
    "rustc",
    "rustfmt",
    "rustdoc",
    "cargo-clippy",
    "clippy-driver",
    "rustup",
    "maturin",
    "wasm-pack",
    "trunk",
    "go",
    "gofmt",
    "golangci-lint",
    "npm",
    "pnpm",
    "yarn",
    "bun",
    "tsc",
    "tsx",
    "ts-node",
    "eslint",
    "prettier",
    "biome",
    "vitest",
    "jest",
    "mocha",
    "ava",
    "playwright",
    "cypress",
    "vite",
    "webpack",
    "rollup",
    "esbuild",
    "turbo",
    "nx",
    "lerna",
    "next",
    "nuxt",
    "astro",
    "pip",
    "pip3",
    "pipx",
    "pipenv",
    "poetry",
    "pdm",
    "uv",
    "conda",
    "mamba",
    "rye",
    "hatch",
    "pytest",
    "py.test",
    "tox",
    "nox",
    "ruff",
    "mypy",
    "pyright",
    "black",
    "isort",
    "flake8",
    "pylint",
    "bandit",
    "coverage",
    "sphinx-build",
    "mkdocs",
    "make",
    "gmake",
    "cmake",
    "ctest",
    "ninja",
    "meson",
    "bazel",
    "bazelisk",
    "buck2",
    "gradle",
    "gradlew",
    "mvn",
    "mvnw",
    "ant",
    "sbt",
    "just",
    "task",
    "rake",
    "bundle",
    "gem",
    "rspec",
    "composer",
    "phpunit",
    "phpstan",
    "dotnet",
    "msbuild",
    "swift",
    "swiftc",
    "xcodebuild",
    "xcrun",
    "javac",
    "java",
    "kotlinc",
    "kotlin",
    "scala",
    "clang",
    "clang++",
    "gcc",
    "g++",
    "cc",
    "c++",
    "protoc",
    "buf",
    "flatc",
    "bison",
    "flex",
    "mix",
    "rebar3",
    "ghc",
    "cabal",
    "stack",
    "dune",
    "zig",
    "dart",
    "flutter",
    "opam",
    "bats",
];

/// Interpreters: the script they run is project code the classifier cannot
/// see; inline code (`-c`, `-e`) is scanned for effect markers.
const INTERPRETERS: &[&str] = &[
    "python", "python3", "py", "node", "nodejs", "deno", "ruby", "perl", "php", "lua", "luajit",
    "rscript", "julia", "elixir",
];

const SHELLS: &[&str] = &[
    "sh", "bash", "zsh", "dash", "ksh", "ash", "fish", "csh", "tcsh", "mksh", "busybox",
];

/// Programs that reach the network or an external service.
const NETWORK: &[&str] = &[
    "curl",
    "wget",
    "nc",
    "ncat",
    "netcat",
    "telnet",
    "ftp",
    "sftp",
    "tftp",
    "scp",
    "ssh",
    "socat",
    "nmap",
    "mtr",
    "ping",
    "ping6",
    "traceroute",
    "dig",
    "nslookup",
    "host",
    "whois",
    "aria2c",
    "httpie",
    "http",
    "https",
    "xh",
    "lftp",
    "mosh",
    "docker",
    "podman",
    "docker-compose",
    "kubectl",
    "helm",
    "minikube",
    "kind",
    "terraform",
    "tofu",
    "pulumi",
    "ansible",
    "ansible-playbook",
    "vagrant",
    "aws",
    "gcloud",
    "az",
    "gsutil",
    "doctl",
    "gh",
    "glab",
    "heroku",
    "vercel",
    "netlify",
    "fly",
    "flyctl",
    "wrangler",
    "firebase",
    "supabase",
    "stripe",
    "railway",
    "pipx",
    "npx",
    "bunx",
    "uvx",
    "pnpx",
    "brew",
    "apt",
    "apt-get",
    "aptitude",
    "dnf",
    "yum",
    "apk",
    "pacman",
    "zypper",
    "snap",
    "flatpak",
    "port",
    "nix",
    "nix-env",
    "choco",
    "winget",
    "scoop",
    "open",
    "xdg-open",
    "mail",
    "sendmail",
    "mutt",
    "msmtp",
    "twine",
    "cargo-publish",
    "jfrog",
    "gem-push",
    "conan",
    "vcpkg",
    "pod",
    "carthage",
];

/// System administration, privileged and OS-configuration programs.
const SYSTEM: &[&str] = &[
    "launchctl",
    "systemctl",
    "service",
    "crontab",
    "at",
    "defaults",
    "networksetup",
    "scutil",
    "pmset",
    "csrutil",
    "spctl",
    "tccutil",
    "dscl",
    "sysctl",
    "mount",
    "umount",
    "diskutil",
    "hdiutil",
    "ifconfig",
    "route",
    "iptables",
    "pfctl",
    "ufw",
    "useradd",
    "userdel",
    "usermod",
    "groupadd",
    "passwd",
    "chsh",
    "visudo",
    "chown",
    "chgrp",
    "xattr",
    "osascript",
    "kill",
    "pkill",
    "killall",
    "shutdown",
    "reboot",
    "halt",
    "poweroff",
    "telinit",
    "chroot",
    "setcap",
    "update-alternatives",
    "ldconfig",
    "modprobe",
    "insmod",
    "rmmod",
    "nvram",
    "bless",
];

/// Programs that read credential stores.
const SECRET_PROGRAMS: &[&str] = &[
    "security",
    "op",
    "pass",
    "gpg",
    "gpg2",
    "ssh-add",
    "ssh-keygen",
    "ssh-agent",
    "age",
    "keychain",
    "secret-tool",
    "vault",
    "sops",
];

/// Destructive by nature.
const DESTRUCTIVE: &[&str] = &[
    "shred", "srm", "wipefs", "mkfs", "mke2fs", "fdisk", "sfdisk", "parted", "gdisk",
];

/// Programs that write the paths they are given: those paths are checked
/// against system and credential locations.
const WRITERS_ALL: &[&str] = &[
    "tee", "touch", "mkdir", "chmod", "truncate", "patch", "tar", "zip", "unzip", "mv", "gzip",
    "gunzip", "bzip2", "xz", "7z", "7za", "split",
];
/// Programs whose last argument is the destination.
const WRITERS_LAST: &[&str] = &["cp", "ln", "install", "rsync"];

/// A subset of arguments of a build tool that names a delivery.
const DEPLOY_WORDS: &[&str] = &[
    "deploy", "publish", "release", "push", "upload", "login", "logout",
];

fn network_verbs(prog: &str) -> Option<&'static [&'static str]> {
    const NODE_PM: &[&str] = &[
        "install",
        "i",
        "in",
        "ins",
        "add",
        "ci",
        "clean-install",
        "update",
        "up",
        "upgrade",
        "upgrade-interactive",
        "remove",
        "rm",
        "uninstall",
        "un",
        "link",
        "ln",
        "dedupe",
        "find-dupes",
        "publish",
        "unpublish",
        "login",
        "logout",
        "adduser",
        "token",
        "owner",
        "access",
        "deprecate",
        "dist-tag",
        "view",
        "info",
        "search",
        "audit",
        "outdated",
        "dlx",
        "create",
        "init",
        "ping",
        "doctor",
        "fund",
        "star",
        "unstar",
        "team",
        "org",
        "hook",
        "profile",
        "self-update",
        "patch",
        "patch-commit",
        "env",
        "setup",
        "store",
        "import",
        "x",
    ];
    const PY_PM: &[&str] = &[
        "install",
        "i",
        "uninstall",
        "download",
        "add",
        "remove",
        "update",
        "upgrade",
        "sync",
        "lock",
        "publish",
        "tool",
        "self",
        "create",
        "wheel",
        "index",
        "search",
        "inject",
        "ensurepath",
        "python",
    ];
    const CARGO: &[&str] = &[
        "install",
        "publish",
        "update",
        "fetch",
        "search",
        "login",
        "logout",
        "owner",
        "yank",
        "add",
        "generate-lockfile",
        "vendor",
        "audit",
        "deny",
    ];
    const GO: &[&str] = &["get", "install", "download", "tidy", "vendor", "sync"];
    const GEM_LIKE: &[&str] = &[
        "install",
        "uninstall",
        "update",
        "upgrade",
        "add",
        "remove",
        "require",
        "push",
        "yank",
        "signin",
        "signout",
        "create-project",
        "global",
        "outdated",
        "owner",
        "fetch",
        "cert",
        "pristine",
    ];
    const DOTNET: &[&str] = &["add", "nuget", "tool", "workload", "new"];
    const DART: &[&str] = &[
        "get",
        "add",
        "upgrade",
        "downgrade",
        "publish",
        "global",
        "remove",
    ];
    const SWIFT: &[&str] = &["resolve", "update"];
    const PW: &[&str] = &["install", "install-deps"];
    const RUSTUP: &[&str] = &[
        "update",
        "install",
        "toolchain",
        "component",
        "target",
        "self",
        "default",
        "override",
        "set",
        "run",
        "man",
        "doc",
    ];
    const OPAM: &[&str] = &[
        "install", "update", "upgrade", "init", "switch", "pin", "remove",
    ];
    match prog {
        "npm" | "pnpm" | "yarn" | "bun" => Some(NODE_PM),
        "pip" | "pip3" | "pipx" | "pipenv" | "poetry" | "pdm" | "uv" | "conda" | "mamba"
        | "rye" | "hatch" => Some(PY_PM),
        "cargo" => Some(CARGO),
        "go" => Some(GO),
        "gem" | "bundle" | "composer" | "mix" | "cabal" | "stack" => Some(GEM_LIKE),
        "dotnet" => Some(DOTNET),
        "dart" | "flutter" => Some(DART),
        "swift" => Some(SWIFT),
        "playwright" | "cypress" => Some(PW),
        "rustup" => Some(RUSTUP),
        "opam" => Some(OPAM),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// path risk
// ---------------------------------------------------------------------------

/// Credential stores under a home directory (relative to it).
const HOME_SECRETS: &[&str] = &[
    ".ssh",
    ".aws",
    ".gnupg",
    ".kube",
    ".netrc",
    ".npmrc",
    ".pypirc",
    ".git-credentials",
    ".docker/config.json",
    ".config/gh",
    ".config/gcloud",
    ".azure",
    ".password-store",
    ".cargo/credentials",
    ".m2/settings.xml",
    ".terraform.d/credentials",
    "Library/Keychains",
];

/// The part of `p` below a home directory (`~/…`, `$HOME/…`, `/Users/x/…`,
/// `/home/x/…`), if `p` is under one.
fn under_home(p: &str) -> Option<&str> {
    for pre in ["~/", "$HOME/", "${HOME}/"] {
        if let Some(r) = p.strip_prefix(pre) {
            return Some(r);
        }
    }
    for pre in ["/Users/", "/home/"] {
        if let Some(r) = p.strip_prefix(pre) {
            return r.split_once('/').map(|(_, rest)| rest);
        }
    }
    None
}

fn is_secret_path(p: &str) -> bool {
    if p.starts_with("/etc/shadow") || p.starts_with("/etc/sudoers") {
        return true;
    }
    under_home(p).is_some_and(|rest| {
        HOME_SECRETS
            .iter()
            .any(|s| rest == *s || rest.starts_with(&format!("{s}/")))
    })
}

/// Why writing `p` is more than a workspace write, if it is.
fn write_path_risk(p: &str) -> Option<&'static str> {
    if p.is_empty() {
        return None;
    }
    if p == "/" || p == "~" || p == "$HOME" || p == "${HOME}" {
        return Some("a filesystem or home-directory root");
    }
    if p.split('/').any(|c| c == "..") {
        return Some("a path that climbs out of the workspace");
    }
    if let Some(rest) = under_home(p)
        && rest.starts_with('.')
    {
        return Some("a dotfile or config directory in the home directory");
    }
    if p.starts_with("/dev/") && !matches!(p, "/dev/null" | "/dev/stdout" | "/dev/stderr") {
        return Some("a device");
    }
    const SYSTEM_ROOTS: &[&str] = &[
        "/etc",
        "/usr",
        "/bin",
        "/sbin",
        "/lib",
        "/lib64",
        "/System",
        "/Library",
        "/Applications",
        "/opt",
        "/boot",
        "/root",
        "/proc",
        "/sys",
        "/private/etc",
        "/var/db",
        "/var/lib",
        "/var/log",
        "/var/root",
        "/usr/local",
    ];
    if SYSTEM_ROOTS
        .iter()
        .any(|r| p == *r || p.strip_prefix(r).is_some_and(|x| x.starts_with('/')))
    {
        return Some("a system location");
    }
    None
}

fn scan_secret_paths(args: &[String], acc: &mut Acc) {
    for a in args {
        let v = a.split_once('=').map_or(a.as_str(), |(_, v)| v);
        for cand in [a.as_str(), v] {
            if is_secret_path(cand) {
                acc.raise(
                    EffectClass::SecretAccess,
                    format!("touches a credential store (`{cand}`)"),
                );
            }
        }
    }
}

fn check_write_paths(prog: &str, args: &[String], acc: &mut Acc) {
    let operands: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    let targets: Vec<&&String> = if WRITERS_LAST.contains(&prog) {
        operands.last().into_iter().collect()
    } else {
        operands.iter().collect()
    };
    for t in targets {
        if let Some(why) = write_path_risk(t) {
            acc.outside_workspace = true;
            acc.raise(
                EffectClass::ProtectedWrite,
                format!("`{prog}` writes {why} (`{t}`)"),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// one command
// ---------------------------------------------------------------------------

fn basename(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// The args before a `--` separator.
fn before_dashdash(args: &[String]) -> &[String] {
    match args.iter().position(|a| a == "--") {
        Some(i) => &args[..i],
        None => args,
    }
}

/// Index of the first argument after the leading options of a wrapper.
/// `value_opts` are options that take a separate value.
fn skip_options(args: &[String], value_opts: &[&str]) -> usize {
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            return i + 1;
        }
        if !a.starts_with('-') || a == "-" {
            return i;
        }
        i += if value_opts.contains(&a.as_str()) {
            2
        } else {
            1
        };
    }
    args.len()
}

fn classify_cmd(argv: &[String], dyn_args: bool, stdin: Option<&str>, depth: usize, acc: &mut Acc) {
    if depth > MAX_DEPTH {
        acc.unknown("commands nested too deeply to resolve");
        return;
    }
    let Some(first) = argv.first() else {
        return;
    };
    let rest = &argv[1..];
    let qualified = first.contains('/');
    let mut prog = basename(first).to_ascii_lowercase();
    if let Some(p) = prog.strip_suffix(".exe") {
        prog = p.to_owned();
    }
    let prog = prog.as_str();
    scan_secret_paths(rest, acc);

    // Wrappers: classify what they run.
    match prog {
        "env" => {
            // options, then NAME=VALUE assignments, then the command.
            let mut i = skip_options(
                rest,
                &["-u", "-C", "--unset", "--chdir", "-S", "--split-string"],
            );
            while i < rest.len() && rest[i].contains('=') && !rest[i].starts_with('-') {
                let name = rest[i].split('=').next().unwrap_or_default();
                if dangerous_env(name) {
                    acc.raise(
                        EffectClass::ProtectedWrite,
                        format!("`{name}` changes what programs load or run"),
                    );
                }
                i += 1;
            }
            classify_cmd(&rest[i.min(rest.len())..], dyn_args, stdin, depth + 1, acc);
            return;
        }
        "sudo" | "doas" => {
            acc.raise(
                EffectClass::ProtectedWrite,
                format!("`{prog}` runs with elevated privileges"),
            );
            let i = skip_options(
                rest,
                &[
                    "-u", "-g", "-h", "-p", "-C", "-D", "-r", "-t", "-U", "-R", "-T", "--user",
                    "--group", "--host", "--prompt", "--chdir",
                ],
            );
            let inner = &rest[i.min(rest.len())..];
            if inner.is_empty() {
                acc.unknown("a privileged interactive shell");
            } else if rest[..i].iter().any(|a| a == "-s" || a == "-i") {
                classify_string(&inner.join(" "), depth + 1, acc);
            } else {
                classify_cmd(inner, dyn_args, stdin, depth + 1, acc);
            }
            return;
        }
        "su" => {
            acc.raise(EffectClass::ProtectedWrite, "`su` switches user");
            if let Some(i) = rest.iter().position(|a| a == "-c" || a == "--command") {
                if let Some(s) = rest.get(i + 1) {
                    classify_string(s, depth + 1, acc);
                }
            } else {
                acc.unknown("an interactive shell as another user");
            }
            return;
        }
        "nice" | "ionice" | "nohup" | "setsid" | "caffeinate" | "stdbuf" | "unbuffer" | "time"
        | "chronic" | "command" | "builtin" | "exec" | "arch"
            if !rest.is_empty() =>
        {
            let value_opts: &[&str] = match prog {
                "nice" => &["-n", "--adjustment"],
                "ionice" => &["-c", "-n", "-p", "-P", "-u"],
                "caffeinate" => &["-t", "-w"],
                "stdbuf" => &["-i", "-o", "-e"],
                "time" => &["-f", "-o", "--format", "--output"],
                "exec" => &["-a"],
                "arch" => &["-arch"],
                _ => &[],
            };
            // `command -v x` / `command -V x` only look a name up.
            if prog == "command" && rest.iter().any(|a| a == "-v" || a == "-V") {
                return;
            }
            if prog != "arch" || rest[0].starts_with('-') {
                let i = skip_options(rest, value_opts);
                classify_cmd(&rest[i.min(rest.len())..], dyn_args, stdin, depth + 1, acc);
                return;
            }
        }
        "timeout" if !rest.is_empty() => {
            let i = skip_options(rest, &["-k", "--kill-after", "-s", "--signal"]);
            // the duration, then the command
            let inner = &rest[(i + 1).min(rest.len())..];
            classify_cmd(inner, dyn_args, stdin, depth + 1, acc);
            return;
        }
        "xargs" => {
            let i = skip_options(
                rest,
                &[
                    "-I", "-J", "-L", "-l", "-n", "-P", "-s", "-d", "-E", "-a", "-R", "-S",
                ],
            );
            let inner = &rest[i.min(rest.len())..];
            // The operands come from standard input: unknown targets.
            if inner.is_empty() {
                return;
            }
            classify_cmd(inner, true, None, depth + 1, acc);
            return;
        }
        "find" | "gfind" => {
            classify_find(rest, depth, acc);
            return;
        }
        "eval" => {
            classify_string(&rest.join(" "), depth + 1, acc);
            return;
        }
        "trap" => {
            if let Some(s) = rest.iter().find(|a| !a.starts_with('-')) {
                classify_string(s, depth + 1, acc);
            }
            return;
        }
        "source" | "." => {
            // a script of the project: its writes are the barrier's business
            return;
        }
        _ => {}
    }

    if SHELLS.contains(&prog) {
        classify_shell(prog, rest, stdin, depth, acc);
        return;
    }
    if prog == "git" {
        classify_git(rest, dyn_args, depth, acc);
        return;
    }
    if matches!(prog, "rm" | "rmdir" | "unlink" | "trash" | "trash-put") {
        classify_rm(rest, dyn_args, acc);
        return;
    }
    if prog == "dd" {
        if rest.iter().any(|a| a.starts_with("of=")) {
            acc.raise(EffectClass::Destructive, "`dd of=` overwrites its target");
        }
        return;
    }
    if DESTRUCTIVE.contains(&prog) || prog.starts_with("mkfs.") {
        acc.raise(
            EffectClass::Destructive,
            format!("`{prog}` destroys data irrecoverably"),
        );
        return;
    }
    if SECRET_PROGRAMS.contains(&prog) {
        acc.raise(
            EffectClass::SecretAccess,
            format!("`{prog}` reads or manages credentials"),
        );
        return;
    }
    if NETWORK.contains(&prog) {
        acc.raise(
            EffectClass::ExternalSideEffect,
            format!("`{prog}` reaches the network or an external service"),
        );
        return;
    }
    if SYSTEM.contains(&prog) {
        acc.outside_workspace = true;
        acc.raise(
            EffectClass::ProtectedWrite,
            format!("`{prog}` changes system state outside the workspace"),
        );
        return;
    }
    if prog == "rsync" {
        if rest.iter().any(|a| a.starts_with("--delete")) {
            acc.raise(EffectClass::Destructive, "`rsync --delete` removes files");
        }
        if rest
            .iter()
            .any(|a| !a.starts_with('-') && (a.contains('@') || a.find(':').is_some_and(|i| i > 1)))
        {
            acc.raise(
                EffectClass::ExternalSideEffect,
                "`rsync` copies to or from a remote host",
            );
        }
    }
    if matches!(prog, "sed" | "gsed") && !rest.iter().any(|a| a == "-i" || a.starts_with("-i")) {
        // not an in-place edit: nothing is written
    } else if WRITERS_ALL.contains(&prog) || WRITERS_LAST.contains(&prog) {
        check_write_paths(prog, rest, acc);
    }
    if matches!(prog, "awk" | "gawk" | "mawk") && rest.iter().any(|a| a.contains("system(")) {
        acc.unknown("an awk program that runs commands");
    }
    if prog == "openssl" && rest.iter().any(|a| a == "s_client" || a == "s_server") {
        acc.raise(
            EffectClass::ExternalSideEffect,
            "`openssl s_client` opens a connection",
        );
    }

    if INTERPRETERS.contains(&prog) {
        classify_interpreter(prog, rest, acc);
        return;
    }
    if BUILD.contains(&prog) || prog == "gradlew" || prog == "mvnw" {
        classify_build(prog, rest, acc);
        return;
    }
    if SAFE.contains(&prog) {
        return;
    }
    // Not a program of the tables.
    if qualified {
        let is_relative =
            !first.starts_with('/') && !first.starts_with("../") && !first.starts_with('~');
        if is_relative {
            // `./scripts/test.sh`: project code, like an interpreter on a script.
            return;
        }
    }
    acc.unknown(format!("`{first}` is not a program the classifier knows"));
}

fn classify_rm(args: &[String], dyn_args: bool, acc: &mut Acc) {
    let mut recursive = false;
    let mut operands: Vec<&String> = vec![];
    let mut options_done = false;
    for a in args {
        if !options_done && a == "--" {
            options_done = true;
        } else if !options_done && a.starts_with("--") {
            if matches!(a.as_str(), "--recursive" | "--dir" | "--no-preserve-root") {
                recursive = true;
            }
        } else if !options_done && a.starts_with('-') && a.len() > 1 {
            if a[1..].chars().any(|c| matches!(c, 'r' | 'R' | 'd')) {
                recursive = true;
            }
        } else {
            operands.push(a);
        }
    }
    if recursive {
        acc.raise(EffectClass::Destructive, "recursive removal");
        return;
    }
    if dyn_args {
        acc.raise(
            EffectClass::Destructive,
            "removal of targets named by a glob, a variable or a pipe",
        );
        return;
    }
    for o in operands {
        let risky = o.starts_with('/')
            || o.starts_with('~')
            || o.contains('$')
            || o.split('/').any(|c| c == "..")
            || matches!(o.as_str(), "." | "./" | "");
        if risky {
            acc.raise(
                EffectClass::Destructive,
                format!("removal of `{o}`, a path outside or at the root of the workspace"),
            );
        }
    }
}

fn classify_find(args: &[String], depth: usize, acc: &mut Acc) {
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-delete" => acc.raise(EffectClass::Destructive, "`find -delete` removes files"),
            "-exec" | "-execdir" | "-ok" | "-okdir" => {
                let mut j = i + 1;
                let mut cmd: Vec<String> = vec![];
                while j < args.len() && args[j] != ";" && args[j] != "+" && args[j] != "\\;" {
                    cmd.push(args[j].clone());
                    j += 1;
                }
                // `{}` stands for files the walk finds: unknown targets.
                classify_cmd(&cmd, true, None, depth + 1, acc);
                i = j;
            }
            _ => {}
        }
        i += 1;
    }
}

fn classify_shell(prog: &str, args: &[String], stdin: Option<&str>, depth: usize, acc: &mut Acc) {
    // options with a separate value
    let value_opts = ["-o", "+o", "-O", "+O", "--rcfile", "--init-file"];
    let mut i = 0;
    let mut inline = false;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            i += 1;
            break;
        }
        if !a.starts_with('-') && !a.starts_with('+') {
            break;
        }
        if value_opts.contains(&a.as_str()) {
            i += 2;
            continue;
        }
        if a.starts_with('-') && !a.starts_with("--") && a[1..].contains('c') {
            inline = true;
            i += 1;
            break;
        }
        i += 1;
    }
    if inline {
        // `bash -c <string> [name args…]`
        match args.get(i) {
            Some(s) => classify_string(s, depth + 1, acc),
            None => acc.unknown("`-c` without a command string"),
        }
        return;
    }
    if prog == "busybox" && args.first().is_some_and(|a| !SHELLS.contains(&a.as_str())) {
        classify_cmd(args, false, stdin, depth + 1, acc);
        return;
    }
    if args.get(i).is_some() {
        // a script file of the project
        return;
    }
    // no script: it reads commands from standard input
    match stdin {
        Some(s) => classify_string(s, depth + 1, acc),
        None => acc.unknown("an interactive shell whose commands are not known"),
    }
}

fn classify_interpreter(prog: &str, args: &[String], acc: &mut Acc) {
    let (inline_flags, value_flags): (&[&str], &[&str]) = match prog {
        "python" | "python3" | "py" => (&["-c"], &["-W", "-X", "-Q"]),
        "node" | "nodejs" => (
            &["-e", "--eval", "-p", "--print"],
            &[
                "-r",
                "--require",
                "--import",
                "--loader",
                "--env-file",
                "--inspect-port",
            ],
        ),
        "deno" => (&["eval"], &[]),
        "ruby" => (&["-e"], &["-I", "-r"]),
        "perl" => (&["-e", "-E"], &["-I", "-M"]),
        "php" => (&["-r"], &["-d", "-c"]),
        "lua" | "luajit" => (&["-e"], &["-l"]),
        "rscript" => (&["-e"], &[]),
        "julia" => (&["-e", "--eval", "-E"], &["-p", "-t"]),
        "elixir" => (&["-e"], &["-r", "-pr", "-pa"]),
        _ => (&[], &[]),
    };
    if args.iter().any(|a| {
        matches!(
            a.as_str(),
            "--version" | "-V" | "-version" | "--help" | "-h"
        )
    }) {
        return;
    }
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if inline_flags.contains(&a.as_str()) {
            match args.get(i + 1) {
                Some(code) => scan_inline_code(code, acc),
                None => acc.unknown("inline code flag without code"),
            }
            return;
        }
        if prog == "deno" {
            // deno subcommands that fetch or install
            if matches!(
                a.as_str(),
                "install" | "add" | "upgrade" | "publish" | "deploy"
            ) {
                acc.raise(
                    EffectClass::ExternalSideEffect,
                    format!("`deno {a}` reaches the network"),
                );
                return;
            }
            if matches!(
                a.as_str(),
                "run" | "test" | "task" | "check" | "fmt" | "lint" | "bench"
            ) {
                return;
            }
        }
        if prog.starts_with("python") || prog == "py" {
            if a == "-m" {
                let module = args.get(i + 1).map_or("", String::as_str);
                match module {
                    "pip" | "pipx" | "ensurepip" | "venv" => {
                        let verbs = network_verbs("pip").unwrap_or(&[]);
                        if module == "ensurepip"
                            || args
                                .get(i + 2..)
                                .unwrap_or(&[])
                                .iter()
                                .any(|t| verbs.contains(&t.as_str()))
                        {
                            acc.raise(
                                EffectClass::ExternalSideEffect,
                                format!("`python -m {module}` installs packages from the network"),
                            );
                        }
                    }
                    "http.server" | "smtpd" | "aiosmtpd" | "xmlrpc.server" | "ftplib" => {
                        acc.raise(
                            EffectClass::ExternalSideEffect,
                            format!("`python -m {module}` opens a network service"),
                        );
                    }
                    _ => {}
                }
                return;
            }
            if a == "-" {
                acc.unknown("code read from standard input");
                return;
            }
        }
        if a.starts_with('-') {
            i += if value_flags.contains(&a.as_str()) {
                2
            } else {
                1
            };
            continue;
        }
        // the script file of the project
        return;
    }
    if prog == "node" && args.iter().any(|a| a == "--test") {
        return;
    }
    acc.unknown(format!("`{prog}` with no script: an interactive session"));
}

/// Inline interpreter code is scanned for the effects it names; this is a
/// tripwire for the obvious (`os.system("rm -rf …")`, `requests.post(…)`),
/// not a parser of Python or JavaScript.
fn scan_inline_code(code: &str, acc: &mut Acc) {
    let c = code.to_ascii_lowercase();
    fn first<'a>(c: &str, ms: &[&'a str]) -> Option<&'a str> {
        ms.iter().find(|m| c.contains(**m)).copied()
    }
    let any = |ms: &[&'static str]| first(&c, ms);
    if let Some(m) = any(&[
        "rm -rf",
        "rm -fr",
        "rmtree",
        "rmsync",
        "fs.rm",
        "rimraf",
        "removedirs",
        "os.remove",
        "os.unlink",
        "os.rmdir",
        "fileutils.rm",
        "file.delete",
        "git push --force",
        "git reset --hard",
        "git clean",
    ]) {
        acc.raise_inline(
            EffectClass::Destructive,
            format!("inline code deletes files (`{m}`)"),
        );
    }
    if let Some(m) = any(&[
        "http://",
        "https://",
        "socket",
        "urllib",
        "requests.",
        "fetch(",
        "xmlhttprequest",
        "net.connect",
        "smtplib",
        "ftplib",
        "paramiko",
        "http.client",
        "aiohttp",
        "httpx",
        "curl ",
        "wget ",
        "net/http",
        "open-uri",
        "lwp",
        "git push",
        "ssh ",
    ]) {
        acc.raise_inline(
            EffectClass::ExternalSideEffect,
            format!("inline code reaches the network (`{m}`)"),
        );
    }
    if let Some(m) = any(&[
        "os.system",
        "subprocess",
        "child_process",
        "popen",
        "system(",
        "exec(",
        "execsync",
        "spawn(",
        "`",
        "shell_exec",
        "proc_open",
        "passthru",
        "eval(",
    ]) {
        acc.unknown(format!("inline code runs other programs (`{m}`)"));
    }
}

fn classify_build(prog: &str, args: &[String], acc: &mut Acc) {
    let head = before_dashdash(args);
    // Tools whose first word is the sub-command (`cargo +nightly test`,
    // `dotnet add package`): only that word names the verb, so a test
    // filter called `login` is not `cargo login`.
    let by_sub = matches!(
        prog,
        "cargo"
            | "go"
            | "dotnet"
            | "gem"
            | "bundle"
            | "composer"
            | "mix"
            | "cabal"
            | "stack"
            | "rustup"
            | "opam"
            | "playwright"
            | "cypress"
    );
    if let Some(verbs) = network_verbs(prog) {
        let words: Vec<&str> = head
            .iter()
            .map(String::as_str)
            .filter(|a| !a.starts_with('-') && !(prog == "cargo" && a.starts_with('+')))
            .collect();
        let mut hit: Option<&str> = if by_sub {
            words.first().copied().filter(|w| verbs.contains(w))
        } else {
            head.iter().map(String::as_str).find(|a| verbs.contains(a))
        };
        // `go get` / `go install`, and `go mod download|tidy|vendor`,
        // `go work sync`: but not `go test ./tidy`.
        if prog == "go" {
            hit = match words.as_slice() {
                [first @ ("get" | "install"), ..] => Some(*first),
                [
                    "mod" | "work",
                    second @ ("download" | "tidy" | "vendor" | "sync"),
                    ..,
                ] => Some(*second),
                _ => None,
            };
        }
        // `pnpm exec`/`yarn exec`/`bun x` run a local binary: classify it.
        if matches!(prog, "pnpm" | "yarn" | "bun")
            && let Some(i) = head.iter().position(|a| a == "exec")
        {
            classify_cmd(&args[i + 1..], false, None, 1, acc);
            return;
        }
        if prog == "npm" && head.iter().any(|a| a == "exec") {
            hit = Some("exec");
        }
        if hit.is_none() && prog == "yarn" && head.iter().all(|a| a.starts_with('-')) {
            hit = Some("(install)");
        }
        if let Some(h) = hit {
            acc.raise(
                EffectClass::ExternalSideEffect,
                format!("`{prog} {h}` fetches, installs or publishes through the network"),
            );
            return;
        }
    }
    // Script runners: a target named like a delivery (`make deploy`,
    // `npm run publish:prod`). Compiled test runners are not scanned: a test
    // filter named `login` is not a delivery.
    let target: Option<&String> = match prog {
        "make" | "gmake" | "just" | "task" | "rake" | "gradle" | "gradlew" | "mvn" | "mvnw"
        | "ant" | "sbt" => head.iter().find(|a| names_delivery(a)),
        "npm" | "pnpm" | "yarn" | "bun" => {
            let mut non_flags = head.iter().filter(|a| !a.starts_with('-'));
            match non_flags.next().map(String::as_str) {
                Some("run" | "run-script" | "run-s" | "run-p") => non_flags.next(),
                Some(_) if prog != "npm" => head.iter().find(|a| !a.starts_with('-')),
                _ => None,
            }
            .filter(|t| names_delivery(t))
        }
        _ => None,
    };
    if let Some(t) = target {
        acc.raise(
            EffectClass::ExternalSideEffect,
            format!("`{prog} {t}` names a delivery to an external service"),
        );
    }
}

/// A script or target named for a delivery: `deploy`, `publish:prod`,
/// `release-notes`.
fn names_delivery(t: &str) -> bool {
    let t = t.to_ascii_lowercase();
    DEPLOY_WORDS.iter().any(|w| {
        t == *w
            || t.strip_prefix(w)
                .is_some_and(|r| r.starts_with([':', '-', '_', '.']))
    })
}

// ---------------------------------------------------------------------------
// git
// ---------------------------------------------------------------------------

fn short_has(a: &str, letters: &str) -> bool {
    a.starts_with('-') && !a.starts_with("--") && a[1..].chars().any(|c| letters.contains(c))
}

fn classify_git(args: &[String], dyn_args: bool, depth: usize, acc: &mut Acc) {
    // global options
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "-C" | "--git-dir" | "--work-tree" | "--namespace" | "--super-prefix"
            | "--config-env" => i += 2,
            "-c" => {
                if let Some(kv) = args.get(i + 1) {
                    let key = kv
                        .split('=')
                        .next()
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                    const RISKY: &[&str] = &[
                        "core.",
                        "alias.",
                        "credential.",
                        "diff.",
                        "merge.",
                        "filter.",
                        "protocol.",
                        "url.",
                        "sequence.",
                        "gpg.",
                        "uploadpack.",
                        "receive.",
                        "http.",
                        "ssh.",
                    ];
                    if RISKY.iter().any(|p| key.starts_with(p)) {
                        acc.protected_path = true;
                        acc.raise(
                            EffectClass::ProtectedWrite,
                            format!("`git -c {key}` can make git run arbitrary programs"),
                        );
                    }
                }
                i += 2;
            }
            _ if a.starts_with('-') => i += 1,
            _ => break,
        }
    }
    let Some(sub) = args.get(i).map(String::as_str) else {
        return;
    };
    let rest = &args[(i + 1).min(args.len())..];
    let flags: Vec<&str> = before_dashdash(rest).iter().map(String::as_str).collect();
    let has = |names: &[&str]| flags.iter().any(|f| names.contains(f));
    let has_prefix = |p: &str| flags.iter().any(|f| f.starts_with(p));
    let sub2 = flags
        .iter()
        .find(|f| !f.starts_with('-'))
        .copied()
        .unwrap_or("");

    match sub {
        "push" => {
            let forced = has(&[
                "--force",
                "-f",
                "--force-if-includes",
                "--mirror",
                "--delete",
                "--prune",
                "-d",
            ]) || has_prefix("--force-with-lease")
                || flags.iter().any(|f| short_has(f, "fd"))
                || rest.iter().any(|a| {
                    (a.starts_with('+') && a.len() > 1) || (a.starts_with(':') && a.len() > 1)
                })
                || dyn_args;
            if forced {
                acc.raise(
                    EffectClass::Destructive,
                    "`git push` that forces, deletes or may force remote refs",
                );
            } else {
                acc.raise(
                    EffectClass::ExternalSideEffect,
                    "`git push` publishes to a remote",
                );
            }
        }
        "fetch" | "pull" | "clone" | "ls-remote" | "lfs" | "svn" | "cvs" | "p4" | "send-email"
        | "daemon" | "http-fetch" | "http-push" | "fetch-pack" | "send-pack" | "imap-send" => {
            acc.raise(
                EffectClass::ExternalSideEffect,
                format!("`git {sub}` talks to a remote"),
            );
        }
        "archive" if has_prefix("--remote") => {
            acc.raise(
                EffectClass::ExternalSideEffect,
                "`git archive --remote` talks to a remote",
            );
        }
        "remote" => match sub2 {
            "" | "get-url" => {}
            "show" | "update" | "prune" => acc.raise(
                EffectClass::ExternalSideEffect,
                format!("`git remote {sub2}` talks to a remote"),
            ),
            _ if flags.first().is_some_and(|f| *f == "-v") => {}
            _ => acc.raise(
                EffectClass::ProtectedWrite,
                format!("`git remote {sub2}` rewrites the repository configuration"),
            ),
        },
        "submodule" => {
            if !matches!(sub2, "status" | "summary" | "") {
                acc.raise(
                    EffectClass::ExternalSideEffect,
                    format!("`git submodule {sub2}` fetches or runs commands in submodules"),
                );
            }
        }
        "credential" => acc.raise(
            EffectClass::SecretAccess,
            "`git credential` reads stored credentials",
        ),
        s if s.starts_with("credential-") => {
            acc.raise(
                EffectClass::SecretAccess,
                "a git credential helper reads stored credentials",
            );
        }
        "reset" => {
            if has(&["--hard", "--merge", "--keep"]) {
                acc.raise(
                    EffectClass::Destructive,
                    "`git reset --hard` discards uncommitted work",
                );
            }
        }
        "clean" => {
            if !(has(&["-n", "--dry-run"]) || flags.iter().any(|f| short_has(f, "n"))) {
                acc.raise(
                    EffectClass::Destructive,
                    "`git clean` deletes untracked files",
                );
            }
        }
        "checkout" => {
            if rest.iter().any(|a| {
                a == "--"
                    || a == "."
                    || a == "-f"
                    || a == "--force"
                    || a == "--ours"
                    || a == "--theirs"
            }) {
                acc.raise(
                    EffectClass::Destructive,
                    "`git checkout` that overwrites files of the working tree",
                );
            }
        }
        "switch" => {
            if has(&["-f", "--force", "--discard-changes"]) {
                acc.raise(
                    EffectClass::Destructive,
                    "`git switch --discard-changes` discards work",
                );
            }
        }
        "restore" => {
            let staged_only = has(&["--staged", "-S"]) && !has(&["--worktree", "-W"]);
            if !staged_only {
                acc.raise(
                    EffectClass::Destructive,
                    "`git restore` overwrites files of the working tree",
                );
            }
        }
        "stash" => {
            if matches!(sub2, "drop" | "clear") {
                acc.raise(
                    EffectClass::Destructive,
                    format!("`git stash {sub2}` discards stashed work"),
                );
            }
        }
        "branch" => {
            if has(&["-D", "-d", "--delete", "-f", "--force"])
                || flags.iter().any(|f| short_has(f, "dDf"))
            {
                acc.raise(
                    EffectClass::Destructive,
                    "`git branch` that deletes or resets a branch",
                );
            } else if has(&["-u", "--set-upstream-to", "--unset-upstream"])
                || has_prefix("--set-upstream-to")
            {
                acc.protected_path = true;
                acc.raise(
                    EffectClass::ProtectedWrite,
                    "`git branch -u` writes the repository configuration",
                );
            }
        }
        "tag" => {
            if has(&["-d", "--delete", "-f", "--force"]) {
                acc.raise(
                    EffectClass::Destructive,
                    "`git tag` that deletes or moves a tag",
                );
            }
        }
        "rebase" => acc.raise(EffectClass::ProtectedWrite, "`git rebase` rewrites history"),
        "filter-branch" | "filter-repo" | "replace" | "prune" => {
            acc.raise(
                EffectClass::Destructive,
                format!("`git {sub}` rewrites or deletes history"),
            );
        }
        "reflog" => {
            if matches!(sub2, "expire" | "delete") {
                acc.raise(
                    EffectClass::Destructive,
                    "`git reflog` expiry deletes recovery points",
                );
            }
        }
        "gc" | "repack" | "maintenance" => {
            let prune = has_prefix("--prune") || has(&["-d", "-a", "--cruft"]);
            acc.raise(
                if prune {
                    EffectClass::Destructive
                } else {
                    EffectClass::ProtectedWrite
                },
                format!("`git {sub}` rewrites the object store"),
            );
        }
        "update-ref" => {
            acc.raise(
                if has(&["-d"]) {
                    EffectClass::Destructive
                } else {
                    EffectClass::ProtectedWrite
                },
                "`git update-ref` writes refs directly",
            );
        }
        "worktree" => {
            if matches!(sub2, "remove" | "prune") && has(&["-f", "--force"]) {
                acc.raise(
                    EffectClass::Destructive,
                    "`git worktree remove --force` deletes a worktree",
                );
            }
        }
        "config" => {
            let read = has(&[
                "--get",
                "--get-all",
                "--get-regexp",
                "--list",
                "-l",
                "--show-origin",
                "--get-urlmatch",
                "--show-scope",
                "--name-only",
            ]);
            if !read || has(&["--global", "--system"]) {
                acc.protected_path = true;
                acc.raise(
                    EffectClass::ProtectedWrite,
                    "`git config` writes configuration, which can name programs git runs",
                );
            }
        }
        "rm" => {
            if has(&["-f", "--force"]) {
                acc.raise(
                    EffectClass::Destructive,
                    "`git rm --force` deletes local modifications",
                );
            }
        }
        "bisect" => {
            if let Some(p) = rest.iter().position(|a| a == "run") {
                classify_cmd(&rest[p + 1..], false, None, depth + 1, acc);
            }
        }
        "add" | "commit" | "mv" | "merge" | "cherry-pick" | "revert" | "am" | "apply"
        | "status" | "diff" | "log" | "show" | "blame" | "ls-files" | "ls-tree" | "rev-parse"
        | "rev-list" | "describe" | "shortlog" | "cat-file" | "grep" | "format-patch"
        | "diff-tree" | "diff-index" | "merge-base" | "for-each-ref" | "symbolic-ref"
        | "show-ref" | "name-rev" | "count-objects" | "fsck" | "verify-commit" | "verify-tag"
        | "check-ignore" | "check-attr" | "notes" | "init" | "help" | "version" | "whatchanged"
        | "range-diff" | "show-branch" | "stage" | "hash-object" | "mktree" | "write-tree"
        | "commit-tree" | "read-tree" | "sparse-checkout" | "bundle" | "ls-remote-local"
        | "diff-files" | "apply-mailbox" | "var" | "annotate" | "cherry" | "difftool-none"
        | "get-tar-commit-id" => {}
        other => acc.unknown(format!(
            "`git {other}` is not a sub-command the classifier knows (an alias runs anything)"
        )),
    }
}

// ---------------------------------------------------------------------------
// shell strings
// ---------------------------------------------------------------------------

/// A word of a shell string.
struct Word {
    text: String,
    /// Built from a variable, a substitution or a glob.
    dynamic: bool,
}

struct Cmd {
    words: Vec<Word>,
    here_string: Option<String>,
    heredoc: Option<String>,
}

fn classify_string(src: &str, depth: usize, acc: &mut Acc) {
    if depth > MAX_DEPTH {
        acc.unknown("shell strings nested too deeply to resolve");
        return;
    }
    let mut p = Parser {
        c: src.chars().collect(),
        i: 0,
        depth,
        pending_heredocs: vec![],
    };
    let mut cmds: Vec<Cmd> = vec![];
    let mut cur = Cmd {
        words: vec![],
        here_string: None,
        heredoc: None,
    };
    let mut redirect_risks: Vec<(String, bool)> = vec![];
    loop {
        p.skip_blanks();
        let Some(ch) = p.peek() else { break };
        match ch {
            '\n' => {
                p.i += 1;
                p.consume_heredocs(&mut cur);
                end_cmd(&mut cur, &mut cmds);
            }
            ';' | '&' | '|' | '(' | ')' => {
                // `name() {` — a function definition, not a command
                if ch == '(' && cur.words.len() == 1 && p.next_non_blank_is_close_paren() {
                    p.skip_to_close_paren();
                    cur.words.clear();
                    continue;
                }
                p.i += 1;
                // `&&`, `||`, `|&`, `;;`, `&>` handled as one separator
                if ch == '&' && p.peek() == Some('>') {
                    // `&> file` is a redirect, not a separator
                    p.i += 1;
                    if p.peek() == Some('>') {
                        p.i += 1;
                    }
                    p.redirect_target(&mut redirect_risks, acc);
                    continue;
                }
                if matches!(p.peek(), Some('&' | '|' | ';')) && ch != '(' && ch != ')' {
                    p.i += 1;
                }
                end_cmd(&mut cur, &mut cmds);
            }
            '<' | '>' => {
                // process substitution `<(cmd)` / `>(cmd)` is a word
                if p.c.get(p.i + 1) == Some(&'(') {
                    let w = p.read_word(acc);
                    cur.words.push(w);
                } else {
                    p.redirect(&mut redirect_risks, &mut cur, acc);
                }
            }
            '#' if cur.words.is_empty() || p.prev_is_blank() => {
                while p.peek().is_some_and(|c| c != '\n') {
                    p.i += 1;
                }
            }
            _ => {
                let w = p.read_word(acc);
                // `2>file`: a number glued to a redirection is a descriptor
                if w.text.chars().all(|c| c.is_ascii_digit())
                    && !w.text.is_empty()
                    && matches!(p.peek(), Some('<' | '>'))
                {
                    continue;
                }
                cur.words.push(w);
            }
        }
    }
    end_cmd(&mut cur, &mut cmds);
    for (target, dynamic) in redirect_risks {
        if dynamic {
            acc.unknown("a redirection to a path built from a variable");
        } else if let Some(why) = write_path_risk(&target) {
            acc.outside_workspace = true;
            acc.raise(
                EffectClass::ProtectedWrite,
                format!("a redirection writes {why} (`{target}`)"),
            );
        }
        if is_secret_path(&target) {
            acc.raise(
                EffectClass::SecretAccess,
                format!("touches a credential store (`{target}`)"),
            );
        }
    }
    for cmd in cmds {
        classify_simple(cmd, depth, acc);
    }
}

fn end_cmd(cur: &mut Cmd, cmds: &mut Vec<Cmd>) {
    if !cur.words.is_empty() {
        cmds.push(Cmd {
            words: std::mem::take(&mut cur.words),
            here_string: cur.here_string.take(),
            heredoc: cur.heredoc.take(),
        });
    } else {
        cur.here_string = None;
        cur.heredoc = None;
    }
}

const RESERVED: &[&str] = &[
    "if", "then", "elif", "else", "fi", "while", "until", "do", "done", "!", "{", "}", "time",
    "coproc",
];

fn is_assignment(w: &str) -> bool {
    let Some((name, _)) = w.split_once('=') else {
        return false;
    };
    let name = name.strip_suffix('+').unwrap_or(name);
    !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.chars().next().is_some_and(|c| c.is_ascii_digit())
}

fn classify_simple(cmd: Cmd, depth: usize, acc: &mut Acc) {
    let mut words = cmd.words;
    // reserved words and leading assignments
    loop {
        match words.first() {
            Some(w) if !w.dynamic && RESERVED.contains(&w.text.as_str()) => {
                words.remove(0);
            }
            Some(w) if is_assignment(&w.text) => {
                if let Some((name, _)) = w.text.split_once('=')
                    && dangerous_env(name)
                {
                    acc.raise(
                        EffectClass::ProtectedWrite,
                        format!("`{name}` changes what programs load or run"),
                    );
                }
                words.remove(0);
            }
            _ => break,
        }
    }
    let Some(head) = words.first() else { return };
    if !head.dynamic && matches!(head.text.as_str(), "for" | "select" | "function") {
        // the list words are data; substitutions in them were already parsed
        return;
    }
    if !head.dynamic && head.text == "case" {
        acc.unknown("a `case` statement");
        return;
    }
    if !head.dynamic && matches!(head.text.as_str(), "in" | "esac") {
        return;
    }
    if head.dynamic {
        acc.unknown("the command word is built from a variable or a substitution");
        return;
    }
    let argv: Vec<String> = words.iter().map(|w| w.text.clone()).collect();
    let dyn_args = words.iter().skip(1).any(|w| w.dynamic);
    // a here-document / here-string is standard input
    let stdin = cmd.heredoc.as_deref().or(cmd.here_string.as_deref());
    classify_cmd(&argv, dyn_args, stdin, depth + 1, acc);
}

struct Parser {
    c: Vec<char>,
    i: usize,
    depth: usize,
    pending_heredocs: Vec<(String, bool)>,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }

    fn prev_is_blank(&self) -> bool {
        self.i == 0 || self.c[self.i - 1].is_whitespace()
    }

    fn skip_blanks(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' || c == '\r' {
                self.i += 1;
            } else if c == '\\' && self.c.get(self.i + 1) == Some(&'\n') {
                self.i += 2;
            } else {
                break;
            }
        }
    }

    fn next_non_blank_is_close_paren(&self) -> bool {
        let mut j = self.i + 1;
        while self.c.get(j).is_some_and(|c| *c == ' ' || *c == '\t') {
            j += 1;
        }
        self.c.get(j) == Some(&')')
    }

    fn skip_to_close_paren(&mut self) {
        while let Some(c) = self.peek() {
            self.i += 1;
            if c == ')' {
                break;
            }
        }
    }

    /// Read to the `)` closing a `$(`, honouring nesting and quotes; the
    /// opening `(` is already consumed.
    fn read_balanced(&mut self, open: char, close: char) -> Option<String> {
        let mut depth = 1;
        let start = self.i;
        let mut quote: Option<char> = None;
        while let Some(c) = self.peek() {
            self.i += 1;
            match quote {
                Some(q) => {
                    if c == '\\' && q == '"' {
                        self.i += 1;
                    } else if c == q {
                        quote = None;
                    }
                }
                None => {
                    if c == '\\' {
                        self.i += 1;
                    } else if c == '\'' || c == '"' {
                        quote = Some(c);
                    } else if c == open {
                        depth += 1;
                    } else if c == close {
                        depth -= 1;
                        if depth == 0 {
                            return Some(self.c[start..self.i - 1].iter().collect());
                        }
                    }
                }
            }
        }
        None
    }

    /// Read a word, resolving quotes; substitutions are classified in place.
    fn read_word(&mut self, acc: &mut Acc) -> Word {
        let mut text = String::new();
        let mut dynamic = false;
        // process substitution
        if matches!(self.peek(), Some('<' | '>')) && self.c.get(self.i + 1) == Some(&'(') {
            self.i += 2;
            match self.read_balanced('(', ')') {
                Some(inner) => classify_string(&inner, self.depth + 1, acc),
                None => acc.unknown("unterminated process substitution"),
            }
            return Word {
                text: "<(...)".into(),
                dynamic: true,
            };
        }
        while let Some(c) = self.peek() {
            match c {
                ' ' | '\t' | '\n' | '\r' | ';' | '&' | '|' | '(' | ')' | '<' | '>' => break,
                '\\' => {
                    self.i += 1;
                    if let Some(n) = self.peek() {
                        if n != '\n' {
                            text.push(n);
                        }
                        self.i += 1;
                    }
                }
                '\'' => {
                    self.i += 1;
                    loop {
                        match self.peek() {
                            Some('\'') => {
                                self.i += 1;
                                break;
                            }
                            Some(ch) => {
                                text.push(ch);
                                self.i += 1;
                            }
                            None => {
                                acc.unknown("unterminated single quote");
                                return Word {
                                    text,
                                    dynamic: true,
                                };
                            }
                        }
                    }
                }
                '"' => {
                    self.i += 1;
                    loop {
                        match self.peek() {
                            Some('"') => {
                                self.i += 1;
                                break;
                            }
                            Some('\\') => {
                                self.i += 1;
                                if let Some(n) = self.peek() {
                                    if !matches!(n, '"' | '\\' | '$' | '`' | '\n') {
                                        text.push('\\');
                                    }
                                    if n != '\n' {
                                        text.push(n);
                                    }
                                    self.i += 1;
                                }
                            }
                            Some('$') | Some('`') => {
                                if self.expansion(&mut text, acc) {
                                    dynamic = true;
                                }
                            }
                            Some(ch) => {
                                text.push(ch);
                                self.i += 1;
                            }
                            None => {
                                acc.unknown("unterminated double quote");
                                return Word {
                                    text,
                                    dynamic: true,
                                };
                            }
                        }
                    }
                }
                '$' | '`' => {
                    if self.expansion(&mut text, acc) {
                        dynamic = true;
                    }
                }
                '*' | '?' => {
                    dynamic = true;
                    text.push(c);
                    self.i += 1;
                }
                _ => {
                    text.push(c);
                    self.i += 1;
                }
            }
        }
        Word { text, dynamic }
    }

    /// At a `$` or backtick: consume the expansion. Returns whether the
    /// word became dynamic (it always does unless it is a literal `$`).
    fn expansion(&mut self, text: &mut String, acc: &mut Acc) -> bool {
        let c = self.peek();
        if c == Some('`') {
            self.i += 1;
            let mut inner = String::new();
            loop {
                match self.peek() {
                    Some('`') => {
                        self.i += 1;
                        break;
                    }
                    Some('\\') => {
                        self.i += 1;
                        if let Some(n) = self.peek() {
                            inner.push(n);
                            self.i += 1;
                        }
                    }
                    Some(ch) => {
                        inner.push(ch);
                        self.i += 1;
                    }
                    None => {
                        acc.unknown("unterminated backtick substitution");
                        return true;
                    }
                }
            }
            classify_string(&inner, self.depth + 1, acc);
            return true;
        }
        // `$`
        self.i += 1;
        match self.peek() {
            Some('(') => {
                self.i += 1;
                if self.peek() == Some('(') {
                    // arithmetic `$(( … ))`
                    self.i += 1;
                    if self.read_balanced('(', ')').is_none() {
                        acc.unknown("unterminated arithmetic expansion");
                    } else if self.peek() == Some(')') {
                        self.i += 1;
                    }
                    text.push('0');
                    return false;
                }
                match self.read_balanced('(', ')') {
                    Some(inner) => classify_string(&inner, self.depth + 1, acc),
                    None => acc.unknown("unterminated command substitution"),
                }
                true
            }
            Some('{') => {
                self.i += 1;
                if self.read_balanced('{', '}').is_none() {
                    acc.unknown("unterminated parameter expansion");
                }
                true
            }
            Some(ch)
                if ch.is_ascii_alphanumeric()
                    || ch == '_'
                    || matches!(ch, '@' | '*' | '#' | '?' | '$' | '!' | '-') =>
            {
                self.i += 1;
                if ch.is_ascii_alphabetic() || ch == '_' {
                    while self
                        .peek()
                        .is_some_and(|x| x.is_ascii_alphanumeric() || x == '_')
                    {
                        self.i += 1;
                    }
                }
                true
            }
            _ => {
                text.push('$');
                false
            }
        }
    }

    /// At `<` or `>`.
    fn redirect(&mut self, risks: &mut Vec<(String, bool)>, cur: &mut Cmd, acc: &mut Acc) {
        let ch = self.peek().unwrap_or('>');
        self.i += 1;
        if ch == '>' {
            if matches!(self.peek(), Some('>' | '|')) {
                self.i += 1;
            } else if self.peek() == Some('&') {
                // `>&2`, `>&-`: a descriptor, not a file
                self.i += 1;
                self.skip_blanks();
                let _ = self.read_word(acc);
                return;
            }
            self.redirect_target(risks, acc);
        } else if self.peek() == Some('<') {
            self.i += 1;
            if self.peek() == Some('<') {
                // here-string
                self.i += 1;
                self.skip_blanks();
                let w = self.read_word(acc);
                cur.here_string = Some(w.text);
                return;
            }
            let strip_tabs = if self.peek() == Some('-') {
                self.i += 1;
                true
            } else {
                false
            };
            self.skip_blanks();
            let w = self.read_word(acc);
            self.pending_heredocs.push((w.text, strip_tabs));
        } else {
            if self.peek() == Some('&') {
                self.i += 1;
            }
            self.skip_blanks();
            let _ = self.read_word(acc);
        }
    }

    fn redirect_target(&mut self, risks: &mut Vec<(String, bool)>, acc: &mut Acc) {
        self.skip_blanks();
        let w = self.read_word(acc);
        if matches!(
            w.text.as_str(),
            "/dev/null" | "/dev/stdout" | "/dev/stderr" | "/dev/tty" | ""
        ) || w.text.starts_with("/dev/fd/")
        {
            return;
        }
        risks.push((w.text, w.dynamic));
    }

    /// At the newline ending a command line: take the bodies of the
    /// here-documents opened on it. The body of a here-document handed to a
    /// shell is that shell's script.
    fn consume_heredocs(&mut self, cur: &mut Cmd) {
        let pending = std::mem::take(&mut self.pending_heredocs);
        for (delim, strip) in pending {
            let mut body = String::new();
            loop {
                let mut line = String::new();
                if self.peek().is_none() {
                    break;
                }
                while let Some(c) = self.peek() {
                    self.i += 1;
                    if c == '\n' {
                        break;
                    }
                    line.push(c);
                }
                let cmp = if strip {
                    line.trim_start_matches('\t')
                } else {
                    line.as_str()
                };
                if cmp == delim {
                    break;
                }
                body.push_str(&line);
                body.push('\n');
            }
            cur.heredoc = Some(body);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use EffectClass::{
        Destructive, ExternalSideEffect, ProtectedWrite, ReversibleWrite, SecretAccess,
    };

    fn argv(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_owned()).collect()
    }

    fn sh(script: &str) -> ShellEffect {
        classify_argv(&argv(&["sh", "-c", script]), None)
    }

    /// (command, class, unclassified)
    #[test]
    fn the_table_maps_argv_to_the_lattice() {
        let cases: &[(&[&str], EffectClass, bool)] = &[
            // reads and ordinary work stay plain workspace writes
            (&["git", "status", "--porcelain"], ReversibleWrite, false),
            (&["git", "diff", "HEAD~1"], ReversibleWrite, false),
            (&["git", "log", "--oneline"], ReversibleWrite, false),
            (&["git", "commit", "-m", "x"], ReversibleWrite, false),
            (
                &["git", "commit", "--amend", "-m", "x"],
                ReversibleWrite,
                false,
            ),
            (&["git", "checkout", "-b", "topic"], ReversibleWrite, false),
            (&["git", "-C", "sub", "status"], ReversibleWrite, false),
            (
                &["git", "config", "--get", "user.name"],
                ReversibleWrite,
                false,
            ),
            (
                &["git", "restore", "--staged", "a.rs"],
                ReversibleWrite,
                false,
            ),
            (
                &["git", "stash", "push", "-m", "wip"],
                ReversibleWrite,
                false,
            ),
            (&["ls", "-la"], ReversibleWrite, false),
            (&["cat", "README.md"], ReversibleWrite, false),
            (&["grep", "-rn", "x", "src"], ReversibleWrite, false),
            (&["cargo", "test", "-p", "x"], ReversibleWrite, false),
            (&["cargo", "test", "login"], ReversibleWrite, false),
            (&["cargo", "build", "--release"], ReversibleWrite, false),
            (
                &["cargo", "clippy", "--all-targets", "--", "-D", "warnings"],
                ReversibleWrite,
                false,
            ),
            (&["cargo", "+nightly", "fmt"], ReversibleWrite, false),
            (&["npm", "test"], ReversibleWrite, false),
            (&["npm", "run", "build"], ReversibleWrite, false),
            (&["npm", "test", "--", "release"], ReversibleWrite, false),
            (&["pnpm", "-r", "test"], ReversibleWrite, false),
            (&["pnpm", "exec", "vitest", "run"], ReversibleWrite, false),
            (&["yarn", "build"], ReversibleWrite, false),
            (
                &["pytest", "-q", "tests/test_upload.py"],
                ReversibleWrite,
                false,
            ),
            (&["python3", "-m", "pytest"], ReversibleWrite, false),
            (&["python3", "--version"], ReversibleWrite, false),
            (&["python3", "script.py"], ReversibleWrite, false),
            (&["python3", "-c", "print(1+1)"], ReversibleWrite, false),
            (
                &["node", "-e", "process.stdout.write('hi')"],
                ReversibleWrite,
                false,
            ),
            (&["go", "test", "./..."], ReversibleWrite, false),
            (&["make", "test"], ReversibleWrite, false),
            (&["./gradlew", "test"], ReversibleWrite, false),
            (&["./scripts/check.sh"], ReversibleWrite, false),
            (&["sh", "check.sh"], ReversibleWrite, false),
            (&["touch", "a.txt"], ReversibleWrite, false),
            (&["cp", "a", "b"], ReversibleWrite, false),
            (&["cp", "/etc/hosts", "local-copy"], ReversibleWrite, false),
            (&["rm", "build.log"], ReversibleWrite, false),
            (&["sleep", "1"], ReversibleWrite, false),
            // destructive
            (&["rm", "-rf", "dist"], Destructive, false),
            (&["rm", "-fr", "/tmp/x"], Destructive, false),
            (&["rm", "-r", "x"], Destructive, false),
            (&["rm", "/abs/path"], Destructive, false),
            (&["rm", "../sibling"], Destructive, false),
            (&["rmdir", "-p", "a/b"], ReversibleWrite, false),
            (&["rmdir", "/var/x"], Destructive, false),
            (&["shred", "f"], Destructive, false),
            (&["dd", "if=/dev/zero", "of=/dev/disk2"], Destructive, false),
            (
                &["find", ".", "-name", "*.o", "-delete"],
                Destructive,
                false,
            ),
            (
                &["find", ".", "-exec", "rm", "-f", "{}", ";"],
                Destructive,
                false,
            ),
            (
                &["find", ".", "-exec", "echo", "{}", "+"],
                ReversibleWrite,
                false,
            ),
            (&["xargs", "rm"], Destructive, false),
            (
                &["git", "push", "--force", "origin", "main"],
                Destructive,
                false,
            ),
            (&["git", "push", "-f"], Destructive, false),
            (&["git", "push", "origin", "+main"], Destructive, false),
            (
                &["git", "push", "origin", ":old-branch"],
                Destructive,
                false,
            ),
            (&["git", "push", "--force-with-lease"], Destructive, false),
            (
                &["git", "push", "--delete", "origin", "b"],
                Destructive,
                false,
            ),
            (&["git", "reset", "--hard", "HEAD~1"], Destructive, false),
            (&["git", "clean", "-fdx"], Destructive, false),
            (&["git", "clean", "-n"], ReversibleWrite, false),
            (&["git", "checkout", "--", "."], Destructive, false),
            (&["git", "checkout", "."], Destructive, false),
            (&["git", "restore", "src/a.rs"], Destructive, false),
            (&["git", "branch", "-D", "x"], Destructive, false),
            (&["git", "stash", "drop"], Destructive, false),
            (&["git", "reflog", "expire", "--all"], Destructive, false),
            (&["git", "filter-branch", "--all"], Destructive, false),
            // external
            (
                &["git", "push", "origin", "main"],
                ExternalSideEffect,
                false,
            ),
            (
                &["git", "push", "-u", "origin", "topic"],
                ExternalSideEffect,
                false,
            ),
            (&["git", "fetch", "origin"], ExternalSideEffect, false),
            (
                &["git", "clone", "https://x/y.git"],
                ExternalSideEffect,
                false,
            ),
            (&["git", "pull"], ExternalSideEffect, false),
            (
                &["curl", "-X", "POST", "https://x"],
                ExternalSideEffect,
                false,
            ),
            (&["wget", "https://x"], ExternalSideEffect, false),
            (&["ssh", "host", "ls"], ExternalSideEffect, false),
            (&["scp", "a", "h:b"], ExternalSideEffect, false),
            (&["gh", "pr", "create"], ExternalSideEffect, false),
            (&["docker", "push", "x"], ExternalSideEffect, false),
            (&["npm", "install"], ExternalSideEffect, false),
            (&["npm", "i", "left-pad"], ExternalSideEffect, false),
            (&["npm", "publish"], ExternalSideEffect, false),
            (&["npm", "run", "deploy"], ExternalSideEffect, false),
            (&["npx", "create-react-app", "x"], ExternalSideEffect, false),
            (&["pnpm", "add", "x"], ExternalSideEffect, false),
            (&["yarn"], ExternalSideEffect, false),
            (&["yarn", "add", "x"], ExternalSideEffect, false),
            (&["pip", "install", "x"], ExternalSideEffect, false),
            (
                &["python3", "-m", "pip", "install", "x"],
                ExternalSideEffect,
                false,
            ),
            (&["uv", "add", "x"], ExternalSideEffect, false),
            (&["cargo", "install", "ripgrep"], ExternalSideEffect, false),
            (&["cargo", "publish"], ExternalSideEffect, false),
            (&["go", "get", "x"], ExternalSideEffect, false),
            (&["go", "mod", "tidy"], ExternalSideEffect, false),
            (&["brew", "install", "x"], ExternalSideEffect, false),
            (&["apt-get", "install", "x"], ExternalSideEffect, false),
            (&["make", "deploy"], ExternalSideEffect, false),
            (
                &["python3", "-c", "import requests; requests.post('x')"],
                ExternalSideEffect,
                false,
            ),
            (
                &["node", "-e", "fetch('http://x')"],
                ExternalSideEffect,
                false,
            ),
            (&["python3", "-m", "http.server"], ExternalSideEffect, false),
            // system, privilege, secrets
            (&["sudo", "ls"], ProtectedWrite, false),
            (&["sudo", "rm", "-rf", "/x"], Destructive, false),
            (&["chown", "-R", "root", "."], ProtectedWrite, false),
            (&["launchctl", "unload", "x"], ProtectedWrite, false),
            (&["kill", "-9", "1"], ProtectedWrite, false),
            (&["tee", "/etc/hosts"], ProtectedWrite, false),
            (&["touch", "../escape"], ProtectedWrite, false),
            (&["touch", "~/.zshrc"], ProtectedWrite, false),
            (&["git", "config", "user.name", "x"], ProtectedWrite, false),
            (&["git", "remote", "add", "o", "u"], ProtectedWrite, false),
            (&["git", "rebase", "main"], ProtectedWrite, false),
            (
                &["git", "-c", "core.hooksPath=/x", "status"],
                ProtectedWrite,
                false,
            ),
            (&["cat", "~/.ssh/id_ed25519"], SecretAccess, false),
            (&["cat", "/Users/me/.aws/credentials"], SecretAccess, false),
            (&["security", "find-generic-password"], SecretAccess, false),
            (&["env", "LD_PRELOAD=/x.so", "ls"], ProtectedWrite, false),
            // unclassified
            (&["frobnicate"], ReversibleWrite, true),
            (&["/opt/tools/frobnicate"], ReversibleWrite, true),
            (&["../tools/x"], ReversibleWrite, true),
            (&["python3"], ReversibleWrite, true),
            (
                &["python3", "-c", "import os; os.system('ls')"],
                ReversibleWrite,
                true,
            ),
            (&["git", "my-alias"], ReversibleWrite, true),
            (&["bash"], ReversibleWrite, true),
        ];
        for (a, class, unclassified) in cases {
            let e = classify_argv(&argv(a), None);
            assert_eq!(
                (e.class.max(e.inline_class), e.unclassified),
                (*class, *unclassified),
                "{a:?}: {:?}",
                e.reasons
            );
        }
    }

    #[test]
    fn shell_strings_and_wrappers_are_resolved() {
        let cases: &[(&str, EffectClass, bool)] = &[
            ("echo hi", ReversibleWrite, false),
            ("cd sub && ls -la | grep x", ReversibleWrite, false),
            (
                "mkdir -p out && echo hi > out/a.txt && cat out/a.txt",
                ReversibleWrite,
                false,
            ),
            ("echo a; echo b\necho c", ReversibleWrite, false),
            ("cargo test 2>&1 | tail -5", ReversibleWrite, false),
            ("cargo test > /dev/null 2>&1", ReversibleWrite, false),
            (
                "i=0; while [ $i -lt 3 ]; do echo $i; i=$((i+1)); done",
                ReversibleWrite,
                false,
            ),
            ("for f in $(ls); do echo $f; done", ReversibleWrite, false),
            (
                "if [ -f x ]; then echo y; else echo n; fi",
                ReversibleWrite,
                false,
            ),
            ("FOO=1 BAR=2 cargo build", ReversibleWrite, false),
            ("echo $(date) `uname`", ReversibleWrite, false),
            ("cat <<EOF\nrm -rf /\nEOF", ReversibleWrite, false),
            ("f() { echo x; }; f2", ReversibleWrite, true),
            // the effects hidden in strings
            ("rm -rf /tmp/x", Destructive, false),
            ("cd / && rm -fr 'x y'", Destructive, false),
            ("echo ok; git push --force", Destructive, false),
            ("git push origin main", ExternalSideEffect, false),
            ("echo x | xargs curl http://x", ExternalSideEffect, false),
            ("ls && npm install", ExternalSideEffect, false),
            ("echo $(curl http://x)", ExternalSideEffect, false),
            ("echo `rm -rf x`", Destructive, false),
            ("echo \"$(rm -rf x)\"", Destructive, false),
            ("echo hi > /etc/hosts", ProtectedWrite, false),
            ("echo hi >> ../x", ProtectedWrite, false),
            ("echo hi > ~/.bashrc", ProtectedWrite, false),
            ("for f in a b; do rm -rf $f; done", Destructive, false),
            ("sudo sh -c 'rm -rf x'", Destructive, false),
            (
                "env FOO=1 nice -n 5 timeout 10 git push -f",
                Destructive,
                false,
            ),
            ("eval 'rm -rf x'", Destructive, false),
            ("trap 'rm -rf x' EXIT; true", Destructive, false),
            ("bash -c 'git push --force'", Destructive, false),
            ("sh -c 'sh -c \"curl x\"'", ExternalSideEffect, false),
            ("$CMD arg", ReversibleWrite, true),
            ("echo 'unterminated", ReversibleWrite, true),
            ("case x in a) rm -rf y;; esac", Destructive, true),
            ("rm $FILES", Destructive, false),
            ("rm *.o", Destructive, false),
            ("ls | xargs rm", Destructive, false),
        ];
        for (script, class, unclassified) in cases {
            let e = sh(script);
            assert_eq!(
                (e.class, e.unclassified),
                (*class, *unclassified),
                "`{script}`: {:?}",
                e.reasons
            );
        }
        // wrappers and interpreters
        for (a, class) in [
            (&["bash", "-lc", "git push -f"][..], Destructive),
            (&["zsh", "-ec", "rm -rf x"][..], Destructive),
            (&["sh", "-c", "echo hi", "name", "arg"][..], ReversibleWrite),
            (&["env", "-i", "FOO=1", "curl", "x"][..], ExternalSideEffect),
            (&["nohup", "git", "push"][..], ExternalSideEffect),
            (&["time", "-p", "rm", "-rf", "x"][..], Destructive),
            (
                &["timeout", "-s", "KILL", "5", "curl", "x"][..],
                ExternalSideEffect,
            ),
            (&["sudo", "-u", "bob", "cat", "f"][..], ProtectedWrite),
            (&["command", "-v", "git"][..], ReversibleWrite),
        ] {
            let e = classify_argv(&argv(a), None);
            assert_eq!(e.class, class, "{a:?}: {:?}", e.reasons);
        }
    }

    #[test]
    fn a_shell_fed_through_stdin_is_classified_by_what_it_is_fed() {
        let e = classify_argv(&argv(&["sh"]), Some("rm -rf /tmp/x\n"));
        assert_eq!(e.class, Destructive);
        let e = classify_argv(&argv(&["bash", "-s"]), Some("echo hi\n"));
        assert_eq!((e.class, e.unclassified), (ReversibleWrite, false));
        let e = classify_argv(&argv(&["sh"]), None);
        assert!(e.unclassified);
        // a here-document handed to a shell is its script
        let e = sh("bash <<'EOF'\ngit push --force\nEOF");
        assert_eq!(e.class, Destructive, "{:?}", e.reasons);
        let e = sh("cat <<'EOF' > notes.txt\nrm -rf /\nEOF");
        assert_eq!(e.class, ReversibleWrite, "{:?}", e.reasons);
    }

    #[test]
    fn unclassified_commands_are_stricter_only_where_an_approval_can_be_asked() {
        let e = classify_argv(&argv(&["frobnicate"]), None);
        assert_eq!(e.class_in("local_trusted"), ProtectedWrite);
        assert_eq!(e.class_in("local_autonomous"), ReversibleWrite);
        assert_eq!(e.class_in("review_isolated"), ReversibleWrite);
        assert_eq!(e.class_in("cloud_isolated"), ReversibleWrite);
        assert!(e.reason_in("local_trusted").is_some());
        assert!(e.reason_in("local_autonomous").is_none());
        // a known destructive command is judged the same everywhere
        let e = classify_argv(&argv(&["rm", "-rf", "x"]), None);
        assert_eq!(e.class_in("local_autonomous"), Destructive);
    }

    #[test]
    fn inline_interpreter_code_counts_only_where_an_approval_can_be_asked() {
        // the review sandbox proves its own network denial by running exactly this
        let probe = classify_argv(
            &argv(&[
                "python3",
                "-c",
                "import socket\ns=socket.socket()\ns.connect(('127.0.0.1', 9))",
            ]),
            None,
        );
        assert_eq!(probe.inline_class, ExternalSideEffect);
        assert_eq!(probe.class_in("local_trusted"), ExternalSideEffect);
        for profile in ["review_isolated", "local_autonomous", "cloud_isolated"] {
            assert_eq!(probe.class_in(profile), ReversibleWrite, "{profile}");
        }
        let rm = classify_argv(
            &argv(&["python3", "-c", "import shutil; shutil.rmtree('x')"]),
            None,
        );
        assert_eq!(rm.class_in("local_trusted"), Destructive);
        assert_eq!(rm.class_in("local_autonomous"), ReversibleWrite);
    }

    #[test]
    fn dangerous_env_names_raise_the_class() {
        let e = classify_args(&serde_json::json!({
            "argv": ["cargo", "test"],
            "env": {"RUSTC_WRAPPER": "/tmp/evil", "FOO": "1"}
        }));
        assert_eq!(e.class, ProtectedWrite, "{:?}", e.reasons);
        let e = classify_args(&serde_json::json!({
            "argv": ["cargo", "test"], "env": {"RUST_LOG": "debug"}
        }));
        assert_eq!(e.class, ReversibleWrite);
    }

    #[test]
    fn typed_input_is_classified_as_the_shell_text_it_would_run() {
        let typed =
            |text: &str| classify_input(&serde_json::json!({"session_id": "s", "text": text}));
        // Answers to prompts and key presses are not commands.
        for plain in ["", "y", "No", "42", "yes\n3\n"] {
            let e = typed(plain);
            assert_eq!(
                e.class_in("local_trusted"),
                ReversibleWrite,
                "{plain:?}: {:?}",
                e.reasons
            );
            assert!(!e.unclassified, "{plain:?}");
        }
        // What `shell.exec` would raise, typing raises the same, in any line.
        let push = typed("echo ok\ngit push --force origin main\n");
        assert_eq!(
            push.class,
            classify_argv(&argv(&["git", "push", "--force"]), None).class
        );
        assert!(push.class > ReversibleWrite, "{:?}", push.reasons);
        assert!(typed("rm -rf build").class > ReversibleWrite);
        assert_eq!(typed("sudo reboot").class, ProtectedWrite);
        assert!(typed("curl -X POST https://x.test -d @f").class > ReversibleWrite);
        // A command nothing can resolve needs an approval where one can be asked.
        let unknown = typed("some-unknown-program --flag");
        assert!(unknown.unclassified);
        assert_eq!(unknown.class_in("local_trusted"), ProtectedWrite);
        assert_eq!(unknown.class_in("local_autonomous"), ReversibleWrite);
        // A bare word is an answer unless the tables know it as dangerous.
        let name = typed("ada");
        assert!(!name.unclassified, "{:?}", name.reasons);
        assert_eq!(name.class_in("local_trusted"), ReversibleWrite);
        assert!(typed("reboot").class > ReversibleWrite);
        assert!(typed("ada --flag").unclassified);
        // Typed into a REPL, what the interpreter would run is scanned too.
        let repl = typed("import shutil; shutil.rmtree('x')");
        assert_eq!(
            repl.class_in("local_trusted"),
            Destructive,
            "{:?}",
            repl.reasons
        );
        // A variable in command position is unresolvable, never safe.
        assert!(typed("$CMD now").unclassified);
        // A plain read stays a plain write.
        let ls = typed("ls -la\ngit status");
        assert_eq!(
            ls.class_in("local_trusted"),
            ReversibleWrite,
            "{:?}",
            ls.reasons
        );
    }

    /// PX-057: what a run-mode rule may cover, and the typed facts the
    /// always-ask classes are built from.
    #[test]
    fn a_rule_covers_an_argv_but_not_one_that_overrides_the_environment() {
        let plain = serde_json::json!({"argv": ["cargo", "test"]});
        assert_eq!(rule_argv(&plain), Some(argv(&["cargo", "test"])));
        let empty_env = serde_json::json!({"argv": ["cargo", "test"], "env": {}});
        assert_eq!(rule_argv(&empty_env), Some(argv(&["cargo", "test"])));
        let env =
            serde_json::json!({"argv": ["cargo", "test"], "env": {"RUSTC_WRAPPER": "/tmp/x"}});
        assert_eq!(
            rule_argv(&env),
            None,
            "a changed environment is not the rule's command"
        );
        assert_eq!(rule_argv(&serde_json::json!({"stdin": "ls"})), None);
    }

    #[test]
    fn the_classifier_names_writes_outside_the_workspace_and_to_configuration() {
        for a in [
            &["touch", "../x"][..],
            &["tee", "/etc/hosts"],
            &["cp", "a", "/usr/local/bin/a"],
        ] {
            let e = classify_argv(&argv(a), None);
            assert!(e.outside_workspace, "{a:?}: {:?}", e.reasons);
            assert!(!e.protected_path, "{a:?}");
        }
        let e = sh("echo hi > ../escape.txt");
        assert!(e.outside_workspace, "{:?}", e.reasons);
        for a in [
            &["git", "config", "user.name", "x"][..],
            &["git", "config", "--global", "core.editor", "vi"],
        ] {
            let e = classify_argv(&argv(a), None);
            assert!(e.protected_path, "{a:?}: {:?}", e.reasons);
        }
        // Ordinary work names neither.
        for a in [&["ls", "-la"][..], &["git", "status"], &["cargo", "test"]] {
            let e = classify_argv(&argv(a), None);
            assert!(!e.outside_workspace && !e.protected_path, "{a:?}");
        }
    }

    #[test]
    fn nesting_beyond_the_limit_is_unclassified_not_safe() {
        let mut s = String::from("echo x");
        for _ in 0..12 {
            s = format!("sh -c '{}'", s.replace('\'', "'\\''"));
        }
        let e = sh(&s);
        assert!(e.unclassified, "{:?}", e.reasons);
    }
}
