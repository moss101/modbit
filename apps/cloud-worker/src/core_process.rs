//! One `modbit-core` per hosted session: spawned in the session's tenant,
//! tethered to this worker, driven over the authenticated local surface as
//! a cloud-worker client (docs/33: the same Core runtime/domain code
//! executes in cloud workers).

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use modbit_domain::TenantId;
use modbit_protocol::client::Client;
use modbit_protocol::local::{ReadyLine, decode_hex};
use modbit_protocol::v1::ClientKind;

/// A spawned Core and its ready line.
pub struct CoreProcess {
    child: Child,
    /// The ready line (endpoint, boot secret).
    pub ready: ReadyLine,
}

impl CoreProcess {
    /// Spawn a Core for `tenant` over `data_dir`; its stderr goes to `core.log` there.
    pub fn spawn(core_bin: &Path, data_dir: &Path, tenant: TenantId) -> anyhow::Result<Self> {
        Self::spawn_with(core_bin, data_dir, Some(tenant))
    }

    /// Spawn a Core in the local profile's tenant (a laptop's Core; the
    /// handoff qualification's origin, M8.7).
    pub fn spawn_local(core_bin: &Path, data_dir: &Path) -> anyhow::Result<Self> {
        Self::spawn_with(core_bin, data_dir, None)
    }

    fn spawn_with(
        core_bin: &Path,
        data_dir: &Path,
        tenant: Option<TenantId>,
    ) -> anyhow::Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(data_dir.join("core.log"))?;
        let mut cmd = Command::new(core_bin);
        cmd.arg("--data-dir").arg(data_dir);
        if let Some(t) = tenant {
            cmd.arg("--tenant-id").arg(t.to_string());
        }
        let mut child = cmd
            .arg("--tether-stdin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log))
            .spawn()
            .map_err(|e| {
                anyhow::anyhow!(
                    "spawning `{}`: {e} (set MODBIT_CORE_BIN)",
                    core_bin.display()
                )
            })?;
        let stdout = child.stdout.take().expect("piped stdout");
        let mut lines = BufReader::new(stdout).lines();
        let ready = loop {
            match lines.next() {
                Some(Ok(line)) => {
                    if let Some(r) = ReadyLine::parse(&line) {
                        break r;
                    }
                }
                _ => {
                    let _ = child.kill();
                    anyhow::bail!(
                        "modbit-core exited before it was ready (see {})",
                        data_dir.join("core.log").display()
                    );
                }
            }
        };
        std::thread::spawn(move || for _ in lines {});
        Ok(Self { child, ready })
    }

    /// A cloud-worker client on this Core.
    pub async fn client(&self) -> anyhow::Result<Client> {
        self.client_as(ClientKind::CloudWorker).await
    }

    /// A client of `kind` on this Core.
    pub async fn client_as(&self, kind: ClientKind) -> anyhow::Result<Client> {
        let secret = decode_hex(&self.ready.boot_secret_hex)
            .ok_or_else(|| anyhow::anyhow!("bad ready line"))?;
        Ok(Client::connect(
            &self.ready.endpoint,
            &secret,
            kind,
            concat!("modbit-cloud-worker ", env!("CARGO_PKG_VERSION")),
        )
        .await?)
    }

    /// Stop the Core (the tether does it too when this process dies).
    pub fn stop(mut self) {
        drop(self.child.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for CoreProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
