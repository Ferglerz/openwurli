//! Untimed public power-amplifier diagnostics; never sample inside CPU runs.
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Clone, Copy, Serialize)]
pub struct Snapshot {
    pub sample_position: usize,
    pub phase: &'static str,
    pub clamp_count: u64,
    pub nr_max_iter_count: u64,
    pub peak_output_volts: f64,
    pub peak_output_bits: u64,
}
impl Snapshot {
    pub fn new(at: usize, phase: &'static str, value: (u64, u64, f64)) -> Self {
        Self {sample_position: at, phase, clamp_count: value.0, nr_max_iter_count: value.1,
            peak_output_volts: value.2, peak_output_bits: value.2.to_bits()}
    }
}
fn totals(rows: &[Snapshot]) -> (u64, u64) {
    let (mut clamp, mut nr, mut previous_clamp, mut previous_nr) = (0u64, 0u64, 0u64, 0u64);
    for row in rows {
        // Observe count drops across resets; include the new counter's current
        // activity instead of losing previously observed convergence failures.
        clamp += if row.clamp_count >= previous_clamp {row.clamp_count-previous_clamp} else {row.clamp_count};
        nr += if row.nr_max_iter_count >= previous_nr {row.nr_max_iter_count-previous_nr} else {row.nr_max_iter_count};
        previous_clamp=row.clamp_count;previous_nr=row.nr_max_iter_count;
    }
    (clamp,nr)
}
pub fn compare(reference: &[Snapshot], candidate: &[Snapshot]) -> Value {
    let (mut count_mismatch,mut peak_mismatch,mut alignment_mismatch,mut nonfinite)=(0usize,0usize,0usize,0usize);
    let (mut extra_clamp,mut extra_nr,mut peak_difference)=(0u64,0u64,0.0_f64);
    let mut differences=Vec::new();
    for (index,(a,b)) in reference.iter().zip(candidate).enumerate() {
        let count_diff=a.clamp_count!=b.clamp_count||a.nr_max_iter_count!=b.nr_max_iter_count;
        let peak_diff=a.peak_output_bits!=b.peak_output_bits;
        let alignment_diff=a.sample_position!=b.sample_position||a.phase!=b.phase;
        count_mismatch+=usize::from(count_diff);peak_mismatch+=usize::from(peak_diff);alignment_mismatch+=usize::from(alignment_diff);
        extra_clamp=extra_clamp.max(b.clamp_count.saturating_sub(a.clamp_count));extra_nr=extra_nr.max(b.nr_max_iter_count.saturating_sub(a.nr_max_iter_count));
        if a.peak_output_volts.is_finite()&&b.peak_output_volts.is_finite(){peak_difference=peak_difference.max((b.peak_output_volts-a.peak_output_volts).abs());}else{nonfinite+=1;}
        if (count_diff||peak_diff||alignment_diff)&&differences.len()<20 {differences.push(json!({"snapshot_index":index,"reference":a,"candidate":b}));}
    }
    let a_totals=totals(reference);let b_totals=totals(candidate);
    let exact=reference.len()==candidate.len()&&count_mismatch==0&&peak_mismatch==0&&alignment_mismatch==0&&nonfinite==0;
    let increases=extra_clamp>0||extra_nr>0||b_totals.0>a_totals.0||b_totals.1>a_totals.1;
    let observed_activity=a_totals.0>0||a_totals.1>0||b_totals.0>0||b_totals.1>0;
    json!({"scope":"selected power amp public diagnostics; no preamp reset/NR counters are exposed by this API", "reference_snapshots":reference.len(),"candidate_snapshots":candidate.len(),"counter_mismatch_snapshots":count_mismatch,"peak_bit_mismatch_snapshots":peak_mismatch,"alignment_mismatches":alignment_mismatch,"nonfinite_peak_pairs":nonfinite,"maximum_extra_candidate_clamps_at_snapshot":extra_clamp,"maximum_extra_candidate_nr_limits_at_snapshot":extra_nr,"reference_observed_clamp_events":a_totals.0,"candidate_observed_clamp_events":b_totals.0,"reference_observed_nr_limit_events":a_totals.1,"candidate_observed_nr_limit_events":b_totals.1,"maximum_peak_voltage_difference":peak_difference,"reference_final":reference.last(),"candidate_final":candidate.last(),"first_differences":differences,"first_differences_limit":20,"trajectory_bit_exact":exact,"candidate_counter_increase":increases,"finite_and_no_counter_increase":!increases&&nonfinite==0&&reference.len()==candidate.len()&&alignment_mismatch==0,"review_required":!exact||observed_activity,"counter_total_convention":"sum observed positive counter increments, including initialization/warmup; restart accumulation when a counter drops across reset; hidden within-call resets remain outside this API"})
}
