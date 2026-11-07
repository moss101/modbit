//! Run-mode presets and durable allowlist rules (PX-057; docs/65 AFW-F06 to
//! AFW-F12), owned by the Capability Kernel.
//!
//! A run mode answers exactly one question, and only after the kernel has
//! answered every other: *may this effect, which the kernel would otherwise
//! ask a person about, run without asking?* It cannot widen a capability, a
//! lease selector, a path, an egress or an effect-class ceiling, because it
//! is consulted at the approval step and nowhere earlier
//! ([`crate::kernel::CapabilityKernel::decide`]).
//!
//! * `ASK` never approves. Every protected effect asks.
//! * `ALLOWLIST` approves an effect that a live durable rule covers.
//! * `ALLOWLIST_SANDBOX` approves what `ALLOWLIST` does, and an effect that
//!   is fully contained in a sandbox (the host says so; it never rests on the
//!   model's word).
//! * `RUN_EVERYTHING` approves every in-envelope effect that is not in an
//!   always-ask class.
//! * `AUTONOMOUS` approves what `RUN_EVERYTHING` does, and the install of the
//!   project's own declared dependencies (the one network reach it lets pass,
//!   and only when the tool's classifier says that is all the call does).
//!   It is durable; the other modes are as before (DR-PX-2026-10-25-017).
//!
//! In every mode the always-ask classes ([`AskClass`]) ask, unless a durable
//! rule created with `covers_always_ask` (explicit, task or repository scope
//! only) covers the command. There is **no classifier** here that approves an
//! effect on its own judgement (a second policy engine, docs/81): a rule is
//! a person's literal argv prefix and nothing else.
//!
//! A rule's pattern never matches shell operators, redirections,
//! substitutions or pipelines. A command that a shell would parse (`sh -c`)
//! is split into its simple sub-commands and every one of them must be
//! covered; anything the splitter cannot prove is a plain list of simple
//! commands matches nothing.

use modbit_domain::runmode::{AllowRule, AskClass, RuleScope, RunMode};
use modbit_domain::toolcall::EffectClass;

/// Longest pattern, in tokens.
pub const MAX_PATTERN_TOKENS: usize = 16;
/// Longest token, in bytes.
pub const MAX_TOKEN_BYTES: usize = 512;

/// Programs whose arguments are themselves a program or a script: a rule
/// that names one covers whatever it is given, so it is not a rule.
const WRAPPERS: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "dash",
    "ksh",
    "ash",
    "fish",
    "csh",
    "tcsh",
    "busybox",
    "env",
    "sudo",
    "doas",
    "su",
    "xargs",
    "eval",
    "exec",
    "nohup",
    "time",
    "timeout",
    "nice",
    "ionice",
    "setsid",
    "stdbuf",
    "command",
    "builtin",
    "watch",
    "strace",
    "ltrace",
    "chroot",
    "unshare",
    "nsenter",
    "script",
    "screen",
    "tmux",
    "ssh",
    "find",
    "awk",
    "gawk",
    "cmd",
    "cmd.exe",
    "powershell",
    "powershell.exe",
    "pwsh",
    "pwsh.exe",
    "osascript",
    "expect",
    "parallel",
    "rlwrap",
    "trap",
];

/// Interpreters: a rule must name a script, not an inline program.
const INTERPRETERS: &[&str] = &[
    "python", "python3", "python2", "node", "nodejs", "ruby", "perl", "php", "lua", "deno", "bun",
    "tclsh", "Rscript", "julia",
];

/// Programs with sub-commands: a bare program name covers all of them
/// (`git` covers `git push`), so a rule names the sub-command too.
const SUBCOMMAND_PROGRAMS: &[&str] = &[
    "git",
    "cargo",
    "npm",
    "pnpm",
    "yarn",
    "npx",
    "docker",
    "kubectl",
    "make",
    "go",
    "pip",
    "pip3",
    "brew",
    "apt",
    "apt-get",
    "gh",
    "aws",
    "gcloud",
    "az",
    "terraform",
    "helm",
    "rustup",
    "gradle",
    "mvn",
    "dotnet",
    "swift",
    "xcodebuild",
    "bundle",
    "gem",
    "poetry",
    "uv",
    "conda",
    "mise",
];

/// Why a pattern or a rule was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleRefusal {
    /// Stable code.
    pub code: &'static str,
    /// Reason.
    pub reason: String,
}

fn refuse(code: &'static str, reason: impl Into<String>) -> RuleRefusal {
    RuleRefusal {
        code,
        reason: reason.into(),
    }
}

fn program_name(token: &str) -> &str {
    token
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(token)
        .trim_end_matches(".exe")
}

/// Whether a pattern token could be read by a shell as an operator,
/// redirection, substitution or expansion.
fn has_shell_syntax(token: &str) -> bool {
    token.is_empty()
        || token.chars().any(|c| {
            matches!(
                c,
                '|' | '&' | ';' | '<' | '>' | '(' | ')' | '`' | '$' | '\n' | '\r' | '\0'
            )
        })
}

/// The typed reason a rule that names a computer tool is refused (PX-070).
pub const COMPUTER_NOT_ALLOWLISTABLE: &str = "COMPUTER_NOT_ALLOWLISTABLE";

/// Validate an argv-prefix pattern for a new rule.
///
/// # Errors
/// A [`RuleRefusal`] naming why the pattern cannot be a rule.
pub fn validate_pattern(pattern: &[String]) -> Result<(), RuleRefusal> {
    if pattern.is_empty() {
        return Err(refuse("EMPTY_PATTERN", "a rule needs at least one token"));
    }
    if pattern.len() > MAX_PATTERN_TOKENS {
        return Err(refuse(
            "PATTERN_TOO_LONG",
            format!("a pattern has at most {MAX_PATTERN_TOKENS} tokens"),
        ));
    }
    // PX-070 (CUC-C01): an input to an application on the person's machine
    // is approved per call, with its exact intent, and nothing stands for
    // that approval - not a rule, not a preset. A pattern that names a
    // computer tool is not a rule, whatever else it says.
    if pattern.iter().any(|t| {
        let t = t.trim().to_ascii_lowercase();
        t == "computer" || t.starts_with("computer.")
    }) {
        return Err(refuse(
            COMPUTER_NOT_ALLOWLISTABLE,
            "computer control (`computer.*`) is approved per call with its exact intent and can never be covered by an allowlist rule, a preset or a run mode; the person is asked every time",
        ));
    }
    for t in pattern {
        if t.len() > MAX_TOKEN_BYTES {
            return Err(refuse("PATTERN_TOKEN_TOO_LONG", "a token is too long"));
        }
        if has_shell_syntax(t) {
            return Err(refuse(
                "PATTERN_SHELL_SYNTAX",
                format!(
                    "`{}` holds a shell operator, redirection, substitution or is empty; a rule is a literal argv prefix and never matches those",
                    t.escape_debug()
                ),
            ));
        }
    }
    let program = program_name(&pattern[0]);
    if WRAPPERS.contains(&program) {
        return Err(refuse(
            "PATTERN_WRAPPER",
            format!(
                "`{program}` runs whatever it is given, so a rule that names it covers everything; allow the program it would run instead"
            ),
        ));
    }
    if INTERPRETERS.contains(&program)
        && (pattern.len() < 2
            || pattern[1].starts_with('-')
            || matches!(pattern[1].as_str(), "eval" | "e" | "c"))
    {
        return Err(refuse(
            "PATTERN_INTERPRETER",
            format!(
                "`{program}` needs the script it runs in the rule, not an inline program or a bare interpreter"
            ),
        ));
    }
    if SUBCOMMAND_PROGRAMS.contains(&program) && pattern.len() < 2 {
        return Err(refuse(
            "PATTERN_TOO_BROAD",
            format!(
                "`{program}` has sub-commands; name the one the rule covers (for example `{program} <sub-command>`)"
            ),
        ));
    }
    Ok(())
}

/// Validate the scope-related shape of a new rule.
///
/// # Errors
/// A [`RuleRefusal`] when the combination is not allowed.
pub fn validate_rule(rule: &AllowRule) -> Result<(), RuleRefusal> {
    validate_pattern(&rule.pattern)?;
    match rule.scope {
        RuleScope::Task | RuleScope::Repo if rule.scope_key.trim().is_empty() => {
            return Err(refuse(
                "SCOPE_KEY_REQUIRED",
                "a task or repository rule names what it applies to",
            ));
        }
        RuleScope::User if !rule.scope_key.is_empty() => {
            return Err(refuse(
                "SCOPE_KEY_UNEXPECTED",
                "a user rule names no task or repository",
            ));
        }
        _ => {}
    }
    if rule.covers_always_ask {
        if rule.scope == RuleScope::User {
            return Err(refuse(
                "ALWAYS_ASK_SCOPE",
                "a rule that stands for an always-ask effect is narrow: task or repository scope only",
            ));
        }
        if rule.pattern.len() < 2 {
            return Err(refuse(
                "ALWAYS_ASK_PATTERN",
                "a rule that stands for an always-ask effect names at least a program and one argument",
            ));
        }
    }
    Ok(())
}

/// Split the script a shell is asked to run into its simple commands, only
/// when it is nothing but simple commands joined by `&&`, `||`, `;` or a
/// newline. A pipeline, a background `&`, a redirection, a substitution, an
/// expansion, a subshell, a group, a comment, a negation, a line
/// continuation or an unterminated quote is `None`: the script is not proven
/// to be a list of simple commands.
#[must_use]
pub fn split_script(script: &str) -> Option<Vec<Vec<String>>> {
    #[derive(PartialEq)]
    enum Q {
        None,
        Single,
        Double,
    }
    let chars: Vec<char> = script.chars().collect();
    let mut commands: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quote = Q::None;
    let mut i = 0;
    let end_word = |word: &mut String, in_word: &mut bool, current: &mut Vec<String>| {
        if *in_word {
            current.push(std::mem::take(word));
            *in_word = false;
        }
    };
    while i < chars.len() {
        let c = chars[i];
        match quote {
            Q::Single => {
                if c == '\'' {
                    quote = Q::None;
                } else {
                    word.push(c);
                }
            }
            Q::Double => match c {
                '"' => quote = Q::None,
                '$' | '`' => return None,
                '\\' => match chars.get(i + 1) {
                    Some(n @ ('"' | '\\' | '$' | '`')) => {
                        word.push(*n);
                        i += 1;
                    }
                    Some('\n') | None => return None,
                    Some(_) => word.push('\\'),
                },
                _ => word.push(c),
            },
            Q::None => match c {
                ' ' | '\t' => end_word(&mut word, &mut in_word, &mut current),
                '\n' | ';' => {
                    end_word(&mut word, &mut in_word, &mut current);
                    if current.is_empty() {
                        // `a;;b`, a leading `;`: not a list of commands. A
                        // blank line between commands is fine.
                        if c == ';' {
                            return None;
                        }
                    } else {
                        commands.push(std::mem::take(&mut current));
                    }
                }
                '&' | '|' => {
                    if chars.get(i + 1) != Some(&c) {
                        return None;
                    }
                    end_word(&mut word, &mut in_word, &mut current);
                    if current.is_empty() {
                        return None;
                    }
                    commands.push(std::mem::take(&mut current));
                    i += 1;
                }
                '<' | '>' | '(' | ')' | '`' | '$' | '{' | '}' | '\r' | '\0' => return None,
                '#' if !in_word => return None,
                '!' if !in_word => return None,
                '\\' => match chars.get(i + 1) {
                    Some('\n') | None => return None,
                    Some(n) => {
                        word.push(*n);
                        in_word = true;
                        i += 1;
                    }
                },
                '\'' => {
                    quote = Q::Single;
                    in_word = true;
                }
                '"' => {
                    quote = Q::Double;
                    in_word = true;
                }
                _ => {
                    word.push(c);
                    in_word = true;
                }
            },
        }
        i += 1;
    }
    if quote != Q::None {
        return None;
    }
    end_word(&mut word, &mut in_word, &mut current);
    if !current.is_empty() {
        commands.push(current);
    }
    // A trailing `&&` / `||` leaves its right-hand side missing.
    let trailing_operator = {
        let t = script.trim_end();
        t.ends_with("&&") || t.ends_with("||")
    };
    (!commands.is_empty() && !trailing_operator).then_some(commands)
}

const SHELLS: &[&str] = &["sh", "bash", "zsh", "dash", "ksh", "ash"];

/// The simple commands a call's argv amounts to: itself, or — when it is a
/// shell given a script — the script's simple commands. `None` when a shell
/// is asked for something that is not provably a list of simple commands.
#[must_use]
pub fn sub_commands(argv: &[String]) -> Option<Vec<Vec<String>>> {
    let first = argv.first()?;
    if !SHELLS.contains(&program_name(first)) {
        return Some(vec![argv.to_vec()]);
    }
    // `sh -c '<script>'` (and `-lc`, `-ec`): one flag word carrying `c`, then
    // the script, then nothing.
    let flag = argv.get(1)?;
    let is_c_flag = flag.len() >= 2
        && flag.starts_with('-')
        && !flag.starts_with("--")
        && flag[1..].chars().all(|c| c.is_ascii_alphabetic())
        && flag.contains('c');
    if !is_c_flag || argv.len() != 3 {
        return None;
    }
    split_script(&argv[2])
}

fn prefix_matches(pattern: &[String], command: &[String]) -> bool {
    command.len() >= pattern.len() && command.iter().zip(pattern).all(|(a, b)| a == b)
}

/// The typed reasons a rule whose pattern matches a command still does not
/// cover it (PX-149).
pub mod pin {
    /// The rule predates program pinning: it records no path and asks. Making
    /// the rule again (the "Always" button) pins it.
    pub const RULE_NOT_PINNED: &str = "RULE_NOT_PINNED";
    /// The program now resolves to a different path than the one the rule was
    /// made for (a repository's `bin/` earlier on the search path, a moved
    /// tool): the rule does not apply and the person is asked.
    pub const PROGRAM_PATH_CHANGED: &str = "PROGRAM_PATH_CHANGED";
    /// The program resolves to nothing, so no rule can say it is the one that
    /// was approved: fail closed.
    pub const PROGRAM_NOT_FOUND: &str = "PROGRAM_NOT_FOUND";
    /// A shell that wraps the script resolves to a file inside the task's own
    /// workspace: the interpreter that would run the approved commands is
    /// repository content, not the system's.
    pub const PROGRAM_IN_WORKSPACE: &str = "PROGRAM_IN_WORKSPACE";
}

/// Where a program name resolves to, for each program a command names: the
/// absolute path of the file the process would execute, or `None` when
/// nothing is there. Built by the host from the search path and working
/// directory the process will actually get ([`resolve_programs`]); the policy
/// only compares.
pub type Programs = std::collections::BTreeMap<String, Option<String>>;

/// Remove `.` components and resolve `..` lexically (no filesystem access, no
/// symlink resolution: the pin is the path as the search found it).
fn clean_path(p: &std::path::Path) -> std::path::PathBuf {
    use std::path::Component;
    let mut out = std::path::PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(unix)]
fn is_runnable(p: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_runnable(p: &std::path::Path) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.is_file())
}

/// The absolute path `token` would execute as, searched the way the process
/// will: a token with a path separator is taken against `cwd`; a bare name is
/// looked up in `path`'s entries in order (an empty entry is `cwd`, as a shell
/// reads it). The result is lexically clean and not symlink-resolved, so a tool
/// that is a symlink into a package directory keeps its pin across upgrades,
/// and a different directory earlier on the path does not match it.
#[must_use]
pub fn resolve_program(token: &str, cwd: &str, path: &str) -> Option<String> {
    if token.is_empty() {
        return None;
    }
    let cwd = std::path::Path::new(cwd);
    let absolute = |p: &std::path::Path| {
        if p.is_absolute() {
            clean_path(p)
        } else {
            clean_path(&cwd.join(p))
        }
    };
    let candidates = |dir: &std::path::Path| -> Vec<std::path::PathBuf> {
        let base = absolute(&dir.join(token));
        #[cfg(windows)]
        {
            let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
            let mut v = vec![base.clone()];
            for e in exts.split(';').filter(|e| !e.is_empty()) {
                let mut s = base.clone().into_os_string();
                s.push(e);
                v.push(s.into());
            }
            v
        }
        #[cfg(not(windows))]
        {
            vec![base]
        }
    };
    let found = |cands: Vec<std::path::PathBuf>| {
        cands
            .into_iter()
            .find(|c| is_runnable(c))
            .map(|c| c.to_string_lossy().into_owned())
    };
    if token.contains('/') || (cfg!(windows) && token.contains('\\')) {
        return found(candidates(std::path::Path::new("")));
    }
    for entry in std::env::split_paths(path) {
        let dir = if entry.as_os_str().is_empty() {
            cwd.to_path_buf()
        } else {
            entry
        };
        if let Some(f) = found(candidates(&dir)) {
            return Some(f);
        }
    }
    None
}

/// The programs a call's argv names, resolved: the first word of every simple
/// command ([`sub_commands`]) and, when the call is a shell given a script,
/// the shell itself. Empty when the argv is not a list of simple commands (no
/// rule can cover it anyway).
#[must_use]
pub fn resolve_programs(argv: &[String], cwd: &str, path: &str) -> Programs {
    let mut tokens: Vec<&str> = Vec::new();
    if let Some(first) = argv.first() {
        tokens.push(first);
    }
    let subs = sub_commands(argv);
    for c in subs.iter().flatten() {
        if let Some(t) = c.first() {
            tokens.push(t);
        }
    }
    tokens
        .into_iter()
        .map(|t| (t.to_owned(), resolve_program(t, cwd, path)))
        .collect()
}

/// Why no rule covers a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Uncovered {
    /// No rule's pattern is a prefix of it.
    NoRule,
    /// A rule's pattern matches and its pin does not hold; the code is one of
    /// [`pin`].
    Pin(&'static str),
}

/// The rule that covers one simple command: the first, in order, whose pattern
/// is a token prefix of it **and** whose pinned program is the one the command
/// would run. A matching rule that fails the pin is reported as the reason when
/// nothing else covers the command.
fn covering_rule<'r>(
    rules: &[&'r AllowRule],
    command: &[String],
    programs: &Programs,
) -> Result<&'r AllowRule, Uncovered> {
    let mut why: Option<&'static str> = None;
    for rule in rules.iter().filter(|r| prefix_matches(&r.pattern, command)) {
        if rule.program_path.is_empty() {
            why.get_or_insert(pin::RULE_NOT_PINNED);
            continue;
        }
        match command.first().and_then(|t| programs.get(t)) {
            Some(Some(now)) if *now == rule.program_path => return Ok(rule),
            Some(Some(_)) => {
                why.get_or_insert(pin::PROGRAM_PATH_CHANGED);
            }
            _ => {
                why.get_or_insert(pin::PROGRAM_NOT_FOUND);
            }
        }
    }
    Err(why.map_or(Uncovered::NoRule, Uncovered::Pin))
}

/// The rules (by id) that together cover every sub-command of `argv`, or why
/// not. Each sub-command is covered by the first rule, in order, whose pattern
/// is a token prefix of it and whose pinned program is the one it would run
/// (`programs`, see [`resolve_programs`]).
///
/// # Errors
/// [`Uncovered`] for the first sub-command that no rule covers, or when the
/// argv is not provably a list of simple commands.
pub fn covering_rules<'r>(
    rules: &[&'r AllowRule],
    argv: &[String],
    programs: &Programs,
) -> Result<Vec<&'r AllowRule>, Uncovered> {
    let commands = sub_commands(argv).ok_or(Uncovered::NoRule)?;
    let mut used: Vec<&AllowRule> = Vec::new();
    for command in &commands {
        let rule = covering_rule(rules, command, programs)?;
        if !used.iter().any(|u| u.rule_id == rule.rule_id) {
            used.push(rule);
        }
    }
    Ok(used)
}

/// The always-ask classes an effect falls in, from what the host resolved:
/// its registered effect class, the capabilities it needs, and two facts the
/// host's own classifier and path policy established. Nothing here reads
/// argument text.
#[must_use]
pub fn ask_classes(
    effect_class: EffectClass,
    required_capabilities: &[String],
    outside_workspace_write: bool,
    protected_path: bool,
    declared_escalation: &str,
) -> Vec<AskClass> {
    let has = |c: &str| required_capabilities.iter().any(|r| r == c);
    let mut out = Vec::new();
    if outside_workspace_write {
        out.push(AskClass::OutsideWorkspaceWrite);
    }
    if has("network.egress")
        || has("external.call")
        || effect_class == EffectClass::ExternalSideEffect
    {
        out.push(AskClass::Network);
    }
    if has("secret.use") || effect_class == EffectClass::SecretAccess {
        out.push(AskClass::Secret);
    }
    if protected_path {
        out.push(AskClass::ProtectedPath);
    }
    if effect_class == EffectClass::Destructive {
        out.push(AskClass::Deletion);
    }
    if has("git.push") || has("deploy") {
        out.push(AskClass::Push);
    }
    if !matches!(declared_escalation, "" | "none") {
        out.push(AskClass::Escalation);
    }
    out.sort();
    out.dedup();
    out
}

/// What a call looks like to the run mode: everything the host resolved
/// about the task and the call, and the rules in force.
#[derive(Clone, Debug, Default)]
pub struct RunPolicy {
    /// The task's mode in force.
    pub mode: RunMode,
    /// The task (its id text), for task-scoped rules.
    pub task_id: String,
    /// The task's canonical workspace root, for repository-scoped rules.
    pub repo_root: String,
    /// Every non-revoked rule the Core holds (scope and expiry are judged
    /// here, against this call).
    pub rules: Vec<AllowRule>,
    /// The call's argv, for a shell-class tool; `None` otherwise (no rule
    /// can cover a call that is not a command).
    pub argv: Option<Vec<String>>,
    /// Where each program the argv names resolves to, from the search path and
    /// working directory the process will get (PX-149). A rule covers a command
    /// only while its program still resolves to the path the rule recorded.
    pub programs: Programs,
    /// The always-ask classes the call falls in.
    pub ask_classes: Vec<AskClass>,
    /// The call's only network reach is the install of the project's declared
    /// dependencies, as the tool's own classifier established
    /// (DR-PX-2026-10-25-017). Only the `AUTONOMOUS` mode lets it pass the
    /// NETWORK class, and nothing else about the call.
    pub dependency_install: bool,
    /// Whether the call runs fully contained in a sandbox: an isolated
    /// profile, which enforces no network and writes confined to its tree.
    pub contained: bool,
}

/// The run mode's answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunDecision {
    /// Run without asking; `rule` is what the receipt names.
    Approve {
        /// `allowlist:<rule id>[+<rule id>]` | `run_mode:<MODE>[:contained]`.
        rule: String,
    },
    /// Ask a person; `code` is the typed reason the card shows.
    Ask {
        /// `MODE_ASK` | `NOT_IN_ALLOWLIST` | `ALWAYS_ASK:<CLASS>` | one of
        /// [`pin`] (`RULE_NOT_PINNED`, `PROGRAM_PATH_CHANGED`,
        /// `PROGRAM_NOT_FOUND`, `PROGRAM_IN_WORKSPACE`).
        code: String,
    },
}

fn same_root(a: &str, b: &str) -> bool {
    !a.is_empty()
        && std::path::Path::new(a.trim_end_matches(['/', '\\']))
            == std::path::Path::new(b.trim_end_matches(['/', '\\']))
}

impl RunPolicy {
    /// The rules that apply to this task at `now_ms`: in scope and not expired.
    #[must_use]
    pub fn live_rules(&self, now_ms: i64) -> Vec<&AllowRule> {
        let mut rules: Vec<&AllowRule> = self
            .rules
            .iter()
            .filter(|r| r.expires_at_ms.is_none_or(|e| e > now_ms))
            .filter(|r| match r.scope {
                RuleScope::Task => r.scope_key == self.task_id,
                RuleScope::Repo => same_root(&r.scope_key, &self.repo_root),
                RuleScope::User => true,
            })
            .collect();
        rules.sort_by(|a, b| (a.created_at_ms, &a.rule_id).cmp(&(b.created_at_ms, &b.rule_id)));
        rules
    }

    /// Whether the mode lets the call run without asking.
    #[must_use]
    pub fn decide(&self, now_ms: i64) -> RunDecision {
        // The classes that ask in this mode. `AUTONOMOUS` is the one mode
        // that lets the NETWORK class pass, and only for a call the host's
        // classifier found to be nothing but a manifest install; any other
        // class the call falls in still asks.
        let asking: Vec<AskClass> = self
            .ask_classes
            .iter()
            .copied()
            .filter(|c| {
                !(self.mode == RunMode::Autonomous
                    && self.dependency_install
                    && *c == AskClass::Network)
            })
            .collect();
        let always_ask = !asking.is_empty();
        let always_ask_code = asking.first().map(|c| format!("ALWAYS_ASK:{}", c.name()));
        if self.mode == RunMode::Ask {
            return RunDecision::Ask {
                code: always_ask_code.unwrap_or_else(|| "MODE_ASK".into()),
            };
        }
        // A durable rule covers a command, and covers an always-ask effect
        // only when every rule that is needed says so. A rule whose pinned
        // program is not the one the command would run covers nothing, and
        // the reason is the typed code the card shows (PX-149).
        let mut uncovered_code: Option<&'static str> = None;
        if let Some(argv) = &self.argv {
            if self.wrapper_in_workspace(argv) {
                uncovered_code = Some(pin::PROGRAM_IN_WORKSPACE);
            } else {
                let live = self.live_rules(now_ms);
                match covering_rules(&live, argv, &self.programs) {
                    Ok(used) if !always_ask || used.iter().all(|r| r.covers_always_ask) => {
                        return RunDecision::Approve {
                            rule: format!(
                                "allowlist:{}",
                                used.iter()
                                    .map(|r| r.rule_id.as_str())
                                    .collect::<Vec<_>>()
                                    .join("+")
                            ),
                        };
                    }
                    Ok(_) => {}
                    Err(Uncovered::Pin(code)) => uncovered_code = Some(code),
                    Err(Uncovered::NoRule) => {}
                }
            }
        }
        if always_ask {
            return RunDecision::Ask {
                code: always_ask_code.unwrap_or_default(),
            };
        }
        match self.mode {
            RunMode::AllowlistSandbox if self.contained => RunDecision::Approve {
                rule: "run_mode:ALLOWLIST_SANDBOX:contained".into(),
            },
            RunMode::RunEverything => RunDecision::Approve {
                rule: "run_mode:RUN_EVERYTHING".into(),
            },
            RunMode::Autonomous => RunDecision::Approve {
                rule: if self.dependency_install && self.ask_classes.contains(&AskClass::Network) {
                    "run_mode:AUTONOMOUS:dependency_install".into()
                } else {
                    "run_mode:AUTONOMOUS".into()
                },
            },
            _ => RunDecision::Ask {
                code: uncovered_code.unwrap_or("NOT_IN_ALLOWLIST").into(),
            },
        }
    }

    /// The shell that wraps a script, when it resolves to a file inside the
    /// task's workspace: the interpreter that would run the approved commands
    /// is then repository content.
    fn wrapper_in_workspace(&self, argv: &[String]) -> bool {
        let Some(first) = argv.first() else {
            return false;
        };
        if !SHELLS.contains(&program_name(first)) || self.repo_root.is_empty() {
            return false;
        }
        let Some(Some(resolved)) = self.programs.get(first) else {
            return false;
        };
        let root = std::path::Path::new(self.repo_root.trim_end_matches(['/', '\\']));
        std::path::Path::new(resolved).starts_with(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| (*x).to_owned()).collect()
    }

    /// Every program a command names resolves to `/bin/<name>`, the path
    /// `rule` pins.
    fn progs_of(argv: &[String]) -> Programs {
        let mut tokens: Vec<String> = argv.first().cloned().into_iter().collect();
        tokens.extend(
            sub_commands(argv)
                .into_iter()
                .flatten()
                .filter_map(|c| c.first().cloned()),
        );
        tokens
            .into_iter()
            .map(|t| {
                let path = format!("/bin/{}", program_name(&t));
                (t, Some(path))
            })
            .collect()
    }

    fn covering<'r>(
        rules: &[&'r AllowRule],
        argv: &[String],
    ) -> Result<Vec<&'r AllowRule>, Uncovered> {
        covering_rules(rules, argv, &progs_of(argv))
    }

    fn rule(id: &str, pattern: &[&str], scope: RuleScope, key: &str) -> AllowRule {
        AllowRule {
            rule_id: id.into(),
            pattern: v(pattern),
            program_path: format!("/bin/{}", program_name(pattern[0])),
            scope,
            scope_key: key.into(),
            created_by: "user:u".into(),
            created_at_ms: 1,
            expires_at_ms: None,
            covers_always_ask: false,
            origin_task: String::new(),
        }
    }

    fn policy(mode: RunMode, rules: Vec<AllowRule>, argv: &[&str]) -> RunPolicy {
        RunPolicy {
            mode,
            task_id: "t1".into(),
            repo_root: "/repo".into(),
            rules,
            argv: Some(v(argv)),
            programs: progs_of(&v(argv)),
            ask_classes: vec![],
            dependency_install: false,
            contained: false,
        }
    }

    #[test]
    fn a_pattern_is_a_literal_argv_prefix_with_no_shell_syntax() {
        assert!(validate_pattern(&v(&["cargo", "test"])).is_ok());
        assert!(validate_pattern(&v(&["./scripts/build.sh"])).is_ok());
        for bad in [
            v(&[]),
            v(&["cargo", "test", "|", "tee"]),
            v(&["echo", "a;b"]),
            v(&["cat", ">out"]),
            v(&["echo", "$(id)"]),
            v(&["echo", "`id`"]),
            v(&["ls", "&&", "rm"]),
            v(&["ls", ""]),
        ] {
            assert!(validate_pattern(&bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            validate_pattern(&v(&["bash", "-c"])).unwrap_err().code,
            "PATTERN_WRAPPER"
        );
        assert_eq!(
            validate_pattern(&v(&["sudo", "ls"])).unwrap_err().code,
            "PATTERN_WRAPPER"
        );
        assert_eq!(
            validate_pattern(&v(&["git"])).unwrap_err().code,
            "PATTERN_TOO_BROAD"
        );
        assert_eq!(
            validate_pattern(&v(&["python3", "-c"])).unwrap_err().code,
            "PATTERN_INTERPRETER"
        );
        assert!(validate_pattern(&v(&["python3", "tests/run.py"])).is_ok());
    }

    #[test]
    fn a_rule_covers_the_commands_that_start_with_its_prefix_token_by_token() {
        let r = rule("r1", &["cargo", "test"], RuleScope::Task, "t1");
        let rs = [&r];
        assert!(covering(&rs, &v(&["cargo", "test"])).is_ok());
        assert!(covering(&rs, &v(&["cargo", "test", "--lib", "x"])).is_ok());
        assert!(covering(&rs, &v(&["cargo", "tests"])).is_err());
        assert!(covering(&rs, &v(&["cargo"])).is_err());
        assert!(covering(&rs, &v(&["xcargo", "test"])).is_err());
    }

    #[test]
    fn compound_commands_are_matched_by_their_sub_commands() {
        let a = rule("a", &["cargo", "build"], RuleScope::User, "");
        let b = rule("b", &["cargo", "test"], RuleScope::User, "");
        let both = [&a, &b];
        let only_a = [&a];
        let script = v(&["sh", "-c", "cargo build && cargo test --lib"]);
        let used = covering(&both, &script).expect("every sub-command is covered");
        assert_eq!(used.len(), 2);
        assert!(covering(&only_a, &script).is_err(), "one is not");
        let script = v(&["bash", "-lc", "cargo build; cargo build"]);
        assert_eq!(covering(&only_a, &script).unwrap().len(), 1);
    }

    #[test]
    fn operators_redirections_substitutions_and_pipelines_match_nothing() {
        let r = rule("r", &["cargo", "test"], RuleScope::User, "");
        let rs = [&r];
        for script in [
            "cargo test | tee out",
            "cargo test > out.txt",
            "cargo test >> out.txt",
            "cargo test < in.txt",
            "cargo test $(id)",
            "cargo test `id`",
            "cargo test \"$HOME\"",
            "cargo test $HOME",
            "cargo test &",
            "cargo test && rm -rf x",
            "cargo test; curl evil",
            "(cargo test)",
            "{ cargo test; }",
            "cargo test # comment",
            "! cargo test",
            "cargo test \\\n --lib",
            "cargo test 'unterminated",
            "cargo test &&",
            "cargo test ;; cargo test",
        ] {
            let argv = v(&["sh", "-c", script]);
            // Either it is refused outright or a sub-command is not covered;
            // it is never approved.
            assert!(covering(&rs, &argv).is_err(), "{script}");
        }
        // A quoted operator is a literal argument, not an operator.
        let argv = v(&["sh", "-c", "cargo test 'a|b' \"c>d\""]);
        assert!(covering(&rs, &argv).is_ok());
        // A shell that is not given a plain `-c script` is not provable.
        assert!(covering(&rs, &v(&["sh", "run.sh"])).is_err());
        assert!(covering(&rs, &v(&["sh", "-c", "cargo test", "$0"])).is_err());
        assert!(covering(&rs, &v(&["sh", "--norc", "-c", "cargo test"])).is_err());
    }

    #[test]
    fn a_pattern_that_matches_a_command_inside_a_substitution_is_not_a_rule() {
        // The pattern itself cannot name the substitution...
        assert!(validate_pattern(&v(&["echo", "$(cargo test)"])).is_err());
        // ...and a command that hides one never reaches a rule.
        let r = rule("r", &["cargo", "test"], RuleScope::User, "");
        assert!(covering(&[&r], &v(&["sh", "-c", "echo $(cargo test)"])).is_err());
        assert!(covering(&[&r], &v(&["sh", "-c", "echo `cargo test`"])).is_err());
    }

    #[test]
    fn scope_and_expiry_bound_what_a_rule_covers() {
        let mut task = rule("task", &["make", "check"], RuleScope::Task, "t1");
        let other = rule("other", &["make", "other"], RuleScope::Task, "t2");
        let repo = rule("repo", &["make", "lint"], RuleScope::Repo, "/repo/");
        let elsewhere = rule("elsewhere", &["make", "docs"], RuleScope::Repo, "/other");
        let user = rule("user", &["make", "fmt"], RuleScope::User, "");
        task.expires_at_ms = Some(100);
        let mut p = policy(
            RunMode::Allowlist,
            vec![task, other, repo, elsewhere, user],
            &["make", "check"],
        );
        let ids = |p: &RunPolicy, now| {
            p.live_rules(now)
                .iter()
                .map(|r| r.rule_id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&p, 50), ["repo", "task", "user"]);
        assert_eq!(ids(&p, 100), ["repo", "user"], "an expired rule is gone");
        assert!(matches!(p.decide(50), RunDecision::Approve { .. }));
        assert_eq!(
            p.decide(100),
            RunDecision::Ask {
                code: "NOT_IN_ALLOWLIST".into()
            }
        );
        p.argv = Some(v(&["make", "other"]));
        assert!(matches!(p.decide(50), RunDecision::Ask { .. }), "t2's rule");
        p.argv = Some(v(&["make", "docs"]));
        assert!(
            matches!(p.decide(50), RunDecision::Ask { .. }),
            "/other repo"
        );
    }

    #[test]
    fn ask_mode_never_approves_and_allowlist_needs_a_rule() {
        let r = rule("r", &["cargo", "test"], RuleScope::User, "");
        let ask = policy(RunMode::Ask, vec![r.clone()], &["cargo", "test"]);
        assert_eq!(
            ask.decide(0),
            RunDecision::Ask {
                code: "MODE_ASK".into()
            }
        );
        let list = policy(RunMode::Allowlist, vec![r], &["cargo", "test"]);
        assert_eq!(
            list.decide(0),
            RunDecision::Approve {
                rule: "allowlist:r".into()
            }
        );
        let none = policy(RunMode::Allowlist, vec![], &["cargo", "test"]);
        assert_eq!(
            none.decide(0),
            RunDecision::Ask {
                code: "NOT_IN_ALLOWLIST".into()
            }
        );
    }

    #[test]
    fn the_sandbox_mode_approves_only_what_the_host_says_is_contained() {
        let mut p = policy(RunMode::AllowlistSandbox, vec![], &["make", "x"]);
        assert!(matches!(p.decide(0), RunDecision::Ask { .. }));
        p.contained = true;
        assert_eq!(
            p.decide(0),
            RunDecision::Approve {
                rule: "run_mode:ALLOWLIST_SANDBOX:contained".into()
            }
        );
        // The plain allowlist mode ignores containment.
        p.mode = RunMode::Allowlist;
        assert!(matches!(p.decide(0), RunDecision::Ask { .. }));
    }

    #[test]
    fn every_always_ask_class_asks_in_the_widest_mode() {
        for class in [
            AskClass::OutsideWorkspaceWrite,
            AskClass::Network,
            AskClass::Secret,
            AskClass::ProtectedPath,
            AskClass::Deletion,
            AskClass::Push,
            AskClass::Escalation,
        ] {
            let mut p = policy(RunMode::RunEverything, vec![], &["tool"]);
            p.ask_classes = vec![class];
            p.contained = true;
            assert_eq!(
                p.decide(0),
                RunDecision::Ask {
                    code: format!("ALWAYS_ASK:{}", class.name())
                },
                "{class:?}"
            );
            // A plain rule does not stand for it either...
            let r = rule("r", &["tool"], RuleScope::User, "");
            p.rules = vec![r];
            assert!(matches!(p.decide(0), RunDecision::Ask { .. }), "{class:?}");
            // ...but one created for it, explicitly, does.
            let mut r = rule("narrow", &["tool", "x"], RuleScope::Task, "t1");
            r.covers_always_ask = true;
            p.rules = vec![r];
            p.argv = Some(v(&["tool", "x"]));
            assert_eq!(
                p.decide(0),
                RunDecision::Approve {
                    rule: "allowlist:narrow".into()
                },
                "{class:?}"
            );
        }
    }

    #[test]
    fn run_everything_approves_what_is_not_an_always_ask_class() {
        let p = policy(RunMode::RunEverything, vec![], &["./do-it.sh"]);
        assert_eq!(
            p.decide(0),
            RunDecision::Approve {
                rule: "run_mode:RUN_EVERYTHING".into()
            }
        );
    }

    #[test]
    fn classes_come_from_the_effect_the_capabilities_and_the_hosts_facts() {
        let caps = |c: &[&str]| c.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(
            ask_classes(
                EffectClass::ReversibleWrite,
                &caps(&["shell.exec"]),
                false,
                false,
                "none"
            )
            .is_empty()
        );
        assert_eq!(
            ask_classes(
                EffectClass::ExternalSideEffect,
                &caps(&["shell.exec"]),
                false,
                false,
                ""
            ),
            [AskClass::Network]
        );
        assert_eq!(
            ask_classes(
                EffectClass::Destructive,
                &caps(&["shell.exec"]),
                false,
                false,
                ""
            ),
            [AskClass::Deletion]
        );
        assert_eq!(
            ask_classes(
                EffectClass::SecretAccess,
                &caps(&["secret.use"]),
                false,
                false,
                ""
            ),
            [AskClass::Secret]
        );
        assert_eq!(
            ask_classes(
                EffectClass::ProtectedWrite,
                &caps(&["git.push"]),
                true,
                true,
                "all"
            ),
            [
                AskClass::OutsideWorkspaceWrite,
                AskClass::ProtectedPath,
                AskClass::Push,
                AskClass::Escalation
            ]
        );
    }

    #[test]
    fn a_rule_that_stands_for_an_always_ask_effect_is_narrow() {
        let mut r = rule(
            "r",
            &["curl", "https://api.example.com/v1"],
            RuleScope::User,
            "",
        );
        r.covers_always_ask = true;
        assert_eq!(validate_rule(&r).unwrap_err().code, "ALWAYS_ASK_SCOPE");
        r.scope = RuleScope::Repo;
        r.scope_key = "/repo".into();
        assert!(validate_rule(&r).is_ok());
        r.pattern = v(&["curl"]);
        assert_eq!(validate_rule(&r).unwrap_err().code, "ALWAYS_ASK_PATTERN");
        let mut t = rule("t", &["cargo", "test"], RuleScope::Task, "");
        assert_eq!(validate_rule(&t).unwrap_err().code, "SCOPE_KEY_REQUIRED");
        t.scope_key = "t1".into();
        assert!(validate_rule(&t).is_ok());
    }
}
