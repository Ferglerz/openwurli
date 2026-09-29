//! Temporary, non-shipping pre/post evidence harness. Delete only after review sign-off.
use serde::Serialize;
use serde_json::{Value, json};
use std::{error::Error, fs, hint::black_box, path::Path, time::Instant};

const NAMES: [&str; 3] = ["scalar", "shipping", "optimized"];
#[derive(Clone, Serialize)]
struct Scenario {
    name: String,
    sample_rate: u32,
    block: usize,
    mlp: bool,
    notes: Vec<u8>,
    velocity: u8,
    profile: u8,
    lifecycle: bool,
    seconds: f64,
}
#[derive(Clone, Copy)]
enum Event {
    On(u8, f32),
    Off(u8),
    Sustain(bool),
    Params(u8),
}

macro_rules! engine_dispatch {
    ($s:expr, $e:ident, $body:expr) => { match $s { Engine::Scalar($e) => $body, Engine::Shipping($e) => $body, Engine::Optimized($e) => $body } };
}
enum Engine {
    Scalar(cpu_ab_scalar::WurliEngine),
    Shipping(cpu_ab_shipping::WurliEngine),
    Optimized(openwurli_dsp::WurliEngine),
}
impl Engine {
    fn new(variant: usize, s: &Scenario) -> Self {
        let sr = s.sample_rate as f64;
        let mut e = match variant {
            0 => Self::Scalar(cpu_ab_scalar::WurliEngine::new(sr)),
            1 => Self::Shipping(cpu_ab_shipping::WurliEngine::new(sr)),
            _ => Self::Optimized(openwurli_dsp::WurliEngine::new(sr)),
        };
        engine_dispatch!(&mut e, x, {
            x.ensure_buffer_capacity(s.block);
            x.set_noise_enabled(false);
            x.set_mlp_enabled(s.mlp);
        });
        e.event(Event::Params(s.profile));
        engine_dispatch!(&mut e, x, x.warm_up());
        e
    }
    fn event(&mut self, event: Event) {
        engine_dispatch!(
            self,
            e,
            match event {
                Event::On(n, v) => e.note_on(n, v),
                Event::Off(n) => e.note_off(n),
                Event::Sustain(b) => e.set_sustain(b),
                Event::Params(p) => {
                    let (v, t, s, c, r) = match p {
                        1 => (1.0, 1.0, 0.0, 2.0, 0.5),
                        2 => (0.25, 0.0, 1.0, 0.5, 2.0),
                        _ => (0.7, 0.5, 0.5, 1.0, 1.0),
                    };
                    e.set_volume(v);
                    e.set_tremolo_depth(t);
                    e.set_speaker_character(s);
                    e.set_reed_decay(c);
                    e.set_hammer_hardness(c);
                    e.set_pickup_drive(c);
                    e.set_tremolo_response(r);
                    e.set_rail_sag(p != 2);
                }
            }
        );
    }
    fn render(&mut self, out: &mut [f32]) {
        engine_dispatch!(self, e, e.render(black_box(out)));
    }
    fn nan_guard(&self) -> u64 {
        engine_dispatch!(self, e, e.nan_guard_fires())
    }
}
fn events(s: &Scenario) -> Vec<(usize, Event)> {
    let mut ev = Vec::new();
    let sample = |time: f64| (time * s.sample_rate as f64) as usize;
    for &n in &s.notes {
        ev.push((0, Event::On(n, s.velocity as f32 / 127.0)));
    }
    if s.lifecycle {
        ev.push((sample(0.081) + 1, Event::Sustain(true)));
        for &n in &s.notes {
            ev.push((sample(0.143) + 3, Event::Off(n)));
        }
        // Same note restrike, sustained predecessor, >64 allocations, damper release.
        for &n in &s.notes {
            ev.push((sample(0.211) + 7, Event::On(n, s.velocity as f32 / 127.0)));
        }
        ev.push((sample(0.249) + 1, Event::Params((s.profile + 1) % 3)));
        ev.push((sample(0.321) + 5, Event::Sustain(false)));
        for &n in &s.notes {
            ev.push((sample(0.389) + 1, Event::Off(n)));
        }
    }
    ev.sort_by_key(|e| e.0);
    ev
}
struct Render {
    samples: Vec<f32>,
    nan_guard: u64,
    blocks: Vec<u64>,
    elapsed_ns: u64,
}
fn render(s: &Scenario, variant: usize, timed: bool) -> Render {
    let mut engine = Engine::new(variant, s);
    let schedule = events(s);
    let length = (s.seconds * s.sample_rate as f64) as usize;
    let mut output = vec![0.0f32; length];
    let mut blocks = Vec::with_capacity(length / s.block + schedule.len() + 1);
    let (mut at, mut event_at, mut elapsed) = (0, 0, 0u64);
    while at < length {
        while event_at < schedule.len() && schedule[event_at].0 == at {
            engine.event(schedule[event_at].1);
            event_at += 1;
        }
        let next_event = schedule.get(event_at).map_or(length, |e| e.0);
        let end = (at + s.block).min(length).min(next_event);
        assert!(end > at, "invalid schedule");
        if timed {
            let begin = Instant::now();
            engine.render(&mut output[at..end]);
            let ns = begin.elapsed().as_nanos() as u64;
            elapsed += ns;
            blocks.push(ns);
        } else {
            engine.render(&mut output[at..end]);
        }
        black_box(&output[at..end]);
        at = end;
    }
    Render {
        samples: output,
        nan_guard: engine.nan_guard(),
        blocks,
        elapsed_ns: elapsed,
    }
}
fn db(v: f64) -> Option<f64> {
    if v > 0.0 && v.is_finite() {
        Some(20.0 * v.log10())
    } else {
        None
    }
}
fn compare(a: &Render, b: &Render) -> Value {
    let (mut mismatch, mut bad, mut peak, mut sum, mut reference, mut max_sample) =
        (0u64, 0u64, 0.0f64, 0.0, 0.0, 0usize);
    let mut first = None;
    for (i, (&x, &y)) in a.samples.iter().zip(&b.samples).enumerate() {
        if x.to_bits() != y.to_bits() {
            mismatch += 1;
            if first.is_none() {
                first = Some(i);
            }
        }
        if !x.is_finite() || !y.is_finite() {
            bad += 1;
            continue;
        }
        let d = y as f64 - x as f64;
        if d.abs() > peak {
            peak = d.abs();
            max_sample = i;
        }
        sum += d * d;
        reference += (x as f64) * (x as f64);
    }
    let n = a.samples.len() as f64;
    let rms = (sum / n).sqrt();
    let ref_rms = (reference / n).sqrt();
    json!({"samples":a.samples.len(),"bit_mismatches":mismatch,"first_mismatch_sample":first,"nonfinite_pairs":bad,"peak_residual":peak,"peak_residual_sample":max_sample,"rms_residual":rms,"peak_residual_dbfs":db(peak),"rms_residual_dbfs":db(rms),"reference_rms_dbfs":db(ref_rms),"residual_relative_db":db(rms/ref_rms),"reference_nan_guards":a.nan_guard,"candidate_nan_guards":b.nan_guard,"exact":mismatch==0&&bad==0&&a.nan_guard==0&&b.nan_guard==0})
}
fn wav(path: &Path, sr: u32, samples: &[f32]) -> Result<(), Box<dyn Error>> {
    let mut w = hound::WavWriter::create(
        path,
        hound::WavSpec {
            channels: 1,
            sample_rate: sr,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )?;
    for &s in samples {
        w.write_sample(s)?;
    }
    w.finalize()?;
    Ok(())
}
fn audio_scenarios(full: bool) -> Vec<Scenario> {
    let rates = if full {
        vec![44100, 48000, 88200, 96000]
    } else {
        vec![44100, 96000]
    };
    let notes = if full {
        (33..=96).collect::<Vec<_>>()
    } else {
        vec![33, 45, 57, 60, 69, 81, 93, 96]
    };
    let velocities = if full {
        vec![1, 64, 127]
    } else {
        vec![64, 127]
    };
    let mut out = Vec::new();
    for sr in rates {
        for &n in &notes {
            for &v in &velocities {
                for mlp in [false, true] {
                    let idx = out.len();
                    let block = [1, 64, 257, 1024][idx % 4];
                    let profile = (idx % 3) as u8;
                    out.push(Scenario {
                        name: format!(
                            "note-{n}-vel-{v}-sr-{sr}-mlp-{mlp}-block-{block}-profile-{profile}"
                        ),
                        sample_rate: sr,
                        block,
                        mlp,
                        notes: vec![n],
                        velocity: v,
                        profile,
                        lifecycle: true,
                        seconds: if full { 0.8 } else { 0.5 },
                    });
                }
            }
        }
    }
    for count in [6, 32, 64, 80] {
        out.push(Scenario {
            name: format!("lifecycle-{count}-voices"),
            sample_rate: 48000,
            block: 127,
            mlp: true,
            notes: (0..count).map(|i| 33 + (i % 64) as u8).collect(),
            velocity: 127,
            profile: 1,
            lifecycle: true,
            seconds: 1.5,
        });
    }
    out.push(Scenario {
        name: "long-release-tail".into(),
        sample_rate: 44100,
        block: 257,
        mlp: true,
        notes: vec![33, 48, 60, 72, 84, 96],
        velocity: 127,
        profile: 0,
        lifecycle: true,
        seconds: 8.0,
    });
    out.push(Scenario {
        name: "long-tremolo-hold".into(),
        sample_rate: 44100,
        block: 257,
        mlp: false,
        notes: vec![48, 55, 60, 64, 67, 72],
        velocity: 100,
        profile: 1,
        lifecycle: false,
        seconds: 8.0,
    });
    out
}
fn cpu_scenarios(full: bool) -> Vec<Scenario> {
    let mut out = Vec::new();
    for sr in if full {
        vec![44100, 48000, 96000]
    } else {
        vec![44100]
    } {
        for block in if full { vec![64, 256, 1024] } else { vec![256] } {
            for count in [1, 6, 32, 64] {
                out.push(Scenario {
                    name: format!("hold-{count}-sr-{sr}-block-{block}"),
                    sample_rate: sr,
                    block,
                    mlp: true,
                    notes: (0..count).map(|i| 33 + (i * 7 % 64) as u8).collect(),
                    velocity: 100,
                    profile: 0,
                    lifecycle: false,
                    seconds: 0.5,
                });
            }
        }
    }
    for count in [1, 6, 32] {
        out.push(Scenario {
            name: format!("tremolo-off-{count}-sr-44100-block-256"),
            sample_rate: 44100,
            block: 256,
            mlp: true,
            notes: (0..count).map(|i| 33 + (i * 7 % 64) as u8).collect(),
            velocity: 100,
            profile: 2,
            lifecycle: false,
            seconds: 0.5,
        });
    }
    out
}
fn percentile(v: &[f64], q: f64) -> f64 {
    let mut v = v.to_vec();
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * q).ceil() as usize]
}
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    let value = |key: &str| args.windows(2).find(|w| w[0] == key).map(|w| w[1].clone());
    let full = value("--mode").as_deref() == Some("full");
    let repeats = value("--repeats").unwrap_or("7".into()).parse::<usize>()?;
    assert!(repeats >= 3);
    let dir = value("--output").unwrap_or("cpu-ab-results".into());
    let dir = Path::new(&dir);
    fs::create_dir_all(dir)?;
    let audio_cases = if args.iter().any(|arg| arg == "--cpu-only") {
        Vec::new()
    } else {
        audio_scenarios(full)
    };
    let mut audio = Vec::new();
    let mut exact = true;
    let mut finite = true;
    let mut within_bounds = true;
    let mut historical_exact = true;
    let export_all = args.iter().any(|arg| arg == "--export-all");
    if export_all {
        fs::create_dir_all(dir.join("waveforms"))?;
    }
    let peak_bound = value("--max-peak-dbfs")
        .map(|s| s.parse::<f64>())
        .transpose()?;
    let relative_bound = value("--max-rms-relative-db")
        .map(|s| s.parse::<f64>())
        .transpose()?;
    for (i, s) in audio_cases.iter().enumerate() {
        let renders: [Render; 3] = std::array::from_fn(|v| render(s, v, false));
        let comparisons = [
            ("scalar_to_shipping", 0, 1),
            ("shipping_to_optimized", 1, 2),
            ("scalar_to_optimized", 0, 2),
        ]
        .map(|(name, a, b)| (name.to_owned(), compare(&renders[a], &renders[b])))
        .into_iter()
        .collect::<serde_json::Map<String, Value>>();
        exact &= comparisons.values().all(|v| v["exact"] == true);
        historical_exact &= comparisons["scalar_to_shipping"]["exact"] == true;
        finite &= comparisons.values().all(|v| {
            v["nonfinite_pairs"] == 0
                && v["reference_nan_guards"] == 0
                && v["candidate_nan_guards"] == 0
        });
        let post = &comparisons["shipping_to_optimized"];
        if let Some(bound) = peak_bound {
            within_bounds &= post["peak_residual_dbfs"]
                .as_f64()
                .is_none_or(|db| db <= bound);
        }
        if let Some(bound) = relative_bound {
            within_bounds &= post["residual_relative_db"]
                .as_f64()
                .is_none_or(|db| db <= bound);
        }

        if s.name.starts_with("long-") || s.name == "lifecycle-6-voices" {
            for v in 0..3 {
                wav(
                    &dir.join(format!("{}-{}.wav", s.name, NAMES[v])),
                    s.sample_rate,
                    &renders[v].samples,
                )?;
            }
            let diff: Vec<f32> = renders[2]
                .samples
                .iter()
                .zip(&renders[1].samples)
                .map(|(b, a)| b - a)
                .collect();
            wav(
                &dir.join(format!("{}-optimized-minus-shipping.wav", s.name)),
                s.sample_rate,
                &diff,
            )?;
        }
        let waveform = if export_all {
            let relative = format!("waveforms/{}.f32le", s.name);
            let bytes: Vec<u8> = renders[2]
                .samples
                .iter()
                .flat_map(|sample| sample.to_le_bytes())
                .collect();
            fs::write(dir.join(&relative), bytes)?;
            Some(relative)
        } else {
            None
        };
        audio.push(json!({"scenario":s,"comparisons":comparisons,"optimized_waveform":waveform}));
        if i % 32 == 0 {
            eprintln!("audio {}/{}", i + 1, audio_cases.len());
        }
    }
    fs::write(dir.join("audio.json"), serde_json::to_vec_pretty(&audio)?)?;
    let mut cpu = Vec::new();
    for s in if args.iter().any(|arg| arg == "--audio-only") {
        Vec::new()
    } else {
        cpu_scenarios(full)
    } {
        // Warm each implementation's code/cache before measured repetitions.
        for v in 0..3 {
            black_box(render(&s, v, false));
        }
        let mut raw: Vec<Vec<Value>> = vec![Vec::new(), Vec::new(), Vec::new()];
        let mut times: Vec<Vec<f64>> = vec![Vec::new(), Vec::new(), Vec::new()];
        let mut orders = Vec::new();
        for repeat in 0..repeats {
            let order = match repeat % 6 {
                0 => [0, 1, 2],
                1 => [2, 1, 0],
                2 => [1, 2, 0],
                3 => [0, 2, 1],
                4 => [2, 0, 1],
                _ => [1, 0, 2],
            };
            orders.push(order);
            for v in order {
                let r = render(&s, v, true);
                let ns = r.elapsed_ns as f64 / r.samples.len() as f64;
                times[v].push(ns);
                raw[v].push(json!({"repeat":repeat,"render_ns":r.elapsed_ns,"samples":r.samples.len(),"ns_per_sample":ns,"block_ns":r.blocks,"nan_guards":r.nan_guard}));
            }
        }
        let summary:Vec<Value>=(0..3).map(|v|json!({"variant":NAMES[v],"median_ns_per_sample":percentile(&times[v],0.5),"p95_ns_per_sample":percentile(&times[v],0.95),"median_realtime_percent":percentile(&times[v],0.5)*s.sample_rate as f64/1e7,"raw":raw[v]})).collect();
        let ratios:Vec<Value>=[(0,1),(1,2),(0,2)].iter().map(|&(a,b)|{let ratio=percentile(&times[b],0.5)/percentile(&times[a],0.5);json!({"reference":NAMES[a],"candidate":NAMES[b],"median_time_ratio":ratio,"median_cpu_reduction_percent":100.0*(1.0-ratio),"p95_time_ratio":percentile(&times[b],0.95)/percentile(&times[a],0.95)})}).collect();
        eprintln!("cpu {} complete", s.name);
        cpu.push(json!({"scenario":s,"order":orders,"variants":summary,"ratios":ratios}));
    }
    fs::write(dir.join("cpu.json"), serde_json::to_vec_pretty(&cpu)?)?;
    let report = json!({"schema":1,"mode":if full{"full"}else{"quick"},"repeats":repeats,"experimental_circuit_lut":cfg!(feature="experimental-circuit-lut"),"all_audio_exact":exact,"historical_scalar_to_shipping_exact":historical_exact,"all_audio_finite_without_guard":finite,"within_configured_bounds":within_bounds,"max_peak_dbfs":peak_bound,"max_rms_relative_db":relative_bound,"audio_scenarios":audio.len(),"cpu_scenarios":cpu.len(),"db_null_convention":"null means zero residual (-infinity dB) or undefined reference; consult raw amplitudes and nonfinite_pairs","audio":audio,"cpu":cpu});
    fs::write(dir.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
    eprintln!(
        "report: {} (all audio exact: {exact})",
        dir.join("report.json").display()
    );
    if !finite
        || !historical_exact
        || !within_bounds
        || ((!cfg!(feature = "experimental-circuit-lut")
            || args.iter().any(|a| a == "--require-exact"))
            && !exact)
    {
        return Err("pre/post audio equality failed; inspect report.json".into());
    }
    Ok(())
}
