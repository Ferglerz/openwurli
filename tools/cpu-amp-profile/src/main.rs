use std::{hint::black_box, time::Instant};
use serde_json::json;
const SR: f64 = 48000.0;
const BLOCK: usize = 256;
const SAMPLES: usize = 24000;
fn notes(n: usize) -> Vec<u8> { (0..n).map(|i| 48+i as u8*3).collect() }
fn reference_engine(n: usize) -> reference::WurliEngine {
    let mut e=reference::WurliEngine::new(SR);
    e.ensure_buffer_capacity(BLOCK); e.set_noise_enabled(false); e.set_mlp_enabled(false);
    e.set_volume(0.7); e.set_tremolo_depth(0.5); e.set_speaker_character(0.5); e.warm_up();
    for note in notes(n) { e.note_on(note, 100.0/127.0); } e
}
fn trace(n: usize) -> Vec<f64> {
    let mut e=capture::WurliEngine::new(SR);
    e.ensure_buffer_capacity(BLOCK); e.set_noise_enabled(false); e.set_mlp_enabled(false);
    e.set_volume(0.7); e.set_tremolo_depth(0.5); e.set_speaker_character(0.5); e.warm_up();
    for note in notes(n) { e.note_on(note, 100.0/127.0); }
    capture::drive_capture::start(SAMPLES*2);
    let mut out=vec![0.0;SAMPLES];for b in out.chunks_mut(BLOCK) { e.render(b); }
    capture::drive_capture::finish()
}
fn total(n: usize) -> u128 {
    let mut e=reference_engine(n);let mut out=vec![0.0;SAMPLES];
    let start=Instant::now();for b in out.chunks_mut(BLOCK) { e.render(black_box(b)); }black_box(out);
    start.elapsed().as_nanos()
}
fn amp(input: &[f64]) -> u128 {
    let mut e=reference::power_amp::PowerAmp::new();
    let start=Instant::now();for &x in input { black_box(e.process(black_box(x))); }
    start.elapsed().as_nanos()
}
fn median(v: &[u128])->u128 { let mut sorted=v.to_vec();sorted.sort();sorted[sorted.len()/2] }
fn main() {
    let mut cases=Vec::new();
    for n in [1,6,12] {
        let input=trace(n);assert_eq!(input.len(),SAMPLES*2);black_box(total(n));black_box(amp(&input));
        let mut all=Vec::new();let mut stage=Vec::new();
        for r in 0..11 { if r%2==0 { all.push(total(n));stage.push(amp(&input)); } else {stage.push(amp(&input));all.push(total(n));} }
        let tm=median(&all);let am=median(&stage);
        cases.push(json!({"voices":n,"engine_ns":all,"amp_replay_ns":stage,"engine_median_ns":tm,"amp_median_ns":am,"amp_share_percent":100.0*am as f64/tm as f64,"amp_input_min":input.iter().copied().fold(f64::INFINITY,f64::min),"amp_input_max":input.iter().copied().fold(f64::NEG_INFINITY,f64::max),"amp_inputs":input.len()}));
    }
    println!("{}",serde_json::to_string_pretty(&json!({"baseline":"fd4f603dd8deef7a57d4449826f2c9d5dd53757e","sample_rate":SR,"block":BLOCK,"samples":SAMPLES,"repeats":11,"method":"unmodified native engine vs independent amp replay of real captured inputs; capture crate never timed; diagnostic estimate excludes stage scheduling/context and includes loop black_box overhead","cases":cases})).unwrap());
}
