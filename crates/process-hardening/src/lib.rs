//! `modbit-process-hardening` (PX-148): what the Core and the execution broker
//! do to themselves in the first statement of `main`, before any thread or
//! child exists.
//!
//! * The core-dump limit is set to zero (soft and hard), so a crash cannot
//!   write the process's memory, which holds model keys and secret-broker
//!   handles, to disk.
//! * The process is made un-attachable: `PR_SET_DUMPABLE=0` on Linux (a
//!   same-user process can no longer `ptrace` it or read its memory through
//!   `/proc`), `PT_DENY_ATTACH` on macOS.
//! * The library-injection variables (`LD_*`, `DYLD_*`) are removed from the
//!   environment. A variable that is already in the environment at start has
//!   done its work by the time `main` runs: the loader has mapped the library.
//!   So when one is present the process re-executes itself once with the
//!   variables removed, before it opens a store or reads a key, and the image
//!   that serves is clean. The injected library's constructor can run once, in
//!   the short image that is replaced; it cannot run in the serving process,
//!   and no child inherits the variable.
//!
//! A flag that cannot be set is reported to the caller, which logs it; nothing
//! continues silently. On Windows the module does nothing and says so.
//!
//! Children keep their own pinned environment (the broker and the Core build
//! it explicitly); this module does not touch it. A child inherits the
//! core-dump limit of zero, which is the intent.
//!
//! This crate holds the two `unsafe` blocks that make the calls (`setrlimit`
//! and `prctl`/`ptrace`), each with a SAFETY note; the workspace denies
//! `unsafe_code` and notes this exception in the root `Cargo.toml`.

#![cfg_attr(not(unix), forbid(unsafe_code))]

/// The environment variable that marks the second image of the re-execution,
/// so the process executes itself at most once. It is removed on sight.
pub const REEXEC_MARK: &str = "MODBIT_HARDENING_REEXEC";

/// What `harden` did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// The names of the protections that are in effect.
    pub applied: Vec<&'static str>,
    /// Each protection that could not be applied, with the reason. The caller
    /// logs these.
    pub failed: Vec<String>,
    /// The injection variables that were removed from the environment.
    pub removed_env: Vec<String>,
    /// A developer opted out of the attach protection (debug builds only).
    pub attach_allowed_for_debugging: bool,
}

/// Whether `name` is a variable the dynamic loader reads to load or locate
/// code: every `LD_*` and `DYLD_*`.
#[must_use]
pub fn is_loader_variable(name: &str) -> bool {
    name.starts_with("LD_") || name.starts_with("DYLD_")
}

/// The loader variables in the current environment, sorted.
#[must_use]
pub fn loader_variables_present() -> Vec<String> {
    let mut v: Vec<String> = std::env::vars_os()
        .filter_map(|(k, _)| k.into_string().ok())
        .filter(|k| is_loader_variable(k))
        .collect();
    // A name that is not valid Unicode cannot be matched above; check the bytes.
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        for (k, _) in std::env::vars_os() {
            let b = k.as_bytes();
            if (b.starts_with(b"LD_") || b.starts_with(b"DYLD_"))
                && !v.iter().any(|n| n.as_bytes() == b)
            {
                v.push(String::from_utf8_lossy(b).into_owned());
            }
        }
    }
    v.sort();
    v
}

/// Apply the hardening. Call it as the first statement of `main`, before any
/// thread starts and before anything is opened or read. On Unix it may not
/// return: when an injection variable is present it replaces the process image
/// with a clean one (see the crate docs) and the new image calls `harden` again.
pub fn harden() -> Outcome {
    #[cfg(unix)]
    {
        unix::harden()
    }
    #[cfg(not(unix))]
    {
        Outcome {
            failed: vec![
                "process hardening is not implemented on this platform (no core-dump limit, no attach denial, no loader-variable removal)"
                    .to_owned(),
            ],
            ..Outcome::default()
        }
    }
}

/// A one-line JSON description of the state the hardening leaves, read from
/// the running process: the core-dump limit, the dumpable flag (Linux), the
/// loader variables still in the environment and the libraries the loader has
/// mapped, with what `harden` applied and what it could not. It is what
/// `--hardening-report` of the Core and the broker prints, so a qualification
/// reads the state from the process and not from the code. (macOS offers no
/// way to read the attach-denial flag back, so there the report states that
/// the call succeeded.)
#[must_use]
pub fn report_json(outcome: &Outcome) -> String {
    #[cfg(unix)]
    {
        unix::report_json(outcome)
    }
    #[cfg(not(unix))]
    {
        let _ = outcome;
        "{\"platform\":\"unsupported\"}".to_owned()
    }
}

#[cfg(unix)]
mod unix {
    use super::{Outcome, REEXEC_MARK, loader_variables_present};

    pub(super) fn harden() -> Outcome {
        let mut out = Outcome::default();
        let marked = std::env::var_os(REEXEC_MARK).is_some();
        let present = loader_variables_present();
        if !present.is_empty() && !marked {
            reexec_without(&present, &mut out);
        }
        // Reached as the second image, or when the re-execution failed: the
        // variables are removed from this process's environment either way.
        for name in loader_variables_present() {
            // SAFETY: `harden` runs before any thread is started (it is the
            // first statement of `main`), so no other thread reads the
            // environment concurrently.
            #[allow(unsafe_code)]
            unsafe {
                std::env::remove_var(&name);
            }
            out.removed_env.push(name);
        }
        if marked {
            // SAFETY: as above.
            #[allow(unsafe_code)]
            unsafe {
                std::env::remove_var(REEXEC_MARK);
            }
        }
        no_core_dumps(&mut out);
        deny_attach(&mut out);
        out
    }

    /// Replace this process with a copy of itself that has no injection
    /// variable. On failure the reason is recorded and the process goes on with
    /// its environment cleaned (the library, if any, stays mapped in this image).
    fn reexec_without(present: &[String], out: &mut Outcome) {
        use std::os::unix::process::CommandExt;
        let exe = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                out.failed.push(format!(
                    "loader variables {present:?} were present and the process could not find its own executable to re-execute without them: {e}"
                ));
                return;
            }
        };
        let mut cmd = std::process::Command::new(&exe);
        cmd.args(std::env::args_os().skip(1));
        for name in present {
            cmd.env_remove(name);
        }
        cmd.env(REEXEC_MARK, "1");
        // `exec` only returns on failure.
        let err = cmd.exec();
        out.failed.push(format!(
            "loader variables {present:?} were present and re-executing without them failed: {err}"
        ));
    }

    fn no_core_dumps(out: &mut Outcome) {
        let zero = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: `setrlimit` reads one fully initialised `rlimit` that lives
        // for the call and has no other effect on memory.
        #[allow(unsafe_code)]
        let rc = unsafe { libc::setrlimit(libc::RLIMIT_CORE, &zero) };
        if rc == 0 {
            out.applied.push("core_dump_limit_zero");
        } else {
            out.failed.push(format!(
                "setting the core-dump limit to zero failed: {}",
                std::io::Error::last_os_error()
            ));
        }
    }

    #[cfg(target_os = "linux")]
    fn deny_attach(out: &mut Outcome) {
        // SAFETY: `prctl(PR_SET_DUMPABLE, 0)` takes integer arguments only.
        #[allow(unsafe_code)]
        let rc = unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) };
        if rc == 0 {
            out.applied.push("not_dumpable");
        } else {
            out.failed.push(format!(
                "marking the process non-dumpable failed: {}",
                std::io::Error::last_os_error()
            ));
        }
    }

    #[cfg(target_os = "macos")]
    fn deny_attach(out: &mut Outcome) {
        if cfg!(debug_assertions) && std::env::var_os("MODBIT_ALLOW_DEBUGGER").is_some() {
            out.attach_allowed_for_debugging = true;
            return;
        }
        // SAFETY: `ptrace(PT_DENY_ATTACH, 0, null, 0)` takes no pointer that is
        // read or written; it sets a flag on the calling process.
        #[allow(unsafe_code)]
        let rc = unsafe { libc::ptrace(libc::PT_DENY_ATTACH, 0, std::ptr::null_mut(), 0) };
        if rc == 0 {
            out.applied.push("attach_denied");
        } else {
            out.failed.push(format!(
                "denying debugger attach failed: {}",
                std::io::Error::last_os_error()
            ));
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fn deny_attach(out: &mut Outcome) {
        out.failed
            .push("denying attach is not implemented on this platform".to_owned());
    }

    fn core_limit() -> (i128, i128) {
        let mut l = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: `getrlimit` writes one `rlimit` the call owns.
        #[allow(unsafe_code)]
        let rc = unsafe { libc::getrlimit(libc::RLIMIT_CORE, &mut l) };
        if rc == 0 {
            (i128::from(l.rlim_cur), i128::from(l.rlim_max))
        } else {
            (-1, -1)
        }
    }

    #[cfg(target_os = "linux")]
    fn dumpable() -> String {
        // SAFETY: `prctl(PR_GET_DUMPABLE)` takes integer arguments only.
        #[allow(unsafe_code)]
        let v = unsafe { libc::prctl(libc::PR_GET_DUMPABLE, 0, 0, 0, 0) };
        v.to_string()
    }

    #[cfg(not(target_os = "linux"))]
    fn dumpable() -> String {
        "null".to_owned()
    }

    #[cfg(target_os = "linux")]
    fn images() -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_to_string("/proc/self/maps")
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split_whitespace().nth(5))
            .filter(|p| p.starts_with('/'))
            .map(str::to_owned)
            .collect();
        v.sort();
        v.dedup();
        v
    }

    #[cfg(target_os = "macos")]
    #[allow(unsafe_code)]
    unsafe extern "C" {
        // The dyld image list. `libc` marks these deprecated in favour of a
        // crate this one does not take for two functions; the symbols are in
        // libSystem.
        fn _dyld_image_count() -> u32;
        fn _dyld_get_image_name(image_index: u32) -> *const std::ffi::c_char;
    }

    #[cfg(target_os = "macos")]
    fn images() -> Vec<String> {
        // SAFETY: the dyld image list is read through its two documented
        // accessors; each returned name is a NUL-terminated string owned by
        // dyld and copied here before the call returns.
        #[allow(unsafe_code)]
        unsafe {
            (0.._dyld_image_count())
                .filter_map(|i| {
                    let p = _dyld_get_image_name(i);
                    (!p.is_null())
                        .then(|| std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned())
                })
                .collect()
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fn images() -> Vec<String> {
        Vec::new()
    }

    fn json_string(s: &str) -> String {
        let mut o = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => o.push_str("\\\""),
                '\\' => o.push_str("\\\\"),
                c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
                c => o.push(c),
            }
        }
        o.push('"');
        o
    }

    pub(super) fn report_json(outcome: &Outcome) -> String {
        let (soft, hard) = core_limit();
        let list = |v: Vec<String>| {
            format!(
                "[{}]",
                v.iter()
                    .map(|s| json_string(s))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        format!(
            "{{\"pid\":{},\"applied\":{},\"failed\":{},\"removed_env\":{},\"core_limit_soft\":{soft},\"core_limit_hard\":{hard},\"dumpable\":{},\"loader_env\":{},\"reexec_mark\":{},\"images\":{}}}",
            std::process::id(),
            list(outcome.applied.iter().map(|s| (*s).to_owned()).collect()),
            list(outcome.failed.clone()),
            list(outcome.removed_env.clone()),
            dumpable(),
            list(loader_variables_present()),
            std::env::var_os(REEXEC_MARK).is_some(),
            list(images()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_variables_are_every_ld_and_dyld_name() {
        for yes in [
            "LD_PRELOAD",
            "LD_LIBRARY_PATH",
            "LD_AUDIT",
            "DYLD_INSERT_LIBRARIES",
            "DYLD_LIBRARY_PATH",
            "DYLD_FRAMEWORK_PATH",
        ] {
            assert!(is_loader_variable(yes), "{yes}");
        }
        for no in [
            "PATH",
            "HOME",
            "OLD_PRELOAD",
            "MODBIT_LD_X",
            "LDFLAGS",
            "DYLD",
        ] {
            assert!(!is_loader_variable(no), "{no}");
        }
    }
}
