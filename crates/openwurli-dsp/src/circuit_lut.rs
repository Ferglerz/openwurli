//! Experimental numerical approximations, excluded from the shipping feature set.
//!
//! These tables change floating-point results. They are generated from the
//! retained canonical analytical functions in circuit_math, never
//! independently fitted or hand-maintained formulas. Keep the analytical paths
//! permanently for comparisons; table implementation can change independently.
//!
//! Cubic Hermite interpolation stores polynomial coefficients and returns BOTH
//! its value and its actual derivative. Newton must differentiate the interpolant,
//! not pretend that the derivative of an approximate exponential is itself.
//! Native libm evaluates points outside each bounded interval, including NaN.

use std::sync::OnceLock;

const SEGMENTS: usize = 2048;

pub(crate) struct Table {
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

    /// The amplifier retains this table reference at construction, avoiding
    /// an atomic OnceLock access for every Newton evaluation.
    #[inline]
    pub(crate) fn tanh(&self, x: f64) -> (f64, f64) {
        self.lookup(x)
            .unwrap_or_else(|| crate::circuit_math::tanh(x))
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

static TANH: OnceLock<Table> = OnceLock::new();

pub(crate) fn tanh_table() -> &'static Table {
    TANH.get_or_init(|| Table::new(-12.0, 12.0, crate::circuit_math::tanh))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_function_and_derivative_bounds() {
        let table = tanh_table();
        let (mut value_error, mut derivative_error) = (0.0_f64, 0.0_f64);
        for i in 0..100_000 {
            let x = -12.0 + 24.0 * (i as f64 + 0.37) / 100_000.0;
            let (reference, reference_derivative) = crate::circuit_math::tanh(x);
            let (value, derivative) = table.tanh(x);
            value_error = value_error.max((value - reference).abs());
            derivative_error = derivative_error.max((derivative - reference_derivative).abs());
        }
        eprintln!(
            "Tanh LUT bounds: value abs={value_error:e}, derivative abs={derivative_error:e}"
        );
        assert!(value_error < 1e-9, "tanh absolute error: {value_error:e}");
        assert!(
            derivative_error < 1e-6,
            "tanh derivative absolute error: {derivative_error:e}"
        );
        for x in [-20.0_f64, 12.0, 100.0] {
            assert_eq!(table.tanh(x).0.to_bits(), x.tanh().to_bits());
        }
        assert!(table.tanh(f64::NAN).0.is_nan());
    }
}
