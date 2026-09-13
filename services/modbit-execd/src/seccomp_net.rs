//! Linux review sandbox (EPR-018, docs/21 `review_isolated`): a seccomp-BPF
//! filter that leaves a process — and everything it spawns — unable to open
//! a network socket. `socket(2)` answers `EPERM` for every family but
//! `AF_UNIX`, and `io_uring_setup(2)` is refused so no ring can open one
//! either; `no_new_privs` locks the filter in so a setuid helper cannot shed
//! it. It needs no user namespace, which hosts increasingly withhold from
//! unprivileged processes (Ubuntu 24.04 restricts them by default), and no
//! helper binary: the broker re-executes itself as the launcher.
//!
//! Writes are not confined here; on Linux they are the lease's to confine.
//! `AF_UNIX` stays open, so a local daemon reached over a Unix socket that
//! has network of its own is a way out this filter does not close (neither
//! does a network namespace).

use std::ffi::OsString;
use std::io;

#[cfg(target_arch = "x86_64")]
const AUDIT_ARCH: u32 = 0xc000003e; // AUDIT_ARCH_X86_64
#[cfg(target_arch = "aarch64")]
const AUDIT_ARCH: u32 = 0xc00000b7; // AUDIT_ARCH_AARCH64

fn stmt(code: u32, k: u32) -> libc::sock_filter {
    libc::sock_filter {
        code: code as u16,
        jt: 0,
        jf: 0,
        k,
    }
}

fn jeq(k: u32, jt: u8, jf: u8) -> libc::sock_filter {
    libc::sock_filter {
        code: (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16,
        jt,
        jf,
        k,
    }
}

#[cfg(target_arch = "x86_64")]
fn jge(k: u32, jt: u8, jf: u8) -> libc::sock_filter {
    libc::sock_filter {
        code: (libc::BPF_JMP | libc::BPF_JGE | libc::BPF_K) as u16,
        jt,
        jf,
        k,
    }
}

/// Install the no-network filter in this process. Every child inherits it.
// SAFETY exception (workspace `unsafe_code = "deny"`, noted in Cargo.toml):
// two prctl calls on this process with a filter program that outlives them.
#[allow(unsafe_code)]
pub fn install() -> io::Result<()> {
    let load = libc::BPF_LD | libc::BPF_W | libc::BPF_ABS;
    let ret = libc::BPF_RET | libc::BPF_K;
    let arch = std::mem::offset_of!(libc::seccomp_data, arch) as u32;
    let nr = std::mem::offset_of!(libc::seccomp_data, nr) as u32;
    // Low 32 bits of args[0] (little-endian on both supported arches).
    let arg0 = std::mem::offset_of!(libc::seccomp_data, args) as u32;
    let eperm = libc::SECCOMP_RET_ERRNO | (libc::EPERM as u32 & libc::SECCOMP_RET_DATA);
    let filter = [
        // A foreign ABI is not inspected: the process is killed. On x86_64
        // the x32 ABI shares the arch value and numbers its calls above
        // X32_SYSCALL_BIT; it is killed too, or `socket` would slip past.
        stmt(load, arch),
        jeq(AUDIT_ARCH, 1, 0),
        stmt(ret, libc::SECCOMP_RET_KILL_PROCESS),
        stmt(load, nr),
        #[cfg(target_arch = "x86_64")]
        jge(0x4000_0000, 0, 1),
        #[cfg(target_arch = "x86_64")]
        stmt(ret, libc::SECCOMP_RET_KILL_PROCESS),
        // io_uring_setup: refused outright.
        jeq(libc::SYS_io_uring_setup as u32, 0, 1),
        stmt(ret, eperm),
        // Anything but socket: allowed.
        jeq(libc::SYS_socket as u32, 1, 0),
        stmt(ret, libc::SECCOMP_RET_ALLOW),
        // socket(AF_UNIX, ..): allowed; every other family: EPERM.
        stmt(load, arg0),
        jeq(libc::AF_UNIX as u32, 0, 1),
        stmt(ret, libc::SECCOMP_RET_ALLOW),
        stmt(ret, eperm),
    ];
    let prog = libc::sock_fprog {
        len: filter.len() as u16,
        filter: filter.as_ptr().cast_mut(),
    };
    // SAFETY: plain prctl calls with a filter program that outlives them.
    unsafe {
        if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
            return Err(io::Error::last_os_error());
        }
        if libc::prctl(
            libc::PR_SET_SECCOMP,
            libc::SECCOMP_MODE_FILTER,
            &prog as *const libc::sock_fprog,
        ) != 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// `modbit-execd --review-sandbox-exec -- <argv...>`: install the filter and
/// become `argv`. Only returns on failure.
pub fn launch(argv: Vec<OsString>) -> io::Error {
    use std::os::unix::process::CommandExt;
    let Some((program, rest)) = argv.split_first() else {
        return io::Error::new(io::ErrorKind::InvalidInput, "no program to launch");
    };
    if let Err(e) = install() {
        return e;
    }
    std::process::Command::new(program).args(rest).exec()
}

/// `modbit-execd --review-sandbox-selfcheck`: true when this process cannot
/// open an inet socket (the launcher's filter is in force).
// SAFETY exception (workspace `unsafe_code = "deny"`, noted in Cargo.toml):
// one socket call whose descriptor, if any, is closed at once.
#[allow(unsafe_code)]
pub fn selfcheck() -> bool {
    // SAFETY: a socket call whose descriptor, if any, is closed at once.
    unsafe {
        let fd = libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0);
        if fd >= 0 {
            libc::close(fd);
            return false;
        }
        *libc::__errno_location() == libc::EPERM
    }
}
