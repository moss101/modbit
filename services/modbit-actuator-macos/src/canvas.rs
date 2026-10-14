//! The model-facing canvas (docs/66 CUC-B02): a source surface (a window or a
//! display) fitted into 1280 x 800 by ONE scaler, letterboxed and never
//! stretched, then secure regions painted opaque black BEFORE the frame
//! leaves this process (CUC-F01). Pure and portable: every line here is
//! tested on every operating system.

use sha2::{Digest, Sha256};

/// Canvas width in pixels.
pub const W: u32 = 1280;
/// Canvas height in pixels.
pub const H: u32 = 800;

/// A rectangle in whatever unit its caller says (points or canvas pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    /// Left.
    pub x: f64,
    /// Top.
    pub y: f64,
    /// Width.
    pub w: f64,
    /// Height.
    pub h: f64,
}

/// How a source of `source_w` x `source_h` display points is fitted: a canvas
/// point `p` maps to the source point `(p - offset) / scale`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Letterbox {
    /// Source width in display points.
    pub source_w: u32,
    /// Source height in display points.
    pub source_h: u32,
    /// Canvas pixels per display point.
    pub scale: f64,
    /// Left margin in canvas pixels.
    pub offset_x: f64,
    /// Top margin in canvas pixels.
    pub offset_y: f64,
}

/// The one scaler.
#[must_use]
pub fn fit(source_w: u32, source_h: u32) -> Letterbox {
    let (sw, sh) = (f64::from(source_w.max(1)), f64::from(source_h.max(1)));
    let scale = (f64::from(W) / sw).min(f64::from(H) / sh);
    Letterbox {
        source_w,
        source_h,
        scale,
        offset_x: (f64::from(W) - sw * scale) / 2.0,
        offset_y: (f64::from(H) - sh * scale) / 2.0,
    }
}

impl Letterbox {
    /// The source point (display points, relative to the source origin) a
    /// canvas point denotes, or `None` for the margin or outside the canvas.
    #[must_use]
    pub fn to_source(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        if !(0.0..=f64::from(W)).contains(&x) || !(0.0..=f64::from(H)).contains(&y) {
            return None;
        }
        if self.scale <= 0.0 {
            return None;
        }
        let (sx, sy) = ((x - self.offset_x) / self.scale, (y - self.offset_y) / self.scale);
        (sx >= -0.5
            && sy >= -0.5
            && sx <= f64::from(self.source_w) + 0.5
            && sy <= f64::from(self.source_h) + 0.5)
            .then_some((sx, sy))
    }

    /// The canvas rectangle a source rectangle (display points relative to
    /// the source origin) covers, rounded outward by one pixel so a mask
    /// never leaves a sliver of a secret at an edge. Clipped to the canvas.
    #[must_use]
    pub fn to_canvas(&self, r: Rect) -> Option<(u32, u32, u32, u32)> {
        let x0 = (r.x * self.scale + self.offset_x).floor() - 1.0;
        let y0 = (r.y * self.scale + self.offset_y).floor() - 1.0;
        let x1 = ((r.x + r.w) * self.scale + self.offset_x).ceil() + 1.0;
        let y1 = ((r.y + r.h) * self.scale + self.offset_y).ceil() + 1.0;
        let (x0, y0) = (x0.max(0.0), y0.max(0.0));
        let (x1, y1) = (x1.min(f64::from(W)), y1.min(f64::from(H)));
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        // Truncation is the intent: the values are already clipped to the canvas.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some((x0 as u32, y0 as u32, x1 as u32, y1 as u32))
    }
}

/// Compose `rgba` (`px_w` x `px_h`, straight alpha, row-major) into the
/// canvas through `lb`: the source fills the letterboxed area by area
/// averaging, the margins are mid-grey. Opaque output.
///
/// # Panics
/// `rgba` shorter than `px_w * px_h * 4` (a caller bug).
#[must_use]
pub fn compose(rgba: &[u8], px_w: u32, px_h: u32, lb: &Letterbox) -> Vec<u8> {
    assert!(rgba.len() >= (px_w as usize) * (px_h as usize) * 4);
    let mut out = vec![0u8; (W as usize) * (H as usize) * 4];
    for px in out.chunks_exact_mut(4) {
        px.copy_from_slice(&[60, 60, 60, 255]);
    }
    if px_w == 0 || px_h == 0 {
        return out;
    }
    // Destination rectangle in canvas pixels.
    let dx0 = lb.offset_x.round().max(0.0);
    let dy0 = lb.offset_y.round().max(0.0);
    let dw = (f64::from(lb.source_w) * lb.scale).round().max(1.0);
    let dh = (f64::from(lb.source_h) * lb.scale).round().max(1.0);
    let (sxr, syr) = (f64::from(px_w) / dw, f64::from(px_h) / dh);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (dx0, dy0, dw, dh) = (dx0 as u32, dy0 as u32, dw as u32, dh as u32);
    for y in 0..dh {
        let cy = dy0 + y;
        if cy >= H {
            break;
        }
        let (sy0, sy1) = span(y, syr, px_h);
        for x in 0..dw {
            let cx = dx0 + x;
            if cx >= W {
                break;
            }
            let (sx0, sx1) = span(x, sxr, px_w);
            let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
            for sy in sy0..sy1 {
                let row = (sy as usize) * (px_w as usize);
                for sx in sx0..sx1 {
                    let i = (row + sx as usize) * 4;
                    r += u32::from(rgba[i]);
                    g += u32::from(rgba[i + 1]);
                    b += u32::from(rgba[i + 2]);
                    n += 1;
                }
            }
            if n > 0 {
                let o = ((cy as usize) * (W as usize) + cx as usize) * 4;
                // Averages of u8 values fit u8.
                #[allow(clippy::cast_possible_truncation)]
                {
                    out[o] = (r / n) as u8;
                    out[o + 1] = (g / n) as u8;
                    out[o + 2] = (b / n) as u8;
                }
                out[o + 3] = 255;
            }
        }
    }
    out
}

/// The source pixel span `[a, b)` that destination pixel `d` averages.
fn span(d: u32, ratio: f64, limit: u32) -> (u32, u32) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let a = (f64::from(d) * ratio).floor() as u32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let b = (((f64::from(d) + 1.0) * ratio).ceil() as u32).max(a + 1);
    (a.min(limit.saturating_sub(1)), b.min(limit))
}

/// Paint `(x0, y0, x1, y1)` canvas rectangles opaque black (0,0,0,255).
pub fn mask(rgba: &mut [u8], rects: &[(u32, u32, u32, u32)]) {
    for &(x0, y0, x1, y1) in rects {
        for y in y0..y1.min(H) {
            for x in x0..x1.min(W) {
                let o = ((y as usize) * (W as usize) + x as usize) * 4;
                rgba[o..o + 4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
    }
}

/// Encode the canvas as PNG (lossless; the Core re-encodes by content).
///
/// # Panics
/// Only if the PNG encoder fails on an in-memory buffer.
#[must_use]
pub fn encode_png(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, W, H);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut w = enc.write_header().expect("png header");
        w.write_image_data(rgba).expect("png data");
    }
    out
}

/// sha256 of the canvas pixels, hex: the memoisation key.
#[must_use]
pub fn digest(rgba: &[u8]) -> String {
    hex::encode(Sha256::digest(rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scaler_letterboxes_and_never_stretches() {
        let lb = fit(2560, 1600);
        assert!((lb.scale - 0.5).abs() < 1e-9 && lb.offset_x == 0.0 && lb.offset_y == 0.0);
        let sq = fit(800, 800);
        assert!((sq.scale - 1.0).abs() < 1e-9 && (sq.offset_x - 240.0).abs() < 1e-9);
        assert!(sq.to_source(100.0, 100.0).is_none(), "the margin has no source");
        let (x, y) = sq.to_source(240.0, 400.0).unwrap();
        assert!(x.abs() < 1e-9 && (y - 400.0).abs() < 1e-9);
    }

    #[test]
    fn a_mask_covers_its_region_and_one_pixel_beyond_it() {
        let lb = fit(1280, 800);
        let (x0, y0, x1, y1) = lb.to_canvas(Rect { x: 100.0, y: 50.0, w: 200.0, h: 30.0 }).unwrap();
        assert!(x0 <= 99 && y0 <= 49 && x1 >= 301 && y1 >= 81, "{x0},{y0},{x1},{y1}");
        assert!(lb.to_canvas(Rect { x: 5000.0, y: 0.0, w: 10.0, h: 10.0 }).is_none());
    }

    #[test]
    fn compose_averages_down_and_a_secret_does_not_survive_the_mask() {
        // A 2560 x 1600 source (a 2x display): left half white, a "secret"
        // block of distinctive colour inside it.
        let (pw, ph) = (2560u32, 1600u32);
        let mut src = vec![255u8; (pw * ph * 4) as usize];
        for y in 200..260u32 {
            for x in 400..800u32 {
                let o = ((y * pw + x) * 4) as usize;
                src[o..o + 4].copy_from_slice(&[12, 200, 34, 255]);
            }
        }
        let lb = fit(1280, 800);
        let mut c = compose(&src, pw, ph, &lb);
        // The secret is visible before masking: points (200..400, 100..130).
        let at = |c: &[u8], x: u32, y: u32| {
            let o = ((y * W + x) * 4) as usize;
            [c[o], c[o + 1], c[o + 2], c[o + 3]]
        };
        assert_eq!(at(&c, 300, 115), [12, 200, 34, 255]);
        let r = lb.to_canvas(Rect { x: 200.0, y: 100.0, w: 200.0, h: 30.0 }).unwrap();
        mask(&mut c, &[r]);
        for y in r.1..r.3 {
            for x in r.0..r.2 {
                assert_eq!(at(&c, x, y), [0, 0, 0, 255]);
            }
        }
        assert!(!c.chunks_exact(4).any(|p| p == [12, 200, 34, 255]), "no pixel of the secret is left");
        assert_eq!(encode_png(&c)[..4], [0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn a_narrow_source_gets_grey_margins() {
        let src = vec![255u8; 400 * 800 * 4];
        let c = compose(&src, 400, 800, &fit(400, 800));
        assert_eq!(&c[..4], &[60, 60, 60, 255], "margin");
        let mid = ((400 * W + 640) * 4) as usize;
        assert_eq!(&c[mid..mid + 4], &[255, 255, 255, 255]);
    }
}
