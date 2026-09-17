//! PID 1 of a MicroVM (docs/21 "`modbit-guest`"): mounts, hostname, the
//! workspace block device, then the vsock listener the gateway connects
//! to. Nothing here reads a secret: the root image carries none and the
//! credential arrives at admission, per boot.

use std::path::Path;
use std::time::Instant;

use nix::mount::{MsFlags, mount};

fn cmdline_value(key: &str) -> Option<String> {
    let cmdline = std::fs::read_to_string("/proc/cmdline").ok()?;
    cmdline
        .split_whitespace()
        .find_map(|kv| kv.strip_prefix(&format!("{key}=")).map(str::to_owned))
}

fn mount_fs(source: &str, target: &str, fstype: &str, flags: MsFlags, data: Option<&str>) {
    if let Err(e) = std::fs::create_dir_all(target) {
        eprintln!("modbit-guest: creating {target}: {e}");
    }
    match mount(Some(source), target, Some(fstype), flags, data) {
        Ok(()) => eprintln!("modbit-guest: mounted {fstype} on {target}"),
        Err(e) => eprintln!("modbit-guest: mount {fstype} on {target}: {e}"),
    }
}

/// Run as init: mounts, hostname, workspace, then serve on vsock forever.
pub async fn run_as_init() -> std::io::Result<()> {
    // /proc first (the command line lives there), then the rest.
    mount_fs(
        "proc",
        "/proc",
        "proc",
        MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOEXEC,
        None,
    );
    mount_fs(
        "sysfs",
        "/sys",
        "sysfs",
        MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOEXEC,
        None,
    );
    mount_fs(
        "devtmpfs",
        "/dev",
        "devtmpfs",
        MsFlags::MS_NOSUID,
        Some("mode=0755"),
    );
    // PTY slaves live on devpts (M8.5): `/dev/pts/N` for every PTY opened.
    mount_fs(
        "devpts",
        "/dev/pts",
        "devpts",
        MsFlags::MS_NOSUID | MsFlags::MS_NOEXEC,
        Some("mode=0620,ptmxmode=0666"),
    );
    mount_fs(
        "tmpfs",
        "/tmp",
        "tmpfs",
        MsFlags::MS_NOSUID | MsFlags::MS_NODEV,
        Some("mode=1777"),
    );
    mount_fs(
        "tmpfs",
        "/run",
        "tmpfs",
        MsFlags::MS_NOSUID | MsFlags::MS_NODEV,
        Some("mode=0755"),
    );
    // PTYs: `posix_openpt` opens `/dev/ptmx`; where devtmpfs did not
    // create it, the devpts multiplexer stands in for it.
    if !Path::new("/dev/ptmx").exists() {
        match std::os::unix::fs::symlink("pts/ptmx", "/dev/ptmx") {
            Ok(()) => eprintln!("modbit-guest: /dev/ptmx -> pts/ptmx"),
            Err(e) => eprintln!("modbit-guest: creating /dev/ptmx: {e}"),
        }
    }
    let _ = nix::unistd::sethostname("modbit-guest");
    // The loopback interface is down until something raises it; the
    // guest's own egress proxy listens on it (M8.6).
    match std::process::Command::new("/bin/ip")
        .args(["link", "set", "lo", "up"])
        .status()
    {
        Ok(st) if st.success() => eprintln!("modbit-guest: lo up"),
        Ok(st) => eprintln!("modbit-guest: ip link set lo up: {st}"),
        Err(e) => eprintln!("modbit-guest: ip link set lo up: {e}"),
    }
    let workspace_dev = cmdline_value("modbit.workspace_dev").unwrap_or_else(|| "/dev/vdb".into());
    let _ = std::fs::create_dir_all("/workspace");
    let started = Instant::now();
    // The block device may appear a moment after the kernel hands over.
    let mut mounted = false;
    for _ in 0..200 {
        if Path::new(&workspace_dev).exists() {
            match mount(
                Some(workspace_dev.as_str()),
                "/workspace",
                Some("ext4"),
                MsFlags::MS_NOATIME,
                None::<&str>,
            ) {
                Ok(()) => {
                    mounted = true;
                    break;
                }
                Err(e) => eprintln!("modbit-guest: mount {workspace_dev} on /workspace: {e}"),
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    if !mounted {
        eprintln!(
            "modbit-guest: no workspace device at {workspace_dev}; /workspace is the root's (read-only)"
        );
    }
    let port: u32 = cmdline_value("modbit.vsock_port")
        .and_then(|p| p.parse().ok())
        .unwrap_or(5000);
    let egress_port: u32 = cmdline_value("modbit.egress_port")
        .and_then(|p| p.parse().ok())
        .unwrap_or(5001);
    crate::serve::set_broker(crate::proxy::BrokerAddr::Vsock(egress_port));
    let boot_id = uuid::Uuid::now_v7().to_string();
    let procs = std::sync::Arc::new(crate::procs::ProcTable::default());
    eprintln!(
        "modbit-guest: init up; sandbox {}; listening on vsock port {port}",
        cmdline_value("modbit.sandbox_id").unwrap_or_default()
    );
    let listener = vsock::VsockListener::bind_with_cid_port(vsock::VMADDR_CID_ANY, port)
        .map_err(std::io::Error::other)?;
    listener.set_nonblocking(true)?;
    let listener = tokio::io::unix::AsyncFd::new(listener)?;
    loop {
        let stream = loop {
            let mut guard = listener.readable().await?;
            match guard.try_io(|l| l.get_ref().accept()) {
                Ok(Ok((s, _addr))) => break s,
                Ok(Err(e)) => {
                    eprintln!("modbit-guest: accept: {e}");
                    continue;
                }
                Err(_would_block) => continue,
            }
        };
        stream.set_nonblocking(true)?;
        let stream = VsockAsync::new(stream)?;
        eprintln!("modbit-guest: vsock link accepted");
        let procs = std::sync::Arc::clone(&procs);
        let boot_id = boot_id.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::serve::serve_stream(stream, None, &boot_id, started, procs).await
            {
                eprintln!("modbit-guest: link ended: {e}");
            }
        });
    }
}

/// A vsock stream driven by tokio's `AsyncFd`.
pub(crate) struct VsockAsync(tokio::io::unix::AsyncFd<vsock::VsockStream>);

impl VsockAsync {
    /// Wrap a non-blocking vsock stream.
    pub(crate) fn new(s: vsock::VsockStream) -> std::io::Result<Self> {
        Ok(Self(tokio::io::unix::AsyncFd::new(s)?))
    }
}

impl tokio::io::AsyncRead for VsockAsync {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        use std::io::Read;
        loop {
            let mut guard = match self.0.poll_read_ready(cx) {
                std::task::Poll::Ready(Ok(g)) => g,
                std::task::Poll::Ready(Err(e)) => return std::task::Poll::Ready(Err(e)),
                std::task::Poll::Pending => return std::task::Poll::Pending,
            };
            let unfilled = buf.initialize_unfilled();
            match guard.try_io(|inner| inner.get_ref().read(unfilled)) {
                Ok(Ok(n)) => {
                    buf.advance(n);
                    return std::task::Poll::Ready(Ok(()));
                }
                Ok(Err(e)) => return std::task::Poll::Ready(Err(e)),
                Err(_would_block) => continue,
            }
        }
    }
}

impl tokio::io::AsyncWrite for VsockAsync {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        data: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        use std::io::Write;
        loop {
            let mut guard = match self.0.poll_write_ready(cx) {
                std::task::Poll::Ready(Ok(g)) => g,
                std::task::Poll::Ready(Err(e)) => return std::task::Poll::Ready(Err(e)),
                std::task::Poll::Pending => return std::task::Poll::Pending,
            };
            match guard.try_io(|inner| inner.get_ref().write(data)) {
                Ok(r) => return std::task::Poll::Ready(r),
                Err(_would_block) => continue,
            }
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let _ = self.0.get_ref().shutdown(std::net::Shutdown::Write);
        std::task::Poll::Ready(Ok(()))
    }
}
