//! Listening-socket discovery bound to a session's process tree (REQ-PX-132,
//! docs/21 "Durable modbit-execd").
//!
//! The broker started the processes, so it is the one place that can say
//! which listening sockets belong to them: the shell a session runs, every
//! process that shell started (by parentage, and by the process group the
//! session was started in), and nothing else. A listener owned by an
//! unrelated process of the machine is never reported — the scan asks the
//! operating system for the sockets of *these* processes only, it does not
//! enumerate the machine's sockets and filter afterwards.
//!
//! Unix reads `/proc` where it exists and otherwise asks `ps` and `lsof`;
//! Windows asks `netstat -ano` and the CIM process table. The parsers are
//! pure and compiled on every platform so their tests run everywhere.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

/// One process of the process table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proc {
    /// Process id.
    pub pid: u32,
    /// Parent process id.
    pub ppid: u32,
    /// Process group id (0 where the platform has none).
    pub pgid: u32,
    /// Its command line, as the platform reports it.
    pub command: String,
}

/// One listening TCP socket and the process that holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listener {
    /// The process that holds the socket.
    pub pid: u32,
    /// The port.
    pub port: u16,
    /// The bound address (`127.0.0.1`, `::1`, `0.0.0.0`, `::`, ...).
    pub address: String,
}

/// The longest command line the broker reports.
pub const COMMAND_CAP: usize = 512;

/// The pids of the tree a session started: `root`, everything below it by
/// parentage, and every process still in the group the session was started in.
#[must_use]
pub fn tree(table: &[Proc], root: u32) -> HashSet<u32> {
    let mut out: HashSet<u32> = HashSet::new();
    if !table.iter().any(|p| p.pid == root) {
        return out;
    }
    out.insert(root);
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for p in table {
        children.entry(p.ppid).or_default().push(p.pid);
    }
    let mut stack = vec![root];
    while let Some(pid) = stack.pop() {
        for c in children.get(&pid).into_iter().flatten() {
            if out.insert(*c) {
                stack.push(*c);
            }
        }
    }
    // The session leader's group: a child that detached from the shell's
    // parentage (a double fork) stays in the group the session started.
    if let Some(leader) = table.iter().find(|p| p.pid == root)
        && leader.pgid == root
    {
        for p in table.iter().filter(|p| p.pgid == root) {
            out.insert(p.pid);
        }
    }
    out
}

/// Truncate a command line to [`COMMAND_CAP`] on a character boundary.
#[must_use]
pub fn bounded(command: &str) -> String {
    let c = command.trim();
    if c.len() <= COMMAND_CAP {
        return c.to_owned();
    }
    let mut end = COMMAND_CAP;
    while !c.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &c[..end])
}

// ----------------------------------------------------------------- parsers

/// `ps -axo pid=,ppid=,pgid=,command=`.
#[must_use]
pub fn parse_ps(text: &str) -> Vec<Proc> {
    text.lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let pid = it.next()?.parse().ok()?;
            let ppid = it.next()?.parse().ok()?;
            let pgid = it.next()?.parse().ok()?;
            let rest: Vec<&str> = it.collect();
            Some(Proc {
                pid,
                ppid,
                pgid,
                command: rest.join(" "),
            })
        })
        .collect()
}

/// `lsof -nP -F pnt`: `p<pid>` opens a process, `t<type>` and `n<name>` follow
/// for each socket. The name of a listening socket is `*:3000`,
/// `127.0.0.1:3000` or `[::1]:3000`.
#[must_use]
pub fn parse_lsof(text: &str) -> Vec<Listener> {
    let mut out = Vec::new();
    let (mut pid, mut ty) = (None::<u32>, String::new());
    for line in text.lines() {
        let (tag, rest) = line.split_at(line.chars().next().map_or(0, char::len_utf8));
        match tag {
            "p" => {
                pid = rest.parse().ok();
                ty.clear();
            }
            "t" => rest.clone_into(&mut ty),
            "n" => {
                let Some(pid) = pid else { continue };
                // `n127.0.0.1:3000` (and `n*:3000`); a connected socket has
                // `->` and is not a listener.
                if rest.contains("->") {
                    continue;
                }
                let Some((host, port)) = rest.rsplit_once(':') else {
                    continue;
                };
                let Ok(port) = port.parse::<u16>() else {
                    continue;
                };
                let host = host.trim_start_matches('[').trim_end_matches(']');
                let address = if host == "*" {
                    if ty.eq_ignore_ascii_case("IPv6") {
                        "::".to_owned()
                    } else {
                        "0.0.0.0".to_owned()
                    }
                } else {
                    host.to_owned()
                };
                out.push(Listener { pid, port, address });
            }
            _ => {}
        }
    }
    out
}

/// `netstat -ano -p TCP` (Windows): `TCP 0.0.0.0:3000 0.0.0.0:0 LISTENING 1234`.
#[must_use]
pub fn parse_netstat(text: &str) -> Vec<Listener> {
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 5 || !f[0].eq_ignore_ascii_case("TCP") || f[3] != "LISTENING" {
                return None;
            }
            let (host, port) = f[1].rsplit_once(':')?;
            Some(Listener {
                pid: f[4].parse().ok()?,
                port: port.parse().ok()?,
                address: host
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_owned(),
            })
        })
        .collect()
}

/// `/proc/net/tcp` or `/proc/net/tcp6` (Linux): the listening sockets as
/// `(inode, port, address)`. State `0A` is LISTEN.
#[must_use]
pub fn parse_proc_net_tcp(text: &str, v6: bool) -> Vec<(u64, u16, String)> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 10 || f[3] != "0A" {
                return None;
            }
            let (addr_hex, port_hex) = f[1].split_once(':')?;
            let port = u16::from_str_radix(port_hex, 16).ok()?;
            let inode: u64 = f[9].parse().ok()?;
            let address = if v6 {
                if addr_hex.len() != 32 {
                    return None;
                }
                let mut bytes = [0u8; 16];
                for w in 0..4 {
                    let word = u32::from_str_radix(&addr_hex[w * 8..w * 8 + 8], 16).ok()?;
                    bytes[w * 4..w * 4 + 4].copy_from_slice(&word.to_ne_bytes());
                }
                std::net::Ipv6Addr::from(bytes).to_string()
            } else {
                let word = u32::from_str_radix(addr_hex, 16).ok()?;
                std::net::Ipv4Addr::from(word.to_ne_bytes()).to_string()
            };
            Some((inode, port, address))
        })
        .collect()
}

// --------------------------------------------------------------- platforms

/// A snapshot of the process table.
///
/// # Errors
/// The platform's process listing failed.
pub fn process_table() -> Result<Vec<Proc>, String> {
    imp::process_table()
}

/// The listening TCP sockets held by any of `pids`.
///
/// # Errors
/// The platform's socket listing failed.
pub fn listeners(pids: &HashSet<u32>) -> Result<Vec<Listener>, String> {
    if pids.is_empty() {
        return Ok(Vec::new());
    }
    imp::listeners(pids)
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{Listener, Proc, parse_proc_net_tcp};
    use std::collections::{HashMap, HashSet};

    pub fn process_table() -> Result<Vec<Proc>, String> {
        let mut out = Vec::new();
        for e in std::fs::read_dir("/proc")
            .map_err(|e| e.to_string())?
            .flatten()
        {
            let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else {
                continue;
            };
            // `pid (comm) S ppid pgrp ...`: comm may contain spaces and
            // parentheses, so split after the last `)`.
            let Ok(stat) = std::fs::read_to_string(e.path().join("stat")) else {
                continue;
            };
            let Some(rest) = stat.rsplit_once(')').map(|(_, r)| r) else {
                continue;
            };
            let f: Vec<&str> = rest.split_whitespace().collect();
            let (Some(ppid), Some(pgid)) = (
                f.get(1).and_then(|v| v.parse().ok()),
                f.get(2).and_then(|v| v.parse().ok()),
            ) else {
                continue;
            };
            let command = std::fs::read(e.path().join("cmdline"))
                .map(|b| {
                    String::from_utf8_lossy(&b)
                        .split('\0')
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            out.push(Proc {
                pid,
                ppid,
                pgid,
                command,
            });
        }
        Ok(out)
    }

    pub fn listeners(pids: &HashSet<u32>) -> Result<Vec<Listener>, String> {
        let mut sockets: HashMap<u64, (u16, String)> = HashMap::new();
        for (file, v6) in [("/proc/net/tcp", false), ("/proc/net/tcp6", true)] {
            if let Ok(text) = std::fs::read_to_string(file) {
                for (inode, port, address) in parse_proc_net_tcp(&text, v6) {
                    sockets.insert(inode, (port, address));
                }
            }
        }
        let mut out = Vec::new();
        for pid in pids {
            let Ok(dir) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
                continue;
            };
            for fd in dir.flatten() {
                let Ok(target) = std::fs::read_link(fd.path()) else {
                    continue;
                };
                let target = target.to_string_lossy();
                let Some(inode) = target
                    .strip_prefix("socket:[")
                    .and_then(|r| r.strip_suffix(']'))
                    .and_then(|r| r.parse::<u64>().ok())
                else {
                    continue;
                };
                if let Some((port, address)) = sockets.get(&inode) {
                    out.push(Listener {
                        pid: *pid,
                        port: *port,
                        address: address.clone(),
                    });
                }
            }
        }
        Ok(out)
    }
}

#[cfg(all(unix, not(target_os = "linux")))]
mod imp {
    use super::{Listener, Proc, parse_lsof, parse_ps};
    use std::collections::HashSet;
    use std::process::{Command, Stdio};

    pub fn process_table() -> Result<Vec<Proc>, String> {
        let out = Command::new("ps")
            .args(["-axo", "pid=,ppid=,pgid=,command="])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(|e| format!("ps: {e}"))?;
        Ok(parse_ps(&String::from_utf8_lossy(&out.stdout)))
    }

    pub fn listeners(pids: &HashSet<u32>) -> Result<Vec<Listener>, String> {
        let list = pids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        // `-a` ANDs the filters: only these processes' TCP listeners. Exit
        // status 1 means a listed pid had nothing to report (or has exited).
        let out = Command::new("lsof")
            .args([
                "-nP",
                "-a",
                "-p",
                &list,
                "-iTCP",
                "-sTCP:LISTEN",
                "-F",
                "pnt",
            ])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(|e| format!("lsof: {e}"))?;
        Ok(parse_lsof(&String::from_utf8_lossy(&out.stdout)))
    }
}

#[cfg(windows)]
mod imp {
    use super::{Listener, Proc, parse_netstat};
    use std::collections::HashSet;
    use std::process::{Command, Stdio};

    pub fn process_table() -> Result<Vec<Proc>, String> {
        let script = "Get-CimInstance Win32_Process | ForEach-Object { \"$($_.ProcessId)`t$($_.ParentProcessId)`t$($_.CommandLine)\" }";
        let out = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(|e| format!("powershell: {e}"))?;
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| {
                let mut it = l.splitn(3, '\t');
                Some(Proc {
                    pid: it.next()?.trim().parse().ok()?,
                    ppid: it.next()?.trim().parse().ok()?,
                    pgid: 0,
                    command: it.next().unwrap_or_default().trim().to_owned(),
                })
            })
            .collect())
    }

    pub fn listeners(pids: &HashSet<u32>) -> Result<Vec<Listener>, String> {
        let out = Command::new("netstat")
            .args(["-ano", "-p", "TCP"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(|e| format!("netstat: {e}"))?;
        // netstat lists the machine's sockets; only those held by the
        // session's own processes are kept.
        Ok(parse_netstat(&String::from_utf8_lossy(&out.stdout))
            .into_iter()
            .filter(|l| pids.contains(&l.pid))
            .collect())
    }
}

#[cfg(not(any(unix, windows)))]
mod imp {
    use super::{Listener, Proc};
    use std::collections::HashSet;

    pub fn process_table() -> Result<Vec<Proc>, String> {
        Err("process discovery is not available on this platform".into())
    }

    pub fn listeners(_pids: &HashSet<u32>) -> Result<Vec<Listener>, String> {
        Err("socket discovery is not available on this platform".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tree_is_the_session_its_descendants_and_its_group_and_nothing_else() {
        let p = |pid, ppid, pgid| Proc {
            pid,
            ppid,
            pgid,
            command: format!("c{pid}"),
        };
        // 100 is the session leader; 101 its child; 102 a grandchild; 103 a
        // double-forked process reparented to init but still in the group;
        // 200 is an unrelated process with its own tree.
        let table = vec![
            p(1, 0, 1),
            p(100, 1, 100),
            p(101, 100, 100),
            p(102, 101, 102),
            p(103, 1, 100),
            p(200, 1, 200),
            p(201, 200, 200),
        ];
        let t = tree(&table, 100);
        assert_eq!(t, HashSet::from([100, 101, 102, 103]));
        assert!(!t.contains(&200) && !t.contains(&201));
        assert!(
            tree(&table, 999).is_empty(),
            "a root that is gone has no tree"
        );
    }

    #[test]
    fn lsof_output_parses_to_listeners_with_their_bound_addresses() {
        let text = "p4242\ntIPv4\nn127.0.0.1:3000\ntIPv6\nn[::1]:3001\ntIPv4\nn*:3002\ntIPv6\nn*:3003\nn10.0.0.2:5->10.0.0.9:80\np77\ntIPv4\nn0.0.0.0:8080\n";
        let l = parse_lsof(text);
        assert_eq!(
            l,
            vec![
                Listener {
                    pid: 4242,
                    port: 3000,
                    address: "127.0.0.1".into()
                },
                Listener {
                    pid: 4242,
                    port: 3001,
                    address: "::1".into()
                },
                Listener {
                    pid: 4242,
                    port: 3002,
                    address: "0.0.0.0".into()
                },
                Listener {
                    pid: 4242,
                    port: 3003,
                    address: "::".into()
                },
                Listener {
                    pid: 77,
                    port: 8080,
                    address: "0.0.0.0".into()
                },
            ],
            "a connected socket is not a listener"
        );
    }

    #[test]
    fn netstat_and_proc_net_parse() {
        let n = "  TCP    0.0.0.0:3000     0.0.0.0:0      LISTENING       1234\n  TCP    [::1]:3001       [::]:0         LISTENING       1234\n  TCP    10.0.0.2:50000   10.0.0.9:443   ESTABLISHED     9\n";
        let l = parse_netstat(n);
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].port, l[0].pid), (3000, 1234));
        assert_eq!(l[1].address, "::1");
        let proc_net = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 0100007F:0BB8 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 55555 1 0000000000000000 100 0 0 10 0\n   1: 0100007F:C350 0100007F:0BB8 01 00000000:00000000 00:00000000 00000000  1000        0 66666 1 0000000000000000 100 0 0 10 0\n";
        let s = parse_proc_net_tcp(proc_net, false);
        assert_eq!(s.len(), 1, "only LISTEN rows");
        assert_eq!((s[0].0, s[0].1), (55555, 3000));
        if cfg!(target_endian = "little") {
            assert_eq!(s[0].2, "127.0.0.1");
        }
    }

    #[test]
    fn ps_parses_pid_ppid_pgid_and_command() {
        let t =
            parse_ps("  1     0     1 /sbin/launchd\n 500   1   500 node server.js --port 3000\n");
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].command, "node server.js --port 3000");
        assert_eq!((t[1].ppid, t[1].pgid), (1, 500));
    }

    #[test]
    fn a_long_command_is_bounded_on_a_character_boundary() {
        let long = "é".repeat(COMMAND_CAP);
        let b = bounded(&long);
        assert!(b.len() <= COMMAND_CAP + 3 && b.ends_with("..."));
    }
}
