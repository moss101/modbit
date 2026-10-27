//! DR-PX-2026-10-25-017 (PX-145): the derived verification plan names the
//! checks a JavaScript, TypeScript or Go project already has — a type check,
//! a build, a lint, the tests — with the package manager the project uses, so
//! a coding task that breaks the build is told by the engine and not left to
//! whichever tool the model happens to call.

use std::path::Path;

use modbit_verification::derive;

/// npm installs `tsc.cmd` on Windows and a bare `tsc` shim elsewhere.
const TSC_SHIM: &str = if cfg!(windows) {
    "node_modules/.bin/tsc.cmd"
} else {
    "node_modules/.bin/tsc"
};

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn ids(plan: &modbit_verification::VerificationPlan) -> Vec<(String, bool)> {
    plan.commands
        .iter()
        .map(|c| (c.id.clone(), c.mandatory))
        .collect()
}

#[test]
fn a_typescript_project_gets_a_build_a_lint_and_its_tests_with_its_own_package_manager() {
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        "package.json",
        r#"{"scripts":{"build":"tsc -p .","lint":"eslint .","test":"node --test"}}"#,
    );
    write(d.path(), "package-lock.json", "{}");
    write(d.path(), "tsconfig.json", "{}");
    write(d.path(), TSC_SHIM, "#!/bin/sh\n");
    let plan = derive(d.path(), &[], &[]);
    assert_eq!(
        ids(&plan),
        vec![
            ("build:node".into(), true),
            ("lint:node".into(), false),
            ("suite:npm-test".into(), true)
        ],
        "a build that already runs tsc is the type check: {plan:?}"
    );
    let argv = |id: &str| {
        plan.commands
            .iter()
            .find(|c| c.id == id)
            .unwrap()
            .argv
            .join(" ")
    };
    assert_eq!(argv("build:node"), "npm run build");
    assert_eq!(argv("suite:npm-test"), "npm test");
    assert!(
        plan.environment_inputs
            .contains(&"package-lock.json".to_string())
    );
    assert!(
        plan.limitations.iter().any(|l| l.contains("HEURISTIC")),
        "{plan:?}"
    );
}

#[test]
fn a_project_with_tsc_installed_and_no_script_is_type_checked_directly() {
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        "package.json",
        r#"{"scripts":{"start":"node ."}}"#,
    );
    write(d.path(), "tsconfig.json", "{}");
    // tsc is not installed yet: nothing is invented.
    assert!(derive(d.path(), &[], &[]).commands.is_empty());
    write(d.path(), TSC_SHIM, "#!/bin/sh\n");
    let plan = derive(d.path(), &[], &[]);
    assert_eq!(ids(&plan), vec![("typecheck:tsc".into(), true)]);
    let argv = &plan.commands[0].argv;
    assert_eq!(argv[1], "--noEmit");
    assert!(argv[0].starts_with(TSC_SHIM), "{argv:?}");
    // A named type-check script wins over the direct call.
    write(
        d.path(),
        "package.json",
        r#"{"scripts":{"typecheck":"tsc --noEmit -p tsconfig.app.json"}}"#,
    );
    let plan = derive(d.path(), &[], &[]);
    assert_eq!(ids(&plan), vec![("typecheck:node".into(), true)]);
}

#[test]
fn the_package_manager_comes_from_the_project_not_from_a_default() {
    for (lock, pm) in [
        ("pnpm-lock.yaml", "pnpm"),
        ("yarn.lock", "yarn"),
        ("bun.lockb", "bun"),
        ("none", "npm"),
    ] {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "package.json",
            r#"{"scripts":{"build":"vite build"}}"#,
        );
        if lock != "none" {
            write(d.path(), lock, "");
        }
        let plan = derive(d.path(), &[], &[]);
        assert_eq!(plan.commands[0].argv[0], pm, "{lock}");
        assert_eq!(plan.commands[0].argv[1], "run");
    }
    // The declared packageManager field outranks a stray lockfile.
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        "package.json",
        r#"{"packageManager":"pnpm@9.1.0","scripts":{"build":"vite build"}}"#,
    );
    write(d.path(), "package-lock.json", "{}");
    assert_eq!(derive(d.path(), &[], &[]).commands[0].argv[0], "pnpm");
}

#[test]
fn the_placeholder_test_script_is_not_a_check_and_a_vitest_project_keeps_its_reporter() {
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        "package.json",
        r#"{"scripts":{"test":"echo \"Error: no test specified\" && exit 1"}}"#,
    );
    assert!(derive(d.path(), &[], &[]).commands.is_empty());
    write(
        d.path(),
        "package.json",
        r#"{"devDependencies":{"vitest":"1"},"scripts":{"build":"tsc","test":"vitest"}}"#,
    );
    let plan = derive(d.path(), &[], &[]);
    let got = ids(&plan);
    assert_eq!(
        got,
        vec![("build:node".into(), true), ("suite:vitest".into(), true)]
    );
    assert_eq!(
        plan.commands[1].reporter_file.as_deref(),
        Some(".modbit-vitest.json")
    );
}

#[test]
fn a_go_module_gets_a_build_a_vet_and_its_tests() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "go.mod", "module x\n\ngo 1.22\n");
    let plan = derive(d.path(), &[], &[]);
    assert_eq!(
        ids(&plan),
        vec![
            ("build:go".into(), true),
            ("vet:go".into(), false),
            ("suite:go".into(), true)
        ]
    );
    assert_eq!(plan.stacks, vec!["go".to_string()]);
}
