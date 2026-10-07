//! Segmented, indexed output log of one session (docs/21 "sliding replay
//! window", REQ-EV-0271).
//!
//! The output of a session is one append-only byte stream addressed by a
//! cursor (the byte offset in the stream). On disk it is a run of
//! *segments*: `<id>.<base>.log` holds the bytes `[base, base + len)` and
//! `<id>.<base>.idx` holds one fixed 13-byte entry per record (8-byte
//! absolute data offset, 1-byte stream tag, 4-byte length) in offset order.
//!
//! Two bounds follow from that layout:
//!
//! * **Retention.** When the bytes after the second-oldest segment's base
//!   still cover the replay window, the oldest segment is deleted. Disk per
//!   session is therefore at most `window + segment_bytes` (plus the index,
//!   13 bytes per record), however much the process writes. A cursor older
//!   than the oldest retained segment is *expired*: it is reported as such,
//!   never answered with the wrong bytes.
//! * **Indexed reads.** A read at a cursor finds its segment in the segment
//!   table and its record by binary search over the segment's index file, so
//!   waking a reader costs `O(log records)` and never re-reads the log.
//!
//! The sealed `OutputRef` object is the retained bytes (`retained_from`
//! says where they start), content-addressed in the Core's object store.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Index entry size: 8-byte offset, 1-byte tag, 4-byte length.
pub const ENTRY: u64 = 13;

/// Stream tags stored in the index.
pub const TAG_STDOUT: u8 = 1;
pub const TAG_STDERR: u8 = 2;
pub const TAG_PTY: u8 = 3;

/// One segment: the bytes `[base, base + len)` of the stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub base: u64,
    pub len: u64,
}

/// What a read at a cursor found.
#[derive(Debug)]
pub enum ReadOutcome {
    /// Up to `max` bytes of one record starting exactly at `cursor`.
    Chunk { tag: u8, data: Vec<u8> },
    /// The cursor is older than the replay window; the oldest retained
    /// cursor is given.
    Expired { oldest: u64 },
    /// The cursor is at or beyond the high-water mark.
    End,
}

pub fn data_path(dir: &Path, id: &str, base: u64) -> PathBuf {
    dir.join(format!("{id}.{base:020}.log"))
}

pub fn index_path(dir: &Path, id: &str, base: u64) -> PathBuf {
    dir.join(format!("{id}.{base:020}.idx"))
}

fn create_files(dir: &Path, id: &str, base: u64) -> std::io::Result<(File, File)> {
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(data_path(dir, id, base))?;
    let idx = OpenOptions::new()
        .create(true)
        .append(true)
        .open(index_path(dir, id, base))?;
    Ok((log, idx))
}

/// The writer side: owns the segment table and the active segment's files.
pub struct SegLog {
    dir: PathBuf,
    id: String,
    window: u64,
    segment_bytes: u64,
    /// Oldest first; never empty. The last one is active.
    segments: Vec<Segment>,
    active: Option<(File, File)>,
    /// High-water mark: the cursor of the next byte.
    written: u64,
}

impl SegLog {
    /// A new, empty log for session `id`.
    pub fn create(dir: &Path, id: &str, window: u64, segment_bytes: u64) -> std::io::Result<Self> {
        let active = create_files(dir, id, 0)?;
        Ok(Self {
            dir: dir.to_owned(),
            id: id.to_owned(),
            window: window.max(segment_bytes),
            segment_bytes,
            segments: vec![Segment { base: 0, len: 0 }],
            active: Some(active),
            written: 0,
        })
    }

    /// Reopen the log a previous broker left (`None` when it holds none).
    /// A pre-segment log (`<id>.log` + `<id>.idx`) is adopted as the segment
    /// at base 0. A torn tail (index and data disagreeing after a crash) is
    /// reconciled: the index is cut to whole entries and any data it does
    /// not describe gets one entry, so every retained byte stays readable.
    pub fn open(
        dir: &Path,
        id: &str,
        window: u64,
        segment_bytes: u64,
    ) -> std::io::Result<Option<Self>> {
        let legacy_log = dir.join(format!("{id}.log"));
        let legacy_idx = dir.join(format!("{id}.idx"));
        if legacy_log.exists() && !data_path(dir, id, 0).exists() {
            if legacy_idx.exists() {
                std::fs::rename(&legacy_idx, index_path(dir, id, 0))?;
            }
            std::fs::rename(&legacy_log, data_path(dir, id, 0))?;
        }
        let prefix = format!("{id}.");
        let mut bases: Vec<u64> = Vec::new();
        for e in std::fs::read_dir(dir)?.flatten() {
            let name = e.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(rest) = name.strip_prefix(&prefix) else {
                continue;
            };
            let Some(base) = rest.strip_suffix(".log") else {
                continue;
            };
            if base.len() == 20
                && let Ok(b) = base.parse::<u64>()
            {
                bases.push(b);
            }
        }
        bases.sort_unstable();
        if bases.is_empty() {
            return Ok(None);
        }
        // Keep the contiguous run that ends at the newest segment.
        let mut segments: Vec<Segment> = Vec::new();
        for &base in &bases {
            let len = std::fs::metadata(data_path(dir, id, base))?.len();
            if let Some(prev) = segments.last()
                && prev.base + prev.len != base
            {
                for gone in segments.drain(..) {
                    let _ = std::fs::remove_file(data_path(dir, id, gone.base));
                    let _ = std::fs::remove_file(index_path(dir, id, gone.base));
                }
            }
            segments.push(Segment { base, len });
        }
        let last = *segments.last().expect("non-empty");
        let (log, mut idx) = create_files(dir, id, last.base)?;
        // Reconcile the active index with its data.
        let ipath = index_path(dir, id, last.base);
        let ilen = std::fs::metadata(&ipath)?.len();
        let whole = ilen - ilen % ENTRY;
        if whole != ilen {
            OpenOptions::new()
                .write(true)
                .open(&ipath)?
                .set_len(whole)?;
        }
        let (mut covered, mut tag) = (last.base, TAG_STDOUT);
        if whole >= ENTRY {
            let mut f = File::open(&ipath)?;
            let (off, t, len) = read_entry(&mut f, whole / ENTRY - 1)?;
            covered = off + u64::from(len);
            tag = t;
        }
        let end = last.base + last.len;
        if covered < end {
            idx.write_all(&entry_bytes(covered, tag, (end - covered) as u32))?;
        } else if covered > end {
            // The index claims bytes the data lost: cut it back to the data.
            let mut keep = whole / ENTRY;
            let mut f = File::open(&ipath)?;
            while keep > 0 {
                let (off, _, _) = read_entry(&mut f, keep - 1)?;
                if off < end {
                    break;
                }
                keep -= 1;
            }
            OpenOptions::new()
                .write(true)
                .open(&ipath)?
                .set_len(keep * ENTRY)?;
        }
        Ok(Some(Self {
            dir: dir.to_owned(),
            id: id.to_owned(),
            window: window.max(segment_bytes),
            segment_bytes,
            written: end,
            segments,
            active: Some((log, idx)),
        }))
    }

    /// High-water mark.
    pub fn written(&self) -> u64 {
        self.written
    }

    /// The oldest cursor still readable.
    pub fn oldest(&self) -> u64 {
        self.segments.first().map_or(0, |s| s.base)
    }

    /// The segment table and the high-water mark, for readers outside the
    /// writer's lock.
    pub fn snapshot(&self) -> (Vec<Segment>, u64) {
        (self.segments.clone(), self.written)
    }

    /// Append one record; returns the new high-water mark.
    pub fn append(&mut self, tag: u8, data: &[u8]) -> std::io::Result<u64> {
        if data.is_empty() {
            return Ok(self.written);
        }
        let last = *self.segments.last().expect("non-empty");
        if last.len > 0 && last.len + data.len() as u64 > self.segment_bytes {
            self.rotate()?;
        }
        let (log, idx) = self.active.as_mut().expect("active segment");
        log.write_all(data)?;
        idx.write_all(&entry_bytes(self.written, tag, data.len() as u32))?;
        self.segments.last_mut().expect("non-empty").len += data.len() as u64;
        self.written += data.len() as u64;
        self.evict();
        Ok(self.written)
    }

    fn rotate(&mut self) -> std::io::Result<()> {
        let active = create_files(&self.dir, &self.id, self.written)?;
        self.segments.push(Segment {
            base: self.written,
            len: 0,
        });
        self.active = Some(active);
        Ok(())
    }

    /// Delete the oldest segments while what remains still covers the window.
    fn evict(&mut self) {
        while self.segments.len() > 1 && self.written - self.segments[1].base >= self.window {
            let gone = self.segments.remove(0);
            let _ = std::fs::remove_file(data_path(&self.dir, &self.id, gone.base));
            let _ = std::fs::remove_file(index_path(&self.dir, &self.id, gone.base));
        }
    }

    /// Delete every file of the log (the session is being pruned).
    pub fn remove_all(&mut self) {
        self.active = None;
        for s in self.segments.drain(..) {
            let _ = std::fs::remove_file(data_path(&self.dir, &self.id, s.base));
            let _ = std::fs::remove_file(index_path(&self.dir, &self.id, s.base));
        }
        self.segments.push(Segment {
            base: self.written,
            len: 0,
        });
    }
}

fn entry_bytes(offset: u64, tag: u8, len: u32) -> [u8; ENTRY as usize] {
    let mut e = [0u8; ENTRY as usize];
    e[..8].copy_from_slice(&offset.to_be_bytes());
    e[8] = tag;
    e[9..].copy_from_slice(&len.to_be_bytes());
    e
}

fn read_entry(f: &mut File, n: u64) -> std::io::Result<(u64, u8, u32)> {
    let mut e = [0u8; ENTRY as usize];
    f.seek(SeekFrom::Start(n * ENTRY))?;
    f.read_exact(&mut e)?;
    Ok((
        u64::from_be_bytes(e[..8].try_into().expect("8 bytes")),
        e[8],
        u32::from_be_bytes(e[9..].try_into().expect("4 bytes")),
    ))
}

/// Read at `cursor` from the retained segments, up to `high` (the writer's
/// high-water mark when the reader looked) and `max` bytes, within a single
/// record. `NotFound` means a segment was deleted underneath the read: the
/// caller re-reads the segment table.
pub fn read_at(
    dir: &Path,
    id: &str,
    segments: &[Segment],
    high: u64,
    cursor: u64,
    max: usize,
) -> std::io::Result<ReadOutcome> {
    let Some(first) = segments.first() else {
        return Ok(ReadOutcome::End);
    };
    if cursor < first.base {
        return Ok(ReadOutcome::Expired { oldest: first.base });
    }
    if cursor >= high {
        return Ok(ReadOutcome::End);
    }
    let i = segments.partition_point(|s| s.base <= cursor) - 1;
    let seg = segments[i];
    let seg_end = segments.get(i + 1).map_or(seg.base + seg.len, |n| n.base);
    let limit = seg_end.min(high);
    if cursor >= limit {
        return Ok(ReadOutcome::End);
    }
    // The record holding `cursor`: the last entry whose offset <= cursor.
    let mut idx = File::open(index_path(dir, id, seg.base))?;
    let count = idx.metadata()?.len() / ENTRY;
    let (mut lo, mut hi) = (0u64, count);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if read_entry(&mut idx, mid)?.0 > cursor {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    let (tag, record_end) = if lo == 0 {
        (TAG_STDOUT, limit)
    } else {
        let (off, tag, len) = read_entry(&mut idx, lo - 1)?;
        let end = off + u64::from(len);
        if end <= cursor {
            // Bytes the index does not describe: serve them untagged.
            (TAG_STDOUT, limit)
        } else {
            (tag, end)
        }
    };
    let stop = record_end.min(limit).min(cursor.saturating_add(max as u64));
    let mut log = File::open(data_path(dir, id, seg.base))?;
    log.seek(SeekFrom::Start(cursor - seg.base))?;
    let mut data = vec![0u8; (stop - cursor) as usize];
    log.read_exact(&mut data)?;
    Ok(ReadOutcome::Chunk { tag, data })
}

/// What sealing a log into the object store produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sealed {
    /// sha256 hex of the retained bytes: the object's name.
    pub hash: String,
    /// The cursor of the object's first byte (0 when nothing was dropped).
    pub retained_from: u64,
}

/// Stream the retained bytes into a content-addressed object under
/// `object_root` (`<root>/<hash[..2]>/<hash[2..]>`, the layout of the
/// Core's `ObjectStore`): written to a temporary file, fsynced, renamed into
/// place; an object that already exists is never rewritten.
pub fn seal(
    dir: &Path,
    id: &str,
    segments: &[Segment],
    object_root: &Path,
) -> std::io::Result<Sealed> {
    std::fs::create_dir_all(object_root)?;
    let tmp = object_root.join(format!(".execd-{id}.tmp"));
    let mut out = File::create(&tmp)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    for s in segments {
        let mut f = match File::open(data_path(dir, id, s.base)) {
            Ok(f) => f,
            Err(e) if s.len == 0 && e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
        };
        let mut left = s.len;
        while left > 0 {
            let want = buf.len().min(left as usize);
            if let Err(e) = f.read_exact(&mut buf[..want]) {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
            hasher.update(&buf[..want]);
            out.write_all(&buf[..want])?;
            left -= want as u64;
        }
    }
    out.sync_all()?;
    drop(out);
    let hash = hex::encode(hasher.finalize());
    let shard = object_root.join(&hash[..2]);
    let dest = shard.join(&hash[2..]);
    if dest.exists() {
        let _ = std::fs::remove_file(&tmp);
    } else {
        std::fs::create_dir_all(&shard)?;
        std::fs::rename(&tmp, &dest)?;
        if let Ok(d) = File::open(&shard) {
            let _ = d.sync_all();
        }
    }
    Ok(Sealed {
        hash,
        retained_from: segments.first().map_or(0, |s| s.base),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the log should hold: every record appended, in order.
    #[derive(Default)]
    struct Model {
        bytes: Vec<u8>,
        /// (offset, len, tag)
        records: Vec<(u64, usize, u8)>,
    }

    impl Model {
        fn push(&mut self, tag: u8, data: &[u8]) {
            self.records
                .push((self.bytes.len() as u64, data.len(), tag));
            self.bytes.extend_from_slice(data);
        }
        fn record_at(&self, cursor: u64) -> (u64, usize, u8) {
            *self
                .records
                .iter()
                .find(|(o, l, _)| *o <= cursor && cursor < o + *l as u64)
                .expect("a record holds the cursor")
        }
    }

    /// A small deterministic generator (no external crate needed).
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 33
        }
    }

    fn read_all(
        dir: &Path,
        id: &str,
        segs: &[Segment],
        high: u64,
        from: u64,
    ) -> Vec<(u64, u8, Vec<u8>)> {
        let mut out = Vec::new();
        let mut cursor = from;
        while cursor < high {
            match read_at(dir, id, segs, high, cursor, 64).unwrap() {
                ReadOutcome::Chunk { tag, data } => {
                    assert!(!data.is_empty() && data.len() <= 64);
                    out.push((cursor, tag, data.clone()));
                    cursor += data.len() as u64;
                }
                other => panic!("cursor {cursor} of {high}: {other:?}"),
            }
        }
        out
    }

    /// Against a model: rotation, eviction and indexed reads agree with the
    /// bytes appended, from every retained cursor, and the retained size
    /// stays within `window + segment`.
    #[test]
    fn log_matches_a_model_through_rotation_and_eviction() {
        let dir = tempfile::tempdir().unwrap();
        let (window, segment) = (3000u64, 1000u64);
        let mut log = SegLog::create(dir.path(), "s", window, segment).unwrap();
        let mut model = Model::default();
        let mut rng = Lcg(7);
        for round in 0..400 {
            let len = 1 + (rng.next() % 300) as usize;
            let tag = 1 + (rng.next() % 3) as u8;
            let data: Vec<u8> = (0..len).map(|i| (round + i) as u8).collect();
            let high = log.append(tag, &data).unwrap();
            model.push(tag, &data);
            assert_eq!(high, model.bytes.len() as u64);
            let (segs, written) = log.snapshot();
            assert_eq!(written, high);
            let oldest = log.oldest();
            assert!(
                high - oldest <= window + segment,
                "retained {} > {}",
                high - oldest,
                window + segment
            );
            if oldest > 0 {
                assert!(high - oldest >= window, "evicted below the window");
            }
            if round % 25 == 0 {
                // Every retained byte, record boundaries and tags included.
                let chunks = read_all(dir.path(), "s", &segs, high, oldest);
                let mut joined = Vec::new();
                for (cursor, tag, data) in &chunks {
                    let (off, rlen, rtag) = model.record_at(*cursor);
                    assert_eq!(*tag, rtag);
                    assert!(cursor + data.len() as u64 <= off + rlen as u64);
                    joined.extend_from_slice(data);
                }
                assert_eq!(joined, model.bytes[oldest as usize..]);
                // A cursor mid-stream starts exactly there.
                let mid = oldest + (high - oldest) / 3;
                let chunks = read_all(dir.path(), "s", &segs, high, mid);
                assert_eq!(chunks[0].0, mid);
                // Before the window: expired, naming the window's start.
                if oldest > 0 {
                    assert!(matches!(
                        read_at(dir.path(), "s", &segs, high, oldest - 1, 64).unwrap(),
                        ReadOutcome::Expired { oldest: o } if o == oldest
                    ));
                }
                assert!(matches!(
                    read_at(dir.path(), "s", &segs, high, high, 64).unwrap(),
                    ReadOutcome::End
                ));
            }
        }
        // Only the retained segments are on disk.
        let on_disk = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(on_disk, log.snapshot().0.len() * 2);
        // Sealing keeps exactly the retained tail and names it by digest.
        let (segs, high) = log.snapshot();
        let root = dir.path().join("objects");
        let sealed = seal(dir.path(), "s", &segs, &root).unwrap();
        assert_eq!(sealed.retained_from, log.oldest());
        let object = std::fs::read(root.join(&sealed.hash[..2]).join(&sealed.hash[2..])).unwrap();
        assert_eq!(object, model.bytes[log.oldest() as usize..high as usize]);
        assert_eq!(hex::encode(Sha256::digest(&object)), sealed.hash);
    }

    /// A log with a million tiny records is read at a cursor by binary
    /// search: the reads below would take minutes if each re-read the index
    /// (the index here is 13 MB; 20 000 reads of it would be 260 GB).
    #[test]
    fn reads_seek_the_index_instead_of_scanning_it() {
        let dir = tempfile::tempdir().unwrap();
        let n: u64 = 1_000_000;
        let data_len = n * 4;
        std::fs::write(
            data_path(dir.path(), "big", 0),
            vec![b'x'; data_len as usize],
        )
        .unwrap();
        let mut idx = Vec::with_capacity((n * ENTRY) as usize);
        for i in 0..n {
            idx.extend_from_slice(&entry_bytes(i * 4, TAG_PTY, 4));
        }
        std::fs::write(index_path(dir.path(), "big", 0), &idx).unwrap();
        let segs = [Segment {
            base: 0,
            len: data_len,
        }];
        let started = std::time::Instant::now();
        let mut rng = Lcg(3);
        for _ in 0..20_000 {
            let cursor = rng.next() % data_len;
            let ReadOutcome::Chunk { tag, data } =
                read_at(dir.path(), "big", &segs, data_len, cursor, 64).unwrap()
            else {
                panic!()
            };
            assert_eq!(tag, TAG_PTY);
            assert_eq!(data.len() as u64, 4 - cursor % 4, "ends at its record");
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(20),
            "20 000 indexed reads took {:?}",
            started.elapsed()
        );
    }

    /// A broker that died mid-write leaves an index torn or lagging behind
    /// its data; reopening reconciles it so every retained byte is readable.
    #[test]
    fn reopen_reconciles_a_torn_index_with_its_data() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = SegLog::create(dir.path(), "t", 1 << 20, 1 << 20).unwrap();
        log.append(TAG_STDOUT, b"first-record").unwrap();
        log.append(TAG_STDERR, b"second-record").unwrap();
        log.append(TAG_STDOUT, b"third-record").unwrap();
        drop(log);
        // Crash shape: the third record's data is there, its index entry is
        // missing and the second's is cut in half.
        let idx = index_path(dir.path(), "t", 0);
        let full = std::fs::read(&idx).unwrap();
        std::fs::write(&idx, &full[..(ENTRY as usize + 5)]).unwrap();
        let log = SegLog::open(dir.path(), "t", 1 << 20, 1 << 20)
            .unwrap()
            .expect("log");
        let (segs, high) = log.snapshot();
        assert_eq!(high, 12 + 13 + 12);
        let chunks = read_all(dir.path(), "t", &segs, high, 0);
        let joined: Vec<u8> = chunks.iter().flat_map(|(_, _, d)| d.clone()).collect();
        assert_eq!(joined, b"first-recordsecond-recordthird-record");
        assert_eq!(chunks[0].1, TAG_STDOUT);
        // The record that lost its entry is still tagged (as the last known
        // stream) and appending continues at the right cursor.
        let mut log = log;
        assert_eq!(log.append(TAG_PTY, b"!").unwrap(), high + 1);
        let (segs, high) = log.snapshot();
        let tail = read_all(dir.path(), "t", &segs, high, high - 1);
        assert_eq!(tail, vec![(high - 1, TAG_PTY, b"!".to_vec())]);
    }

    /// A log written before segments (`<id>.log` + `<id>.idx`) is adopted as
    /// the segment at base 0 with its bytes and records intact.
    #[test]
    fn reopen_adopts_a_pre_segment_log() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("old.log"), b"aaabbb").unwrap();
        let mut idx = Vec::new();
        idx.extend_from_slice(&entry_bytes(0, TAG_STDOUT, 3));
        idx.extend_from_slice(&entry_bytes(3, TAG_STDERR, 3));
        std::fs::write(dir.path().join("old.idx"), idx).unwrap();
        let log = SegLog::open(dir.path(), "old", 1 << 20, 1 << 20)
            .unwrap()
            .expect("legacy log adopted");
        let (segs, high) = log.snapshot();
        let chunks = read_all(dir.path(), "old", &segs, high, 0);
        assert_eq!(
            chunks,
            vec![
                (0, TAG_STDOUT, b"aaa".to_vec()),
                (3, TAG_STDERR, b"bbb".to_vec())
            ]
        );
        assert!(SegLog::open(dir.path(), "absent", 1, 1).unwrap().is_none());
    }
}
