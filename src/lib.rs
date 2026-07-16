//! Interactive Smith chart for the browser. The JS shell owns DOM events and
//! UI chrome; everything else — math, grid generation, Touchstone parsing,
//! canvas rendering, shareable state — lives here.

pub mod chart;
pub mod complex;
pub mod grid;
pub mod render;
pub mod state;
pub mod touchstone;
pub mod transforms;
pub mod view;

use chart::{ChartCore, Marker};
use serde::Serialize;
use transforms::Readout;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

#[derive(Serialize)]
struct SnapOut {
    trace: usize,
    name: String,
    index: usize,
    freq_hz: f64,
    param: String,
}

#[derive(Serialize)]
struct HoverOut {
    #[serde(flatten)]
    readout: Readout,
    snap: Option<SnapOut>,
}

#[derive(Serialize)]
struct MarkerOut {
    index: usize,
    freq_hz: Option<f64>,
    #[serde(flatten)]
    readout: Readout,
}

#[derive(Serialize)]
struct TraceOut {
    index: usize,
    name: String,
    color: usize,
    visible: bool,
    param: usize,
    params: Vec<String>,
    points: usize,
    f_min_hz: f64,
    f_max_hz: f64,
    z0: f64,
}

#[wasm_bindgen]
pub struct SmithChart {
    core: ChartCore,
    ctx: CanvasRenderingContext2d,
    canvas: HtmlCanvasElement,
}

fn err(msg: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&msg.to_string())
}

#[wasm_bindgen]
impl SmithChart {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: &str) -> Result<SmithChart, JsValue> {
        let document = web_sys::window()
            .and_then(|w| w.document())
            .ok_or_else(|| err("no document"))?;
        let canvas: HtmlCanvasElement = document
            .get_element_by_id(canvas_id)
            .ok_or_else(|| err(format!("no element #{canvas_id}")))?
            .dyn_into()
            .map_err(|_| err("element is not a canvas"))?;
        let ctx: CanvasRenderingContext2d = canvas
            .get_context("2d")?
            .ok_or_else(|| err("no 2d context"))?
            .dyn_into()
            .map_err(|_| err("bad 2d context"))?;
        Ok(SmithChart {
            core: ChartCore::new(),
            ctx,
            canvas,
        })
    }

    /// Resize the backing store (CSS pixels + devicePixelRatio) and refit.
    pub fn resize(&mut self, w: f64, h: f64, dpr: f64) {
        self.core.view.resize(w, h, dpr);
        self.canvas.set_width((w * self.core.view.dpr) as u32);
        self.canvas.set_height((h * self.core.view.dpr) as u32);
    }

    pub fn render(&self) {
        render::draw(&self.core, &self.ctx);
    }

    // ----- view -----

    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.core.view.pan(dx, dy);
    }

    pub fn zoom_at(&mut self, x: f64, y: f64, factor: f64) {
        self.core.view.zoom_at(x, y, factor);
    }

    pub fn reset_view(&mut self) {
        self.core.view.reset();
    }

    pub fn zoom_level(&self) -> f64 {
        self.core.view.zoom_rel()
    }

    // ----- hover / readout -----

    /// Update hover from a screen position; returns the readout as JSON.
    pub fn set_hover(&mut self, x: f64, y: f64) -> String {
        let hover = self.core.resolve(x, y);
        self.core.hover = Some(hover);
        let snap = hover.snap.map(|(ti, i)| {
            let t = &self.core.traces[ti];
            SnapOut {
                trace: ti,
                name: t.net.name.clone(),
                index: i,
                freq_hz: t.net.freqs_hz[i],
                param: t.net.param_labels()[t.param].clone(),
            }
        });
        let out = HoverOut {
            readout: transforms::readout(hover.gamma, self.core.opts.z0),
            snap,
        };
        serde_json::to_string(&out).unwrap_or_default()
    }

    pub fn clear_hover(&mut self) {
        self.core.hover = None;
    }

    // ----- markers -----

    /// Add a marker at a screen position (snaps to nearby trace points).
    pub fn add_marker(&mut self, x: f64, y: f64) -> String {
        let hover = self.core.resolve(x, y);
        let freq = hover
            .snap
            .map(|(ti, i)| self.core.traces[ti].net.freqs_hz[i]);
        self.core.markers.push(Marker {
            gamma: hover.gamma,
            freq_hz: freq,
        });
        self.markers_json()
    }

    /// Add a marker at a given impedance (ohms).
    pub fn add_marker_impedance(&mut self, re: f64, im: f64) -> String {
        let z = complex::Complex::new(re / self.core.opts.z0, im / self.core.opts.z0);
        self.core.markers.push(Marker {
            gamma: transforms::z_to_gamma(z),
            freq_hz: None,
        });
        self.markers_json()
    }

    pub fn remove_marker(&mut self, i: usize) -> String {
        if i < self.core.markers.len() {
            self.core.markers.remove(i);
        }
        self.markers_json()
    }

    pub fn clear_markers(&mut self) {
        self.core.markers.clear();
    }

    pub fn markers_json(&self) -> String {
        let out: Vec<MarkerOut> = self
            .core
            .markers
            .iter()
            .enumerate()
            .map(|(i, m)| MarkerOut {
                index: i,
                freq_hz: m.freq_hz,
                readout: transforms::readout(m.gamma, self.core.opts.z0),
            })
            .collect();
        serde_json::to_string(&out).unwrap_or_default()
    }

    // ----- options -----

    pub fn set_z0(&mut self, z0: f64) {
        if z0.is_finite() && z0 > 0.0 {
            self.core.opts.z0 = z0;
        }
    }

    pub fn z0(&self) -> f64 {
        self.core.opts.z0
    }

    pub fn set_show_impedance(&mut self, on: bool) {
        self.core.opts.show_impedance = on;
    }

    pub fn set_show_admittance(&mut self, on: bool) {
        self.core.opts.show_admittance = on;
    }

    pub fn set_show_labels(&mut self, on: bool) {
        self.core.opts.show_labels = on;
    }

    pub fn set_show_vswr(&mut self, on: bool) {
        self.core.opts.show_vswr = on;
    }

    pub fn set_q(&mut self, q: f64) {
        self.core.opts.q = if q.is_finite() && q > 0.0 { q } else { 0.0 };
    }

    pub fn set_dark(&mut self, dark: bool) {
        self.core.opts.dark = dark;
    }

    /// All display options as JSON (for syncing UI controls after load).
    pub fn options_json(&self) -> String {
        let o = &self.core.opts;
        format!(
            r#"{{"z0":{},"show_impedance":{},"show_admittance":{},"show_labels":{},"show_vswr":{},"q":{},"dark":{}}}"#,
            o.z0, o.show_impedance, o.show_admittance, o.show_labels, o.show_vswr, o.q, o.dark
        )
    }

    // ----- traces -----

    /// Parse a Touchstone file and add it as a trace. Returns traces JSON.
    pub fn add_touchstone(&mut self, name: &str, text: &str) -> Result<String, JsValue> {
        let hint = touchstone::ports_from_name(name);
        let net = touchstone::parse(name, text, hint).map_err(err)?;
        self.core.add_trace(net);
        Ok(self.traces_json())
    }

    pub fn remove_trace(&mut self, i: usize) -> String {
        if i < self.core.traces.len() {
            self.core.traces.remove(i);
        }
        self.traces_json()
    }

    pub fn set_trace_param(&mut self, i: usize, param: usize) {
        if let Some(t) = self.core.traces.get_mut(i) {
            if param < t.net.nports * t.net.nports {
                t.param = param;
            }
        }
    }

    pub fn set_trace_visible(&mut self, i: usize, visible: bool) {
        if let Some(t) = self.core.traces.get_mut(i) {
            t.visible = visible;
        }
    }

    pub fn traces_json(&self) -> String {
        let out: Vec<TraceOut> = self
            .core
            .traces
            .iter()
            .enumerate()
            .map(|(i, t)| TraceOut {
                index: i,
                name: t.net.name.clone(),
                color: t.color % render::LIGHT.traces.len(),
                visible: t.visible,
                param: t.param,
                params: t.net.param_labels(),
                points: t.net.freqs_hz.len(),
                f_min_hz: t.net.freqs_hz.first().copied().unwrap_or(0.0),
                f_max_hz: t.net.freqs_hz.last().copied().unwrap_or(0.0),
                z0: t.net.z0,
            })
            .collect();
        serde_json::to_string(&out).unwrap_or_default()
    }

    /// Hex color for a trace index, so the UI list matches the canvas.
    pub fn trace_color(&self, i: usize) -> String {
        let th = render::theme(self.core.opts.dark);
        self.core
            .traces
            .get(i)
            .map(|t| th.traces[t.color % th.traces.len()].to_string())
            .unwrap_or_default()
    }

    // ----- shareable state -----

    pub fn state_json(&self) -> String {
        serde_json::to_string(&self.core.to_state()).unwrap_or_default()
    }

    pub fn load_state(&mut self, json: &str) -> Result<(), JsValue> {
        let s: state::State = serde_json::from_str(json).map_err(err)?;
        self.core.apply_state(s).map_err(err)
    }
}
