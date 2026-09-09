//! Server-sent events parser over a byte stream (both adapters stream SSE).

/// One SSE event.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SseEvent {
    /// `event:` field.
    pub event: Option<String>,
    /// Joined `data:` lines.
    pub data: String,
}

/// Incremental parser: feed bytes, drain complete events.
#[derive(Debug, Default)]
pub struct SseParser {
    buf: Vec<u8>,
}

impl SseParser {
    /// Feed bytes and return every complete event.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        // An event ends at a blank line (\n\n or \r\n\r\n).
        while let Some((end, skip)) = find_blank_line(&self.buf) {
            let block = self.buf.drain(..end + skip).collect::<Vec<u8>>();
            let text = String::from_utf8_lossy(&block[..end]);
            let mut ev = SseEvent::default();
            let mut data_lines = Vec::new();
            for line in text.split(['\n', '\r']) {
                if line.is_empty() || line.starts_with(':') {
                    continue;
                }
                let (field, value) = match line.split_once(':') {
                    Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
                    None => (line, ""),
                };
                match field {
                    "event" => ev.event = Some(value.to_owned()),
                    "data" => data_lines.push(value.to_owned()),
                    _ => {}
                }
            }
            ev.data = data_lines.join("\n");
            if ev.event.is_some() || !ev.data.is_empty() {
                out.push(ev);
            }
        }
        out
    }
}

fn find_blank_line(buf: &[u8]) -> Option<(usize, usize)> {
    let mut i = 0;
    while i + 1 < buf.len() {
        if buf[i] == b'\n' && buf[i + 1] == b'\n' {
            return Some((i, 2));
        }
        if i + 3 < buf.len() && &buf[i..i + 4] == b"\r\n\r\n" {
            return Some((i, 4));
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_events_across_chunks_and_joins_data_lines() {
        let mut p = SseParser::default();
        assert!(p.feed(b"event: ping\ndata: {\"a\":").is_empty());
        let evs = p.feed(b"1}\n\n: comment\ndata: x\ndata: y\n\ndata: tail");
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].event.as_deref(), Some("ping"));
        assert_eq!(evs[0].data, "{\"a\":1}");
        assert_eq!(evs[1].data, "x\ny");
        let evs = p.feed(b"\r\n\r\n");
        assert_eq!(evs[0].data, "tail");
    }
}
