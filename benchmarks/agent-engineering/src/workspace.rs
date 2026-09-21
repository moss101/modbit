//! A fixture copy the product can work on: build products left out, text
//! LF-normalized, the developer's installed modules linked in by an absolute
//! path (a relative link target would resolve against the link's own
//! directory and dangle, which is exactly what happened on the first hosted
//! runs).

use std::path::Path;

/// Copy a fixture without its build products, LF-normalized, and link its
/// installed modules so a configured command runs as a developer's would.
///
/// # Errors
/// A file cannot be read or written, or the installed modules cannot be resolved.
pub fn copy_fixture(src: &Path, dst: &Path) -> Result<(), String> {
    fn walk(src: &Path, dst: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dst).map_err(|e| format!("{}: {e}", dst.display()))?;
        for e in std::fs::read_dir(src).map_err(|e| format!("{}: {e}", src.display()))? {
            let e = e.map_err(|e| e.to_string())?;
            let n = e.file_name();
            let name = n.to_string_lossy();
            if name == "target"
                || name == "node_modules"
                || name == ".vitest"
                || name.starts_with(".vite")
            {
                continue;
            }
            let p = e.path();
            if p.is_dir() {
                walk(&p, &dst.join(&n))?;
            } else {
                let bytes = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
                std::fs::write(dst.join(&n), text).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    walk(src, dst)?;
    // The link's target must be absolute: a relative target is resolved
    // against the link's own directory, not the caller's, and dangles.
    let installed = src.join("node_modules");
    if installed.exists() {
        let installed = installed
            .canonicalize()
            .map_err(|e| format!("{}: {e}", installed.display()))?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(&installed, dst.join("node_modules"))
            .map_err(|e| e.to_string())?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&installed, dst.join("node_modules"))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
