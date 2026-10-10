//! The model-facing coordinate canvas (docs/66 CUC-B02): fixed at 1280 x 800
//! with the origin at the top left. One scaler fits the real window or
//! display into it (letterboxed, never stretched); a frame whose size is not
//! the canvas, or whose fit disagrees with the one scaler beyond a tolerance
//! of 0.02, is refused. 1280 x 800 is Modbit's own choice.

use serde::{Deserialize, Serialize};

/// Canvas width in pixels.
pub const CANVAS_WIDTH: u32 = 1280;
/// Canvas height in pixels.
pub const CANVAS_HEIGHT: u32 = 800;
/// How far a frame's fit may differ from the scaler's, relative.
pub const TOLERANCE: f64 = 0.02;

/// How a source surface was fitted to the canvas: a canvas point `p` maps to
/// the source point `(p - offset) / scale`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Letterbox {
    /// Source width in source units (display points).
    pub source_width: u32,
    /// Source height.
    pub source_height: u32,
    /// Canvas pixels per source unit.
    pub scale: f64,
    /// Left margin in canvas pixels.
    pub offset_x: f64,
    /// Top margin in canvas pixels.
    pub offset_y: f64,
}

impl Letterbox {
    /// The one scaler: fit `source_width` x `source_height` inside the
    /// canvas, centered, preserving the aspect ratio.
    #[must_use]
    pub fn fit(source_width: u32, source_height: u32) -> Self {
        let (sw, sh) = (
            f64::from(source_width.max(1)),
            f64::from(source_height.max(1)),
        );
        let (cw, ch) = (f64::from(CANVAS_WIDTH), f64::from(CANVAS_HEIGHT));
        let scale = (cw / sw).min(ch / sh);
        Self {
            source_width,
            source_height,
            scale,
            offset_x: (cw - sw * scale) / 2.0,
            offset_y: (ch - sh * scale) / 2.0,
        }
    }

    /// The source point a canvas point denotes, or `None` for a point in the
    /// margin or outside the canvas.
    #[must_use]
    pub fn to_source(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        if !(0.0..=f64::from(CANVAS_WIDTH)).contains(&x)
            || !(0.0..=f64::from(CANVAS_HEIGHT)).contains(&y)
            || self.scale <= 0.0
        {
            return None;
        }
        let sx = (x - self.offset_x) / self.scale;
        let sy = (y - self.offset_y) / self.scale;
        let inside = sx >= -0.5
            && sy >= -0.5
            && sx <= f64::from(self.source_width) + 0.5
            && sy <= f64::from(self.source_height) + 0.5;
        inside.then_some((sx, sy))
    }

    /// Whether this fit is the one scaler's, within the tolerance.
    ///
    /// # Errors
    /// What differs.
    pub fn validate(&self) -> Result<(), String> {
        if self.source_width == 0 || self.source_height == 0 {
            return Err("the frame's source has no size".into());
        }
        let want = Self::fit(self.source_width, self.source_height);
        let rel = |a: f64, b: f64| {
            if b == 0.0 {
                a.abs()
            } else {
                ((a - b) / b).abs()
            }
        };
        if !self.scale.is_finite() || rel(self.scale, want.scale) > TOLERANCE {
            return Err(format!(
                "the frame's scale {:.4} is not the canvas scaler's {:.4} (tolerance {TOLERANCE})",
                self.scale, want.scale
            ));
        }
        let tol_x = TOLERANCE * f64::from(CANVAS_WIDTH);
        let tol_y = TOLERANCE * f64::from(CANVAS_HEIGHT);
        if (self.offset_x - want.offset_x).abs() > tol_x
            || (self.offset_y - want.offset_y).abs() > tol_y
        {
            return Err(format!(
                "the frame's margins ({:.1}, {:.1}) are not the canvas scaler's ({:.1}, {:.1})",
                self.offset_x, self.offset_y, want.offset_x, want.offset_y
            ));
        }
        Ok(())
    }
}

/// Whether a frame is exactly the canvas.
#[must_use]
pub const fn is_canvas(width: u32, height: u32) -> bool {
    width == CANVAS_WIDTH && height == CANVAS_HEIGHT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scaler_letterboxes_and_never_stretches() {
        let wide = Letterbox::fit(2560, 1600);
        assert!((wide.scale - 0.5).abs() < 1e-9);
        assert!(wide.offset_x.abs() < 1e-9 && wide.offset_y.abs() < 1e-9);
        let tall = Letterbox::fit(800, 800);
        assert!((tall.scale - 1.0).abs() < 1e-9);
        assert!((tall.offset_x - 240.0).abs() < 1e-9);
        assert_eq!(tall.to_source(240.0, 0.0), Some((0.0, 0.0)));
        assert_eq!(
            tall.to_source(100.0, 100.0),
            None,
            "a margin point has no source"
        );
        assert_eq!(tall.to_source(2000.0, 10.0), None);
    }

    #[test]
    fn a_fit_that_disagrees_with_the_scaler_beyond_the_tolerance_is_refused() {
        let ok = Letterbox::fit(1920, 1080);
        assert!(ok.validate().is_ok());
        let slightly = Letterbox {
            scale: ok.scale * 1.01,
            ..ok
        };
        assert!(slightly.validate().is_ok(), "within 0.02");
        let stretched = Letterbox {
            scale: ok.scale * 1.2,
            ..ok
        };
        assert!(stretched.validate().unwrap_err().contains("scale"));
        let shifted = Letterbox {
            offset_y: ok.offset_y + 100.0,
            ..ok
        };
        assert!(shifted.validate().unwrap_err().contains("margins"));
        assert!(is_canvas(1280, 800) && !is_canvas(1280, 801));
    }
}
