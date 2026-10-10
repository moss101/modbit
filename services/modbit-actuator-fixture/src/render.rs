//! Frames of the fixture desktop: deterministic pixels drawn from the
//! elements, letterboxed into the 1280 x 800 canvas by the fixture's own
//! scaler (independent of the Core's), with secure fields rendered as their
//! value's pattern and then masked opaque black unless a fault says not to.

use sha2::{Digest, Sha256};

use crate::desktop::{App, El, Rect, Win};

pub const CANVAS_W: u32 = 1280;
pub const CANVAS_H: u32 = 800;

#[derive(Clone, Copy, Debug)]
pub struct Letterbox {
    pub source_w: u32,
    pub source_h: u32,
    pub scale: f64,
    pub offset_x: f64,
    pub offset_y: f64,
}

pub fn fit(source_w: u32, source_h: u32) -> Letterbox {
    let (sw, sh) = (f64::from(source_w.max(1)), f64::from(source_h.max(1)));
    let scale = (f64::from(CANVAS_W) / sw).min(f64::from(CANVAS_H) / sh);
    Letterbox {
        source_w,
        source_h,
        scale,
        offset_x: (f64::from(CANVAS_W) - sw * scale) / 2.0,
        offset_y: (f64::from(CANVAS_H) - sh * scale) / 2.0,
    }
}

impl Letterbox {
    /// The source point of a canvas point (relative to the source origin).
    pub fn to_source(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let sx = (x - self.offset_x) / self.scale;
        let sy = (y - self.offset_y) / self.scale;
        (sx >= 0.0 && sy >= 0.0 && sx <= f64::from(self.source_w) && sy <= f64::from(self.source_h))
            .then_some((sx, sy))
    }

    fn rect(&self, origin: (i32, i32), r: Rect) -> (i64, i64, i64, i64) {
        let x0 = f64::from(r.x - origin.0) * self.scale + self.offset_x;
        let y0 = f64::from(r.y - origin.1) * self.scale + self.offset_y;
        let x1 = f64::from(r.x - origin.0 + r.width) * self.scale + self.offset_x;
        let y1 = f64::from(r.y - origin.1 + r.height) * self.scale + self.offset_y;
        (
            x0.floor() as i64,
            y0.floor() as i64,
            x1.ceil() as i64,
            y1.ceil() as i64,
        )
    }
}

type Px = (i64, i64, i64, i64);

pub struct Canvas {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

impl Canvas {
    fn new(w: u32, h: u32, c: [u8; 4]) -> Self {
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            rgba.extend_from_slice(&c);
        }
        Self { w, h, rgba }
    }

    fn fill(&mut self, r: Px, c: [u8; 4]) {
        let (x0, y0, x1, y1) = r;
        for y in y0.max(0)..y1.min(i64::from(self.h)) {
            for x in x0.max(0)..x1.min(i64::from(self.w)) {
                let i = ((y as u32 * self.w + x as u32) * 4) as usize;
                self.rgba[i..i + 4].copy_from_slice(&c);
            }
        }
    }

    fn pattern(&mut self, r: Px, seed: &[u8], base: [u8; 3]) {
        let h = Sha256::digest(seed);
        let (x0, y0, x1, y1) = r;
        for y in y0.max(0)..y1.min(i64::from(self.h)) {
            for x in x0.max(0)..x1.min(i64::from(self.w)) {
                let k = h[((x / 4 + y / 4) as usize) % 32];
                let i = ((y as u32 * self.w + x as u32) * 4) as usize;
                self.rgba[i] = base[0].wrapping_add(k >> 2);
                self.rgba[i + 1] = base[1].wrapping_add(k >> 3);
                self.rgba[i + 2] = base[2].wrapping_add(k >> 1);
                self.rgba[i + 3] = 255;
            }
        }
    }

    fn noise(&mut self, r: Px, seed: u32) {
        let mut s = seed | 1;
        let (x0, y0, x1, y1) = r;
        for y in y0.max(0)..y1.min(i64::from(self.h)) {
            for x in x0.max(0)..x1.min(i64::from(self.w)) {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let i = ((y as u32 * self.w + x as u32) * 4) as usize;
                self.rgba[i] = (s >> 24) as u8;
                self.rgba[i + 1] = (s >> 16) as u8;
                self.rgba[i + 2] = (s >> 8) as u8;
                self.rgba[i + 3] = 255;
            }
        }
    }
}

fn draw_element(c: &mut Canvas, lb: &Letterbox, origin: (i32, i32), e: &El, masks: &mut Vec<Px>) {
    let r = lb.rect(origin, e.bounds);
    if e.photo {
        c.noise(r, 0x9e37_79b9);
        return;
    }
    match e.role.to_ascii_lowercase().as_str() {
        "button" => {
            c.fill(r, [30, 100, 220, 255]);
            c.pattern(
                (r.0 + 2, r.1 + 2, r.2 - 2, r.3 - 2),
                e.name.as_bytes(),
                [200, 200, 230],
            );
        }
        "label" => c.pattern(r, e.name.as_bytes(), [160, 160, 160]),
        _ => {
            c.fill(r, [255, 255, 255, 255]);
            // The field shows its value: a secure field shows the secret's
            // pattern until it is masked.
            c.pattern(
                (r.0 + 2, r.1 + 2, r.2 - 2, r.3 - 2),
                e.value.as_bytes(),
                [20, 160, 40],
            );
            if e.focused {
                c.fill((r.0, r.1, r.2, r.1 + 2), [220, 30, 30, 255]);
            }
        }
    }
    if e.secure {
        masks.push(r);
    }
}

/// One rendered frame and the canvas rectangles of the secure regions.
pub struct Frame {
    pub png: Vec<u8>,
    pub letterbox: Letterbox,
    pub secure: Vec<Rect>,
    pub masked: u32,
    pub digest: String,
}

fn px_rect(r: Px) -> Rect {
    Rect {
        x: r.0 as i32,
        y: r.1 as i32,
        width: (r.2 - r.0) as i32,
        height: (r.3 - r.1) as i32,
    }
}

pub fn render_window(win: &Win, skip_mask: bool, hide_regions: bool) -> Frame {
    let lb = fit(
        win.bounds.width.max(1) as u32,
        win.bounds.height.max(1) as u32,
    );
    let mut c = Canvas::new(CANVAS_W, CANVAS_H, [60, 60, 60, 255]);
    let origin = (win.bounds.x, win.bounds.y);
    c.fill(lb.rect(origin, win.bounds), [240, 240, 240, 255]);
    let mut masks = Vec::new();
    for e in &win.elements {
        draw_element(&mut c, &lb, origin, e, &mut masks);
    }
    let secure: Vec<Rect> = masks.iter().map(|m| px_rect(*m)).collect();
    finish(c, lb, &masks, secure, skip_mask, hide_regions)
}

pub fn render_display(display: Rect, apps: &[App], skip_mask: bool, hide_regions: bool) -> Frame {
    let lb = fit(display.width.max(1) as u32, display.height.max(1) as u32);
    let mut c = Canvas::new(CANVAS_W, CANVAS_H, [40, 60, 90, 255]);
    let origin = (display.x, display.y);
    let mut masks = Vec::new();
    for app in apps {
        for w in app.windows.iter().filter(|w| !w.hidden && !w.minimized) {
            c.fill(lb.rect(origin, w.bounds), [240, 240, 240, 255]);
            for e in &w.elements {
                draw_element(&mut c, &lb, origin, e, &mut masks);
            }
        }
    }
    let secure: Vec<Rect> = masks.iter().map(|m| px_rect(*m)).collect();
    finish(c, lb, &masks, secure, skip_mask, hide_regions)
}

fn finish(
    mut c: Canvas,
    letterbox: Letterbox,
    masks: &[Px],
    secure: Vec<Rect>,
    skip_mask: bool,
    hide_regions: bool,
) -> Frame {
    let mut masked = 0;
    if !skip_mask {
        for m in masks {
            c.fill(*m, [0, 0, 0, 255]);
            masked += 1;
        }
    }
    let digest = hex::encode(Sha256::digest(&c.rgba));
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, c.w, c.h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut w = enc.write_header().expect("png header");
        w.write_image_data(&c.rgba).expect("png data");
    }
    Frame {
        png: out,
        letterbox,
        secure: if hide_regions { Vec::new() } else { secure },
        masked,
        digest,
    }
}

/// A frame of the wrong size, for the fault that returns one.
pub fn wrong_size_png() -> Vec<u8> {
    let (w, h) = (640u32, 400u32);
    let data = vec![200u8; (w * h * 4) as usize];
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().expect("png header");
        wr.write_image_data(&data).expect("png data");
    }
    out
}
