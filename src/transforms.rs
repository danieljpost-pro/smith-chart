//! Smith-chart transformations between the reflection-coefficient plane and
//! normalized impedance/admittance, plus the derived quantities shown in the
//! readout panel.

use crate::complex::{Complex, ONE};
use serde::Serialize;
use std::f64::consts::PI;

/// Normalized impedance z -> reflection coefficient.
pub fn z_to_gamma(z: Complex) -> Complex {
    (z - ONE) / (z + ONE)
}

/// Reflection coefficient -> normalized impedance. Returns non-finite values
/// at gamma = 1 (open circuit).
pub fn gamma_to_z(g: Complex) -> Complex {
    (ONE + g) / (ONE - g)
}

pub fn vswr(gamma_mag: f64) -> f64 {
    if gamma_mag >= 1.0 {
        f64::INFINITY
    } else {
        (1.0 + gamma_mag) / (1.0 - gamma_mag)
    }
}

pub fn return_loss_db(gamma_mag: f64) -> f64 {
    -20.0 * gamma_mag.log10()
}

pub fn mismatch_loss_db(gamma_mag: f64) -> f64 {
    -10.0 * (1.0 - gamma_mag * gamma_mag).log10()
}

/// Wavelengths toward generator, measured clockwise from the short-circuit
/// point (gamma = -1). Range [0, 0.5).
pub fn wavelengths_toward_generator(g: Complex) -> f64 {
    ((PI - g.arg()) / (4.0 * PI)).rem_euclid(0.5)
}

/// Renormalize a reflection coefficient referenced to `z0_from` ohms so it is
/// referenced to `z0_to` ohms (exact for one-port reflection data).
pub fn renormalize_gamma(g: Complex, z0_from: f64, z0_to: f64) -> Complex {
    if (z0_from - z0_to).abs() < 1e-12 {
        return g;
    }
    let z = gamma_to_z(g).scale(z0_from / z0_to);
    if !z.is_finite() {
        return g;
    }
    z_to_gamma(z)
}

/// Everything the UI shows about one point on the chart. Non-finite values
/// serialize as JSON null (serde_json behavior); the UI renders those as
/// infinity.
#[derive(Serialize, Clone, Debug)]
pub struct Readout {
    pub gamma_re: f64,
    pub gamma_im: f64,
    pub gamma_mag: f64,
    pub gamma_deg: f64,
    /// Normalized impedance.
    pub r: f64,
    pub x: f64,
    /// Impedance in ohms.
    pub z_re: f64,
    pub z_im: f64,
    /// Normalized admittance.
    pub g: f64,
    pub b: f64,
    /// Admittance in millisiemens.
    pub y_re_ms: f64,
    pub y_im_ms: f64,
    pub vswr: f64,
    pub return_loss_db: f64,
    pub mismatch_loss_db: f64,
    pub wtg: f64,
    pub wtl: f64,
}

pub fn readout(gamma: Complex, z0: f64) -> Readout {
    let z = gamma_to_z(gamma);
    let y = ONE / z;
    let mag = gamma.abs();
    let wtg = wavelengths_toward_generator(gamma);
    Readout {
        gamma_re: gamma.re,
        gamma_im: gamma.im,
        gamma_mag: mag,
        gamma_deg: gamma.arg().to_degrees(),
        r: z.re,
        x: z.im,
        z_re: z.re * z0,
        z_im: z.im * z0,
        g: y.re,
        b: y.im,
        y_re_ms: y.re / z0 * 1000.0,
        y_im_ms: y.im / z0 * 1000.0,
        vswr: vswr(mag),
        return_loss_db: return_loss_db(mag),
        mismatch_loss_db: mismatch_loss_db(mag),
        wtg,
        wtl: (0.5 - wtg).rem_euclid(0.5),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn round_trip() {
        for &(re, im) in &[(0.3, -0.4), (0.0, 0.0), (-0.7, 0.2), (0.99, 0.0)] {
            let g = Complex::new(re, im);
            let back = z_to_gamma(gamma_to_z(g));
            assert!((back - g).abs() < 1e-9, "{:?}", g);
        }
    }

    #[test]
    fn known_points() {
        // Matched load: z = 1 -> gamma = 0.
        assert!(z_to_gamma(Complex::new(1.0, 0.0)).abs() < 1e-12);
        // Short: z = 0 -> gamma = -1.
        assert!(close(z_to_gamma(Complex::new(0.0, 0.0)).re, -1.0));
        // z = 2 -> gamma = 1/3.
        assert!(close(z_to_gamma(Complex::new(2.0, 0.0)).re, 1.0 / 3.0));
    }

    #[test]
    fn vswr_and_loss() {
        assert!(close(vswr(0.0), 1.0));
        assert!(close(vswr(1.0 / 3.0), 2.0));
        assert!(vswr(1.0).is_infinite());
        assert!(close(return_loss_db(0.1), 20.0));
    }

    #[test]
    fn wtg_reference_points() {
        // Short circuit is the 0-wavelength reference.
        assert!(close(wavelengths_toward_generator(Complex::new(-1.0, 0.0)), 0.0));
        // Open circuit is a quarter wavelength from the short.
        assert!(close(wavelengths_toward_generator(Complex::new(1.0, 0.0)), 0.25));
    }

    #[test]
    fn renormalization() {
        // z = 100 ohms: gamma is 1/3 at 50-ohm reference, 0 at 100-ohm.
        let g50 = z_to_gamma(Complex::new(2.0, 0.0));
        let g100 = renormalize_gamma(g50, 50.0, 100.0);
        assert!(g100.abs() < 1e-12);
    }

    #[test]
    fn readout_matched() {
        let r = readout(Complex::new(0.0, 0.0), 50.0);
        assert!(close(r.z_re, 50.0));
        assert!(close(r.vswr, 1.0));
        assert!(close(r.y_re_ms, 20.0));
    }
}
