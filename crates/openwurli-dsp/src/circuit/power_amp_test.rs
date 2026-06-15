use crate::circuit::power_amp::PowerAmp;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_power_amp_large_input() {
        let mut pa = PowerAmp::new_at_sample_rate(44100.0);
        let mut s = 0.0;
        for i in 0..10000 {
            s += 0.5 * (std::f64::consts::PI * 100.0 * (i as f64) / 44100.0).sin();
            let input = s * 10.0;
            pa.process(input);
            let (clamps, nr, peak) = pa.diag_snapshot();
            if nr > 10 {
                println!("iter: {}", nr);
            }
        }
    }
}
