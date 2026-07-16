//! Canvas 2D renderer. All geometry is sampled as polylines in screen space
//! with curvature-adaptive steps and Cohen–Sutherland culling, so rendering
//! stays exact and fast at any zoom level.

use crate::chart::ChartCore;
use crate::complex::Complex;
use crate::grid::{self, ArcWindow, Circle, Family, Region};
use crate::transforms::z_to_gamma;
use crate::view::View;
use web_sys::CanvasRenderingContext2d;

pub struct Theme {
    pub bg: &'static str,
    pub grid: &'static str,
    pub grid_adm: &'static str,
    pub axis: &'static str,
    pub label: &'static str,
    pub label_adm: &'static str,
    pub accent: &'static str,
    pub marker: &'static str,
    pub q: &'static str,
    pub vswr: &'static str,
    pub traces: [&'static str; 8],
}

pub const LIGHT: Theme = Theme {
    bg: "#fcfcfb",
    grid: "#898781",
    grid_adm: "#1baf7a",
    axis: "#52514e",
    label: "#6f6d67",
    label_adm: "#0e8a5e",
    accent: "#2a78d6",
    marker: "#0b0b0b",
    q: "#4a3aa7",
    vswr: "#898781",
    traces: [
        "#2a78d6", "#008300", "#e87ba4", "#eda100", "#1baf7a", "#eb6834", "#4a3aa7", "#e34948",
    ],
};

pub const DARK: Theme = Theme {
    bg: "#1a1a19",
    grid: "#898781",
    grid_adm: "#199e70",
    axis: "#c3c2b7",
    label: "#a09e96",
    label_adm: "#3dbd8d",
    accent: "#3987e5",
    marker: "#ffffff",
    q: "#9085e9",
    vswr: "#898781",
    traces: [
        "#3987e5", "#3fae3f", "#d55181", "#c98500", "#199e70", "#d95926", "#9085e9", "#e66767",
    ],
};

pub fn theme(dark: bool) -> &'static Theme {
    if dark {
        &DARK
    } else {
        &LIGHT
    }
}

const CULL_MARGIN: f64 = 8.0;

fn outcode(x: f64, y: f64, w: f64, h: f64) -> u8 {
    let mut c = 0u8;
    if x < -CULL_MARGIN {
        c |= 1;
    } else if x > w + CULL_MARGIN {
        c |= 2;
    }
    if y < -CULL_MARGIN {
        c |= 4;
    } else if y > h + CULL_MARGIN {
        c |= 8;
    }
    c
}

/// Stroke the given angular windows of a circle as an adaptive polyline.
/// `mirror` negates gamma-plane points (used for the admittance grid).
fn trace_arc_path(
    ctx: &CanvasRenderingContext2d,
    view: &View,
    circle: &Circle,
    windows: &[(f64, f64)],
    mirror: bool,
) -> bool {
    let mut any = false;
    let r_px = circle.r * view.scale;
    if r_px < 0.3 {
        return false;
    }
    // Angular step for <= 0.25 px sagitta error.
    let step = (2.0 * (2.0 * 0.25 / r_px).sqrt()).clamp(5e-4, 0.35);
    for &(mid, half) in windows {
        let n = ((2.0 * half / step).ceil() as usize).clamp(2, 4000);
        let mut pen = false;
        let mut prev: Option<(f64, f64, u8)> = None;
        for i in 0..=n {
            let th = mid - half + 2.0 * half * (i as f64) / (n as f64);
            let mut p = circle.point_at(th);
            if mirror {
                p = -p;
            }
            let (sx, sy) = view.to_screen(p);
            let code = outcode(sx, sy, view.w, view.h);
            if let Some((px, py, pc)) = prev {
                if pc & code != 0 {
                    pen = false;
                } else {
                    if !pen {
                        ctx.move_to(px, py);
                        pen = true;
                    }
                    ctx.line_to(sx, sy);
                    any = true;
                }
            }
            prev = Some((sx, sy, code));
        }
    }
    any
}

fn stroke_arc(
    ctx: &CanvasRenderingContext2d,
    view: &View,
    circle: &Circle,
    windows: &[(f64, f64)],
    mirror: bool,
) {
    ctx.begin_path();
    if trace_arc_path(ctx, view, circle, windows, mirror) {
        ctx.stroke();
    }
}

/// Windows of `circle` visible inside both the unit disk and the viewport.
fn visible_windows(circle: &Circle, view: &View, mirror: bool, clip_disk: bool) -> Vec<(f64, f64)> {
    let vc = if mirror {
        -view.center()
    } else {
        view.center()
    };
    let vr = view.half_diag() * 1.15 + 4.0 / view.scale;
    let w_view = grid::disk_window(circle, vc.re, vc.im, vr);
    if w_view == ArcWindow::None {
        return vec![];
    }
    if !clip_disk {
        return grid::window_intersect(ArcWindow::Full, w_view);
    }
    let w_disk = grid::disk_window(circle, 0.0, 0.0, 1.0);
    grid::window_intersect(w_disk, w_view)
}

struct LabelCandidate {
    text: String,
    x: f64,
    y: f64,
    depth: u32,
    adm: bool,
}

/// One grid family (three sub-families: R, +X, -X), impedance or admittance.
fn draw_grid(
    ctx: &CanvasRenderingContext2d,
    core: &ChartCore,
    th: &Theme,
    mirror: bool,
    labels: &mut Vec<LabelCandidate>,
    want_labels: bool,
) {
    let view = &core.view;
    let vr = view.half_diag() * 1.15;
    let base_center = if mirror {
        -view.center()
    } else {
        view.center()
    };

    // (family used for generation, conjugate trick for -x, sign of x)
    let subfams: [(Family, bool); 3] = [(Family::Res, false), (Family::XPos, false), (Family::XPos, true)];
    for (fam, neg) in subfams {
        let p_eff = if neg { base_center.conj() } else { base_center };
        let region = Region {
            center: p_eff,
            radius: vr,
            scale: view.scale,
        };
        let values = grid::generate(fam, &region, 26.0);
        for gv in &values {
            let is_axis = fam == Family::XPos && gv.value == 0.0;
            if is_axis {
                if !neg && !mirror {
                    draw_real_axis(ctx, view, th);
                }
                if want_labels {
                    // "0" reactance label at the short-circuit point on the rim.
                    let p = if mirror {
                        Complex::new(1.0, 0.0)
                    } else {
                        Complex::new(-1.0, 0.0)
                    };
                    push_rim_label(labels, view, p, "0".into(), gv.depth, mirror, neg);
                }
                continue;
            }
            if fam == Family::Res && gv.value == 0.0 {
                continue; // the rim itself, drawn separately
            }
            let signed = if neg { -gv.value } else { gv.value };
            let circle = match fam {
                Family::Res => Circle::resistance(gv.value),
                Family::XPos => Circle::reactance(signed),
            };
            let windows = visible_windows(&circle, view, mirror, true);
            if windows.is_empty() {
                continue;
            }
            let emphasized = (gv.value - 1.0).abs() < 1e-12;
            let (alpha, lw) = match gv.depth {
                _ if emphasized => (0.65, 1.4),
                0 => (0.45, 1.1),
                1 => (0.30, 1.0),
                _ => (0.18, 1.0),
            };
            ctx.set_stroke_style_str(if mirror { th.grid_adm } else { th.grid });
            ctx.set_global_alpha(alpha * if mirror { 0.8 } else { 1.0 });
            ctx.set_line_width(lw);
            stroke_arc(ctx, view, &circle, &windows, mirror);

            if want_labels && gv.depth <= 2 {
                let text = match fam {
                    Family::Res => fmt_value(gv.value),
                    Family::XPos => {
                        if neg {
                            format!("-j{}", fmt_value(gv.value))
                        } else {
                            format!("j{}", fmt_value(gv.value))
                        }
                    }
                };
                let anchor = match fam {
                    Family::Res => Complex::new((gv.value - 1.0) / (gv.value + 1.0), 0.0),
                    Family::XPos => z_to_gamma(Complex::new(0.0, signed)),
                };
                let anchor = if mirror { -anchor } else { anchor };
                if !push_rim_label(labels, view, anchor, text.clone(), gv.depth, mirror, fam == Family::Res)
                {
                    // Fall back to the closest visible point on the circle.
                    push_fallback_label(labels, view, &circle, base_center, text, gv.depth, mirror);
                }
            }
        }
    }
    ctx.set_global_alpha(1.0);
}

/// Try to place a label at its natural anchor (rim point for reactance, real
/// axis for resistance). Returns false if the anchor is offscreen.
fn push_rim_label(
    labels: &mut Vec<LabelCandidate>,
    view: &View,
    anchor: Complex,
    text: String,
    depth: u32,
    mirror: bool,
    on_axis: bool,
) -> bool {
    let (sx, sy) = view.to_screen(anchor);
    if sx < -10.0 || sx > view.w + 10.0 || sy < -10.0 || sy > view.h + 10.0 {
        return false;
    }
    let (ox, oy) = if on_axis && anchor.abs() < 0.985 {
        (2.0, 13.0)
    } else {
        // Radially outward from the chart center.
        let m = anchor.abs().max(1e-9);
        (anchor.re / m * 15.0, -anchor.im / m * 15.0)
    };
    labels.push(LabelCandidate {
        text,
        x: sx + ox,
        y: sy + oy,
        depth,
        adm: mirror,
    });
    true
}

fn push_fallback_label(
    labels: &mut Vec<LabelCandidate>,
    view: &View,
    circle: &Circle,
    base_center: Complex,
    text: String,
    depth: u32,
    mirror: bool,
) {
    let c = Complex::new(circle.cx, circle.cy);
    let d = base_center - c;
    let m = d.abs();
    if m < 1e-12 {
        return;
    }
    let n = d.scale(1.0 / m);
    let p = c + n.scale(circle.r);
    if p.abs() > 0.995 {
        return;
    }
    let drawn = if mirror { -p } else { p };
    let (sx, sy) = view.to_screen(drawn);
    if sx < 15.0 || sx > view.w - 15.0 || sy < 15.0 || sy > view.h - 15.0 {
        return;
    }
    let dir = if mirror { -n } else { n };
    labels.push(LabelCandidate {
        text,
        x: sx + dir.re * 11.0,
        y: sy - dir.im * 11.0,
        depth: depth + 1, // fallback anchors lose priority
        adm: mirror,
    });
}

fn draw_real_axis(ctx: &CanvasRenderingContext2d, view: &View, th: &Theme) {
    let half_w = view.w / (2.0 * view.scale);
    let lo = (-1.0f64).max(view.cx - half_w * 1.05);
    let hi = 1.0f64.min(view.cx + half_w * 1.05);
    if lo >= hi {
        return;
    }
    let (x0, y0) = view.to_screen(Complex::new(lo, 0.0));
    let (x1, y1) = view.to_screen(Complex::new(hi, 0.0));
    ctx.set_stroke_style_str(th.axis);
    ctx.set_global_alpha(0.45);
    ctx.set_line_width(1.0);
    ctx.begin_path();
    ctx.move_to(x0, y0);
    ctx.line_to(x1, y1);
    ctx.stroke();
    ctx.set_global_alpha(1.0);
}

fn fmt_value(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    if v >= 1e6 {
        return format!("{:.0e}", v);
    }
    let s = format!("{}", v);
    if s.len() > 8 {
        format!("{:.4}", v)
    } else {
        s
    }
}

fn place_labels(ctx: &CanvasRenderingContext2d, th: &Theme, labels: &mut [LabelCandidate]) {
    labels.sort_by_key(|l| l.depth);
    ctx.set_font("11px system-ui, sans-serif");
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    ctx.set_line_width(3.0);
    ctx.set_stroke_style_str(th.bg);
    let mut placed: Vec<(f64, f64, f64, f64)> = Vec::new();
    for lab in labels.iter() {
        let w = ctx
            .measure_text(&lab.text)
            .map(|m| m.width())
            .unwrap_or(24.0)
            + 6.0;
        let h = 14.0;
        let rect = (lab.x - w / 2.0, lab.y - h / 2.0, w, h);
        if placed
            .iter()
            .any(|r| rect.0 < r.0 + r.2 && r.0 < rect.0 + rect.2 && rect.1 < r.1 + r.3 && r.1 < rect.1 + rect.3)
        {
            continue;
        }
        placed.push(rect);
        ctx.set_fill_style_str(if lab.adm { th.label_adm } else { th.label });
        let _ = ctx.stroke_text(&lab.text, lab.x, lab.y);
        let _ = ctx.fill_text(&lab.text, lab.x, lab.y);
    }
}

fn set_dash(ctx: &CanvasRenderingContext2d, on: f64, off: f64) {
    let arr = js_sys::Array::new();
    if on > 0.0 {
        arr.push(&wasm_bindgen::JsValue::from_f64(on));
        arr.push(&wasm_bindgen::JsValue::from_f64(off));
    }
    let _ = ctx.set_line_dash(&arr);
}

fn draw_dot(ctx: &CanvasRenderingContext2d, x: f64, y: f64, r: f64) {
    ctx.begin_path();
    let _ = ctx.arc(x, y, r, 0.0, std::f64::consts::TAU);
    ctx.fill();
}

pub fn draw(core: &ChartCore, ctx: &CanvasRenderingContext2d) {
    let view = &core.view;
    let th = theme(core.opts.dark);
    let _ = ctx.set_transform(view.dpr, 0.0, 0.0, view.dpr, 0.0, 0.0);
    set_dash(ctx, 0.0, 0.0);
    ctx.set_line_cap("round");
    ctx.set_line_join("round");
    ctx.set_fill_style_str(th.bg);
    ctx.fill_rect(0.0, 0.0, view.w, view.h);

    let mut labels: Vec<LabelCandidate> = Vec::new();
    // Admittance labels only when they don't compete with impedance labels.
    let adm_labels = core.opts.show_labels && !core.opts.show_impedance;
    let imp_labels = core.opts.show_labels && core.opts.show_impedance;
    if core.opts.show_admittance {
        draw_grid(ctx, core, th, true, &mut labels, adm_labels);
    }
    if core.opts.show_impedance {
        draw_grid(ctx, core, th, false, &mut labels, imp_labels);
    }
    if core.opts.show_admittance && !core.opts.show_impedance {
        draw_real_axis(ctx, view, th);
    }

    // Constant-Q contours.
    if core.opts.q > 0.0 {
        ctx.set_stroke_style_str(th.q);
        ctx.set_line_width(1.2);
        ctx.set_global_alpha(0.7);
        set_dash(ctx, 6.0, 5.0);
        for upper in [true, false] {
            let c = Circle::q_contour(core.opts.q, upper);
            let windows = visible_windows(&c, view, false, true);
            stroke_arc(ctx, view, &c, &windows, false);
        }
        set_dash(ctx, 0.0, 0.0);
        ctx.set_global_alpha(1.0);
    }

    // The rim (|gamma| = 1).
    let rim = Circle { cx: 0.0, cy: 0.0, r: 1.0 };
    let rim_windows = visible_windows(&rim, view, false, false);
    ctx.set_stroke_style_str(th.axis);
    ctx.set_line_width(1.5);
    stroke_arc(ctx, view, &rim, &rim_windows, false);

    // VSWR circles through each marker.
    if core.opts.show_vswr && !core.markers.is_empty() {
        ctx.set_stroke_style_str(th.vswr);
        ctx.set_line_width(1.0);
        ctx.set_global_alpha(0.55);
        set_dash(ctx, 4.0, 4.0);
        for m in &core.markers {
            let r = m.gamma.abs();
            if r < 1e-6 {
                continue;
            }
            let c = Circle { cx: 0.0, cy: 0.0, r };
            let windows = visible_windows(&c, view, false, false);
            stroke_arc(ctx, view, &c, &windows, false);
        }
        set_dash(ctx, 0.0, 0.0);
        ctx.set_global_alpha(1.0);
    }

    // Traces.
    for trace in core.traces.iter().filter(|t| t.visible) {
        let color = th.traces[trace.color % th.traces.len()];
        ctx.set_stroke_style_str(color);
        ctx.set_fill_style_str(color);
        ctx.set_line_width(2.0);
        let n = trace.net.freqs_hz.len();
        ctx.begin_path();
        let mut pen = false;
        let mut prev: Option<(f64, f64, u8)> = None;
        let mut verts: Vec<(f64, f64, u8)> = Vec::with_capacity(n);
        for i in 0..n {
            let (sx, sy) = view.to_screen(core.gamma_plot(trace, i));
            let code = outcode(sx, sy, view.w, view.h);
            verts.push((sx, sy, code));
            if let Some((px, py, pc)) = prev {
                if pc & code != 0 {
                    pen = false;
                } else {
                    if !pen {
                        ctx.move_to(px, py);
                        pen = true;
                    }
                    ctx.line_to(sx, sy);
                }
            }
            prev = Some((sx, sy, code));
        }
        ctx.stroke();

        // Data-point dots once the sweep is spread out enough to see them.
        if n >= 2 {
            let (ax, ay, _) = verts[0];
            let (bx, by, _) = verts[n / 2];
            let spread = (ax - bx).hypot(ay - by);
            if spread > 30.0 * (n as f64).sqrt() {
                for &(sx, sy, code) in &verts {
                    if code == 0 {
                        draw_dot(ctx, sx, sy, 2.0);
                    }
                }
            }
            // Start (hollow) and end (filled) frequency markers.
            let (s0, s1, c0) = verts[0];
            if c0 == 0 {
                ctx.set_fill_style_str(th.bg);
                draw_dot(ctx, s0, s1, 4.0);
                ctx.set_fill_style_str(color);
                ctx.begin_path();
                let _ = ctx.arc(s0, s1, 4.0, 0.0, std::f64::consts::TAU);
                ctx.stroke();
            }
            let (e0, e1, c1) = verts[n - 1];
            if c1 == 0 {
                draw_dot(ctx, e0, e1, 4.0);
            }
        }
    }

    // Markers.
    ctx.set_font("600 11px system-ui, sans-serif");
    ctx.set_text_align("left");
    ctx.set_text_baseline("bottom");
    for (i, m) in core.markers.iter().enumerate() {
        let (sx, sy) = view.to_screen(m.gamma);
        if outcode(sx, sy, view.w, view.h) != 0 {
            continue;
        }
        ctx.set_fill_style_str(th.bg);
        draw_dot(ctx, sx, sy, 6.0);
        ctx.set_fill_style_str(th.marker);
        draw_dot(ctx, sx, sy, 4.0);
        let name = format!("M{}", i + 1);
        ctx.set_line_width(3.0);
        ctx.set_stroke_style_str(th.bg);
        let _ = ctx.stroke_text(&name, sx + 8.0, sy - 6.0);
        ctx.set_fill_style_str(th.marker);
        let _ = ctx.fill_text(&name, sx + 8.0, sy - 6.0);
    }

    // Hover: guide circles through the cursor plus the cursor point itself.
    if let Some(hover) = &core.hover {
        let z = crate::transforms::gamma_to_z(hover.gamma);
        ctx.set_stroke_style_str(th.accent);
        ctx.set_line_width(1.0);
        ctx.set_global_alpha(0.9);
        if z.is_finite() && hover.gamma.abs() <= 1.0 {
            if z.re >= 0.0 {
                let c = Circle::resistance(z.re);
                let windows = visible_windows(&c, view, false, true);
                stroke_arc(ctx, view, &c, &windows, false);
            }
            if z.im.abs() > 1e-9 {
                let c = Circle::reactance(z.im);
                let windows = visible_windows(&c, view, false, true);
                stroke_arc(ctx, view, &c, &windows, false);
            } else {
                draw_real_axis(ctx, view, th);
            }
        }
        ctx.set_global_alpha(1.0);
        let (sx, sy) = view.to_screen(hover.gamma);
        if hover.snap.is_some() {
            ctx.set_line_width(2.0);
            ctx.begin_path();
            let _ = ctx.arc(sx, sy, 7.0, 0.0, std::f64::consts::TAU);
            ctx.stroke();
        } else {
            ctx.set_fill_style_str(th.accent);
            draw_dot(ctx, sx, sy, 2.5);
        }
    }

    place_labels(ctx, th, &mut labels);
}
