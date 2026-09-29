//! Experimental numerical approximations, excluded from the shipping feature set.
//!
//! These tables change floating-point results. They are generated from the
//! retained canonical analytical functions in circuit_math and tremolo, never
//! independently fitted or hand-maintained formulas. Keep the analytical paths
//! permanently for comparisons; table implementation can change independently.
//!
//! Cubic Hermite interpolation stores polynomial coefficients and returns BOTH
//! its value and its actual derivative. Newton must differentiate the interpolant,
//! not pretend that the derivative of an approximate exponential is itself.
//! Native libm evaluates points outside each bounded interval, including NaN.

use std::sync::OnceLock;

const SEGMENTS: usize = 2048;

struct Table {
    lower: f64,
    upper: f64,
    inverse_step: f64,
    coefficients: Box<[[f64; 4]]>,
}

impl Table {
    fn new(lower: f64, upper: f64, evaluate: impl Fn(f64) -> (f64, f64)) -> Self {
        let step = (upper - lower) / SEGMENTS as f64;
        let coefficients = (0..SEGMENTS)
            .map(|i| {
                let (y0, d0) = evaluate(lower + i as f64 * step);
                let (y1, d1) = evaluate(lower + (i + 1) as f64 * step);
                let m0 = d0 * step;
                let m1 = d1 * step;
                [
                    y0,
                    m0,
                    3.0 * (y1 - y0) - 2.0 * m0 - m1,
                    2.0 * (y0 - y1) + m0 + m1,
                ]
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            lower,
            upper,
            inverse_step: 1.0 / step,
            coefficients,
        }
    }

    #[inline]
    fn lookup(&self, x: f64) -> Option<(f64, f64)> {
        if !(x >= self.lower && x < self.upper) {
            return None;
        }
        let position = (x - self.lower) * self.inverse_step;
        let index = (position as usize).min(SEGMENTS - 1);
        let t = position - index as f64;
        let [a, b, c, d] = self.coefficients[index];
        Some((
            a + t * (b + t * (c + t * d)),
            (b + t * (2.0 * c + t * 3.0 * d)) * self.inverse_step,
        ))
    }
}

static EXP: OnceLock<Table> = OnceLock::new();
static TANH: OnceLock<Table> = OnceLock::new();
static LDR: OnceLock<Table> = OnceLock::new();

fn exp_table() -> &'static Table {
    EXP.get_or_init(|| Table::new(-40.0, 40.0, crate::circuit_math::exp))
}

fn tanh_table() -> &'static Table {
    TANH.get_or_init(|| Table::new(-12.0, 12.0, crate::circuit_math::tanh))
}

fn ldr_table() -> &'static Table {
    // The power-law slope is singular at zero. Keep native evaluation in the
    // dark tail instead of interpolating across the corner at drive=1e-6.
    LDR.get_or_init(|| Table::new(1.0 / 64.0, 1.0, crate::tremolo::ldr_law_sample))
}

/// Run at construction, never first in process_sample: allocates the shared
/// immutable tables once. Lookups after this point allocate nothing.
pub(crate) fn initialize() {
    exp_table();
    tanh_table();
    ldr_table();
}

#[inline]
pub(crate) fn exp(x: f64) -> (f64, f64) {
    exp_table()
        .lookup(x)
        .unwrap_or_else(|| crate::circuit_math::exp(x))
}

#[inline]
pub(crate) fn tanh(x: f64) -> (f64, f64) {
    tanh_table()
        .lookup(x)
        .unwrap_or_else(|| crate::circuit_math::tanh(x))
}

#[inline]
pub(crate) fn ldr_resistance(drive: f64) -> Option<f64> {
    ldr_table().lookup(drive).map(|(value, _)| value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_function_and_derivative_bounds() {
        initialize();
        let (
            mut exp_error,
            mut exp_derivative_error,
            mut tanh_error,
            mut tanh_derivative_error,
            mut ldr_error,
        ) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
        // Dense off-grid samples exercise curvature and every table segment.
        for i in 0..100_000 {
            let t = (i as f64 + 0.37) / 100_000.0;
            let x = -40.0 + 80.0 * t;
            let reference = x.exp();
            let (value, derivative) = exp(x);
            exp_error = exp_error.max((value / reference - 1.0).abs());
            exp_derivative_error = exp_derivative_error.max((derivative / reference - 1.0).abs());
            let x = -12.0 + 24.0 * t;
            let reference = x.tanh();
            let (value, derivative) = tanh(x);
            tanh_error = tanh_error.max((value - reference).abs());
            tanh_derivative_error =
                tanh_derivative_error.max((derivative - (1.0 - reference * reference)).abs());
            let drive = 1.0 / 64.0 + (1.0 - 1.0 / 64.0) * t;
            let reference = crate::tremolo::ldr_law_sample(drive).0;
            ldr_error = ldr_error.max((ldr_resistance(drive).unwrap() / reference - 1.0).abs());
        }
        eprintln!(
            "LUT bounds: exp rel={exp_error:e}, exp derivative rel={exp_derivative_error:e}, tanh abs={tanh_error:e}, tanh derivative abs={tanh_derivative_error:e}, LDR rel={ldr_error:e}"
        );
        assert!(exp_error < 1e-8, "exp relative error: {exp_error:e}");
        assert!(
            exp_derivative_error < 1e-6,
            "exp derivative relative error: {exp_derivative_error:e}"
        );
        assert!(tanh_error < 1e-9, "tanh absolute error: {tanh_error:e}");
        assert!(
            tanh_derivative_error < 1e-6,
            "tanh derivative absolute error: {tanh_derivative_error:e}"
        );
        assert!(ldr_error < 1e-8, "LDR relative error: {ldr_error:e}");
        for x in [-100.0_f64, -40.00001, 40.0, 700.0] {
            assert_eq!(exp(x).0.to_bits(), x.exp().to_bits());
        }
        for x in [-20.0_f64, 12.0, 100.0] {
            assert_eq!(tanh(x).0.to_bits(), x.tanh().to_bits());
        }
        assert!(ldr_resistance(0.0).is_none());
        assert!(ldr_resistance(1.0).is_none());
        assert!(exp(f64::NAN).0.is_nan());
    }
}
