//! Who a process is, as the operating system vouches for it (CUC-C01): the
//! executable path of the pid, the bundle identifier of the `.app` that path
//! lives in, the code-signing identity of that bundle, whether it runs as root.
//! Never a window title, never a name the process chose for itself.
#![allow(unsafe_code)]

use std::collections::HashMap;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use modbit_protocol::v1 as wire;

use super::cf::{Cf, CfStr, string_of};
use super::ffi::{
    CFBundleCreate, CFBundleGetIdentifier, CFURLCreateFromFileSystemRepresentation, CFRelease,
    SecCodeCopySigningInformation, SecStaticCodeCreateWithPath, geteuid, kCFAllocatorDefault,
    proc_listallpids, proc_pidinfo, proc_pidpath,
};

/// The executable path of `pid`, or `None` when there is no such process.
#[must_use]
pub fn exe_path(pid: u32) -> Option<PathBuf> {
    let mut buf = vec![0u8; 4096];
    let pid = i32::try_from(pid).ok()?;
    // SAFETY: `buf` is a valid writable buffer of the size passed.
    let n = unsafe { proc_pidpath(pid, buf.as_mut_ptr().cast::<c_void>(), 4096) };
    if n <= 0 {
        return None;
    }
    buf.truncate(usize::try_from(n).ok()?);
    Some(PathBuf::from(String::from_utf8_lossy(&buf).into_owned()))
}

/// Every process id.
#[must_use]
pub fn all_pids() -> Vec<u32> {
    // SAFETY: first call sizes the buffer, second fills it.
    unsafe {
        let n = proc_listallpids(std::ptr::null_mut(), 0);
        if n <= 0 {
            return vec![];
        }
        let cap = usize::try_from(n).unwrap_or(0) + 64;
        let mut buf = vec![0i32; cap];
        let bytes = i32::try_from(cap * 4).unwrap_or(i32::MAX);
        let got = proc_listallpids(buf.as_mut_ptr().cast::<c_void>(), bytes);
        if got <= 0 {
            return vec![];
        }
        buf.truncate(usize::try_from(got).unwrap_or(0).min(cap));
        buf.into_iter()
            .filter(|p| *p > 0)
            .filter_map(|p| u32::try_from(p).ok())
            .collect()
    }
}

/// Whether the process runs as root while the helper does not.
#[must_use]
pub fn is_elevated(pid: u32) -> bool {
    // PROC_PIDT_SHORTBSDINFO = 13; struct proc_bsdshortinfo is 64 bytes and
    // `pbsi_uid` sits at offset 36.
    let mut buf = [0u8; 64];
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: `buf` is the size passed.
    let n = unsafe { proc_pidinfo(pid, 13, 0, buf.as_mut_ptr().cast::<c_void>(), 64) };
    if n < 40 {
        return false;
    }
    let uid = u32::from_ne_bytes([buf[36], buf[37], buf[38], buf[39]]);
    // SAFETY: no arguments, no memory.
    uid == 0 && unsafe { geteuid() } != 0
}

/// The `.app` bundle directory an executable lives in, if any.
fn app_root(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
}

fn url_of(path: &Path) -> Option<Cf> {
    let b = path.as_os_str().as_encoded_bytes();
    // SAFETY: `b` outlives the call; the URL copies the bytes.
    let u = unsafe {
        CFURLCreateFromFileSystemRepresentation(
            kCFAllocatorDefault,
            b.as_ptr(),
            isize::try_from(b.len()).ok()?,
            u8::from(path.is_dir()),
        )
    };
    Cf::own(u)
}

fn bundle_id(root: &Path) -> String {
    let Some(url) = url_of(root) else {
        return String::new();
    };
    // SAFETY: `url` is live; the bundle follows the "create" rule, the
    // identifier the "get" rule (copied out before the bundle is released).
    unsafe {
        let b = CFBundleCreate(kCFAllocatorDefault, url.raw());
        if b.is_null() {
            return String::new();
        }
        let id = CFBundleGetIdentifier(b);
        let s = if id.is_null() {
            String::new()
        } else {
            string_of(id)
        };
        CFRelease(b);
        s
    }
}

type SignKey = (PathBuf, Option<SystemTime>);

fn signing_cache() -> &'static Mutex<HashMap<SignKey, String>> {
    static C: std::sync::OnceLock<Mutex<HashMap<SignKey, String>>> = std::sync::OnceLock::new();
    C.get_or_init(Mutex::default)
}

/// `"<TEAMID>:<identifier>"` for a signed code object (`"-"` for no team,
/// e.g. Apple's own and ad-hoc signed code), `""` for unsigned code. Cached
/// by path and modification time.
#[must_use]
pub fn signing_identity(path: &Path) -> String {
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let key = (path.to_path_buf(), mtime);
    if let Some(v) = signing_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
    {
        return v.clone();
    }
    let v = signing_identity_uncached(path);
    signing_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key, v.clone());
    v
}

fn signing_identity_uncached(path: &Path) -> String {
    let Some(url) = url_of(path) else {
        return String::new();
    };
    let mut code: *const c_void = std::ptr::null();
    // SAFETY: out-pointers are locals; every created object is released.
    unsafe {
        if SecStaticCodeCreateWithPath(url.raw(), 0, &raw mut code) != 0 || code.is_null() {
            return String::new();
        }
        let code = Cf::own(code);
        let Some(code) = code else {
            return String::new();
        };
        let mut info = std::ptr::null();
        // kSecCSSigningInformation = 2
        if SecCodeCopySigningInformation(code.raw(), 2, &raw mut info) != 0 {
            return String::new();
        }
        let Some(info) = Cf::own(info) else {
            return String::new();
        };
        let ident = info
            .dict_get(&CfStr::new("identifier"))
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        if ident.is_empty() {
            return String::new();
        }
        let team = info
            .dict_get(&CfStr::new("teamid"))
            .and_then(|v| v.as_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "-".into());
        format!("{team}:{ident}")
    }
}

/// The identity of `pid`, or `None` if it is gone.
#[must_use]
pub fn identity(pid: u32) -> Option<wire::ApplicationIdentity> {
    let exe = exe_path(pid)?;
    let root = app_root(&exe);
    let (bundle, name, signed_path) = match &root {
        Some(r) => (
            bundle_id(r),
            r.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            r.clone(),
        ),
        None => (
            String::new(),
            exe.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            exe.clone(),
        ),
    };
    Some(wire::ApplicationIdentity {
        bundle_id: bundle,
        executable_path: exe.to_string_lossy().into_owned(),
        signing_identity: signing_identity(&signed_path),
        pid,
        name,
    })
}

/// Whether `live` is the application `approved` named: every field the
/// person's approval was bound to.
#[must_use]
pub fn same(live: &wire::ApplicationIdentity, approved: &wire::ApplicationIdentity) -> bool {
    live.pid == approved.pid
        && live.bundle_id == approved.bundle_id
        && live.executable_path == approved.executable_path
        && live.signing_identity == approved.signing_identity
}
