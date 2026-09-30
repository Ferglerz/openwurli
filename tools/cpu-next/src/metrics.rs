//! Full-output and 20 ms / 50%-overlap residual gates.
use serde::Serialize;

#[derive(Clone, Copy, Serialize)]
pub struct Limits {
    pub peak_dbfs: f64,
    pub rms_relative_db: f64,
}
#[derive(Clone, Serialize)]
pub struct Residual {
    pub samples: usize,
    pub bit_mismatches: usize,
    pub first_mismatch_sample: Option<usize>,
    pub nonfinite_pairs: usize,
    pub peak_residual: f64,
    pub peak_residual_sample: usize,
    pub rms_residual: f64,
    pub reference_rms: f64,
    pub peak_residual_dbfs: Option<f64>,
    pub rms_residual_dbfs: Option<f64>,
    pub rms_relative_db: Option<f64>,
    pub zero_reference: bool,
    pub zero_reference_violation: bool,
    pub exact: bool,
    pub within_bounds: bool,
}
#[derive(Clone, Serialize)]
pub struct Window {
    pub start_sample: usize,
    pub end_sample: usize,
    pub sample_rate: u32,
    #[serde(flatten)]
    pub residual: Residual,
}
#[derive(Serialize)]
pub struct WindowSummary {
    pub duration_ms: u32,
    pub overlap_percent: u32,
    pub windows: usize,
    pub failed_windows: usize,
    pub zero_reference_windows: usize,
    pub zero_reference_violations: usize,
    pub worst_peak: Option<Window>,
    pub worst_relative: Option<Window>,
    pub first_failures: Vec<Window>,
    pub first_failures_limit: usize,
    pub within_bounds: bool,
    pub convention: &'static str,
}
#[derive(Serialize)]
pub struct Comparison {
    pub full: Residual,
    pub sliding_20ms: WindowSummary,
    pub reference_nan_guards: u64,
    pub candidate_nan_guards: u64,
    pub exact: bool,
    pub within_bounds: bool,
}
fn db(value: f64) -> Option<f64> {
    (value > 0.0 && value.is_finite()).then(|| 20.0 * value.log10())
}
pub fn residual(reference: &[f32], candidate: &[f32], limits: Limits) -> Residual {
    assert_eq!(reference.len(), candidate.len());
    assert!(!reference.is_empty());
    let (mut mismatches, mut first, mut bad, mut peak, mut peak_at, mut energy, mut ref_energy) =
        (0, None, 0, 0.0_f64, 0, 0.0, 0.0);
    for (i, (&a, &b)) in reference.iter().zip(candidate).enumerate() {
        if a.to_bits() != b.to_bits() {
            mismatches += 1;
            first.get_or_insert(i);
        }
        if !a.is_finite() || !b.is_finite() {
            bad += 1;
            continue;
        }
        let delta = b as f64 - a as f64;
        if delta.abs() > peak {
            peak = delta.abs();
            peak_at = i;
        }
        energy += delta * delta;
        ref_energy += (a as f64) * (a as f64);
    }
    let n = reference.len() as f64;
    let rms = (energy / n).sqrt();
    let reference_rms = (ref_energy / n).sqrt();
    let zero_ref = ref_energy == 0.0;
    // Gate in linear amplitude: zero energy never becomes a permissive null dB.
    let peak_pass = peak <= 10.0_f64.powf(limits.peak_dbfs / 20.0);
    let relative_pass = if zero_ref {
        energy == 0.0
    } else {
        rms <= reference_rms * 10.0_f64.powf(limits.rms_relative_db / 20.0)
    };
    Residual {
        samples: reference.len(),
        bit_mismatches: mismatches,
        first_mismatch_sample: first,
        nonfinite_pairs: bad,
        peak_residual: peak,
        peak_residual_sample: peak_at,
        rms_residual: rms,
        reference_rms,
        peak_residual_dbfs: db(peak),
        rms_residual_dbfs: db(rms),
        rms_relative_db: if zero_ref {
            None
        } else {
            db(rms / reference_rms)
        },
        zero_reference: zero_ref,
        zero_reference_violation: zero_ref && energy != 0.0,
        exact: mismatches == 0 && bad == 0,
        within_bounds: bad == 0 && peak_pass && relative_pass,
    }
}
/// `rate_segments` partitions the sample array at rate changes. Each tuple is
/// (first sample, sample rate); starts must ascend and begin at sample zero.
/// Windows restart at rate boundaries so 20 ms means the active rate, not the
/// initial rate. A final partial window is included to cover every last sample.
pub fn compare(
    reference: &[f32],
    candidate: &[f32],
    rate_segments: &[(usize, u32)],
    limits: Limits,
    reference_guards: u64,
    candidate_guards: u64,
) -> Comparison {
    assert!(!rate_segments.is_empty() && rate_segments[0].0 == 0);
    let full = residual(reference, candidate, limits);
    let mut summary = WindowSummary {
        duration_ms: 20,
        overlap_percent: 50,
        windows: 0,
        failed_windows: 0,
        zero_reference_windows: 0,
        zero_reference_violations: 0,
        worst_peak: None,
        worst_relative: None,
        first_failures: Vec::new(),
        first_failures_limit: 20,
        within_bounds: true,
        convention: "20ms at active sample rate; 10ms hop; restart at sample-rate changes; include final partial window; sample positions are output frame indices",
    };
    for (segment, &(begin, sr)) in rate_segments.iter().enumerate() {
        let end = rate_segments
            .get(segment + 1)
            .map_or(reference.len(), |&(at, _)| at);
        assert!(begin < end && end <= reference.len() && sr > 0);
        let length = ((sr as usize * 20) / 1000).max(1);
        let hop = ((sr as usize * 10) / 1000).max(1);
        for start in (begin..end).step_by(hop) {
            let stop = (start + length).min(end);
            let mut r = residual(&reference[start..stop], &candidate[start..stop], limits);
            r.first_mismatch_sample = r.first_mismatch_sample.map(|at| at + start);
            r.peak_residual_sample += start;
            let window = Window {
                start_sample: start,
                end_sample: stop,
                sample_rate: sr,
                residual: r,
            };
            summary.windows += 1;
            summary.zero_reference_windows += usize::from(window.residual.zero_reference);
            summary.zero_reference_violations +=
                usize::from(window.residual.zero_reference_violation);
            if !window.residual.within_bounds {
                summary.failed_windows += 1;
                if summary.first_failures.len() < summary.first_failures_limit {
                    summary.first_failures.push(window.clone());
                }
            }
            if summary
                .worst_peak
                .as_ref()
                .is_none_or(|w| window.residual.peak_residual > w.residual.peak_residual)
            {
                summary.worst_peak = Some(window.clone());
            }
            let relative_score = |w: &Window| {
                if w.residual.zero_reference_violation {
                    f64::INFINITY
                } else {
                    w.residual.rms_relative_db.unwrap_or(f64::NEG_INFINITY)
                }
            };
            if summary
                .worst_relative
                .as_ref()
                .is_none_or(|w| relative_score(&window) > relative_score(w))
            {
                summary.worst_relative = Some(window);
            }
        }
    }
    summary.within_bounds = summary.failed_windows == 0;
    let guards_clear = reference_guards == 0 && candidate_guards == 0;
    Comparison {
        exact: full.exact && guards_clear,
        within_bounds: full.within_bounds && summary.within_bounds && guards_clear,
        full,
        sliding_20ms: summary,
        reference_nan_guards: reference_guards,
        candidate_nan_guards: candidate_guards,
    }
}
