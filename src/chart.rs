//! Chart model: options, markers, traces, hover state, and conversion to and
//! from the shareable [`State`]. No rendering here — see `render.rs`.

use crate::complex::Complex;
use crate::state::{MarkerState, State, TraceState, STATE_VERSION};
use crate::touchstone::Network;
use crate::transforms;
use crate::view::View;

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub z0: f64,
    pub show_impedance: bool,
    pub show_admittance: bool,
    pub show_labels: bool,
    pub show_vswr: bool,
    /// Constant-Q contour value; 0 disables.
    pub q: f64,
    pub dark: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            z0: 50.0,
            show_impedance: true,
            show_admittance: false,
            show_labels: true,
            show_vswr: false,
            q: 0.0,
            dark: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Marker {
    pub gamma: Complex,
    pub freq_hz: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct Trace {
    pub net: Network,
    /// Index into each frequency's S-parameter row (s2p: 0=S11 .. 3=S22).
    pub param: usize,
    pub visible: bool,
    pub color: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct Hover {
    pub gamma: Complex,
    /// (trace index, point index) when snapped to a trace point.
    pub snap: Option<(usize, usize)>,
}

pub struct ChartCore {
    pub view: View,
    pub opts: Options,
    pub markers: Vec<Marker>,
    pub traces: Vec<Trace>,
    pub hover: Option<Hover>,
    next_color: usize,
}

const SNAP_RADIUS_PX: f64 = 14.0;

impl Default for ChartCore {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartCore {
    pub fn new() -> ChartCore {
        ChartCore {
            view: View::new(),
            opts: Options::default(),
            markers: Vec::new(),
            traces: Vec::new(),
            hover: None,
            next_color: 0,
        }
    }

    /// Reflection coefficient of a trace point, renormalized from the file's
    /// reference impedance to the chart's Z0.
    pub fn gamma_plot(&self, trace: &Trace, i: usize) -> Complex {
        transforms::renormalize_gamma(trace.net.sparams[i][trace.param], trace.net.z0, self.opts.z0)
    }

    /// Nearest trace point within snapping distance of a screen position.
    pub fn snap_search(&self, x: f64, y: f64) -> Option<(usize, usize)> {
        let mut best = SNAP_RADIUS_PX * SNAP_RADIUS_PX;
        let mut hit = None;
        for (ti, trace) in self.traces.iter().enumerate() {
            if !trace.visible {
                continue;
            }
            for i in 0..trace.net.freqs_hz.len() {
                let (sx, sy) = self.view.to_screen(self.gamma_plot(trace, i));
                let d2 = (sx - x) * (sx - x) + (sy - y) * (sy - y);
                if d2 < best {
                    best = d2;
                    hit = Some((ti, i));
                }
            }
        }
        hit
    }

    /// Resolve a screen position to a chart point, snapping to trace data.
    pub fn resolve(&self, x: f64, y: f64) -> Hover {
        match self.snap_search(x, y) {
            Some((ti, i)) => Hover {
                gamma: self.gamma_plot(&self.traces[ti], i),
                snap: Some((ti, i)),
            },
            None => Hover {
                gamma: self.view.to_gamma(x, y),
                snap: None,
            },
        }
    }

    pub fn add_trace(&mut self, net: Network) -> usize {
        let color = self.next_color;
        self.next_color += 1;
        self.traces.push(Trace {
            net,
            param: 0,
            visible: true,
            color,
        });
        self.traces.len() - 1
    }

    pub fn to_state(&self) -> State {
        State {
            v: STATE_VERSION,
            z0: self.opts.z0,
            dark: self.opts.dark,
            show_impedance: self.opts.show_impedance,
            show_admittance: self.opts.show_admittance,
            show_labels: self.opts.show_labels,
            show_vswr: self.opts.show_vswr,
            q: self.opts.q,
            view_cx: self.view.cx,
            view_cy: self.view.cy,
            view_zoom: self.view.zoom_rel(),
            markers: self
                .markers
                .iter()
                .map(|m| MarkerState {
                    re: m.gamma.re,
                    im: m.gamma.im,
                    freq_hz: m.freq_hz,
                })
                .collect(),
            traces: self
                .traces
                .iter()
                .map(|t| {
                    let mut ts = TraceState::from_network(&t.net, t.param, t.visible);
                    ts.color = t.color;
                    ts
                })
                .collect(),
        }
    }

    pub fn apply_state(&mut self, s: State) -> Result<(), String> {
        if s.v > STATE_VERSION {
            return Err(format!("shared state version {} is too new", s.v));
        }
        if !(s.z0.is_finite() && s.z0 > 0.0) {
            return Err("invalid Z0 in shared state".into());
        }
        let mut traces = Vec::new();
        let mut max_color = 0;
        for ts in &s.traces {
            let net = ts.to_network()?;
            let param = ts.param.min(net.nports * net.nports - 1);
            max_color = max_color.max(ts.color + 1);
            traces.push(Trace {
                net,
                param,
                visible: ts.visible,
                color: ts.color,
            });
        }
        self.opts.z0 = s.z0;
        self.opts.dark = s.dark;
        self.opts.show_impedance = s.show_impedance;
        self.opts.show_admittance = s.show_admittance;
        self.opts.show_labels = s.show_labels;
        self.opts.show_vswr = s.show_vswr;
        self.opts.q = if s.q.is_finite() && s.q > 0.0 { s.q } else { 0.0 };
        self.view.cx = s.view_cx.clamp(-2.0, 2.0);
        self.view.cy = s.view_cy.clamp(-2.0, 2.0);
        let zoom = if s.view_zoom.is_finite() { s.view_zoom } else { 1.0 };
        self.view.scale = (zoom.clamp(0.4, 1e7)) * self.view.fit_scale();
        self.markers = s
            .markers
            .iter()
            .filter(|m| m.re.is_finite() && m.im.is_finite())
            .map(|m| Marker {
                gamma: Complex::new(m.re, m.im),
                freq_hz: m.freq_hz,
            })
            .collect();
        self.traces = traces;
        self.next_color = max_color.max(self.traces.len());
        self.hover = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_network() -> Network {
        Network {
            name: "demo".into(),
            z0: 50.0,
            nports: 1,
            freqs_hz: vec![1e9, 2e9],
            sparams: vec![
                vec![Complex::new(0.5, 0.0)],
                vec![Complex::new(0.0, 0.5)],
            ],
        }
    }

    #[test]
    fn state_round_trip() {
        let mut core = ChartCore::new();
        core.opts.z0 = 75.0;
        core.opts.q = 2.0;
        core.markers.push(Marker {
            gamma: Complex::new(0.25, -0.4),
            freq_hz: Some(1e9),
        });
        core.add_trace(demo_network());
        core.view.zoom_at(400.0, 300.0, 5.0);

        let json = serde_json::to_string(&core.to_state()).unwrap();
        let mut restored = ChartCore::new();
        restored
            .apply_state(serde_json::from_str(&json).unwrap())
            .unwrap();
        assert_eq!(restored.opts.z0, 75.0);
        assert_eq!(restored.markers.len(), 1);
        assert_eq!(restored.traces.len(), 1);
        assert!((restored.view.zoom_rel() - core.view.zoom_rel()).abs() < 1e-9);
    }

    #[test]
    fn snap_finds_trace_point() {
        let mut core = ChartCore::new();
        core.add_trace(demo_network());
        let (sx, sy) = core.view.to_screen(Complex::new(0.5, 0.0));
        let hover = core.resolve(sx + 3.0, sy - 3.0);
        assert_eq!(hover.snap, Some((0, 0)));
        // Far away: no snap.
        assert!(core.resolve(sx + 200.0, sy).snap.is_none());
    }
}
