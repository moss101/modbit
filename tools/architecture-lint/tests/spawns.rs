//! PX-067: the Git spawn lint, on disposable source trees and on this
//! repository itself.

use std::fs;
use std::path::Path;

use architecture_lint::Rules;
use architecture_lint::spawns::check_spawns;

const RULES: &str = r#"
[[spawns]]
program = "git"
roots = ["crates", "services"]
allowed = ["crates/git/src/"]
reason = "test: only the runner spawns git"
"#;

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

#[test]
fn a_spawn_outside_the_runner_is_reported_with_its_place() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    write(
        r,
        "crates/git/src/harden.rs",
        "fn build() { let _ = Command::new(\"git\"); }\n",
    );
    write(
        r,
        "crates/other/src/lib.rs",
        "fn a() {}\nfn b() { let _ = std::process::Command::new(\"git\").arg(\"status\"); }\n",
    );
    write(
        r,
        "services/core/src/x.rs",
        "// Command::new(\"git\") in a comment is not a spawn\nfn ok() { Command::new(\"cargo\"); }\n",
    );
    write(
        r,
        "services/core/src/y.rs",
        "fn prod() {}\n#[cfg(test)]\nmod tests { fn t() { Command::new(\"git\"); } }\n",
    );
    write(
        r,
        "crates/other/tests/it.rs",
        "fn t() { Command::new(\"git\"); }\n",
    );
    let rules = Rules::parse(RULES).unwrap();
    let v = check_spawns(r, &rules.spawns[0]).unwrap();
    assert_eq!(v.len(), 1, "{v:?}");
    assert_eq!(v[0].path, "crates/other/src/lib.rs");
    assert_eq!(v[0].line, 2);
    assert!(v[0].to_string().contains("only the runner spawns git"));
}

/// The repository itself: every Git process outside `crates/git` is gone.
#[test]
fn this_repository_spawns_git_only_in_the_runner() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let rules = Rules::load(&root.join("tools/architecture-lint/rules.toml")).unwrap();
    assert!(!rules.spawns.is_empty(), "the rule is configured");
    for rule in &rules.spawns {
        let v = check_spawns(&root, rule).unwrap();
        assert!(
            v.is_empty(),
            "a Git spawn outside the hardened runner:\n{}",
            v.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}
