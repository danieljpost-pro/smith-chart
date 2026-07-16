//! Touchstone (.s1p / .s2p) parser. Supports MA, DB and RI formats, any
//! frequency unit, comment lines, wrapped data rows, and stops at trailing
//! noise-parameter data (detected by a decreasing frequency).

use crate::complex::Complex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Format {
    Ma,
    Db,
    Ri,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Network {
    pub name: String,
    /// Reference impedance from the option line (ohms).
    pub z0: f64,
    pub nports: usize,
    pub freqs_hz: Vec<f64>,
    /// One entry per frequency; each holds nports^2 S-parameters in file
    /// order (s2p: S11, S21, S12, S22).
    pub sparams: Vec<Vec<Complex>>,
}

impl Network {
    /// Human labels for the parameter columns, in storage order.
    pub fn param_labels(&self) -> Vec<String> {
        match self.nports {
            1 => vec!["S11".into()],
            2 => vec!["S11".into(), "S21".into(), "S12".into(), "S22".into()],
            n => (1..=n)
                .flat_map(|j| (1..=n).map(move |i| format!("S{}{}", i, j)))
                .collect(),
        }
    }
}

/// `ports_hint` normally comes from the file extension (.s1p -> 1).
pub fn parse(name: &str, text: &str, ports_hint: Option<usize>) -> Result<Network, String> {
    let mut format = Format::Ma;
    let mut z0 = 50.0;
    let mut freq_mult = 1e9; // Touchstone default unit is GHz
    let mut saw_option_line = false;
    let mut tokens: Vec<f64> = Vec::new();

    for raw in text.lines() {
        let line = match raw.find('!') {
            Some(i) => &raw[..i],
            None => raw,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('#') {
            if saw_option_line {
                continue; // only the first option line counts
            }
            saw_option_line = true;
            let mut words = rest.split_whitespace().peekable();
            while let Some(w) = words.next() {
                match w.to_ascii_uppercase().as_str() {
                    "HZ" => freq_mult = 1.0,
                    "KHZ" => freq_mult = 1e3,
                    "MHZ" => freq_mult = 1e6,
                    "GHZ" => freq_mult = 1e9,
                    "S" => {}
                    "Y" | "Z" | "H" | "G" => {
                        return Err(format!("only S-parameter files are supported (got {})", w))
                    }
                    "MA" => format = Format::Ma,
                    "DB" => format = Format::Db,
                    "RI" => format = Format::Ri,
                    "R" => {
                        if let Some(v) = words.next() {
                            z0 = v
                                .parse::<f64>()
                                .map_err(|_| format!("bad reference impedance: {}", v))?;
                        }
                    }
                    other => return Err(format!("unknown option-line token: {}", other)),
                }
            }
            continue;
        }
        if line.starts_with('[') {
            return Err("Touchstone v2 files are not supported yet".into());
        }
        for tok in line.split_whitespace() {
            let v = tok
                .parse::<f64>()
                .map_err(|_| format!("bad number in data: {}", tok))?;
            tokens.push(v);
        }
    }

    if tokens.is_empty() {
        return Err("no data rows found".into());
    }

    let nports = match ports_hint {
        Some(n) => n,
        None => {
            // Guess: 2-port records are 9 tokens, 1-port are 3.
            if tokens.len().is_multiple_of(9) {
                2
            } else {
                1
            }
        }
    };
    if nports == 0 || nports > 2 {
        return Err(format!(
            "{}-port files are not supported (only .s1p and .s2p)",
            nports
        ));
    }
    let record_len = 1 + 2 * nports * nports;

    let mut freqs_hz = Vec::new();
    let mut sparams = Vec::new();
    let mut last_freq = f64::NEG_INFINITY;
    for rec in tokens.chunks(record_len) {
        if rec.len() < record_len {
            break; // trailing partial record (or misaligned noise data)
        }
        let f = rec[0] * freq_mult;
        if f < last_freq {
            break; // noise-parameter section of an .s2p file
        }
        last_freq = f;
        let mut row = Vec::with_capacity(nports * nports);
        for pair in rec[1..].chunks(2) {
            let s = match format {
                Format::Ma => Complex::from_polar(pair[0], pair[1].to_radians()),
                Format::Db => {
                    Complex::from_polar(10f64.powf(pair[0] / 20.0), pair[1].to_radians())
                }
                Format::Ri => Complex::new(pair[0], pair[1]),
            };
            row.push(s);
        }
        freqs_hz.push(f);
        sparams.push(row);
    }

    if freqs_hz.is_empty() {
        return Err("no complete data records found".into());
    }

    Ok(Network {
        name: name.to_string(),
        z0,
        nports,
        freqs_hz,
        sparams,
    })
}

/// Infer the port count from a file name extension, e.g. "foo.s2p" -> 2.
pub fn ports_from_name(name: &str) -> Option<usize> {
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit('.').next()?;
    let inner = ext.strip_prefix('s')?.strip_suffix('p')?;
    inner.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s1p_ma() {
        let text = "! comment\n# MHz S MA R 50\n100 0.5 45\n200 0.25 -90\n";
        let n = parse("t.s1p", text, Some(1)).unwrap();
        assert_eq!(n.nports, 1);
        assert_eq!(n.freqs_hz, vec![100e6, 200e6]);
        let s = n.sparams[0][0];
        assert!((s.abs() - 0.5).abs() < 1e-12);
        assert!((s.arg().to_degrees() - 45.0).abs() < 1e-9);
    }

    #[test]
    fn s1p_ri_default_ghz() {
        let text = "# S RI\n1 0.1 -0.2\n";
        let n = parse("t.s1p", text, Some(1)).unwrap();
        assert_eq!(n.freqs_hz, vec![1e9]);
        assert_eq!(n.sparams[0][0], Complex::new(0.1, -0.2));
    }

    #[test]
    fn s1p_db() {
        let text = "# HZ S DB\n10 -6.0206 0\n";
        let n = parse("t.s1p", text, Some(1)).unwrap();
        assert!((n.sparams[0][0].re - 0.5).abs() < 1e-4);
    }

    #[test]
    fn s2p_wrapped_rows() {
        // A record split across two physical lines must still parse.
        let text = "# GHz S MA R 75\n\
                    1 0.9 -10 0.1 80\n0.1 80 0.9 -10\n\
                    2 0.8 -20 0.2 70 0.2 70 0.8 -20\n";
        let n = parse("t.s2p", text, Some(2)).unwrap();
        assert_eq!(n.z0, 75.0);
        assert_eq!(n.freqs_hz.len(), 2);
        assert_eq!(n.sparams[1].len(), 4);
    }

    #[test]
    fn noise_data_ignored() {
        // Frequency drops -> noise section begins; those rows are dropped.
        let text = "# GHz S MA\n\
                    1 0.9 -10 0.1 80 0.1 80 0.9 -10\n\
                    2 0.8 -20 0.2 70 0.2 70 0.8 -20\n\
                    0.5 1.1 0.4 30 0.5 0.9 0.5 0.6 40\n";
        let n = parse("t.s2p", text, Some(2)).unwrap();
        assert_eq!(n.freqs_hz.len(), 2);
    }

    #[test]
    fn extension_hint() {
        assert_eq!(ports_from_name("Foo.S2P"), Some(2));
        assert_eq!(ports_from_name("a/b/c.s1p"), Some(1));
        assert_eq!(ports_from_name("data.csv"), None);
    }

    #[test]
    fn rejects_y_params() {
        assert!(parse("t.s1p", "# HZ Y MA\n1 2 3\n", Some(1)).is_err());
    }
}
