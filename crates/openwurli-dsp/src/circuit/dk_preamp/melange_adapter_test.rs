use crate::circuit::dk_preamp::DkPreamp;
use crate::circuit::preamp::PreampModel;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_large_negative_input() {
        let mut p = DkPreamp::new(44100.0);
        let mut s = 0.0;
        for i in 0..10000 {
            s += 0.5 * (std::f64::consts::PI * 100.0 * (i as f64) / 44100.0).sin();
            let input = -s * 10.0; // Negative
            p.process_sample(input);
            if p.main.last_nr_iterations > 10 {
                println!("i: {}, input: {}, iter: {}", i, input, p.main.last_nr_iterations);
            }
        }
    }
}
