//! The reference backend: `modbit-guest` as a child process on this host,
//! its `/workspace` a per-sandbox copy of the workspace source, the channel
//! a loopback TCP socket the guest announces on its stdout. It implements
//! the whole backend contract — the conformance suite runs on it anywhere,
//! development needs no KVM — and isolates nothing: the guest enforces the
//! policy in its own process (protected paths, the workspace bound, the
//! limits), the host's network is the guest's. `isolates()` is false and a
//! gateway serves tenants with it only when configured to.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use super::{BoxFuture, Provisioned, SandboxBackend, copy_tree};
use crate::image::{self, ImageManifest, SignedManifest, VerifiedImage};
use crate::policy::CompiledPolicy;
use crate::{Result, SandboxError};

/// The reference backend.
pub struct ReferenceBackend {
    guest_bin: PathBuf,
    work_dir: PathBuf,
    image: Option<VerifiedImage>,
    children: Mutex<HashMap<String, (Child, String)>>,
}

impl ReferenceBackend {
    /// `guest_bin` is the `modbit-guest` binary; `work_dir` holds each
    /// sandbox's workspace copy. Unverified: the guest binary is whatever
    /// is at the path (development).
    #[must_use]
    pub fn new(guest_bin: PathBuf, work_dir: PathBuf) -> Self {
        Self {
            guest_bin,
            work_dir,
            image: None,
            children: Mutex::new(HashMap::new()),
        }
    }

    /// Verified: the guest binary must be the one a trusted publisher
    /// signed (`reference-guest` manifest), re-checked at every provision.
    pub fn verified(
        guest_bin: PathBuf,
        work_dir: PathBuf,
        signed: &SignedManifest,
        trusted: &[(String, [u8; 32])],
    ) -> Result<Self> {
        let image = image::verify(signed, &guest_bin, trusted)?;
        if image.manifest.kind != "reference-guest" {
            return Err(SandboxError::Refused {
                code: "IMAGE_UNVERIFIED".into(),
                message: format!(
                    "the manifest describes a `{}`, not a reference guest",
                    image.manifest.kind
                ),
            });
        }
        Ok(Self {
            guest_bin,
            work_dir,
            image: Some(image),
            children: Mutex::new(HashMap::new()),
        })
    }
}

impl SandboxBackend for ReferenceBackend {
    fn kind(&self) -> &'static str {
        "reference"
    }

    fn isolates(&self) -> bool {
        false
    }

    fn image(&self) -> Option<&ImageManifest> {
        self.image.as_ref().map(|i| &i.manifest)
    }

    fn provision<'a>(
        &'a self,
        sandbox_id: &'a str,
        policy: &'a CompiledPolicy,
    ) -> BoxFuture<'a, Result<Provisioned>> {
        Box::pin(async move {
            // Egress (M8.6): the guest's proxy connects to a loopback
            // listener of this sandbox's own; the broker serves what arrives.
            // (The host's network is still the guest's here — the reference
            // backend isolates nothing — but the policy is exercised.)
            let (egress_addr, egress_rx) = if policy.egress_proxy {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
                let addr = listener.local_addr()?;
                let (tx, rx) = tokio::sync::mpsc::channel::<super::Channel>(64);
                tokio::spawn(async move {
                    while let Ok((s, _)) = listener.accept().await {
                        let _ = s.set_nodelay(true);
                        if tx.send(Box::new(s)).await.is_err() {
                            break;
                        }
                    }
                });
                (Some(addr.to_string()), Some(rx))
            } else {
                (None, None)
            };
            if let Some(image) = &self.image {
                image::check_image(&image.manifest, &self.guest_bin)?;
            }
            let dir = self.work_dir.join(sandbox_id);
            let ws = dir.join("workspace");
            copy_tree(&policy.spec.workspace_source, &ws)?;
            let guest_log = dir.join("guest.log");
            let mut cmd = Command::new(&self.guest_bin);
            cmd.arg("--listen")
                .arg("127.0.0.1:0")
                .arg("--workspace-host")
                .arg(&ws)
                .env_clear()
                .env("PATH", std::env::var("PATH").unwrap_or_default());
            if let Some(a) = &egress_addr {
                cmd.arg("--egress-host").arg(a);
            }
            // Windows processes need the system root (and a temp dir) to
            // load their runtime; nothing else of the host's environment
            // reaches the guest.
            for k in ["SystemRoot", "TEMP", "TMP"] {
                if let Ok(v) = std::env::var(k) {
                    cmd.env(k, v);
                }
            }
            let mut child = cmd
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::from(std::fs::File::create(&guest_log)?))
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| {
                    SandboxError::Substrate(format!("spawning `{}`: {e}", self.guest_bin.display()))
                })?;
            let stdout = child.stdout.take().expect("piped");
            let mut lines = BufReader::new(stdout).lines();
            let addr = tokio::time::timeout(Duration::from_secs(20), async {
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some(rest) = line.strip_prefix("ready listen=") {
                        return Some(rest.trim().to_owned());
                    }
                }
                None
            })
            .await
            .ok()
            .flatten()
            .ok_or_else(|| {
                let status = child
                    .try_wait()
                    .ok()
                    .flatten()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "still running".into());
                let log = std::fs::read_to_string(&guest_log).unwrap_or_default();
                SandboxError::Guest(format!(
                    "the guest announced no listener (process {status}); its log:\n{log}"
                ))
            })?;
            let stream = tokio::net::TcpStream::connect(&addr).await?;
            stream.set_nodelay(true)?;
            let pid = child.id().unwrap_or(0);
            tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
            self.children
                .lock()
                .expect("children")
                .insert(sandbox_id.to_owned(), (child, addr.clone()));
            Ok(Provisioned {
                sandbox_id: sandbox_id.to_owned(),
                channel: Box::new(stream),
                backend: "reference",
                isolated: false,
                detail: format!("pid {pid}; workspace {}", ws.display()),
                egress: egress_rx,
            })
        })
    }

    fn reconnect<'a>(&'a self, sandbox_id: &'a str) -> BoxFuture<'a, Result<super::Channel>> {
        Box::pin(async move {
            let addr = self
                .children
                .lock()
                .expect("children")
                .get(sandbox_id)
                .map(|(_, a)| a.clone());
            let Some(addr) = addr else {
                return Err(SandboxError::Guest(format!(
                    "no live guest for sandbox {sandbox_id}"
                )));
            };
            let stream = tokio::net::TcpStream::connect(&addr).await?;
            stream.set_nodelay(true)?;
            Ok(Box::new(stream) as super::Channel)
        })
    }

    fn destroy<'a>(&'a self, sandbox_id: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let child = self.children.lock().expect("children").remove(sandbox_id);
            if let Some((mut c, _)) = child {
                let _ = c.start_kill();
                let _ = c.wait().await;
            }
            let dir = self.work_dir.join(sandbox_id);
            if dir.is_dir() {
                let _ = std::fs::remove_dir_all(&dir);
            }
            Ok(())
        })
    }
}
