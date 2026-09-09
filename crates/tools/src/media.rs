//! Media Pipeline (M2.10; docs/25): typed media reads with provenance,
//! budgets and artifact digests. Images stay images (the egress copy has its
//! metadata stripped), PDFs get deterministic text extraction first, every
//! derivative keeps lineage back to the original bytes, and derived text is
//! untrusted data that can never carry instructions.

use modbit_domain::TaskId;
use modbit_domain::media::{
    MediaBudget, MediaEnvelope, MediaKind, MediaProvenance, MediaTransform, TrustLabel,
};
use sha2::{Digest, Sha256};

use crate::pipeline::ObjectSink;

/// Alpha defaults (docs/25 "Security": decompression/page/size limits).
#[must_use]
pub fn default_budget() -> MediaBudget {
    MediaBudget {
        max_bytes: 16 * 1024 * 1024,
        max_pixels: 40_000_000,
        max_pages: 50,
        max_text_bytes: 64 * 1024,
    }
}

/// Typed pipeline failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaError {
    /// `MEDIA_BUDGET_EXCEEDED` | `MEDIA_MALFORMED` | `MEDIA_UNSUPPORTED`.
    pub code: &'static str,
    /// Detail (never contains media bytes).
    pub message: String,
}

fn err(code: &'static str, message: impl Into<String>) -> MediaError {
    MediaError {
        code,
        message: message.into(),
    }
}

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Media detection by magic bytes (never by extension alone).
#[must_use]
pub fn detect(bytes: &[u8]) -> (MediaKind, &'static str) {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        (MediaKind::Image, "image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        (MediaKind::Image, "image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        (MediaKind::Image, "image/gif")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        (MediaKind::Image, "image/webp")
    } else if bytes.starts_with(b"%PDF-") {
        (MediaKind::Document, "application/pdf")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        (MediaKind::Audio, "audio/wav")
    } else if bytes.len() > 8 && &bytes[4..8] == b"ftyp" {
        (MediaKind::Video, "video/mp4")
    } else if std::str::from_utf8(bytes).is_ok() {
        (MediaKind::Text, "text/plain")
    } else {
        (MediaKind::Binary, "application/octet-stream")
    }
}

/// What a read produced.
#[derive(Clone, Debug)]
pub struct MediaRead {
    /// The envelope.
    pub envelope: MediaEnvelope,
    /// Full text derivative (may exceed the inline budget; spilled by the caller).
    pub full_text: Option<String>,
}

/// Inputs for one read.
pub struct ReadRequest<'a> {
    /// Original bytes.
    pub bytes: &'a [u8],
    /// Source label (root-relative path).
    pub source: &'a str,
    /// Workspace revision.
    pub workspace_revision: Option<u64>,
    /// Task.
    pub task_id: Option<TaskId>,
    /// Page selection for documents (1-based, inclusive); empty = from page 1 within the budget.
    pub pages: Option<(u32, u32)>,
    /// Budget.
    pub budget: MediaBudget,
}

/// Run the pipeline. The original and egress copies are stored through `sink`.
pub fn read(req: &ReadRequest<'_>, sink: &dyn ObjectSink) -> Result<MediaRead, MediaError> {
    if req.bytes.len() as u64 > req.budget.max_bytes {
        return Err(err(
            "MEDIA_BUDGET_EXCEEDED",
            format!(
                "{} bytes exceed the media byte budget of {}",
                req.bytes.len(),
                req.budget.max_bytes
            ),
        ));
    }
    let (kind, mime) = detect(req.bytes);
    let original_digest = sha(req.bytes);
    let content_ref = sink
        .put(req.bytes)
        .map_err(|e| err("MEDIA_STORE", e.to_string()))?;
    let provenance = MediaProvenance {
        source: req.source.to_owned(),
        workspace_revision: req.workspace_revision,
        original_digest: original_digest.clone(),
        task_id: req.task_id,
    };
    let mut env = MediaEnvelope {
        kind,
        mime: mime.into(),
        content_ref,
        byte_length: req.bytes.len() as u64,
        egress_ref: None,
        width: None,
        height: None,
        pages: None,
        pages_covered: vec![],
        duration_ms: None,
        provenance,
        lineage: vec![],
        budget: req.budget.clone(),
        trust: TrustLabel::UntrustedWorkspaceContent,
        text_derivative: None,
        truncated: false,
        metadata_stripped: vec![],
    };
    let mut full_text = None;
    match (kind, mime) {
        (MediaKind::Image, "image/png") => {
            let (w, h) = png_dimensions(req.bytes)?;
            check_pixels(w, h, &req.budget)?;
            env.width = Some(w);
            env.height = Some(h);
            // Validate the image decodes within limits (decoder-enforced).
            let limits = png::Limits {
                bytes: (req.budget.max_pixels as usize)
                    .saturating_mul(4)
                    .min(512 * 1024 * 1024),
            };
            let decoder = png::Decoder::new_with_limits(std::io::Cursor::new(req.bytes), limits);
            let mut reader = decoder
                .read_info()
                .map_err(|e| err("MEDIA_MALFORMED", format!("png: {e}")))?;
            let mut buf = vec![0u8; reader.output_buffer_size().unwrap_or(0)];
            reader
                .next_frame(&mut buf)
                .map_err(|e| err("MEDIA_MALFORMED", format!("png: {e}")))?;
            let (egress, stripped) = strip_png_metadata(req.bytes);
            record_egress(
                &mut env,
                sink,
                "exif_strip",
                &original_digest,
                egress,
                stripped,
            )?;
        }
        (MediaKind::Image, "image/jpeg") => {
            let (w, h) = jpeg_dimensions(req.bytes)?;
            check_pixels(w, h, &req.budget)?;
            env.width = Some(w);
            env.height = Some(h);
            let (egress, stripped) = strip_jpeg_metadata(req.bytes);
            record_egress(
                &mut env,
                sink,
                "exif_strip",
                &original_digest,
                egress,
                stripped,
            )?;
        }
        (MediaKind::Document, _) => {
            let doc = lopdf::Document::load_mem(req.bytes)
                .map_err(|e| err("MEDIA_MALFORMED", format!("pdf: {e}")))?;
            let pages = doc.get_pages();
            let count = pages.len() as u32;
            env.pages = Some(count);
            if count == 0 {
                return Err(err("MEDIA_MALFORMED", "pdf: no pages"));
            }
            let (from, to) = req.pages.unwrap_or((1, count));
            if from == 0 || from > to || to > count {
                return Err(err(
                    "MEDIA_MALFORMED",
                    format!("page range {from}-{to} outside 1-{count}"),
                ));
            }
            let requested = to - from + 1;
            if requested > req.budget.max_pages {
                return Err(err(
                    "MEDIA_BUDGET_EXCEEDED",
                    format!(
                        "{requested} pages requested; the page budget is {} (select a page range)",
                        req.budget.max_pages
                    ),
                ));
            }
            let numbers: Vec<u32> = (from..=to).collect();
            let text = doc
                .extract_text_with_limit(
                    &numbers,
                    (req.budget.max_bytes as usize).min(64 * 1024 * 1024),
                )
                .map_err(|e| err("MEDIA_MALFORMED", format!("pdf text: {e}")))?;
            env.pages_covered = vec![(from, to)];
            env.lineage.push(MediaTransform {
                name: "pdf_text_extract".into(),
                input_digest: original_digest.clone(),
                output_digest: Some(sha(text.as_bytes())),
                detail: format!(
                    "pages {from}-{to} of {count}; deterministic extraction, no vision"
                ),
            });
            full_text = Some(text);
        }
        (MediaKind::Text, _) => {
            full_text = Some(String::from_utf8_lossy(req.bytes).into_owned());
        }
        (MediaKind::Audio | MediaKind::Video, _) => {
            env.lineage.push(MediaTransform {
                name: "metadata_only".into(),
                input_digest: original_digest.clone(),
                output_digest: None,
                detail: "audio/video transcripts and frames arrive with a capable provider; bytes retained by digest".into(),
            });
        }
        _ => {}
    }
    if let Some(t) = &full_text {
        let limit = req.budget.max_text_bytes as usize;
        let mut cut = t.len().min(limit);
        while cut > 0 && !t.is_char_boundary(cut) {
            cut -= 1;
        }
        env.text_derivative = Some(t[..cut].to_owned());
        env.truncated = cut < t.len();
        if env.truncated {
            env.lineage.push(MediaTransform {
                name: "truncate".into(),
                input_digest: sha(t.as_bytes()),
                output_digest: None,
                detail: format!(
                    "inline {cut} of {} bytes; the full text is retained by digest",
                    t.len()
                ),
            });
        }
    }
    Ok(MediaRead {
        envelope: env,
        full_text,
    })
}

fn record_egress(
    env: &mut MediaEnvelope,
    sink: &dyn ObjectSink,
    name: &str,
    original_digest: &str,
    egress: Vec<u8>,
    stripped: Vec<String>,
) -> Result<(), MediaError> {
    let egress_ref = sink
        .put(&egress)
        .map_err(|e| err("MEDIA_STORE", e.to_string()))?;
    env.lineage.push(MediaTransform {
        name: name.into(),
        input_digest: original_digest.to_owned(),
        output_digest: Some(egress_ref.clone()),
        detail: if stripped.is_empty() {
            "no metadata present".into()
        } else {
            format!("stripped {}", stripped.join(", "))
        },
    });
    env.egress_ref = Some(egress_ref);
    env.metadata_stripped = stripped;
    Ok(())
}

fn check_pixels(w: u32, h: u32, budget: &MediaBudget) -> Result<(), MediaError> {
    let pixels = u64::from(w) * u64::from(h);
    if pixels > budget.max_pixels {
        return Err(err(
            "MEDIA_BUDGET_EXCEEDED",
            format!(
                "{w}x{h} = {pixels} pixels exceed the pixel budget of {} (declared before decoding; nothing was decoded)",
                budget.max_pixels
            ),
        ));
    }
    Ok(())
}

/// Width/height from the IHDR chunk (no decoding).
pub fn png_dimensions(bytes: &[u8]) -> Result<(u32, u32), MediaError> {
    if bytes.len() < 24 || &bytes[12..16] != b"IHDR" {
        return Err(err("MEDIA_MALFORMED", "png: missing IHDR"));
    }
    let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    Ok((w, h))
}

/// Width/height from the first SOF marker (no decoding).
pub fn jpeg_dimensions(bytes: &[u8]) -> Result<(u32, u32), MediaError> {
    let mut i = 2usize;
    while i + 4 <= bytes.len() {
        if bytes[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = bytes[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        if (0xD0..=0xD9).contains(&marker) || marker == 0x01 {
            i += 2;
            continue;
        }
        let len = usize::from(u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]));
        let is_sof = matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF);
        if is_sof {
            if i + 9 > bytes.len() {
                break;
            }
            let h = u32::from(u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]));
            let w = u32::from(u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]));
            return Ok((w, h));
        }
        if marker == 0xDA {
            break;
        }
        i += 2 + len;
    }
    Err(err("MEDIA_MALFORMED", "jpeg: no SOF marker"))
}

/// Drop ancillary metadata chunks (tEXt/zTXt/iTXt/eXIf/tIME) from a PNG.
#[must_use]
pub fn strip_png_metadata(bytes: &[u8]) -> (Vec<u8>, Vec<String>) {
    let mut out = Vec::with_capacity(bytes.len());
    let mut stripped = Vec::new();
    if bytes.len() < 8 {
        return (bytes.to_vec(), stripped);
    }
    out.extend_from_slice(&bytes[..8]);
    let mut i = 8usize;
    while i + 12 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;
        let end = i + 12 + len;
        if end > bytes.len() {
            break;
        }
        let kind = &bytes[i + 4..i + 8];
        if matches!(kind, b"tEXt" | b"zTXt" | b"iTXt" | b"eXIf" | b"tIME") {
            stripped.push(String::from_utf8_lossy(kind).into_owned());
        } else {
            out.extend_from_slice(&bytes[i..end]);
        }
        i = end;
    }
    (out, stripped)
}

/// Drop APP1..APP15 (EXIF, XMP, ICC-less vendor blobs) and COM segments from a JPEG.
#[must_use]
pub fn strip_jpeg_metadata(bytes: &[u8]) -> (Vec<u8>, Vec<String>) {
    let mut out = Vec::with_capacity(bytes.len());
    let mut stripped = Vec::new();
    if bytes.len() < 4 {
        return (bytes.to_vec(), stripped);
    }
    out.extend_from_slice(&bytes[..2]);
    let mut i = 2usize;
    while i + 4 <= bytes.len() {
        if bytes[i] != 0xFF {
            out.extend_from_slice(&bytes[i..]);
            break;
        }
        let marker = bytes[i + 1];
        if marker == 0xDA {
            out.extend_from_slice(&bytes[i..]);
            break;
        }
        let len = usize::from(u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]));
        let end = (i + 2 + len).min(bytes.len());
        let drop = (0xE1..=0xEF).contains(&marker) || marker == 0xFE;
        if drop {
            stripped.push(match marker {
                0xE1 => "APP1(EXIF/XMP)".to_owned(),
                0xFE => "COM".to_owned(),
                m => format!("APP{}", m - 0xE0),
            });
        } else {
            out.extend_from_slice(&bytes[i..end]);
        }
        i = end;
    }
    (out, stripped)
}
