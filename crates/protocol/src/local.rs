//! Local transport endpoints: Unix domain socket on macOS/Linux, named pipe on
//! Windows (docs/30). The endpoint name is chosen by Core and handed to the
//! client together with the boot secret in the ready line.

use std::path::{Path, PathBuf};

/// Where a local Core listens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint(pub String);

fn dir_hash(dir: &Path) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in dir.to_string_lossy().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

impl Endpoint {
    /// A fresh endpoint for the Core owning `dir`.
    ///
    /// Unix: a socket file in a short, private (0700) runtime directory, because
    /// socket paths are limited to ~100 bytes (`SUN_LEN`) and user data
    /// directories are often longer. The data directory identity is folded into
    /// the name. Windows: a named pipe carrying the same identity.
    pub fn for_dir(dir: &Path, nonce: &str) -> std::io::Result<Self> {
        let h = dir_hash(dir);
        if cfg!(windows) {
            return Ok(Endpoint(format!(r"\\.\pipe\modbit-core-{h:016x}-{nonce}")));
        }
        let runtime = runtime_dir()?;
        let path = runtime.join(format!("c-{h:016x}-{nonce}.sock"));
        let len = path.as_os_str().len();
        if len >= 100 {
            return Err(std::io::Error::other(format!(
                "socket path {} is {len} bytes; must be under 100",
                path.display()
            )));
        }
        Ok(Endpoint(path.to_string_lossy().into_owned()))
    }

    /// Filesystem path (unix) of the endpoint.
    #[must_use]
    pub fn path(&self) -> PathBuf {
        PathBuf::from(&self.0)
    }
}

/// Private per-user runtime directory for sockets: `$XDG_RUNTIME_DIR/modbit`
/// when set, otherwise `<temp>/modbit-<uid or user>`; created with mode 0700.
pub fn runtime_dir() -> std::io::Result<PathBuf> {
    let base = match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(x) if !x.is_empty() => PathBuf::from(x).join("modbit"),
        _ => {
            let who = std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .unwrap_or_else(|_| "u".into());
            std::env::temp_dir().join(format!(
                "modbit-{}",
                who.chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .take(16)
                    .collect::<String>()
            ))
        }
    };
    std::fs::create_dir_all(&base)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(base)
}

/// Prefix of the single line Core prints on stdout once it is listening.
pub const READY_PREFIX: &str = "MODBIT_CORE_READY ";

/// Parsed ready line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyLine {
    /// Endpoint.
    pub endpoint: Endpoint,
    /// Boot-scoped secret (hex).
    pub boot_secret_hex: String,
    /// Protocol major.minor.
    pub protocol: (u32, u32),
}

impl ReadyLine {
    /// Render.
    #[must_use]
    pub fn render(&self) -> String {
        format!(
            "{READY_PREFIX}endpoint={} secret={} protocol={}.{}",
            self.endpoint.0, self.boot_secret_hex, self.protocol.0, self.protocol.1
        )
    }

    /// Parse; `None` when the line is not a ready line.
    #[must_use]
    pub fn parse(line: &str) -> Option<Self> {
        let rest = line.strip_prefix(READY_PREFIX)?;
        let mut endpoint = None;
        let mut secret = None;
        let mut protocol = None;
        for kv in rest.split_whitespace() {
            let (k, v) = kv.split_once('=')?;
            match k {
                "endpoint" => endpoint = Some(v.to_owned()),
                "secret" => secret = Some(v.to_owned()),
                "protocol" => {
                    let (a, b) = v.split_once('.')?;
                    protocol = Some((a.parse().ok()?, b.parse().ok()?));
                }
                _ => {}
            }
        }
        Some(ReadyLine {
            endpoint: Endpoint(endpoint?),
            boot_secret_hex: secret?,
            protocol: protocol?,
        })
    }
}

/// Decode a hex secret.
#[must_use]
pub fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

/// Encode bytes as lowercase hex.
#[must_use]
pub fn encode_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_fits_the_socket_path_limit_even_for_long_data_dirs() {
        let long = Path::new("/")
            .join("x".repeat(60))
            .join("y".repeat(60))
            .join("z".repeat(60));
        let e = Endpoint::for_dir(&long, "abcdef").unwrap();
        assert!(e.0.len() < 100, "{}", e.0);
        let e2 = Endpoint::for_dir(&long, "abcdef").unwrap();
        assert_eq!(e, e2, "deterministic for the same data dir and nonce");
        assert_ne!(e, Endpoint::for_dir(Path::new("/other"), "abcdef").unwrap());
    }

    #[test]
    fn ready_line_round_trips() {
        let r = ReadyLine {
            endpoint: Endpoint("/tmp/x.sock".into()),
            boot_secret_hex: "00ff".into(),
            protocol: (1, 0),
        };
        assert_eq!(ReadyLine::parse(&r.render()), Some(r));
        assert_eq!(ReadyLine::parse("hello"), None);
        assert_eq!(decode_hex("00ff"), Some(vec![0, 255]));
        assert_eq!(decode_hex("0"), None);
    }
}
