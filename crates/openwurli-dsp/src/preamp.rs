//! PreampModel trait — swappable preamp implementations for A/B testing and
//! runtime hot-swap via [`crate::preamp_variant::PreampVariant`].

pub trait PreampModel {
    fn process_sample(&mut self, input: f64) -> f64;
    fn set_ldr_resistance(&mut self, r_ldr_path: f64);
    fn reset(&mut self);

    /// Johnson–Nyquist thermal noise on preamp resistors (melange path only).
    fn set_noise_enabled(&mut self, _on: bool) {}

    /// Scale thermal noise amplitude; `1.0` = physics-honest (melange path only).
    fn set_thermal_gain(&mut self, _gain: f64) {}
}
