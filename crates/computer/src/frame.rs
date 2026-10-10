//! Frames as artifacts (PX-076; docs/66 CUC-F01, CUC-F02, CUC-B02).
//!
//! A frame an actuator returns is a sensitive artifact. Before it can reach a
//! model, an event or the object store the Core
//!
//! 1. decodes it and refuses it unless it is exactly the 1280 x 800 canvas;
//! 2. verifies that every secure region is masked (opaque black) and masks
//!    the ones that are not — the actuator masks first, the Core never
//!    trusts that it did;
//! 3. classifies the content: flat user-interface frames are encoded
//!    lossless, anything photographic at a quality ladder whose rungs reduce
//!    colour precision and never change the pixel size (every frame is a
//!    coordinate space);
//! 4. memoizes: an identical frame (same pixels after masking) is not
//!    encoded again.
//!
//! The lossy rungs quantize each channel and store the result as PNG, so no
//! image codec beyond the one the media pipeline already links is needed. A
//! real JPEG rung would be a dependency admission; the ladder is the place it
//! would slot in.

use std::collections::HashSet;
use std::io::Cursor;
use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::canvas::{CANVAS_HEIGHT, CANVAS_WIDTH, is_canvas};
use crate::model::Rect;

/// The colour a masked region is filled with: opaque black.
pub const MASK_RGBA: [u8; 4] = [0, 0, 0, 255];

/// The most pixels a frame may have (a canvas is ~1 M).
const MAX_PIXELS: u64 = 4_000_000;

/// A distinct-colour count at or below which a frame is flat user interface.
const FLAT_COLORS: usize = 1024;

/// Why a frame was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    /// Not a PNG, or one the decoder cannot read.
    #[error("the frame is not a readable PNG: {0}")]
    Unreadable(String),
    /// Not 1280 x 800.
    #[error("the frame is {0} x {1}, not the {CANVAS_WIDTH} x {CANVAS_HEIGHT} canvas")]
    WrongSize(u32, u32),
    /// Too large to decode.
    #[error("the frame has too many pixels")]
    TooLarge,
    /// The encoder failed.
    #[error("encoding failed: {0}")]
    Encode(String),
}

/// A decoded frame, 8-bit RGBA.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

impl Frame {
    /// A frame filled with one colour.
    #[must_use]
    pub fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Self {
        let mut data = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..width * height {
            data.extend_from_slice(&rgba);
        }
        Self {
            width,
            height,
            rgba: data,
        }
    }

    fn clamp(&self, r: &Rect) -> Option<(u32, u32, u32, u32)> {
        let x0 = r.x.max(0) as u32;
        let y0 = r.y.max(0) as u32;
        let x1 = (i64::from(r.x) + i64::from(r.width)).clamp(0, i64::from(self.width)) as u32;
        let y1 = (i64::from(r.y) + i64::from(r.height)).clamp(0, i64::from(self.height)) as u32;
        (x1 > x0 && y1 > y0 && x0 < self.width && y0 < self.height).then_some((x0, y0, x1, y1))
    }

    /// Fill `regions` with the mask colour; returns the pixels changed.
    pub fn mask(&mut self, regions: &[Rect]) -> u64 {
        let mut changed = 0u64;
        for r in regions {
            let Some((x0, y0, x1, y1)) = self.clamp(r) else {
                continue;
            };
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = ((y * self.width + x) * 4) as usize;
                    if self.rgba[i..i + 4] != MASK_RGBA {
                        self.rgba[i..i + 4].copy_from_slice(&MASK_RGBA);
                        changed += 1;
                    }
                }
            }
        }
        changed
    }

    /// Whether every pixel of the region (clamped to the frame) is the mask
    /// colour. A region outside the frame has nothing to leak and counts as masked.
    #[must_use]
    pub fn is_masked(&self, r: &Rect) -> bool {
        let Some((x0, y0, x1, y1)) = self.clamp(r) else {
            return true;
        };
        (y0..y1).all(|y| {
            (x0..x1).all(|x| {
                let i = ((y * self.width + x) * 4) as usize;
                self.rgba[i..i + 4] == MASK_RGBA
            })
        })
    }

    /// sha256 over the dimensions and the pixels.
    #[must_use]
    pub fn digest(&self) -> String {
        let mut h = Sha256::new();
        h.update(self.width.to_be_bytes());
        h.update(self.height.to_be_bytes());
        h.update(&self.rgba);
        hex::encode(h.finalize())
    }
}

/// Decode a PNG into 8-bit RGBA.
///
/// # Errors
/// Unreadable, or larger than the pixel limit.
pub fn decode_png(bytes: &[u8]) -> Result<Frame, FrameError> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|e| FrameError::Unreadable(e.to_string()))?;
    let (w, h) = (reader.info().width, reader.info().height);
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(FrameError::TooLarge);
    }
    let mut buf = vec![0u8; reader.output_buffer_size().ok_or(FrameError::TooLarge)?];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| FrameError::Unreadable(e.to_string()))?;
    let px = &buf[..info.buffer_size()];
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    match info.color_type {
        png::ColorType::Rgba => rgba.extend_from_slice(px),
        png::ColorType::Rgb => {
            for c in px.chunks_exact(3) {
                rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        png::ColorType::Grayscale => {
            for g in px {
                rgba.extend_from_slice(&[*g, *g, *g, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for c in px.chunks_exact(2) {
                rgba.extend_from_slice(&[c[0], c[0], c[0], c[1]]);
            }
        }
        png::ColorType::Indexed => {
            return Err(FrameError::Unreadable(
                "an indexed image was not expanded".into(),
            ));
        }
    }
    Ok(Frame {
        width: w,
        height: h,
        rgba,
    })
}

/// What a frame's content is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Content {
    /// Flat user interface: few colours, large uniform areas. Lossless.
    Flat,
    /// Photographic or otherwise colour-rich. Lossy ladder.
    Photographic,
}

/// Classify by the number of distinct colours (early exit past the bound).
#[must_use]
pub fn classify(frame: &Frame) -> Content {
    let mut seen: HashSet<u32> = HashSet::with_capacity(FLAT_COLORS + 1);
    for px in frame.rgba.chunks_exact(4) {
        seen.insert(u32::from_le_bytes([px[0], px[1], px[2], px[3]]));
        if seen.len() > FLAT_COLORS {
            return Content::Photographic;
        }
    }
    Content::Flat
}

/// One rung of the lossy quality ladder: channel precision in bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rung {
    /// The rung's name as the artifact records it.
    pub name: &'static str,
    /// Bits kept per colour channel.
    pub bits: u8,
}

/// The declared ladder, best quality first. A flat frame is never encoded on
/// it; a photographic one takes the first rung that fits the size target.
pub const LADDER: &[Rung] = &[
    Rung {
        name: "lossy-q85",
        bits: 6,
    },
    Rung {
        name: "lossy-q70",
        bits: 5,
    },
    Rung {
        name: "lossy-q55",
        bits: 4,
    },
];

/// The size a photographic frame is encoded to fit, when it can.
pub const TARGET_BYTES: usize = 700 * 1024;

/// An encoded frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoded {
    /// The bytes.
    pub bytes: Vec<u8>,
    /// MIME type.
    pub mime: &'static str,
    /// `lossless` or a rung name.
    pub codec: String,
    /// Whether decoding gives back the pixels exactly.
    pub lossless: bool,
    /// Width (always the canvas).
    pub width: u32,
    /// Height.
    pub height: u32,
}

fn write_png(frame: &Frame, quantize_bits: Option<u8>) -> Result<Vec<u8>, FrameError> {
    let opaque = frame.rgba.chunks_exact(4).all(|p| p[3] == 255);
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, frame.width, frame.height);
        enc.set_color(if opaque {
            png::ColorType::Rgb
        } else {
            png::ColorType::Rgba
        });
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut w = enc
            .write_header()
            .map_err(|e| FrameError::Encode(e.to_string()))?;
        let quant = |v: u8| -> u8 {
            match quantize_bits {
                None => v,
                Some(b) => {
                    let levels = (1u32 << b) - 1;
                    let q = (u32::from(v) * levels + 127) / 255;
                    ((q * 255 + levels / 2) / levels) as u8
                }
            }
        };
        let data: Vec<u8> = if opaque {
            frame
                .rgba
                .chunks_exact(4)
                .flat_map(|p| [quant(p[0]), quant(p[1]), quant(p[2])])
                .collect()
        } else {
            frame
                .rgba
                .chunks_exact(4)
                .flat_map(|p| [quant(p[0]), quant(p[1]), quant(p[2]), p[3]])
                .collect()
        };
        w.write_image_data(&data)
            .map_err(|e| FrameError::Encode(e.to_string()))?;
    }
    Ok(out)
}

/// Encode a frame for its content: lossless for flat user interface; for a
/// photographic one the first ladder rung that fits `target_bytes`, else the
/// last. The pixel size never changes.
///
/// # Errors
/// The encoder failed.
pub fn encode(frame: &Frame, content: Content, target_bytes: usize) -> Result<Encoded, FrameError> {
    match content {
        Content::Flat => Ok(Encoded {
            bytes: write_png(frame, None)?,
            mime: "image/png",
            codec: "lossless".into(),
            lossless: true,
            width: frame.width,
            height: frame.height,
        }),
        Content::Photographic => {
            let mut last: Option<(Vec<u8>, &Rung)> = None;
            for rung in LADDER {
                let bytes = write_png(frame, Some(rung.bits))?;
                let fits = bytes.len() <= target_bytes;
                last = Some((bytes, rung));
                if fits {
                    break;
                }
            }
            let (bytes, rung) = last.expect("the ladder has rungs");
            Ok(Encoded {
                bytes,
                mime: "image/png",
                codec: rung.name.into(),
                lossless: false,
                width: frame.width,
                height: frame.height,
            })
        }
    }
}

/// What preparing a frame produced.
#[derive(Clone, Debug)]
pub struct Prepared {
    /// The encoded, masked frame.
    pub encoded: Arc<Encoded>,
    /// The digest of the masked pixels.
    pub digest: String,
    /// True when the same pixels were encoded before and nothing was encoded again.
    pub reused: bool,
    /// Regions the actuator had masked.
    pub masked_by_actuator: u32,
    /// Regions the Core had to mask because the actuator had not.
    pub masked_by_core: u32,
    /// The content class.
    pub content: Content,
}

/// Per-session memory of the last frame, so an identical one is not encoded again.
#[derive(Debug, Default)]
pub struct FrameMemo {
    last: Option<(String, Arc<Encoded>, Content)>,
    /// Frames encoded.
    pub encodes: u64,
    /// Frames answered from memory.
    pub reuses: u64,
}

impl FrameMemo {
    /// Whether the digest is the last frame's.
    #[must_use]
    pub fn is_last(&self, digest: &str) -> bool {
        self.last.as_ref().is_some_and(|(d, ..)| d == digest)
    }
}

/// Verify, mask, classify and encode (or reuse) one actuator frame.
///
/// `secure_regions` are in canvas pixels: the ones the actuator reported plus
/// the ones the Core derives from the tree; the first `actuator_reported`
/// of them are the actuator's own.
///
/// # Errors
/// The frame is unreadable or not the canvas.
pub fn prepare(
    png: &[u8],
    secure_regions: &[Rect],
    actuator_reported: usize,
    memo: &mut FrameMemo,
) -> Result<Prepared, FrameError> {
    let mut frame = decode_png(png)?;
    if !is_canvas(frame.width, frame.height) {
        return Err(FrameError::WrongSize(frame.width, frame.height));
    }
    let mut by_actuator = 0u32;
    let mut by_core = 0u32;
    for (i, r) in secure_regions.iter().enumerate() {
        // A region is masked when its interior is: the Core pads the regions
        // it derives from the tree by a pixel for the scaler's rounding, and
        // that rim is not the actuator's failure.
        let interior = Rect {
            x: r.x + 2,
            y: r.y + 2,
            width: r.width - 4,
            height: r.height - 4,
        };
        let probe = if interior.has_area() { &interior } else { r };
        if frame.is_masked(probe) {
            frame.mask(std::slice::from_ref(r));
            if i < actuator_reported {
                by_actuator += 1;
            }
        } else {
            frame.mask(std::slice::from_ref(r));
            by_core += 1;
        }
    }
    let digest = frame.digest();
    if let Some((d, enc, content)) = &memo.last
        && *d == digest
    {
        memo.reuses += 1;
        return Ok(Prepared {
            encoded: Arc::clone(enc),
            digest,
            reused: true,
            masked_by_actuator: by_actuator,
            masked_by_core: by_core,
            content: *content,
        });
    }
    let content = classify(&frame);
    let encoded = Arc::new(encode(&frame, content, TARGET_BYTES)?);
    memo.encodes += 1;
    memo.last = Some((digest.clone(), Arc::clone(&encoded), content));
    Ok(Prepared {
        encoded,
        digest,
        reused: false,
        masked_by_actuator: by_actuator,
        masked_by_core: by_core,
        content,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ui_frame() -> Frame {
        let mut f = Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, [240, 240, 240, 255]);
        // A "button" and a "field".
        for y in 100..140 {
            for x in 100..300 {
                let i = ((y * f.width + x) * 4) as usize;
                f.rgba[i..i + 4].copy_from_slice(&[30, 100, 220, 255]);
            }
        }
        f
    }

    fn photo_frame() -> Frame {
        let mut f = Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, [0, 0, 0, 255]);
        let mut s = 0x1234_5678u32;
        for px in f.rgba.chunks_exact_mut(4) {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            px[0] = (s >> 24) as u8;
            px[1] = (s >> 16) as u8;
            px[2] = (s >> 8) as u8;
        }
        f
    }

    fn png_of(f: &Frame) -> Vec<u8> {
        write_png(f, None).unwrap()
    }

    #[test]
    fn a_flat_frame_is_lossless_and_a_photographic_one_is_lossy_at_the_same_pixel_size() {
        let flat = ui_frame();
        assert_eq!(classify(&flat), Content::Flat);
        let e = encode(&flat, Content::Flat, TARGET_BYTES).unwrap();
        assert!(e.lossless && e.codec == "lossless");
        assert_eq!(
            decode_png(&e.bytes).unwrap(),
            flat,
            "lossless decodes to the same pixels"
        );

        let photo = photo_frame();
        assert_eq!(classify(&photo), Content::Photographic);
        let e = encode(&photo, Content::Photographic, TARGET_BYTES).unwrap();
        assert!(!e.lossless);
        assert!(LADDER.iter().any(|r| r.name == e.codec), "{}", e.codec);
        let back = decode_png(&e.bytes).unwrap();
        assert_eq!(
            (back.width, back.height),
            (CANVAS_WIDTH, CANVAS_HEIGHT),
            "the size never changes"
        );
        assert_ne!(back, photo, "a lossy rung changes pixels");
    }

    #[test]
    fn a_secure_region_the_actuator_did_not_mask_is_masked_by_the_core() {
        let mut f = ui_frame();
        let secret = Rect {
            x: 400,
            y: 300,
            width: 200,
            height: 30,
        };
        for y in 300..330 {
            for x in 400..600 {
                let i = ((y * f.width + x) * 4) as usize;
                f.rgba[i..i + 4].copy_from_slice(&[10, 200, 10, 255]);
            }
        }
        let mut memo = FrameMemo::default();
        let p = prepare(&png_of(&f), &[secret], 1, &mut memo).unwrap();
        assert_eq!((p.masked_by_actuator, p.masked_by_core), (0, 1));
        let stored = decode_png(&p.encoded.bytes).unwrap();
        assert!(
            stored.is_masked(&secret),
            "no unmasked pixel of the secure region is retained"
        );
        assert!(
            !stored.is_masked(&Rect {
                x: 100,
                y: 100,
                width: 200,
                height: 40
            }),
            "the rest is untouched"
        );

        // The actuator masked it itself: nothing for the Core to do.
        let mut g = ui_frame();
        g.mask(&[secret]);
        let mut memo2 = FrameMemo::default();
        let p2 = prepare(&png_of(&g), &[secret], 1, &mut memo2).unwrap();
        assert_eq!((p2.masked_by_actuator, p2.masked_by_core), (1, 0));
    }

    #[test]
    fn a_region_padded_by_the_core_is_not_counted_against_an_actuator_that_masked_the_interior() {
        let mut f = ui_frame();
        let field = Rect {
            x: 400,
            y: 300,
            width: 200,
            height: 30,
        };
        f.mask(&[field]);
        let padded = Rect {
            x: 399,
            y: 299,
            width: 202,
            height: 32,
        };
        let mut memo = FrameMemo::default();
        let p = prepare(&png_of(&f), &[field, padded], 1, &mut memo).unwrap();
        assert_eq!((p.masked_by_actuator, p.masked_by_core), (1, 0));
        let stored = decode_png(&p.encoded.bytes).unwrap();
        assert!(stored.is_masked(&padded), "the rim is masked too");
    }

    #[test]
    fn two_identical_captures_produce_one_encode_and_a_wrong_size_is_refused() {
        let f = ui_frame();
        let bytes = png_of(&f);
        let mut memo = FrameMemo::default();
        let a = prepare(&bytes, &[], 0, &mut memo).unwrap();
        let b = prepare(&bytes, &[], 0, &mut memo).unwrap();
        assert!(!a.reused && b.reused);
        assert_eq!((memo.encodes, memo.reuses), (1, 1));
        assert!(Arc::ptr_eq(&a.encoded, &b.encoded));
        let mut changed = f.clone();
        changed.rgba[0] = 1;
        let c = prepare(&png_of(&changed), &[], 0, &mut memo).unwrap();
        assert!(!c.reused);
        assert_eq!(memo.encodes, 2);

        let small = Frame::solid(640, 400, [1, 2, 3, 255]);
        assert_eq!(
            prepare(&png_of(&small), &[], 0, &mut memo).unwrap_err(),
            FrameError::WrongSize(640, 400)
        );
        assert!(matches!(
            decode_png(b"not a png"),
            Err(FrameError::Unreadable(_))
        ));
    }
}
