//! Canonical native transcendental values and derivatives.
//!
//! Both analytical circuit evaluation and optional table construction/fallback
//! call these functions. No independent approximation replaces this source.

#[inline]
pub(crate) fn exp(x: f64) -> (f64, f64) {
    let value = x.exp();
    (value, value)
}

#[inline]
pub(crate) fn tanh(x: f64) -> (f64, f64) {
    let value = x.tanh();
    (value, 1.0 - value * value)
}
