//! Window and display capture with the secure-field contract (CUC-F01):
//! every secure field that is on the frame is located through the
//! accessibility tree and handed back as a region the portable layer paints
//! opaque black BEFORE the frame leaves this process. A frame whose secure
//! fields cannot all be located is not returned at all (fail closed); in
//! SCREEN scope a foreign application whose tree cannot be read is blacked
//! out whole instead.
#![allow(unsafe_code)]

use std::ffi::c_void;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use modbit_protocol::v1 as wire;

use super::ax::{self, Ax};
use super::cgwin;
use super::ffi::{
    CGBitmapContextCreate, CGColorSpaceCreateDeviceRGB, CGColorSpaceRelease, CGContextDrawImage,
    CGContextRelease, CGImageGetHeight, CGImageGetWidth, CGImageRef, CGImageRelease, CGPoint,
    CGRect, CGRectNull, CGSize, CGWindowListCreateImageFn, RTLD_DEFAULT, dlsym,
};
use super::Mac;
use crate::backend::{CaptureArgs, Ctl, Fail, RawFrame, Res};
use crate::canvas::Rect;

const SECURE_SCAN_NODES: usize = 5000;
const SECURE_SCAN_BUDGET: Duration = Duration::from_millis(2500);
const SCREEN_SCAN_BUDGET: Duration = Duration::from_secs(5);

fn create_image_fn() -> Option<CGWindowListCreateImageFn> {
    static F: OnceLock<Option<CGWindowListCreateImageFn>> = OnceLock::new();
    *F.get_or_init(|| {
        let name = c"CGWindowListCreateImage";
        // SAFETY: dlsym of a C symbol; the transmute target is its documented signature.
        let p = unsafe { dlsym(RTLD_DEFAULT, name.as_ptr()) };
        (!p.is_null()).then(|| unsafe { std::mem::transmute::<*mut c_void, CGWindowListCreateImageFn>(p) })
    })
}

/// Straight RGBA of a CGImage, drawn into a buffer we own.
fn rgba_of(img: CGImageRef) -> Option<(Vec<u8>, u32, u32)> {
    // SAFETY: `img` is a live CGImage (owned by the caller); the context draws
    // into `buf`, which outlives it; every created object is released.
    unsafe {
        let (w, h) = (CGImageGetWidth(img), CGImageGetHeight(img));
        if w == 0 || h == 0 || w > 16384 || h > 16384 {
            return None;
        }
        let mut buf = vec![0u8; w * h * 4];
        let space = CGColorSpaceCreateDeviceRGB();
        // kCGImageAlphaPremultipliedLast = 1, default (big-endian) byte order: R G B A.
        let ctx = CGBitmapContextCreate(buf.as_mut_ptr().cast(), w, h, 8, w * 4, space, 1);
        CGColorSpaceRelease(space);
        if ctx.is_null() {
            return None;
        }
        #[allow(clippy::cast_precision_loss)]
        CGContextDrawImage(
            ctx,
            CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize {
                    width: w as f64,
                    height: h as f64,
                },
            },
            img,
        );
        CGContextRelease(ctx);
        // Un-premultiply: the canvas works in straight alpha.
        for px in buf.chunks_exact_mut(4) {
            let a = u32::from(px[3]);
            if a != 0 && a != 255 {
                for c in &mut px[..3] {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
                    }
                }
            }
        }
        Some((buf, u32::try_from(w).ok()?, u32::try_from(h).ok()?))
    }
}

fn grab(rect: CGRect, option: u32, window: u32, image_option: u32) -> Res<(Vec<u8>, u32, u32)> {
    let Some(f) = create_image_fn() else {
        return Err(Fail::nd(
            "CAPTURE_FAILED",
            "this macOS has no CGWindowListCreateImage (a ScreenCaptureKit backend is not implemented)",
        ));
    };
    // SAFETY: the function pointer was resolved from the system; arguments are plain values.
    let img = unsafe { f(rect, option, window, image_option) };
    if img.is_null() {
        return Err(Fail::nd(
            "CAPTURE_FAILED",
            "the window server returned no image (is the window on screen, and Screen Recording granted?)",
        ));
    }
    let out = rgba_of(img);
    // SAFETY: "create" rule.
    unsafe { CGImageRelease(img) };
    out.ok_or_else(|| Fail::nd("CAPTURE_FAILED", "the image could not be read back"))
}

/// Secure regions of one window tree, in global points. `Err(())` when the
/// tree could not be read completely.
fn secure_regions(win: &Ax, ctl: &Ctl, budget: Duration) -> Result<Vec<CGRect>, ()> {
    let Ok(w) = ax::walk(win, SECURE_SCAN_NODES, budget, ctl) else {
        return Err(());
    };
    if w.truncated {
        return Err(());
    }
    Ok(w
        .nodes
        .iter()
        .filter(|n| n.secure)
        .filter_map(|n| n.bounds.as_ref())
        .filter(|b| b.width > 0 && b.height > 0)
        .map(|b| CGRect {
            origin: CGPoint {
                x: f64::from(b.x),
                y: f64::from(b.y),
            },
            size: CGSize {
                width: f64::from(b.width),
                height: f64::from(b.height),
            },
        })
        .collect())
}

fn rel(r: &CGRect, origin: CGPoint) -> Rect {
    Rect {
        x: r.origin.x - origin.x,
        y: r.origin.y - origin.y,
        w: r.size.width,
        h: r.size.height,
    }
}

pub fn capture(mac: &Mac, a: &CaptureArgs<'_>, ctl: &Ctl) -> Res<RawFrame> {
    mac.verified(a.app)?;
    let app = Ax::app(a.app.pid)
        .ok_or_else(|| Fail::nd("WINDOW_UNVERIFIABLE", "no such process"))?;
    let win = ax::find_window(&app, a.window_id).map_err(|f| {
        if f.code == "ACCESSIBILITY_UNAVAILABLE" {
            Fail::nd(
                "ACCESSIBILITY_UNAVAILABLE",
                "the application exposes no accessibility tree, so its secure fields cannot be located; no frame is taken",
            )
        } else {
            f
        }
    })?;
    if win.info.minimized {
        return Err(Fail::nd("CAPTURE_FAILED", "the window is minimised"));
    }
    let screen = a.scope == wire::Scope::Screen;
    let Ok(number) = win.info.window_id.parse::<u32>() else {
        return Err(Fail::nd(
            "CAPTURE_FAILED",
            "the window has no window-server number to capture",
        ));
    };
    // The target's own secure fields: unreadable or cut short fails closed.
    let own = secure_regions(&win.el, ctl, SECURE_SCAN_BUDGET).map_err(|()| {
        Fail::nd(
            "CAPTURE_FAILED",
            "the window's secure fields could not all be located; no frame is taken",
        )
    })?;
    ctl.check().map_err(|h| Ctl::halt_fail(h, false))?;
    if !screen {
        let cg = cgwin::list(false)
            .into_iter()
            .find(|w| w.number == number)
            .ok_or_else(|| Fail::nd("CAPTURE_FAILED", "the window is gone"))?;
        // kCGWindowListOptionIncludingWindow = 8; BoundsIgnoreFraming | BestResolution = 1 | 8.
        let (rgba, px_w, px_h) = grab(CGRectNull, 8, number, 1 | 8)?;
        let origin = cg.bounds.origin;
        return Ok(RawFrame {
            rgba,
            px_w,
            px_h,
            pt_w: u32::try_from(super::round(cg.bounds.size.width)).unwrap_or(1).max(1),
            pt_h: u32::try_from(super::round(cg.bounds.size.height)).unwrap_or(1).max(1),
            secure: own.iter().map(|r| rel(r, origin)).collect(),
            window: win.info,
        });
    }
    // SCREEN: the whole display, every visible application's secure fields.
    let display = cgwin::main_display();
    let mut secure: Vec<Rect> = own.iter().map(|r| rel(r, display.origin)).collect();
    let started = Instant::now();
    let mut pids: Vec<u32> = Vec::new();
    let visible = cgwin::list(true);
    for w in visible.iter().filter(|w| w.layer == 0) {
        if w.pid != a.app.pid && w.pid != std::process::id() && !pids.contains(&w.pid) {
            pids.push(w.pid);
        }
    }
    for pid in pids {
        let readable = if started.elapsed() < SCREEN_SCAN_BUDGET {
            Ax::app(pid)
                .map(|ax_app| {
                    let ws = ax::windows(&ax_app);
                    if ws.is_empty() {
                        return None;
                    }
                    let mut all = Vec::new();
                    for w in ws {
                        all.extend(secure_regions(&w.el, ctl, Duration::from_millis(800)).ok()?);
                    }
                    Some(all)
                })
                .unwrap_or(None)
        } else {
            None
        };
        match readable {
            Some(rs) => secure.extend(rs.iter().map(|r| rel(r, display.origin))),
            // Unreadable foreign application: its windows are blacked out whole.
            None => secure.extend(
                visible
                    .iter()
                    .filter(|w| w.pid == pid && w.layer == 0)
                    .map(|w| rel(&w.bounds, display.origin)),
            ),
        }
        ctl.check().map_err(|h| Ctl::halt_fail(h, false))?;
    }
    // kCGWindowListOptionOnScreenOnly = 1; kCGNullWindowID = 0; BestResolution = 8.
    let (rgba, px_w, px_h) = grab(display, 1, 0, 8)?;
    Ok(RawFrame {
        rgba,
        px_w,
        px_h,
        pt_w: u32::try_from(super::round(display.size.width)).unwrap_or(1).max(1),
        pt_h: u32::try_from(super::round(display.size.height)).unwrap_or(1).max(1),
        secure,
        window: win.info,
    })
}
