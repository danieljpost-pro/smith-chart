//! Adaptive grid generation for the reflection-coefficient plane.
//!
//! Grid circles are never drawn with `canvas.arc` — at deep zoom the radii in
//! pixels overflow the renderer's precision. Instead each circle is reduced to
//! the angular window(s) that are simultaneously inside the unit disk and the
//! viewport, then sampled as a polyline with a curvature-adaptive step.
//!
//! Grid density is chosen by recursive refinement: an interval between two
//! grid values is subdivided (on a 1-2-5 ladder) only while the corresponding
//! circles are more than `min_gap_px` apart near the viewport and the region
//! between them is actually visible.

use crate::complex::Complex;
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Circle {
    pub cx: f64,
    pub cy: f64,
    pub r: f64,
}

impl Circle {
    /// Constant normalized-resistance circle (r >= 0). r = 0 is the unit circle.
    pub fn resistance(r: f64) -> Circle {
        Circle {
            cx: r / (1.0 + r),
            cy: 0.0,
            r: 1.0 / (1.0 + r),
        }
    }

    /// Constant normalized-reactance circle (x != 0).
    pub fn reactance(x: f64) -> Circle {
        Circle {
            cx: 1.0,
            cy: 1.0 / x,
            r: (1.0 / x).abs(),
        }
    }

    /// Constant-Q contour. The x > 0 half has its center below the axis.
    pub fn q_contour(q: f64, upper: bool) -> Circle {
        Circle {
            cx: 0.0,
            cy: if upper { -1.0 / q } else { 1.0 / q },
            r: (1.0 + 1.0 / (q * q)).sqrt(),
        }
    }

    pub fn point_at(&self, theta: f64) -> Complex {
        Complex::new(self.cx + self.r * theta.cos(), self.cy + self.r * theta.sin())
    }

    /// Distance from `p` to the circle; negative inside.
    pub fn signed_dist(&self, p: Complex) -> f64 {
        (p.re - self.cx).hypot(p.im - self.cy) - self.r
    }
}

/// Angular window of a circle, `mid +- half` radians. `Full` is the whole
/// circle, `None` no part of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ArcWindow {
    None,
    Full,
    Arc { mid: f64, half: f64 },
}

/// The portion of `circle` inside the disk centered at (dcx, dcy) with radius dr.
pub fn disk_window(circle: &Circle, dcx: f64, dcy: f64, dr: f64) -> ArcWindow {
    let dx = dcx - circle.cx;
    let dy = dcy - circle.cy;
    let d = dx.hypot(dy);
    if d + circle.r <= dr + 1e-12 {
        return ArcWindow::Full;
    }
    if d >= dr + circle.r || d + dr <= circle.r {
        return ArcWindow::None;
    }
    let cos_half =
        ((d * d + circle.r * circle.r - dr * dr) / (2.0 * d * circle.r)).clamp(-1.0, 1.0);
    ArcWindow::Arc {
        mid: dy.atan2(dx),
        half: cos_half.acos(),
    }
}

fn wrap_pi(a: f64) -> f64 {
    (a + PI).rem_euclid(2.0 * PI) - PI
}

/// Intersect two angular windows on the same circle. Up to two arcs result.
pub fn window_intersect(a: ArcWindow, b: ArcWindow) -> Vec<(f64, f64)> {
    use ArcWindow::*;
    match (a, b) {
        (None, _) | (_, None) => vec![],
        (Full, Full) => vec![(0.0, PI)],
        (Full, Arc { mid, half }) | (Arc { mid, half }, Full) => vec![(mid, half)],
        (Arc { mid: am, half: ah }, Arc { mid: bm, half: bh }) => {
            let delta = wrap_pi(bm - am);
            let mut out = Vec::new();
            for k in [-1.0f64, 0.0, 1.0] {
                let lo = (delta - bh + 2.0 * PI * k).max(-ah);
                let hi = (delta + bh + 2.0 * PI * k).min(ah);
                if hi > lo + 1e-12 {
                    out.push((am + 0.5 * (lo + hi), 0.5 * (hi - lo)));
                }
            }
            out
        }
    }
}

/// A visible region of the gamma plane, used to prune and pace refinement.
#[derive(Clone, Copy, Debug)]
pub struct Region {
    /// Center of the viewport's bounding circle (already transformed for
    /// mirrored/conjugated families).
    pub center: Complex,
    /// Radius of the viewport's bounding circle, gamma units.
    pub radius: f64,
    /// Pixels per gamma unit.
    pub scale: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Family {
    /// Constant-resistance circles; values >= 0.
    Res,
    /// Constant-reactance circles for x > 0; value 0 is the real axis.
    XPos,
}

fn family_signed_dist(family: Family, v: f64, p: Complex) -> f64 {
    match family {
        Family::Res => Circle::resistance(v).signed_dist(p),
        Family::XPos => {
            if v < 1e-9 {
                // Limit of the x -> 0+ circle: the real axis, upper half inside.
                -p.im
            } else {
                Circle::reactance(v).signed_dist(p)
            }
        }
    }
}

/// One-2-5 subdivision: the next finer nice step below an interval of width d.
fn next_step(d: f64) -> f64 {
    if !(d.is_finite() && d > 0.0) {
        return 0.0;
    }
    let p = 10f64.powi(d.log10().floor() as i32);
    let m = (d / p).round() as i64;
    match m {
        1 => p / 2.0,
        2 | 3 | 5 => p,
        10 => p * 5.0,
        _ => d / 2.0,
    }
}

fn snap(v: f64, step: f64) -> f64 {
    let s = (v / step).round() * step;
    // Clean float noise like 0.30000000000000004.
    let digits = (-(step.log10().floor()) as i32 + 2).clamp(0, 12);
    let k = 10f64.powi(digits);
    (s * k).round() / k
}

#[derive(Clone, Copy, Debug)]
pub struct GridValue {
    pub value: f64,
    pub depth: u32,
}

const MAX_DEPTH: u32 = 14;
const MAX_VALUES: usize = 1500;

struct Gen<'a> {
    family: Family,
    region: &'a Region,
    min_gap_px: f64,
    out: Vec<GridValue>,
}

impl<'a> Gen<'a> {
    fn dist(&self, v: f64) -> f64 {
        family_signed_dist(self.family, v, self.region.center)
    }

    /// True when nothing between the two bounding circles can be visible.
    fn irrelevant(&self, sa: f64, sb: f64) -> bool {
        sa.signum() == sb.signum()
            && sa.abs().min(sb.abs()) > self.region.radius * 1.3 + 1e-9
    }

    fn refine(&mut self, a: f64, b: f64, depth: u32) {
        if depth > MAX_DEPTH || self.out.len() > MAX_VALUES {
            return;
        }
        let sa = self.dist(a);
        let sb = self.dist(b);
        if self.irrelevant(sa, sb) {
            return;
        }
        if (sa - sb).abs() * self.region.scale < self.min_gap_px {
            return;
        }
        let step = next_step(b - a);
        if step <= 0.0 || step >= (b - a) * 0.75 {
            return;
        }
        let n = ((b - a) / step).round() as i64;
        if n <= 1 || n > 64 {
            return;
        }
        let mut prev = a;
        for i in 1..n {
            let v = snap(a + step * i as f64, step);
            if v <= prev || v >= b {
                continue;
            }
            self.out.push(GridValue { value: v, depth });
            self.refine(prev, v, depth + 1);
            prev = v;
        }
        self.refine(prev, b, depth + 1);
    }
}

/// Generate grid values for one family. Values start from the classic anchor
/// ladder {0, 0.2, 0.5, 1, 2, 5, 10, ...} and refine where the view demands.
pub fn generate(family: Family, region: &Region, min_gap_px: f64) -> Vec<GridValue> {
    let mut anchors: Vec<f64> = vec![0.0, 0.2, 0.5, 1.0];
    // Extend the 1-2-5 ladder upward until everything beyond is invisible.
    let mantissas = [2.0, 5.0, 10.0];
    let mut decade = 1.0;
    'outer: for _ in 0..10 {
        for m in mantissas {
            let v = m * decade;
            anchors.push(v);
            // Circles for values beyond v are nested inside circle(v); if the
            // viewport is entirely outside circle(v) they are all invisible.
            let sd = family_signed_dist(family, v, region.center);
            if sd > region.radius * 1.3 {
                break 'outer;
            }
        }
        decade *= 10.0;
    }

    let mut g = Gen {
        family,
        region,
        min_gap_px,
        out: Vec::new(),
    };
    for &a in &anchors {
        g.out.push(GridValue { value: a, depth: 0 });
    }
    for w in anchors.windows(2) {
        g.refine(w[0], w[1], 1);
    }
    g.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_region() -> Region {
        Region {
            center: Complex::new(0.0, 0.0),
            radius: 1.5,
            scale: 300.0,
        }
    }

    #[test]
    fn disk_window_cases() {
        // r = 1 circle is internally tangent to the unit disk -> Full.
        let c = Circle::resistance(1.0);
        assert_eq!(disk_window(&c, 0.0, 0.0, 1.0), ArcWindow::Full);
        // A circle far away -> None.
        let far = Circle { cx: 10.0, cy: 0.0, r: 1.0 };
        assert_eq!(disk_window(&far, 0.0, 0.0, 1.0), ArcWindow::None);
        // x = 1 arc: half-angle pi/4 inside the unit disk.
        let x1 = Circle::reactance(1.0);
        match disk_window(&x1, 0.0, 0.0, 1.0) {
            ArcWindow::Arc { half, .. } => assert!((half - PI / 4.0).abs() < 1e-9),
            other => panic!("expected arc, got {:?}", other),
        }
    }

    #[test]
    fn window_intersection() {
        let a = ArcWindow::Arc { mid: 0.0, half: 1.0 };
        let b = ArcWindow::Arc { mid: 0.5, half: 1.0 };
        let out = window_intersect(a, b);
        assert_eq!(out.len(), 1);
        let (mid, half) = out[0];
        assert!((mid - 0.25).abs() < 1e-9);
        assert!((half - 0.75).abs() < 1e-9);
        // Disjoint windows.
        let c = ArcWindow::Arc { mid: PI, half: 0.2 };
        assert!(window_intersect(a, c).is_empty());
    }

    #[test]
    fn step_ladder() {
        assert!((next_step(1.0) - 0.5).abs() < 1e-12);
        assert!((next_step(0.5) - 0.1).abs() < 1e-12);
        assert!((next_step(0.2) - 0.1).abs() < 1e-12);
        assert!((next_step(0.3) - 0.1).abs() < 1e-12);
        assert!((next_step(0.1) - 0.05).abs() < 1e-12);
        assert!((next_step(3.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn generation_default_view() {
        let vals = generate(Family::Res, &default_region(), 26.0);
        let has = |x: f64| vals.iter().any(|g| (g.value - x).abs() < 1e-9);
        for v in [0.0, 0.2, 0.5, 1.0, 2.0, 5.0] {
            assert!(has(v), "missing {}", v);
        }
        assert!(vals.len() < 400, "too many values: {}", vals.len());
    }

    #[test]
    fn generation_zoomed() {
        // Deep zoom near z = 1: grid must refine to fine steps around r = 1.
        let region = Region {
            center: Complex::new(0.001, 0.0),
            radius: 0.005,
            scale: 200_000.0,
        };
        let vals = generate(Family::Res, &region, 26.0);
        let fine = vals
            .iter()
            .filter(|g| (g.value - 1.0).abs() < 0.05 && g.value != 1.0)
            .count();
        assert!(fine > 3, "expected fine refinement near r=1, got {}", fine);
        assert!(vals.len() < MAX_VALUES);
    }

    #[test]
    fn generation_terminates_everywhere() {
        for &(cx, cy, scale) in &[
            (0.999999, 0.0, 1e8),
            (-1.0, 0.0, 1e6),
            (0.0, 1.0, 1e4),
            (0.5, -0.5, 1e7),
        ] {
            let region = Region {
                center: Complex::new(cx, cy),
                radius: 700.0 / scale,
                scale,
            };
            for fam in [Family::Res, Family::XPos] {
                let vals = generate(fam, &region, 26.0);
                assert!(vals.len() <= MAX_VALUES + 64);
            }
        }
    }
}
