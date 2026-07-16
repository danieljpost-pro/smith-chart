//! Minimal complex-number type. Hand-rolled to keep the wasm binary small;
//! only the operations the chart needs.

use serde::{Deserialize, Serialize};
use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Complex {
    pub re: f64,
    pub im: f64,
}

pub const ZERO: Complex = Complex { re: 0.0, im: 0.0 };
pub const ONE: Complex = Complex { re: 1.0, im: 0.0 };

impl Complex {
    pub fn new(re: f64, im: f64) -> Self {
        Complex { re, im }
    }

    pub fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }

    pub fn abs_sq(self) -> f64 {
        self.re * self.re + self.im * self.im
    }

    /// Principal argument in radians, (-pi, pi].
    pub fn arg(self) -> f64 {
        self.im.atan2(self.re)
    }

    pub fn conj(self) -> Self {
        Complex::new(self.re, -self.im)
    }

    pub fn is_finite(self) -> bool {
        self.re.is_finite() && self.im.is_finite()
    }

    pub fn from_polar(mag: f64, arg: f64) -> Self {
        Complex::new(mag * arg.cos(), mag * arg.sin())
    }

    pub fn scale(self, k: f64) -> Self {
        Complex::new(self.re * k, self.im * k)
    }
}

impl Add for Complex {
    type Output = Complex;
    fn add(self, o: Complex) -> Complex {
        Complex::new(self.re + o.re, self.im + o.im)
    }
}

impl Sub for Complex {
    type Output = Complex;
    fn sub(self, o: Complex) -> Complex {
        Complex::new(self.re - o.re, self.im - o.im)
    }
}

impl Mul for Complex {
    type Output = Complex;
    fn mul(self, o: Complex) -> Complex {
        Complex::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
}

impl Div for Complex {
    type Output = Complex;
    fn div(self, o: Complex) -> Complex {
        let d = o.abs_sq();
        Complex::new(
            (self.re * o.re + self.im * o.im) / d,
            (self.im * o.re - self.re * o.im) / d,
        )
    }
}

impl Neg for Complex {
    type Output = Complex;
    fn neg(self) -> Complex {
        Complex::new(-self.re, -self.im)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic() {
        let a = Complex::new(1.0, 2.0);
        let b = Complex::new(3.0, -1.0);
        assert_eq!(a + b, Complex::new(4.0, 1.0));
        assert_eq!(a * b, Complex::new(5.0, 5.0));
        let q = a / b;
        let back = q * b;
        assert!((back - a).abs() < 1e-12);
    }

    #[test]
    fn polar() {
        let c = Complex::from_polar(2.0, std::f64::consts::FRAC_PI_2);
        assert!((c.re).abs() < 1e-12);
        assert!((c.im - 2.0).abs() < 1e-12);
        assert!((c.arg() - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    }
}
