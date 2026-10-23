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

/// The rules (by id) that together cover every sub-command of `argv`, or
/// `None` when any sub-command is not covered. Each sub-command is covered by
/// the first rule, in order, whose pattern is a token prefix of it.
#[must_use]
pub fn covering_rules<'r>(rules: &[&'r AllowRule], argv: &[String]) -> Option<Vec<&'r AllowRule>> {
    let commands = sub_commands(argv)?;
    let mut used: Vec<&AllowRule> = Vec::new();
    for command in &commands {
        let rule = rules.iter().find(|r| prefix_matches(&r.pattern, command))?;
        if !used.iter().any(|u| u.rule_id == rule.rule_id) {
            used.push(rule);
        }
    }
    Some(used)
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
    /// The always-ask classes the call falls in.
    pub ask_classes: Vec<AskClass>,
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
        /// `MODE_ASK` | `NOT_IN_ALLOWLIST` | `ALWAYS_ASK:<CLASS>`.
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
        let always_ask = !self.ask_classes.is_empty();
        let always_ask_code = self
            .ask_classes
            .first()
            .map(|c| format!("ALWAYS_ASK:{}", c.name()));
        if self.mode == RunMode::Ask {
            return RunDecision::Ask {
                code: always_ask_code.unwrap_or_else(|| "MODE_ASK".into()),
            };
        }
        // A durable rule covers a command, and covers an always-ask effect
        // only when every rule that is needed says so.
        if let Some(argv) = &self.argv {
            let live = self.live_rules(now_ms);
            if let Some(used) = covering_rules(&live, argv)
                && (!always_ask || used.iter().all(|r| r.covers_always_ask))
            {
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
            _ => RunDecision::Ask {
                code: "NOT_IN_ALLOWLIST".into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| (*x).to_owned()).collect()
    }

    fn rule(id: &str, pattern: &[&str], scope: RuleScope, key: &str) -> AllowRule {
        AllowRule {
            rule_id: id.into(),
            pattern: v(pattern),
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
            ask_classes: vec![],
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
        assert!(covering_rules(&rs, &v(&["cargo", "test"])).is_some());
        assert!(covering_rules(&rs, &v(&["cargo", "test", "--lib", "x"])).is_some());
        assert!(covering_rules(&rs, &v(&["cargo", "tests"])).is_none());
        assert!(covering_rules(&rs, &v(&["cargo"])).is_none());
        assert!(covering_rules(&rs, &v(&["xcargo", "test"])).is_none());
    }

    #[test]
    fn compound_commands_are_matched_by_their_sub_commands() {
        let a = rule("a", &["cargo", "build"], RuleScope::User, "");
        let b = rule("b", &["cargo", "test"], RuleScope::User, "");
        let both = [&a, &b];
        let only_a = [&a];
        let script = v(&["sh", "-c", "cargo build && cargo test --lib"]);
        let used = covering_rules(&both, &script).expect("every sub-command is covered");
        assert_eq!(used.len(), 2);
        assert!(covering_rules(&only_a, &script).is_none(), "one is not");
        let script = v(&["bash", "-lc", "cargo build; cargo build"]);
        assert_eq!(covering_rules(&only_a, &script).unwrap().len(), 1);
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
            assert!(covering_rules(&rs, &argv).is_none(), "{script}");
        }
        // A quoted operator is a literal argument, not an operator.
        let argv = v(&["sh", "-c", "cargo test 'a|b' \"c>d\""]);
        assert!(covering_rules(&rs, &argv).is_some());
        // A shell that is not given a plain `-c script` is not provable.
        assert!(covering_rules(&rs, &v(&["sh", "run.sh"])).is_none());
        assert!(covering_rules(&rs, &v(&["sh", "-c", "cargo test", "$0"])).is_none());
        assert!(covering_rules(&rs, &v(&["sh", "--norc", "-c", "cargo test"])).is_none());
    }

    #[test]
    fn a_pattern_that_matches_a_command_inside_a_substitution_is_not_a_rule() {
        // The pattern itself cannot name the substitution...
        assert!(validate_pattern(&v(&["echo", "$(cargo test)"])).is_err());
        // ...and a command that hides one never reaches a rule.
        let r = rule("r", &["cargo", "test"], RuleScope::User, "");
        assert!(covering_rules(&[&r], &v(&["sh", "-c", "echo $(cargo test)"])).is_none());
        assert!(covering_rules(&[&r], &v(&["sh", "-c", "echo `cargo test`"])).is_none());
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
