//! Paired real-render proof; DSP hot loops carry no benchmark instrumentation.
use cpu_next::metrics::{self, Limits};
use cpu_next::diagnostics::{self, Snapshot};
use serde::Serialize;
use serde_json::{Value, json};
use std::{error::Error, fs, hint::black_box, path::Path, time::Instant};

const NAMES: [&str; 2] = ["reference", "candidate"];
#[derive(Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Pattern {
    Hold,
    Lifecycle,
    RepeatedChord,
    PedalBursts,
    DepthSwitch,
    RepeatedDamp,
    ResetRates,
    Silence,
}
#[derive(Clone, Serialize)]
struct Scenario {
    name: String,
    sample_rate: u32,
    block: usize,
    mlp: bool,
    notes: Vec<u8>,
    velocity: u8,
    profile: u8,
    pattern: Pattern,
    seconds: f64,
}
#[derive(Clone, Copy, Serialize)]
enum Event {
    On(u8, f32),
    Off(u8),
    Sustain(bool),
    Params(u8),
    Depth(f64),
    Reset,
    Rate(u32),
}
macro_rules! dispatch {
    ($s:expr, $e:ident, $body:expr) => {
        match $s {
            Engine::Reference($e) => $body,
            Engine::Candidate($e) => $body,
        }
    };
}
enum Engine {
    Reference(cpu_next_reference::WurliEngine),
    Candidate(openwurli_dsp::WurliEngine),
}
fn candidate_engine(sample_rate: f64) -> openwurli_dsp::WurliEngine {
    #[cfg(feature = "runtime-modes")]
    {
        let mode = if cfg!(feature = "engine-heavy") {
            openwurli_dsp::CircuitMode::Heavy
        } else {
            openwurli_dsp::CircuitMode::Fast
        };
        openwurli_dsp::WurliEngine::new_with_circuit_mode(sample_rate, mode)
    }
    #[cfg(not(feature = "runtime-modes"))]
    {
        openwurli_dsp::WurliEngine::new(sample_rate)
    }
}
impl Engine {
    fn new(variant: usize, s: &Scenario) -> Self {
        let mut engine = if variant == 0 {
            Self::Reference(cpu_next_reference::WurliEngine::new(s.sample_rate as f64))
        } else {
            Self::Candidate(candidate_engine(s.sample_rate as f64))
        };
        dispatch!(&mut engine, e, {
            e.ensure_buffer_capacity(s.block);
            e.set_noise_enabled(false);
            e.set_mlp_enabled(s.mlp);
        });
        engine.event(Event::Params(s.profile));
        dispatch!(&mut engine, e, e.warm_up());
        engine
    }
    fn event(&mut self, event: Event) {
        dispatch!(
            self,
            e,
            match event {
                Event::On(n, v) => e.note_on(n, v),
                Event::Off(n) => e.note_off(n),
                Event::Sustain(v) => e.set_sustain(v),
                Event::Depth(v) => e.set_tremolo_depth(v),
                Event::Reset => e.reset(),
                Event::Rate(sr) => e.set_sample_rate(sr as f64),
                Event::Params(profile) => {
                    let (v, t, sp, c, r) = match profile {
                        1 => (1.0, 1.0, 0.0, 2.0, 0.5),
                        2 => (0.25, 0.0, 1.0, 0.5, 2.0),
                        _ => (0.7, 0.5, 0.5, 1.0, 1.0),
                    };
                    e.set_volume(v);
                    e.set_tremolo_depth(t);
                    e.set_speaker_character(sp);
                    e.set_reed_decay(c);
                    e.set_hammer_hardness(c);
                    e.set_pickup_drive(c);
                    e.set_tremolo_response(r);
                    e.set_rail_sag(profile != 2);
                }
            }
        );
    }
    fn render(&mut self, out: &mut [f32]) {
        dispatch!(self, e, e.render(black_box(out)));
    }
    fn amp_snapshot(&self, at: usize, phase: &'static str) -> Snapshot {
        Snapshot::new(at, phase, dispatch!(self, e, e.power_amp_diag()))
    }
    fn guards(&self) -> u64 {
        dispatch!(self, e, e.nan_guard_fires())
    }
}
fn events(s: &Scenario) -> Vec<(usize, Event)> {
    let mut out = Vec::new();
    let frame = |seconds: f64| (seconds * s.sample_rate as f64) as usize;
    for &n in &s.notes {
        out.push((0, Event::On(n, s.velocity as f32 / 127.0)));
    }
    match s.pattern {
        Pattern::Hold => {}
        Pattern::Silence => {
            out.push((frame(0.2) + 1, Event::Depth(1.0)));
            out.push((frame(0.4) + 3, Event::Depth(0.0)));
        }
        Pattern::Lifecycle => {
            out.push((frame(0.081) + 1, Event::Sustain(true)));
            for &n in &s.notes {
                out.push((frame(0.143) + 3, Event::Off(n)));
            }
            for &n in &s.notes {
                out.push((frame(0.211) + 7, Event::On(n, s.velocity as f32 / 127.0)));
            }
            out.push((frame(0.249) + 1, Event::Params((s.profile + 1) % 3)));
            out.push((frame(0.321) + 5, Event::Sustain(false)));
            for &n in &s.notes {
                out.push((frame(0.389) + 1, Event::Off(n)));
            }
        }
        Pattern::RepeatedChord | Pattern::PedalBursts | Pattern::RepeatedDamp => {
            for beat in 0..4 {
                let start = frame(beat as f64 * 0.14);
                if beat > 0 {
                    for &n in &s.notes {
                        out.push((start + 3, Event::On(n, s.velocity as f32 / 127.0)));
                    }
                }
                if s.pattern == Pattern::PedalBursts {
                    out.push((start + frame(0.012) + 1, Event::Sustain(true)));
                }
                for &n in &s.notes {
                    out.push((start + frame(0.061) + 5, Event::Off(n)));
                }
                if s.pattern == Pattern::RepeatedDamp {
                    for &n in &s.notes {
                        out.push((start + frame(0.079) + 7, Event::Off(n)));
                    }
                }
                if s.pattern == Pattern::PedalBursts {
                    out.push((start + frame(0.110) + 7, Event::Sustain(false)));
                }
            }
        }
        Pattern::DepthSwitch => {
            out.push((frame(0.13) + 1, Event::Depth(1.0)));
            out.push((frame(0.27) + 3, Event::Depth(0.0)));
            out.push((frame(0.39) + 5, Event::Depth(0.37)));
            for &n in &s.notes {
                out.push((frame(0.51) + 7, Event::Off(n)));
            }
        }
        Pattern::ResetRates => {
            out.push((frame(0.20) + 1, Event::Reset));
            for &n in &s.notes {
                out.push((frame(0.23) + 3, Event::On(n, s.velocity as f32 / 127.0)));
            }
            out.push((frame(0.43) + 3, Event::Reset));
            out.push((
                frame(0.43) + 3,
                Event::Rate(if s.sample_rate < 88200 { 96000 } else { 48000 }),
            ));
            for &n in &s.notes {
                out.push((frame(0.47) + 1, Event::On(n, s.velocity as f32 / 127.0)));
            }
            out.push((frame(0.72) + 5, Event::Reset));
            out.push((frame(0.72) + 5, Event::Rate(44100)));
            for &n in &s.notes {
                out.push((frame(0.76) + 3, Event::On(n, s.velocity as f32 / 127.0)));
                out.push((frame(0.99) + 7, Event::Off(n)));
            }
        }
    }
    out.sort_by_key(|e| e.0);
    out
}
#[derive(Clone, Copy, Serialize)]
struct Callback {
    start_sample: usize,
    frames: usize,
    callback_ns: u64,
    render_ns: u64,
    event_ns: u64,
    events: usize,
    render_calls: usize,
}
struct Render {
    samples: Vec<f32>,
    guards: u64,
    callbacks: Vec<Callback>,
    rate_segments: Vec<(usize, u32)>,
    amp_snapshots: Vec<Snapshot>,
}
fn render(s: &Scenario, variant: usize, timed: bool, collect_diagnostics: bool) -> Render {
    assert!(!(timed && collect_diagnostics));
    let mut engine = Engine::new(variant, s);
    let schedule = events(s);
    let length = (s.seconds * s.sample_rate as f64) as usize;
    assert!(schedule.last().is_none_or(|e| e.0 < length));
    let mut output = vec![0.0; length];
    let mut callbacks = Vec::with_capacity(length.div_ceil(s.block));
    let mut rate_segments = Vec::with_capacity(4);
    rate_segments.push((0, s.sample_rate));
    let mut amp_snapshots = Vec::new();
    if collect_diagnostics {amp_snapshots.push(engine.amp_snapshot(0, "after_initialization"));}
    let (mut at, mut event_index) = (0, 0);
    while at < length {
        let begin = at;
        let callback_end = (begin + s.block).min(length);
        let mut row = Callback {
            start_sample: begin,
            frames: callback_end - begin,
            callback_ns: 0,
            render_ns: 0,
            event_ns: 0,
            events: 0,
            render_calls: 0,
        };
        let callback_start = timed.then(Instant::now);
        while at < callback_end {
            let event_start = event_index;
            while event_index < schedule.len() && schedule[event_index].0 == at {
                event_index += 1;
            }
            if event_index > event_start {
                let start = timed.then(Instant::now);
                for &(_, event) in &schedule[event_start..event_index] {
                    engine.event(event);
                    if collect_diagnostics {
                        match event {
                            Event::Reset => amp_snapshots.push(engine.amp_snapshot(at, "after_reset")),
                            Event::Rate(_) => amp_snapshots.push(engine.amp_snapshot(at, "after_rate_change")),
                            _ => {}
                        }
                    }
                    if let Event::Rate(sr) = event {
                        rate_segments.push((at, sr));
                    }
                }
                if let Some(start) = start {
                    row.event_ns += start.elapsed().as_nanos() as u64;
                }
                row.events += event_index - event_start;
            }
            let stop = callback_end.min(schedule.get(event_index).map_or(length, |e| e.0));
            assert!(stop > at);
            let start = timed.then(Instant::now);
            engine.render(&mut output[at..stop]);
            if let Some(start) = start {
                row.render_ns += start.elapsed().as_nanos() as u64;
            }
            if collect_diagnostics {amp_snapshots.push(engine.amp_snapshot(stop, "after_render_segment"));}
            black_box(&output[at..stop]);
            row.render_calls += 1;
            at = stop;
        }
        if let Some(start) = callback_start {
            row.callback_ns = start.elapsed().as_nanos() as u64;
            callbacks.push(row);
        }
    }
    Render {
        samples: output,
        guards: engine.guards(),
        callbacks,
        rate_segments,
        amp_snapshots,
    }
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
                        pattern: Pattern::Lifecycle,
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
            pattern: Pattern::Lifecycle,
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
        pattern: Pattern::Lifecycle,
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
        pattern: Pattern::Hold,
        seconds: 8.0,
    });
    out.extend(extra_audio_scenarios(full));
    out
}
fn case(
    name: &str,
    sr: u32,
    block: usize,
    count: usize,
    pattern: Pattern,
    mlp: bool,
    profile: u8,
    seconds: f64,
) -> Scenario {
    Scenario {
        name: format!("{name}-{count}-sr-{sr}-block-{block}-mlp-{mlp}"),
        sample_rate: sr,
        block,
        mlp,
        notes: (0..count).map(|i| 33 + (i * 7 % 64) as u8).collect(),
        velocity: 100,
        profile,
        pattern,
        seconds,
    }
}
fn extra_audio_scenarios(full: bool) -> Vec<Scenario> {
    let mut out = Vec::new();
    for sr in if full {
        vec![44100, 48000, 96000]
    } else {
        vec![48000]
    } {
        for mlp in if full { vec![false, true] } else { vec![true] } {
            for count in [1, 6, 12] {
                for (name, pattern, profile) in [
                    ("repeat-chord", Pattern::RepeatedChord, 0),
                    ("pedal-bursts", Pattern::PedalBursts, 1),
                    ("zero-to-active", Pattern::DepthSwitch, 2),
                    ("repeated-damp", Pattern::RepeatedDamp, 0),
                    ("release-long", Pattern::RepeatedDamp, 1),
                ] {
                    out.push(case(
                        name,
                        sr,
                        127,
                        count,
                        pattern,
                        mlp,
                        profile,
                        if name == "release-long" { 4.0 } else { 0.8 },
                    ));
                }
            }
            out.push(case(
                "reset-rate-change",
                sr,
                257,
                6,
                Pattern::ResetRates,
                mlp,
                0,
                1.3,
            ));
        }
    }
    out.push(case(
        "silence-zero-reference",
        48000,
        64,
        0,
        Pattern::Silence,
        false,
        2,
        0.7,
    ));
    out
}
fn cpu_scenarios(full: bool) -> Vec<Scenario> {
    let mut out = Vec::new();
    for sr in if full {
        vec![44100, 48000, 96000]
    } else {
        vec![48000]
    } {
        for block in if full { vec![64, 256] } else { vec![256] } {
            for count in [1, 6, 12, 32, 64] {
                out.push(case("hold", sr, block, count, Pattern::Hold, true, 0, 0.65));
            }
            for count in [1, 6, 12] {
                for (name, pattern, profile) in [
                    ("chord-release", Pattern::RepeatedChord, 0),
                    ("pedal-release", Pattern::PedalBursts, 0),
                    ("depth-zero", Pattern::Hold, 2),
                    ("zero-to-active", Pattern::DepthSwitch, 2),
                ] {
                    out.push(case(name, sr, block, count, pattern, true, profile, 0.65));
                }
            }
        }
    }
    out
}
fn percentile(values: &[f64], q: f64) -> f64 {
    assert!(!values.is_empty());
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[((sorted.len() - 1) as f64 * q).ceil() as usize]
}
fn distribution(values: &[f64]) -> Value {
    if values.is_empty() {
        return Value::Null;
    }
    json!({"count":values.len(),"median":percentile(values,0.5),"p95":percentile(values,0.95),"p99":percentile(values,0.99),"max":percentile(values,1.0),"min":percentile(values,0.0)})
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
    for &x in samples {
        w.write_sample(x)?;
    }
    w.finalize()?;
    Ok(())
}
fn engine_mode() -> &'static str {
    if cfg!(feature = "engine-heavy") {
        "heavy: melange preamp and generated power amp"
    } else {
        "fast: legacy preamp and behavioral power amp"
    }
}
fn enabled_candidates() -> Vec<&'static str> {
    let mut out = Vec::new();
    for (enabled, name) in [
        (
            cfg!(feature = "cpu-study-heavy-matrices"),
            "cpu-study-heavy-matrices",
        ),
        (
            cfg!(feature = "cpu-study-preamp-reuse"),
            "cpu-study-preamp-reuse",
        ),
        (
            cfg!(feature = "cpu-study-tremolo-zero"),
            "cpu-study-tremolo-zero",
        ),
        (
            cfg!(feature = "cpu-study-damper-recurrence"),
            "cpu-study-damper-recurrence",
        ),
        (
            cfg!(feature = "cpu-study-amp-transfer"),
            "cpu-study-amp-transfer",
        ),
    ] {
        if enabled {
            out.push(name);
        }
    }
    out
}
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    let value = |name: &str| args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone());
    let full = value("--mode").as_deref() == Some("full");
    let repeats = value("--repeats").unwrap_or("11".into()).parse::<usize>()?;
    assert!(repeats >= 3);
    let limits = Limits {
        peak_dbfs: value("--max-peak-dbfs").unwrap_or("-60".into()).parse()?,
        rms_relative_db: value("--max-rms-relative-db")
            .unwrap_or("-60".into())
            .parse()?,
    };
    assert!(limits.peak_dbfs.is_finite() && limits.rms_relative_db.is_finite());
    let require_exact = args.iter().any(|a| a == "--require-exact");
    let export_all = args.iter().any(|a| a == "--export-all");
    let filter = value("--scenario-filter");
    let selected = |s: &Scenario| filter.as_ref().is_none_or(|f| f.split(',').any(|part| s.name.contains(part.trim())));
    let audio_cases: Vec<_> = if args.iter().any(|a| a == "--cpu-only") {
        Vec::new()
    } else {
        audio_scenarios(full).into_iter().filter(selected).collect()
    };
    let cpu_cases: Vec<_> = if args.iter().any(|a| a == "--audio-only") {
        Vec::new()
    } else {
        cpu_scenarios(full).into_iter().filter(selected).collect()
    };
    assert!(
        !audio_cases.is_empty() || !cpu_cases.is_empty(),
        "No scenarios selected"
    );
    let coverage = json!({"engine":engine_mode(),"candidate_features":enabled_candidates(),"candidate_runtime_modes":cfg!(feature="runtime-modes"),"reference_runtime_modes":false,"candidate_initial_mode":if cfg!(feature="engine-heavy"){"heavy"}else{"fast"},"audio_cases":audio_cases,"cpu_cases":cpu_cases,"audio_count":audio_cases.len(),"cpu_count":cpu_cases.len(),"full_mode":full,"scenario_filter":filter});
    if args.iter().any(|a| a == "--list") {
        println!("{}", serde_json::to_string_pretty(&coverage)?);
        return Ok(());
    }
    let output = value("--output").ok_or("--output is required")?;
    let dir = Path::new(&output);
    fs::create_dir_all(dir)?;
    fs::write(
        dir.join("coverage.json"),
        serde_json::to_vec_pretty(&coverage)?,
    )?;
    if export_all {
        fs::create_dir_all(dir.join("waveforms"))?;
    }
    let (mut all_exact, mut all_bounds, mut finite, mut total_samples) = (true, true, true, 0usize);
    let mut audio = Vec::new();
    let mut all_diagnostics_exact = true;
    let mut all_diagnostics_safe = true;
    let mut diagnostics_review_required = false;
    for (i, s) in audio_cases.iter().enumerate() {
        let a = render(s, 0, false, true);
        let b = render(s, 1, false, true);
        assert_eq!(a.rate_segments, b.rate_segments);
        let result = metrics::compare(
            &a.samples,
            &b.samples,
            &a.rate_segments,
            limits,
            a.guards,
            b.guards,
        );
        let amp_diagnostics = diagnostics::compare(&a.amp_snapshots, &b.amp_snapshots);
        all_diagnostics_exact &= amp_diagnostics["trajectory_bit_exact"] == true;
        all_diagnostics_safe &= amp_diagnostics["finite_and_no_counter_increase"] == true;
        diagnostics_review_required |= amp_diagnostics["review_required"] == true;
        if export_all {
            fs::write(dir.join("waveforms").join(format!("{}-power-amp-diagnostics.json", s.name)), serde_json::to_vec(&json!({"reference":a.amp_snapshots,"candidate":b.amp_snapshots}))?)?;
        }
        all_exact &= result.exact;
        all_bounds &= result.within_bounds;
        finite &= result.full.nonfinite_pairs == 0 && a.guards == 0 && b.guards == 0;
        total_samples += a.samples.len();
        let waveforms = if export_all {
            let mut paths = Vec::new();
            for (variant, r) in [("reference", &a), ("candidate", &b)] {
                let relative = format!("waveforms/{}-{variant}.f32le", s.name);
                let bytes: Vec<u8> = r.samples.iter().flat_map(|x| x.to_le_bytes()).collect();
                fs::write(dir.join(&relative), bytes)?;
                paths.push(relative);
            }
            Some(paths)
        } else {
            None
        };
        if (s.name.starts_with("long-")
            || s.name.starts_with("pedal-bursts-6")
            || s.name.starts_with("zero-to-active-6"))
            && s.pattern != Pattern::ResetRates
        {
            for (variant, r) in [("reference", &a), ("candidate", &b)] {
                wav(
                    &dir.join(format!("{}-{variant}.wav", s.name)),
                    s.sample_rate,
                    &r.samples,
                )?;
            }
            let difference: Vec<f32> = a
                .samples
                .iter()
                .zip(&b.samples)
                .map(|(a, b)| b - a)
                .collect();
            wav(
                &dir.join(format!("{}-difference.wav", s.name)),
                s.sample_rate,
                &difference,
            )?;
        }
        audio.push(json!({"scenario":s,"events":events(s),"rate_segments":a.rate_segments,"comparison":result,"power_amp_diagnostics":amp_diagnostics,"waveforms":waveforms}));
        if i % 16 == 0 {
            eprintln!("{} audio {}/{}", engine_mode(), i + 1, audio_cases.len());
        }
    }
    fs::write(dir.join("audio.json"), serde_json::to_vec_pretty(&audio)?)?;
    let mut cpu = Vec::new();
    let mut cpu_valid = true;
    for s in &cpu_cases {
        for v in 0..2 {
            black_box(render(s, v, false, false));
        }
        let mut raw: [Vec<Value>; 2] = std::array::from_fn(|_| Vec::new());
        let mut render_times: [Vec<f64>; 2] = std::array::from_fn(|_| Vec::new());
        let mut callback_times = render_times.clone();
        let mut event_times = render_times.clone();
        let mut tail_times = render_times.clone();
        let mut orders = Vec::new();
        for repeat in 0..repeats {
            let order = if repeat % 2 == 0 { [0, 1] } else { [1, 0] };
            orders.push(order);
            for v in order {
                let r = render(s, v, true, false);
                let n = r.samples.len() as f64;
                let render_ns = r.callbacks.iter().map(|r| r.render_ns).sum::<u64>();
                let event_ns = r.callbacks.iter().map(|r| r.event_ns).sum::<u64>();
                let callback_ns = r.callbacks.iter().map(|r| r.callback_ns).sum::<u64>();
                let event_count = r.callbacks.iter().map(|r| r.events).sum::<usize>();
                let durations: Vec<f64> = r
                    .callbacks
                    .iter()
                    .map(|r| r.callback_ns as f64 / 1000.0)
                    .collect();
                let event_durations: Vec<f64> = r
                    .callbacks
                    .iter()
                    .filter(|r| r.events > 0)
                    .map(|r| r.event_ns as f64 / 1000.0)
                    .collect();
                let budgets: Vec<f64> = r
                    .callbacks
                    .iter()
                    .map(|r| r.callback_ns as f64 / (r.frames as f64 / s.sample_rate as f64 * 1e9))
                    .collect();
                render_times[v].push(render_ns as f64 / n);
                callback_times[v].push(callback_ns as f64 / n);
                event_times[v].push(event_ns as f64 / event_count.max(1) as f64);
                tail_times[v].push(percentile(&durations, 0.99));
                let nonfinite = r.samples.iter().filter(|x| !x.is_finite()).count();
                cpu_valid &= nonfinite == 0 && r.guards == 0;
                raw[v].push(json!({"repeat":repeat,"samples":r.samples.len(),"render_ns":render_ns,"event_ns":event_ns,"events":event_count,"callback_ns":callback_ns,"render_ns_per_sample":render_ns as f64/n,"callback_ns_per_sample":callback_ns as f64/n,"event_ns_per_event":if event_count>0{Some(event_ns as f64/event_count as f64)}else{None},"callback_duration_us":distribution(&durations),"callback_deadline_fraction":distribution(&budgets),"callback_event_cost_us":distribution(&event_durations),"nan_guards":r.guards,"nonfinite_samples":nonfinite,"callbacks":r.callbacks}));
            }
        }
        let mut variants = Vec::new();
        for v in 0..2 {
            variants.push(json!({"variant":NAMES[v],"render_ns_per_sample":distribution(&render_times[v]),"callback_ns_per_sample":distribution(&callback_times[v]),"event_ns_per_event":distribution(&event_times[v]),"per_repeat_callback_p99_us":distribution(&tail_times[v]),"raw":raw[v]}));
        }
        let mut ratios = Vec::new();
        for (metric, times) in [
            ("render_ns_per_sample", &render_times),
            ("callback_ns_per_sample", &callback_times),
            ("event_ns_per_event", &event_times),
            ("callback_p99_us", &tail_times),
        ] {
            let reference = percentile(&times[0], 0.5);
            let candidate = percentile(&times[1], 0.5);
            let paired: Vec<f64> = times[0].iter().zip(&times[1]).map(|(a, b)| b / a).collect();
            ratios.push(json!({"metric":metric,"reference_median":reference,"candidate_median":candidate,"median_time_ratio":candidate/reference,"cpu_reduction_percent":100.0*(1.0-candidate/reference),"paired_time_ratios":paired,"paired_ratio_summary":distribution(&paired)}));
        }
        cpu.push(json!({"scenario":s,"events":events(s),"order":orders,"variants":variants,"ratios":ratios}));
        eprintln!("cpu {} complete", s.name);
    }
    fs::write(dir.join("cpu.json"), serde_json::to_vec_pretty(&cpu)?)?;
    let report = json!({"schema":2,"engine":engine_mode(),"candidate_features":enabled_candidates(),"candidate_runtime_modes":cfg!(feature="runtime-modes"),"reference_runtime_modes":false,"candidate_initial_mode":if cfg!(feature="engine-heavy"){"heavy"}else{"fast"},"mode":if full{"full"}else{"quick"},"repeats":repeats,"limits":limits,"require_exact":require_exact,"audio_scenarios":audio.len(),"cpu_scenarios":cpu.len(),"audio_samples_per_pair":total_samples,"audio_verified":!audio.is_empty(),"power_amp_diagnostics_exact":if audio.is_empty(){None}else{Some(all_diagnostics_exact)},"power_amp_no_counter_increase":if audio.is_empty(){None}else{Some(all_diagnostics_safe)},"power_amp_diagnostics_review_required":diagnostics_review_required,"all_audio_exact":if audio.is_empty(){None}else{Some(all_exact)},"all_audio_within_bounds":if audio.is_empty(){None}else{Some(all_bounds)},"audio_finite_guards_clear":finite,"cpu_finite_guards_clear":cpu_valid,"db_null_convention":"Zero error is -infinity dB and serialized null; zero reference is explicit and any nonzero residual fails.","audio":audio,"cpu":cpu});
    fs::write(dir.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
    eprintln!("{}", dir.join("report.json").display());
    if !finite || !cpu_valid || !all_bounds || !all_diagnostics_safe || (require_exact && (!all_exact || !all_diagnostics_exact)) {
        return Err(
            "Comparison failed; inspect full and sliding-window residuals/guards in report.json"
                .into(),
        );
    }
    Ok(())
}
