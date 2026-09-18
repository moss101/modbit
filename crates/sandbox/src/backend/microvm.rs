//! The MicroVM backend: a Firecracker MicroVM per sandbox over KVM
//! (REQ-EV-0285 — a hardened MicroVM-class guest behind the Modbit-owned
//! gateway). The kernel and the guest root image are the gateway's and
//! immutable (the root is attached read-only; it holds `modbit-guest` as
//! `/init` and a static userland, no tenant data and no secret); the
//! workspace is a per-sandbox ext4 image built from the workspace source
//! and attached read-write; the channel is vsock (a Unix socket on the host
//! that Firecracker bridges into the guest). Network: a guest never gets a
//! network interface; what its policy grants leaves through the gateway's
//! egress broker over a second vsock port (M8.6), on the host side.
//! Every step is the substrate's own API over its socket; nothing is
//! simulated: without `/dev/kvm`, `firecracker`, the kernel and the root
//! image the backend refuses to provision.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};

use super::{BoxFuture, Provisioned, SandboxBackend};
use crate::image::{self, ImageManifest, SignedManifest, VerifiedImage};
use crate::policy::CompiledPolicy;
use crate::{Result, SandboxError};

/// Where the substrate's pieces are.
#[derive(Clone, Debug)]
pub struct MicrovmConfig {
    /// The `firecracker` binary.
    pub firecracker_bin: PathBuf,
    /// The guest kernel (`vmlinux`).
    pub kernel: PathBuf,
    /// The immutable guest root image (ext4) with `modbit-guest` as `/init`.
    pub rootfs: PathBuf,
    /// Per-sandbox state (sockets, workspace images, console logs).
    pub work_dir: PathBuf,
    /// The vsock port the guest listens on.
    pub vsock_port: u32,
    /// How long a boot may take before the guest answers on vsock.
    pub boot_timeout: Duration,
    /// The root image's signed manifest (M8.4): the backend boots only an
    /// image whose manifest verifies under a trusted publisher key.
    pub manifest: PathBuf,
    /// Trusted publisher keys (`key id`, verifying key).
    pub trusted_keys: Vec<(String, [u8; 32])>,
    /// What the image's guests can do beyond the contract's core
    /// (IMP-EV-0072; `MODBIT_GUEST_FEATURES`, e.g. `browser`).
    pub features: Vec<String>,
}

impl MicrovmConfig {
    /// From the environment: `MODBIT_FIRECRACKER_BIN`, `MODBIT_GUEST_KERNEL`,
    /// `MODBIT_GUEST_IMAGE_MANIFEST`, `MODBIT_GUEST_IMAGE_KEYS` ("id:hex,..."),
    /// `MODBIT_GUEST_ROOTFS`, `MODBIT_SANDBOX_WORK_DIR`; `None` when any is unset.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let get = |k: &str| {
            std::env::var(k)
                .ok()
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        Some(Self {
            firecracker_bin: get("MODBIT_FIRECRACKER_BIN")?,
            kernel: get("MODBIT_GUEST_KERNEL")?,
            rootfs: get("MODBIT_GUEST_ROOTFS")?,
            work_dir: get("MODBIT_SANDBOX_WORK_DIR")
                .unwrap_or_else(|| std::env::temp_dir().join("modbit-sandboxes")),
            vsock_port: 5000,
            boot_timeout: Duration::from_secs(60),
            manifest: get("MODBIT_GUEST_IMAGE_MANIFEST")?,
            trusted_keys: crate::image::trusted_keys_from_env(
                &std::env::var("MODBIT_GUEST_IMAGE_KEYS").unwrap_or_default(),
            ),
            features: std::env::var("MODBIT_GUEST_FEATURES")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
        })
    }

    /// Why this host cannot run MicroVMs, or `None` when it can.
    #[must_use]
    pub fn unavailable_reason(&self) -> Option<String> {
        if !cfg!(target_os = "linux") {
            return Some("Firecracker runs on Linux only".into());
        }
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/kvm")
        {
            Ok(_) => {}
            Err(e) => return Some(format!("/dev/kvm: {e}")),
        }
        for (what, p) in [
            ("firecracker", &self.firecracker_bin),
            ("kernel", &self.kernel),
            ("rootfs", &self.rootfs),
            ("manifest", &self.manifest),
        ] {
            if !p.is_file() {
                return Some(format!("{what} `{}` is not a file", p.display()));
            }
        }
        None
    }
}

/// The host vsock port the guest's egress proxy connects to (M8.6).
pub const EGRESS_PORT: u32 = 5001;

/// The MicroVM backend.
pub struct MicrovmBackend {
    cfg: MicrovmConfig,
    image: VerifiedImage,
    vms: Mutex<HashMap<String, Child>>,
}

impl MicrovmBackend {
    /// A backend over `cfg`; refuses (`IMAGE_UNVERIFIED`) unless the root
    /// image's manifest verifies under a trusted key and the image on disk
    /// is the one it names.
    pub fn new(cfg: MicrovmConfig) -> Result<Self> {
        let signed: SignedManifest = serde_json::from_str(&std::fs::read_to_string(&cfg.manifest)?)
            .map_err(|e| SandboxError::Refused {
                code: "IMAGE_UNVERIFIED".into(),
                message: format!("`{}` is not a signed manifest: {e}", cfg.manifest.display()),
            })?;
        let image = image::verify(&signed, &cfg.rootfs, &cfg.trusted_keys)?;
        if image.manifest.kind != "microvm-rootfs" {
            return Err(SandboxError::Refused {
                code: "IMAGE_UNVERIFIED".into(),
                message: format!(
                    "the manifest describes a `{}`, not a MicroVM root image",
                    image.manifest.kind
                ),
            });
        }
        if !image.manifest.kernel_sha256.is_empty() {
            let (k, _) = image::sha256_file(&cfg.kernel)?;
            if k != image.manifest.kernel_sha256 {
                return Err(SandboxError::Refused {
                    code: "IMAGE_UNVERIFIED".into(),
                    message: format!(
                        "the kernel `{}` is not the one the image was built for",
                        cfg.kernel.display()
                    ),
                });
            }
        }
        Ok(Self {
            cfg,
            image,
            vms: Mutex::new(HashMap::new()),
        })
    }

    /// The configuration.
    #[must_use]
    pub fn config(&self) -> &MicrovmConfig {
        &self.cfg
    }
}

/// One request to Firecracker's API over its Unix socket.
async fn api(sock: &Path, method: &str, path: &str, body: &str) -> Result<(u16, String)> {
    let mut s = UnixStream::connect(sock)
        .await
        .map_err(|e| SandboxError::Substrate(format!("firecracker api socket: {e}")))?;
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).await?;
    // Firecracker keeps the connection open: read the head, then exactly
    // the body the head announces (never to EOF).
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    let head_end = loop {
        if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        let n = tokio::time::timeout(Duration::from_secs(30), s.read(&mut buf))
            .await
            .map_err(|_| {
                SandboxError::Substrate("firecracker api: no response within 30 s".into())
            })??;
        if n == 0 {
            break raw.len();
        }
        raw.extend_from_slice(&buf[..n]);
    };
    let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let content_length: usize = head
        .lines()
        .find_map(|l| {
            l.split_once(':')
                .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                .map(|(_, v)| v.trim().parse().unwrap_or(0))
        })
        .unwrap_or(0);
    let mut response_body = raw[head_end..].to_vec();
    while response_body.len() < content_length {
        let n = tokio::time::timeout(Duration::from_secs(30), s.read(&mut buf))
            .await
            .map_err(|_| SandboxError::Substrate("firecracker api: truncated response".into()))??;
        if n == 0 {
            break;
        }
        response_body.extend_from_slice(&buf[..n]);
    }
    Ok((status, String::from_utf8_lossy(&response_body).into_owned()))
}

async fn api_put(sock: &Path, path: &str, body: serde_json::Value) -> Result<()> {
    let (status, text) = api(sock, "PUT", path, &body.to_string()).await?;
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(SandboxError::Substrate(format!(
            "firecracker PUT {path}: {status} {text}"
        )))
    }
}

/// Build an ext4 image of `src` (`mkfs.ext4 -d`), `size_mib` large.
fn make_ext4(src: &Path, img: &Path, size_mib: u32) -> Result<()> {
    let f = std::fs::File::create(img)?;
    f.set_len(u64::from(size_mib.max(16)) * 1024 * 1024)?;
    drop(f);
    let mut cmd = std::process::Command::new("mkfs.ext4");
    cmd.arg("-F").arg("-q");
    if src.is_dir() {
        cmd.arg("-d").arg(src);
    }
    cmd.arg(img);
    let out = cmd
        .output()
        .map_err(|e| SandboxError::Substrate(format!("mkfs.ext4: {e}")))?;
    if !out.status.success() {
        return Err(SandboxError::Substrate(format!(
            "mkfs.ext4 failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(())
}

/// Connect to the guest's vsock port through Firecracker's host socket.
async fn connect_vsock(uds: &Path, port: u32) -> std::io::Result<UnixStream> {
    let mut s = UnixStream::connect(uds).await?;
    s.write_all(format!("CONNECT {port}\n").as_bytes()).await?;
    // The answer line is read a byte at a time: the guest's first frame
    // (its hello) can follow `OK` at once, and a buffered read would
    // swallow it.
    let mut line = Vec::new();
    let answer = tokio::time::timeout(Duration::from_secs(5), async {
        let mut b = [0u8; 1];
        loop {
            let n = s.read(&mut b).await?;
            if n == 0 {
                break;
            }
            if b[0] == b'\n' {
                break;
            }
            line.push(b[0]);
            if line.len() > 64 {
                break;
            }
        }
        Ok::<(), std::io::Error>(())
    })
    .await
    .map_err(|_| std::io::Error::other("vsock connect: no answer"));
    answer??;
    let line = String::from_utf8_lossy(&line).into_owned();
    if line.starts_with("OK ") {
        Ok(s)
    } else {
        Err(std::io::Error::other(format!(
            "vsock connect: {}",
            line.trim()
        )))
    }
}

impl SandboxBackend for MicrovmBackend {
    fn kind(&self) -> &'static str {
        "microvm"
    }

    fn isolates(&self) -> bool {
        true
    }

    fn features(&self) -> Vec<String> {
        let mut f = vec!["egress".to_owned(), "pty".to_owned()];
        // The image's features are the deployment's to declare
        // (`MODBIT_GUEST_FEATURES=browser,…`): what its rootfs carries.
        for x in self.cfg.features.iter() {
            if !f.contains(x) {
                f.push(x.clone());
            }
        }
        f
    }

    fn image(&self) -> Option<&ImageManifest> {
        Some(&self.image.manifest)
    }

    fn provision<'a>(
        &'a self,
        sandbox_id: &'a str,
        policy: &'a CompiledPolicy,
    ) -> BoxFuture<'a, Result<Provisioned>> {
        Box::pin(async move {
            if let Some(why) = self.cfg.unavailable_reason() {
                return Err(SandboxError::Substrate(why));
            }

            // The image on disk is still the one the publisher signed —
            // hashed off the runtime's threads: an image with a browser in
            // it is hundreds of megabytes, and the gateway's other work (a
            // worker's lease heartbeat in the same process, in tests) must
            // not wait for it.
            {
                let manifest = self.image.manifest.clone();
                let rootfs = self.cfg.rootfs.clone();
                tokio::task::spawn_blocking(move || image::check_image(&manifest, &rootfs))
                    .await
                    .map_err(|e| SandboxError::Substrate(format!("image check: {e}")))??;
            }
            let dir = self.cfg.work_dir.join(sandbox_id);
            std::fs::create_dir_all(&dir)?;
            let ws_img = dir.join("workspace.ext4");
            {
                let src = policy.spec.workspace_source.clone();
                let img = ws_img.clone();
                let mib = policy.spec.resources.workspace_mib;
                tokio::task::spawn_blocking(move || make_ext4(&src, &img, mib))
                    .await
                    .map_err(|e| SandboxError::Substrate(format!("mkfs: {e}")))??;
            }
            let api_sock = dir.join("api.sock");
            let vsock = dir.join("v.sock");
            let console = dir.join("console.log");
            let _ = std::fs::remove_file(&api_sock);
            let _ = std::fs::remove_file(&vsock);
            // Egress (M8.6): guest-initiated vsock connections to host port
            // 5001 arrive on `<uds>_5001`; the broker serves what arrives.
            // Bound before the boot so nothing the guest opens is missed.
            let egress_rx = if policy.egress_proxy {
                let path = dir.join(format!("v.sock_{}", EGRESS_PORT));
                let _ = std::fs::remove_file(&path);
                let listener = tokio::net::UnixListener::bind(&path)?;
                let (tx, rx) = tokio::sync::mpsc::channel::<super::Channel>(64);
                tokio::spawn(async move {
                    while let Ok((s, _)) = listener.accept().await {
                        if tx.send(Box::new(s)).await.is_err() {
                            break;
                        }
                    }
                });
                Some(rx)
            } else {
                None
            };
            let log = std::fs::File::create(&console)?;
            let mut child = Command::new(&self.cfg.firecracker_bin)
                .arg("--api-sock")
                .arg(&api_sock)
                .arg("--id")
                .arg(sandbox_id)
                .stdin(Stdio::null())
                .stdout(Stdio::from(log.try_clone()?))
                .stderr(Stdio::from(log))
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| SandboxError::Substrate(format!("spawning firecracker: {e}")))?;
            let started = Instant::now();
            while !api_sock.exists() {
                if started.elapsed() > Duration::from_secs(10) {
                    let _ = child.start_kill();
                    return Err(SandboxError::Substrate(
                        "firecracker api socket never appeared".into(),
                    ));
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            let res: Result<()> = async {
                api_put(
                    &api_sock,
                    "/machine-config",
                    serde_json::json!({"vcpu_count": policy.spec.resources.vcpus.max(1), "mem_size_mib": policy.spec.resources.memory_mib.max(128)}),
                )
                .await?;
                api_put(
                    &api_sock,
                    "/boot-source",
                    serde_json::json!({
                        "kernel_image_path": self.cfg.kernel,
                        "boot_args": format!("console=ttyS0 reboot=k panic=1 pci=off root=/dev/vda ro init=/init modbit.vsock_port={} modbit.egress_port={EGRESS_PORT} modbit.workspace_dev=/dev/vdb modbit.sandbox_id={sandbox_id}", self.cfg.vsock_port),
                    }),
                )
                .await?;
                api_put(
                    &api_sock,
                    "/drives/rootfs",
                    serde_json::json!({"drive_id": "rootfs", "path_on_host": self.cfg.rootfs, "is_root_device": true, "is_read_only": true}),
                )
                .await?;
                api_put(
                    &api_sock,
                    "/drives/workspace",
                    serde_json::json!({"drive_id": "workspace", "path_on_host": ws_img, "is_root_device": false, "is_read_only": false}),
                )
                .await?;
                api_put(&api_sock, "/vsock", serde_json::json!({"guest_cid": 3, "uds_path": vsock})).await?;
                api_put(&api_sock, "/actions", serde_json::json!({"action_type": "InstanceStart"})).await?;
                Ok(())
            }
            .await;
            if let Err(e) = res {
                let _ = child.start_kill();
                return Err(e);
            }
            let deadline = Instant::now() + self.cfg.boot_timeout;
            let stream = loop {
                match connect_vsock(&vsock, self.cfg.vsock_port).await {
                    Ok(s) => break s,
                    Err(e) => {
                        if Instant::now() > deadline {
                            let _ = child.start_kill();
                            let tail = std::fs::read_to_string(&console).unwrap_or_default();
                            let tail: String = tail
                                .lines()
                                .rev()
                                .take(20)
                                .collect::<Vec<_>>()
                                .into_iter()
                                .rev()
                                .collect::<Vec<_>>()
                                .join("\n");
                            return Err(SandboxError::Guest(format!(
                                "the guest did not answer on vsock within {:?}: {e}; console tail:\n{tail}",
                                self.cfg.boot_timeout
                            )));
                        }
                        if let Ok(Some(status)) = child.try_wait() {
                            let tail = std::fs::read_to_string(&console).unwrap_or_default();
                            return Err(SandboxError::Substrate(format!(
                                "firecracker exited ({status}) before the guest answered; console:\n{tail}"
                            )));
                        }
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                }
            };
            let pid = child.id().unwrap_or(0);
            self.vms
                .lock()
                .expect("vms")
                .insert(sandbox_id.to_owned(), child);
            Ok(Provisioned {
                sandbox_id: sandbox_id.to_owned(),
                channel: Box::new(stream),
                backend: "microvm",
                isolated: true,
                detail: format!("firecracker pid {pid}; console {}", console.display()),
                egress: egress_rx,
            })
        })
    }

    fn reconnect<'a>(&'a self, sandbox_id: &'a str) -> BoxFuture<'a, Result<super::Channel>> {
        Box::pin(async move {
            if !self.vms.lock().expect("vms").contains_key(sandbox_id) {
                return Err(SandboxError::Guest(format!(
                    "no live MicroVM for sandbox {sandbox_id}"
                )));
            }
            let vsock = self.cfg.work_dir.join(sandbox_id).join("v.sock");
            let stream = connect_vsock(&vsock, self.cfg.vsock_port).await?;
            Ok(Box::new(stream) as super::Channel)
        })
    }

    fn destroy<'a>(&'a self, sandbox_id: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let child = self.vms.lock().expect("vms").remove(sandbox_id);
            if let Some(mut c) = child {
                let _ = c.start_kill();
                let _ = c.wait().await;
            }
            let dir = self.cfg.work_dir.join(sandbox_id);
            if dir.is_dir() {
                let _ = std::fs::remove_dir_all(&dir);
            }
            Ok(())
        })
    }
}
