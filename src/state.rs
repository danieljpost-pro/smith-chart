//! Serializable chart state for URL sharing. Everything needed to reproduce
//! the chart lives here; the JS shell compresses the JSON and puts it in the
//! URL fragment, so sharing never touches a server.

use crate::complex::Complex;
use crate::touchstone::Network;
use serde::{Deserialize, Serialize};

pub const STATE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct State {
    pub v: u32,
    pub z0: f64,
    pub dark: bool,
    pub show_impedance: bool,
    pub show_admittance: bool,
    pub show_labels: bool,
    pub show_vswr: bool,
    /// Constant-Q contour value; 0 disables.
    pub q: f64,
    /// View: gamma-plane center and zoom relative to the fitted scale.
    pub view_cx: f64,
    pub view_cy: f64,
    pub view_zoom: f64,
    pub markers: Vec<MarkerState>,
    pub traces: Vec<TraceState>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MarkerState {
    pub re: f64,
    pub im: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freq_hz: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TraceState {
    pub name: String,
    pub z0: f64,
    pub nports: usize,
    pub param: usize,
    pub visible: bool,
    #[serde(default)]
    pub color: usize,
    pub freqs_hz: Vec<f64>,
    /// Row-major (nfreq x nports^2) flattened S-parameters.
    pub s_re: Vec<f64>,
    pub s_im: Vec<f64>,
}

fn round6(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

impl TraceState {
    pub fn from_network(net: &Network, param: usize, visible: bool) -> TraceState {
        let mut s_re = Vec::new();
        let mut s_im = Vec::new();
        for row in &net.sparams {
            for s in row {
                s_re.push(round6(s.re));
                s_im.push(round6(s.im));
            }
        }
        TraceState {
            name: net.name.clone(),
            z0: net.z0,
            nports: net.nports,
            param,
            visible,
            color: 0,
            freqs_hz: net.freqs_hz.clone(),
            s_re,
            s_im,
        }
    }

    pub fn to_network(&self) -> Result<Network, String> {
        let per_row = self.nports * self.nports;
        if per_row == 0
            || self.s_re.len() != self.s_im.len()
            || self.s_re.len() != self.freqs_hz.len() * per_row
        {
            return Err("inconsistent trace data in shared state".into());
        }
        let sparams = self
            .freqs_hz
            .iter()
            .enumerate()
            .map(|(i, _)| {
                (0..per_row)
                    .map(|j| Complex::new(self.s_re[i * per_row + j], self.s_im[i * per_row + j]))
                    .collect()
            })
            .collect();
        Ok(Network {
            name: self.name.clone(),
            z0: self.z0,
            nports: self.nports,
            freqs_hz: self.freqs_hz.clone(),
            sparams,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_round_trip() {
        let net = Network {
            name: "t".into(),
            z0: 50.0,
            nports: 2,
            freqs_hz: vec![1e9, 2e9],
            sparams: vec![
                vec![
                    Complex::new(0.1, 0.2),
                    Complex::new(0.3, 0.4),
                    Complex::new(0.5, 0.6),
                    Complex::new(0.7, 0.8),
                ],
                vec![
                    Complex::new(-0.1, -0.2),
                    Complex::new(-0.3, -0.4),
                    Complex::new(-0.5, -0.6),
                    Complex::new(-0.7, -0.8),
                ],
            ],
        };
        let st = TraceState::from_network(&net, 3, true);
        let back = st.to_network().unwrap();
        assert_eq!(back.nports, 2);
        assert_eq!(back.freqs_hz, net.freqs_hz);
        assert!((back.sparams[1][3].im - net.sparams[1][3].im).abs() < 1e-6);
    }

    #[test]
    fn corrupt_state_rejected() {
        let st = TraceState {
            name: "bad".into(),
            z0: 50.0,
            nports: 2,
            param: 0,
            visible: true,
            color: 0,
            freqs_hz: vec![1e9],
            s_re: vec![0.0; 3],
            s_im: vec![0.0; 3],
        };
        assert!(st.to_network().is_err());
    }
}
