//! The guest's local egress proxy (M8.6): an HTTP proxy on a loopback
//! port of the guest's own (ephemeral — several reference guests share a
//! host) the guest's processes are pointed at (`http_proxy`,
//! `https_proxy`). It decides nothing: every client connection becomes one
//! channel to the gateway's egress broker on the host — over vsock in a
//! MicroVM, over the loopback address the reference backend named — with
//! the destination the client asked for; the broker admits or refuses it.
//! `CONNECT host:port` becomes a tunnel (TLS stays end to end); a plain
//! absolute-URI request is forwarded as HTTP to the broker, which either
//! relays it to an admitted host or performs it against a credentialed
//! target with the secret it holds.

use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

/// Where the broker is, from the guest's side.
#[derive(Clone, Debug)]
#[allow(dead_code)] // `Vsock` is built by the Linux init path only
pub enum BrokerAddr {
    /// The host's vsock port (a MicroVM).
    Vsock(u32),
    /// A loopback address (the reference backend).
    Tcp(String),
}

/// The proxy's listen address inside the guest, once it listens.
static PROXY_ADDR: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Where the proxy listens (`127.0.0.1:<port>`), if it runs.
pub fn addr() -> Option<String> {
    PROXY_ADDR.get().cloned()
}

/// Bind the proxy on an ephemeral loopback port and serve it in the
/// background; the address is known from here on.
pub fn start(broker: BrokerAddr) -> std::io::Result<String> {
    let std_listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    std_listener.set_nonblocking(true)?;
    let local = std_listener.local_addr()?.to_string();
    let _ = PROXY_ADDR.set(local.clone());
    tokio::spawn(async move {
        match tokio::net::TcpListener::from_std(std_listener) {
            Ok(listener) => {
                if let Err(e) = serve(listener, broker).await {
                    eprintln!("modbit-guest: egress proxy: {e}");
                }
            }
            Err(e) => eprintln!("modbit-guest: egress proxy: {e}"),
        }
    });
    Ok(local)
}

type Channel = Box<dyn ChannelIo>;
pub(crate) trait ChannelIo:
    tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send
{
}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> ChannelIo for T {}

async fn open_channel(addr: &BrokerAddr) -> std::io::Result<Channel> {
    match addr {
        BrokerAddr::Tcp(a) => {
            let s = tokio::net::TcpStream::connect(a).await?;
            s.set_nodelay(true)?;
            Ok(Box::new(s))
        }
        #[cfg(target_os = "linux")]
        BrokerAddr::Vsock(port) => {
            let s = vsock::VsockStream::connect_with_cid_port(vsock::VMADDR_CID_HOST, *port)
                .map_err(std::io::Error::other)?;
            s.set_nonblocking(true)?;
            Ok(Box::new(crate::init::VsockAsync::new(s)?))
        }
        #[cfg(not(target_os = "linux"))]
        BrokerAddr::Vsock(_) => Err(std::io::Error::other("vsock egress needs a Linux guest")),
    }
}

/// Serve the proxy forever.
async fn serve(listener: tokio::net::TcpListener, addr: BrokerAddr) -> std::io::Result<()> {
    eprintln!(
        "modbit-guest: egress proxy on {} → {addr:?}",
        listener
            .local_addr()
            .map(|a| a.to_string())
            .unwrap_or_default()
    );
    let addr = Arc::new(addr);
    loop {
        let (client, _) = listener.accept().await?;
        let addr = Arc::clone(&addr);
        tokio::spawn(async move {
            if let Err(e) = handle_client(client, &addr).await {
                eprintln!("modbit-guest: proxy: {e}");
            }
        });
    }
}

async fn handle_client(client: tokio::net::TcpStream, addr: &BrokerAddr) -> std::io::Result<()> {
    let _ = client.set_nodelay(true);
    let mut r = BufReader::new(client);
    let mut request_line = String::new();
    r.read_line(&mut request_line).await?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default().to_owned();
    // The destination host is logged, never a path or query (a URL may
    // carry a token).
    eprintln!(
        "modbit-guest: proxy: {method} {}",
        target
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or_default()
    );
    if method == "CONNECT" {
        // Drain the headers.
        let mut line = String::new();
        loop {
            line.clear();
            if r.read_line(&mut line).await? == 0 || line == "\r\n" || line == "\n" {
                break;
            }
        }
        let mut ch = match open_channel(addr).await {
            Ok(c) => c,
            Err(e) => {
                let mut c = r.into_inner();
                return c
                    .write_all(
                        format!("HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n{e}")
                            .as_bytes(),
                    )
                    .await;
            }
        };
        ch.write_all(format!("TUNNEL {target}\r\n").as_bytes())
            .await?;
        let mut cr = BufReader::new(ch);
        let mut answer = String::new();
        cr.read_line(&mut answer).await?;
        let mut client = r.into_inner();
        if answer.starts_with("OK") {
            client
                .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                .await?;
            relay(client, cr.into_inner()).await
        } else {
            let why = answer.trim().trim_start_matches("DENIED").trim();
            client
                .write_all(format!("HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{why}", why.len()).as_bytes())
                .await
        }
    } else {
        // An absolute-URI request: the host from the URI, else the Host header.
        let mut head = request_line.clone();
        let mut host_header = String::new();
        let mut line = String::new();
        loop {
            line.clear();
            if r.read_line(&mut line).await? == 0 {
                break;
            }
            if let Some(v) = line
                .strip_prefix("Host:")
                .or_else(|| line.strip_prefix("host:"))
            {
                host_header = v.trim().to_owned();
            }
            head.push_str(&line);
            if line == "\r\n" || line == "\n" {
                break;
            }
        }
        let host = target
            .strip_prefix("http://")
            .map(|rest| rest.split('/').next().unwrap_or_default().to_owned())
            .filter(|h| !h.is_empty())
            .unwrap_or(host_header);
        let mut ch = match open_channel(addr).await {
            Ok(c) => c,
            Err(e) => {
                let mut c = r.into_inner();
                return c
                    .write_all(
                        format!("HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n{e}")
                            .as_bytes(),
                    )
                    .await;
            }
        };
        ch.write_all(format!("HTTP {host}\r\n").as_bytes()).await?;
        let mut cr = BufReader::new(ch);
        let mut answer = String::new();
        cr.read_line(&mut answer).await?;
        let mut ch = cr.into_inner();
        if !answer.starts_with("OK") {
            let why = answer.trim().trim_start_matches("DENIED").trim();
            let mut client = r.into_inner();
            return client
                .write_all(format!("HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{why}", why.len()).as_bytes())
                .await;
        }
        ch.write_all(head.as_bytes()).await?;
        // Whatever the client buffered beyond the head, then the rest.
        let buffered = r.buffer().to_vec();
        if !buffered.is_empty() {
            ch.write_all(&buffered).await?;
        }
        let client = r.into_inner();
        relay(client, ch).await
    }
}

async fn relay(client: tokio::net::TcpStream, ch: Channel) -> std::io::Result<()> {
    let (mut cr, mut cw) = client.into_split();
    let (mut br, mut bw) = tokio::io::split(ch);
    let a = async {
        let mut buf = [0u8; 16 * 1024];
        loop {
            match cr.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if bw.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            }
        }
        let _ = bw.shutdown().await;
    };
    let b = async {
        let _ = tokio::io::copy(&mut br, &mut cw).await;
        let _ = cw.shutdown().await;
    };
    tokio::join!(a, b);
    Ok(())
}
