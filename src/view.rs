//! Viewport: maps between gamma-plane coordinates and CSS-pixel canvas
//! coordinates, with pan/zoom state.

use crate::complex::Complex;

const RIM_MARGIN_PX: f64 = 44.0;
const MAX_SCALE_PX: f64 = 1e8;

#[derive(Clone, Copy, Debug)]
pub struct View {
    /// Gamma-plane point at the canvas center.
    pub cx: f64,
    pub cy: f64,
    /// Pixels per gamma unit.
    pub scale: f64,
    /// Canvas size in CSS pixels.
    pub w: f64,
    pub h: f64,
    pub dpr: f64,
}

impl Default for View {
    fn default() -> Self {
        Self::new()
    }
}

impl View {
    pub fn new() -> View {
        let mut v = View {
            cx: 0.0,
            cy: 0.0,
            scale: 300.0,
            w: 800.0,
            h: 600.0,
            dpr: 1.0,
        };
        v.reset();
        v
    }

    /// Scale that fits the unit circle with room for rim labels.
    pub fn fit_scale(&self) -> f64 {
        (self.w.min(self.h) / 2.0 - RIM_MARGIN_PX).max(40.0)
    }

    pub fn reset(&mut self) {
        self.cx = 0.0;
        self.cy = 0.0;
        self.scale = self.fit_scale();
    }

    pub fn resize(&mut self, w: f64, h: f64, dpr: f64) {
        let rel = self.scale / self.fit_scale();
        self.w = w.max(50.0);
        self.h = h.max(50.0);
        self.dpr = dpr.clamp(0.5, 4.0);
        self.scale = rel * self.fit_scale();
    }

    pub fn to_screen(&self, g: Complex) -> (f64, f64) {
        (
            (g.re - self.cx) * self.scale + self.w / 2.0,
            -(g.im - self.cy) * self.scale + self.h / 2.0,
        )
    }

    pub fn to_gamma(&self, x: f64, y: f64) -> Complex {
        Complex::new(
            (x - self.w / 2.0) / self.scale + self.cx,
            -(y - self.h / 2.0) / self.scale + self.cy,
        )
    }

    /// Zoom by `factor`, keeping the gamma point under (x, y) fixed.
    pub fn zoom_at(&mut self, x: f64, y: f64, factor: f64) {
        let anchor = self.to_gamma(x, y);
        let fit = self.fit_scale();
        self.scale = (self.scale * factor).clamp(0.4 * fit, MAX_SCALE_PX);
        let after = self.to_gamma(x, y);
        self.cx += anchor.re - after.re;
        self.cy += anchor.im - after.im;
        self.clamp_center();
    }

    /// Pan by screen pixels.
    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.cx -= dx / self.scale;
        self.cy += dy / self.scale;
        self.clamp_center();
    }

    /// Keep the chart from being panned entirely out of sight.
    fn clamp_center(&mut self) {
        let slack = 1.0 + self.w.max(self.h) / (2.0 * self.scale);
        self.cx = self.cx.clamp(-slack, slack);
        self.cy = self.cy.clamp(-slack, slack);
    }

    pub fn zoom_rel(&self) -> f64 {
        self.scale / self.fit_scale()
    }

    /// Half-diagonal of the viewport in gamma units.
    pub fn half_diag(&self) -> f64 {
        (self.w / 2.0).hypot(self.h / 2.0) / self.scale
    }

    pub fn center(&self) -> Complex {
        Complex::new(self.cx, self.cy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_round_trip() {
        let v = View::new();
        let g = Complex::new(0.3, -0.4);
        let (x, y) = v.to_screen(g);
        let back = v.to_gamma(x, y);
        assert!((back - g).abs() < 1e-12);
    }

    #[test]
    fn zoom_keeps_anchor() {
        let mut v = View::new();
        let (ax, ay) = (123.0, 456.0);
        let before = v.to_gamma(ax, ay);
        v.zoom_at(ax, ay, 3.0);
        let after = v.to_gamma(ax, ay);
        assert!((after - before).abs() < 1e-12);
        assert!(v.zoom_rel() > 2.9);
    }

    #[test]
    fn y_axis_points_up() {
        let v = View::new();
        let (_, y_top) = v.to_screen(Complex::new(0.0, 0.5));
        let (_, y_bot) = v.to_screen(Complex::new(0.0, -0.5));
        assert!(y_top < y_bot);
    }
}
