//! Temporary, standalone mode-switch evidence. No CPU timing or audio-quality verdict.
use openwurli_dsp::{CircuitMode, WurliEngine};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static DEALLOCS: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
static DEALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
struct CountedAllocator;
#[global_allocator]
static ALLOCATOR: CountedAllocator = CountedAllocator;

// Forward the identical layouts/pointers to System; counters allocate nothing.
unsafe impl GlobalAlloc for CountedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if COUNTING.load(Ordering::Relaxed) {
            DEALLOCS.fetch_add(1, Ordering::Relaxed);
            DEALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            REALLOCS.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
            DEALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[derive(Clone, Copy, Default, Serialize)]
struct Allocations {
    allocations: u64,
    deallocations: u64,
    reallocations: u64,
    allocated_bytes: u64,
    deallocated_bytes: u64,
}
impl Allocations {
    fn add(&mut self, other: Self) {
        self.allocations += other.allocations;
        self.deallocations += other.deallocations;
        self.reallocations += other.reallocations;
        self.allocated_bytes += other.allocated_bytes;
        self.deallocated_bytes += other.deallocated_bytes;
    }
    fn any(self) -> bool {
        self.allocations != 0 || self.deallocations != 0 || self.reallocations != 0
    }
}
struct CountingGuard;
impl Drop for CountingGuard {
    fn drop(&mut self) {
        COUNTING.store(false, Ordering::SeqCst);
    }
}
fn counted(action: impl FnOnce()) -> Allocations {
    ALLOCS.store(0, Ordering::Relaxed);
    DEALLOCS.store(0, Ordering::Relaxed);
    REALLOCS.store(0, Ordering::Relaxed);
    ALLOC_BYTES.store(0, Ordering::Relaxed);
    DEALLOC_BYTES.store(0, Ordering::Relaxed);
    assert!(!COUNTING.swap(true, Ordering::SeqCst));
    let guard = CountingGuard;
    action();
    drop(guard);
    Allocations {
        allocations: ALLOCS.load(Ordering::Relaxed),
        deallocations: DEALLOCS.load(Ordering::Relaxed),
        reallocations: REALLOCS.load(Ordering::Relaxed),
        allocated_bytes: ALLOC_BYTES.load(Ordering::Relaxed),
        deallocated_bytes: DEALLOC_BYTES.load(Ordering::Relaxed),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Held,
    ReleaseReturn,
}
impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Held => "held-reversals",
            Self::ReleaseReturn => "release-silence-return",
        }
    }
}
#[derive(Clone, Copy)]
enum Action {
    Mode(CircuitMode),
    Release,
}
#[derive(Clone, Copy)]
struct Event {
    at: usize,
    action: Action,
}
struct Timeline {
    initial: CircuitMode,
    events: Vec<Event>,
    length: usize,
    audition_anchor: usize,
}
fn timeline(kind: Kind, sr: u32, hidden: f64, settling_ms: f64) -> Timeline {
    let frame = |s: f64| (s * sr as f64).round() as usize;
    match kind {
        Kind::Held => {
            let sequence = [
                (0.0, CircuitMode::Heavy),
                (0.006, CircuitMode::Fast),
                (0.010, CircuitMode::Heavy),
                (0.080, CircuitMode::Fast),
                (0.120, CircuitMode::Heavy),
                (0.170, CircuitMode::Fast),
                (0.181, CircuitMode::Heavy),
            ];
            Timeline {
                initial: CircuitMode::Fast,
                events: sequence
                    .into_iter()
                    .map(|(delta, mode)| Event {
                        at: frame(hidden + delta),
                        action: Action::Mode(mode),
                    })
                    .collect(),
                length: frame(hidden + 0.420)
                    .max(frame(hidden + 0.181 + settling_ms / 1000.0 + 0.020 + 0.120)),
                audition_anchor: frame(hidden),
            }
        }
        Kind::ReleaseReturn => Timeline {
            initial: CircuitMode::Heavy,
            events: vec![
                Event {
                    at: frame(0.100),
                    action: Action::Mode(CircuitMode::Fast),
                },
                Event {
                    at: frame(0.150),
                    action: Action::Release,
                },
                Event {
                    at: frame(0.150 + hidden),
                    action: Action::Mode(CircuitMode::Heavy),
                },
            ],
            length: frame(0.150 + hidden + 0.420)
                .max(frame(0.150 + hidden + settling_ms / 1000.0 + 0.020 + 0.120)),
            audition_anchor: frame(0.150 + hidden),
        },
    }
}
fn name(mode: CircuitMode) -> &'static str {
    match mode {
        CircuitMode::Fast => "Fast",
        CircuitMode::Heavy => "Heavy",
    }
}

#[derive(Serialize)]
struct AllocationEvent {
    start_frame: usize,
    end_frame: usize,
    counts: Allocations,
}
#[derive(Serialize)]
struct ModeEvent {
    frame: usize,
    target: &'static str,
    allocations: Allocations,
    held_before: usize,
    held_after: usize,
    active_before: usize,
    active_after: usize,
}
#[derive(Serialize)]
struct RenderReport {
    initial_mode: &'static str,
    final_mode: &'static str,
    held_final: usize,
    active_final: usize,
    frames: usize,
    finite: bool,
    peak: f64,
    total_render_allocations: Allocations,
    render_calls_with_allocations: usize,
    allocation_events: Vec<AllocationEvent>,
    mode_events: Vec<ModeEvent>,
    amplifier_diagnostics_before: (u64, u64, f64),
    amplifier_diagnostics_after: (u64, u64, f64),
    nan_guard_count: u64,
}
struct Rendered {
    samples: Vec<f32>,
    report: RenderReport,
}
fn render(sr: u32, voices: usize, plan: &Timeline, fixed: Option<CircuitMode>) -> Rendered {
    let initial = fixed.unwrap_or(plan.initial);
    let mut engine = WurliEngine::new_with_circuit_mode(sr as f64, initial);
    engine.warm_up();
    engine.set_noise_enabled(false);
    engine.set_volume(0.5);
    engine.set_tremolo_depth(0.8);
    engine.set_speaker_character(0.2);
    // Identical note/velocity/seed sequence for every matched stream. No noise.
    // For the silence-return case use only damped keys (33..91); otherwise
    // an intentionally undamped top key would masquerade as a stale tail.
    let span = if plan
        .events
        .iter()
        .any(|event| matches!(event.action, Action::Release))
    {
        58
    } else {
        63
    };
    let notes: Vec<_> = (0..voices)
        .map(|i| {
            if voices == 1 {
                60
            } else {
                33 + (i * span / voices.saturating_sub(1).max(1)) as u8
            }
        })
        .collect();
    for &note in &notes {
        engine.note_on(note, 0.9);
    }
    let mut samples = vec![0.0; plan.length];
    let mut events = Vec::new();
    let mut allocation_events = Vec::new();
    let mut allocation_calls = 0;
    let mut total = Allocations::default();
    let diag_before = engine.power_amp_diag();
    let blocks = [1, 15, 127, 257, 1024];
    let mut position = 0;
    let mut block = 0;
    let mut next_event = 0;
    while position < samples.len() {
        while next_event < plan.events.len() && plan.events[next_event].at == position {
            match plan.events[next_event].action {
                Action::Mode(mode) if fixed.is_none() => {
                    let held_before = engine.held_voice_count();
                    let active_before = engine.active_voice_count();
                    let allocations = counted(|| engine.set_circuit_mode(mode));
                    events.push(ModeEvent {
                        frame: position,
                        target: name(mode),
                        allocations,
                        held_before,
                        active_before,
                        held_after: engine.held_voice_count(),
                        active_after: engine.active_voice_count(),
                    });
                }
                Action::Release => {
                    for &note in &notes {
                        engine.note_off(note);
                    }
                }
                _ => {}
            }
            next_event += 1;
        }
        let next_boundary = plan
            .events
            .get(next_event)
            .map_or(samples.len(), |event| event.at);
        let end = (position + blocks[block % blocks.len()])
            .min(next_boundary)
            .min(samples.len());
        let counts = counted(|| engine.render(&mut samples[position..end]));
        total.add(counts);
        if counts.any() {
            allocation_calls += 1;
            // Bound report size; totals remain complete even if fault recovery is frequent.
            if allocation_events.len() < 256 {
                allocation_events.push(AllocationEvent {
                    start_frame: position,
                    end_frame: end,
                    counts,
                });
            }
        }
        block += 1;
        position = end;
    }
    let peak = samples
        .iter()
        .filter(|v| v.is_finite())
        .map(|v| (*v as f64).abs())
        .fold(0.0, f64::max);
    Rendered {
        report: RenderReport {
            initial_mode: name(initial),
            final_mode: name(engine.circuit_mode()),
            held_final: engine.held_voice_count(),
            active_final: engine.active_voice_count(),
            frames: samples.len(),
            finite: samples.iter().all(|x| x.is_finite()),
            peak,
            total_render_allocations: total,
            render_calls_with_allocations: allocation_calls,
            allocation_events,
            mode_events: events,
            amplifier_diagnostics_before: diag_before,
            amplifier_diagnostics_after: engine.power_amp_diag(),
            nan_guard_count: engine.nan_guard_fires(),
        },
        samples,
    }
}

fn max_step(samples: &[f32], start: usize, end: usize) -> f64 {
    (start.max(1)..end.min(samples.len()))
        .map(|i| (samples[i] as f64 - samples[i - 1] as f64).abs())
        .fold(0.0, f64::max)
}
fn peak(samples: &[f32]) -> f64 {
    samples
        .iter()
        .map(|x| (*x as f64).abs())
        .fold(0.0, f64::max)
}
fn difference(a: &[f32], b: &[f32]) -> Value {
    let mut signal = 0.0;
    let mut error = 0.0;
    let mut maximum = 0.0f64;
    for (&a, &b) in a.iter().zip(b) {
        let d = a as f64 - b as f64;
        maximum = maximum.max(d.abs());
        signal += (b as f64) * (b as f64);
        error += d * d;
    }
    json!({"peak_absolute":maximum,"rms_absolute":(error/a.len().max(1) as f64).sqrt(),"rms_relative":if signal>0.0 {Some((error/signal).sqrt())} else {None}})
}
fn transient_reports(
    sr: u32,
    plan: &Timeline,
    switched: &[f32],
    fast: &[f32],
    heavy: &[f32],
) -> Vec<Value> {
    let window = (sr as f64 * 0.040).round() as usize;
    plan.events.iter().filter_map(|event|match event.action {
        Action::Mode(target)=>{
            let at=event.at;let start=at.saturating_sub(window);let end=(at+window).min(switched.len());
            let before=max_step(switched,start,at);let after=max_step(switched,at,end);
            let reference=if target==CircuitMode::Heavy {heavy}else{fast};
            Some(json!({"frame":at,"target":name(target),"edge_step_absolute":max_step(switched,at,at+1),"preceding_40ms_max_step":before,"following_40ms_max_step":after,"after_before_step_ratio":if before>0.0 {Some(after/before)} else {None},"stable_fast_following_max_step":max_step(fast,at,end),"stable_heavy_following_max_step":max_step(heavy,at,end),"following_40ms_peak":peak(&switched[at..end]),"following_40ms_difference_vs_continuous_target":difference(&switched[at..end],&reference[at..end])}))
        }, _=>None,
    }).collect()
}
fn recovery_report(
    sr: u32,
    settling_ms: f64,
    plan: &Timeline,
    switched: &[f32],
    heavy: &[f32],
) -> Value {
    let (at, target) = plan
        .events
        .iter()
        .rev()
        .find_map(|event| match event.action {
            Action::Mode(mode) => Some((event.at, mode)),
            _ => None,
        })
        .unwrap();
    assert_eq!(target, CircuitMode::Heavy);
    let window = (sr as f64 * 0.020).round() as usize;
    let mut windows = Vec::new();
    let mut bounds = Vec::new();
    let settling_frames = (sr as f64 * settling_ms / 1000.0).round() as usize;
    // This descriptive recovery view excludes settling and fade. The separate
    // released-tail gate includes every sample from the command through the end.
    let mut start = at + settling_frames + window;
    assert!(switched.len() >= start + (sr as f64 * 0.100).round() as usize);
    while start + window <= switched.len() {
        let end = start + window;
        let diff = difference(&switched[start..end], &heavy[start..end]);
        let delta = diff["peak_absolute"].as_f64().unwrap_or(f64::INFINITY);
        bounds.push((start, delta));
        windows.push(json!({"start_ms_after_final_switch":1000.0*(start-at) as f64/sr as f64,"switch_peak":peak(&switched[start..end]),"continuous_heavy_peak":peak(&heavy[start..end]),"difference":diff}));
        start = end;
    }
    let recovery = bounds
        .iter()
        .enumerate()
        .find(|(i, _)| {
            bounds[*i..].len() >= 3 && bounds[*i..].iter().all(|(_, delta)| *delta < 1e-4)
        })
        .map(|(_, (start, _))| 1000.0 * (*start - at) as f64 / sr as f64);
    json!({"reference":"Heavy run continuously from the same note/parameter events","analysis_settling_ms":settling_ms,"analysis_fade_ms":20.0,"first_window_start_ms_after_command":settling_ms+20.0,"threshold_description":"First complete 20 ms window with peak difference below 1e-4 (-80 dBFS), remaining there for all subsequent complete windows, with at least 3 windows. Descriptive only; not an audibility gate.","observed_recovery_ms":recovery,"windows":windows})
}
/// Only released/noise-off return cases have a silence-reference gate. Fast
/// and Heavy held-note trajectories are intentionally different circuit models.
fn released_tail_gate(kind: Kind, plan: &Timeline, switched: &[f32], heavy: &[f32]) -> Value {
    if kind != Kind::ReleaseReturn {
        return json!({"applicable":false,"reason":"Held-note mode changes are different model trajectories; waveform identity is not an acceptance test."});
    }
    let start = plan.audition_anchor;
    let delta = difference(&switched[start..], &heavy[start..]);
    let residual_peak = delta["peak_absolute"].as_f64().unwrap_or(f64::INFINITY);
    let finite = switched[start..]
        .iter()
        .chain(&heavy[start..])
        .all(|value| value.is_finite());
    json!({"applicable":true,"passed":finite && residual_peak <= 0.001,"start_frame":start,"end_frame":switched.len(),"scope":"Entire activated interval from main return command, including preparation, fade and all recorded post-fade samples. Noise disabled; all damped notes released before the hidden interval.","reference":"Continuously Heavy matched MIDI/parameter timeline","absolute_residual_peak_limit":0.001,"absolute_residual_peak_limit_dbfs":-60.0,"finite":finite,"switch_peak":peak(&switched[start..]),"continuous_heavy_peak":peak(&heavy[start..]),"difference":delta,"relative_rms_is_gate":false})
}

fn activation_observation(
    sr: u32,
    settling_ms: f64,
    plan: &Timeline,
    switched: &[f32],
    heavy: &[f32],
) -> Value {
    let last_command = plan
        .events
        .iter()
        .rev()
        .find_map(|event| match event.action {
            Action::Mode(CircuitMode::Heavy) => Some(event.at),
            _ => None,
        })
        .unwrap();
    let settling_frames = (sr as f64 * settling_ms / 1000.0).round() as usize;
    let fade_frames = (sr as f64 * 0.020).round() as usize;
    let fade_start = last_command + settling_frames;
    let fade_end = fade_start + fade_frames;
    let observation_end = (fade_end + (sr as f64 * 0.100).round() as usize).min(switched.len());
    json!({"final_heavy_command_frame":last_command,"assumed_settling_end_frame":fade_start,"assumed_fade_end_frame":fade_end,"observed_post_fade_ms":1000.0*(switched.len()-fade_end) as f64/sr as f64,"preparation_peak":peak(&switched[last_command..fade_start]),"fade_peak":peak(&switched[fade_start..fade_end]),"first_100ms_after_fade_peak":peak(&switched[fade_end..observation_end]),"fade_max_sample_step":max_step(switched,fade_start,fade_end),"stable_heavy_same_fade_window_max_step":max_step(heavy,fade_start,fade_end),"first_100ms_after_fade_difference":difference(&switched[fade_end..observation_end],&heavy[fade_end..observation_end]),"settling_parameter_scope":"Analysis assumption only; does not configure the DSP. Must match the tested engine's settling duration.","held_reversals":"Final Heavy request remains uninterrupted through preparation, fade, and at least 100ms afterward; earlier requests intentionally cancel preparation."})
}

fn wav(path: &Path, samples: &[f32], sr: u32) -> Result<()> {
    let mut writer = hound::WavWriter::create(
        path,
        hound::WavSpec {
            channels: 1,
            sample_rate: sr,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )?;
    for &sample in samples {
        writer.write_sample(sample)?;
    }
    writer.finalize()?;
    Ok(())
}
fn verify_source(provenance: &Value) -> Result<()> {
    let root = Path::new(provenance["root"].as_str().ok_or("Missing source root")?);
    for (path, expected) in provenance["source_git_blob_hashes"]
        .as_object()
        .ok_or("Missing source hashes")?
    {
        let output = Command::new("git")
            .args(["hash-object", "--", path])
            .current_dir(root)
            .output()?;
        let actual = String::from_utf8(output.stdout)?;
        if !output.status.success() || Some(actual.trim()) != expected.as_str() {
            return Err(format!(
                "Source changed since compilation: {path}; rebuild before recording evidence"
            )
            .into());
        }
    }
    Ok(())
}
fn command(program: &str, args: &[&str]) -> Option<String> {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|x| x.status.success())
        .map(|x| String::from_utf8_lossy(&x.stdout).trim().to_owned())
}
fn main() -> Result<()> {
    let args: Vec<_> = env::args().collect();
    let mut output = PathBuf::from("/tmp/openwurli-mode-switch");
    let mut rates = vec![44100u32, 48000, 96000];
    let mut voices = vec![0usize, 1, 6, 12, 32];
    let mut kinds = vec![Kind::Held, Kind::ReleaseReturn];
    let mut hidden = 1.0f64;
    let mut settling_ms = 100.0f64;
    let mut i = 1;
    while i < args.len() {
        let flag = &args[i];
        if flag == "--help" {
            println!(
                "cpu-mode-switch [--output DIR] [--rates 44100,48000,96000] [--voices 0,1,6,12,32] [--kind held|release-return|both] [--hidden-seconds 1.0] [--settling-ms 100]\nSettling is an analysis assumption, not a DSP setting. Separate each option and value with a space. No CPU timings are collected."
            );
            return Ok(());
        }
        let value = args.get(i + 1).ok_or("Missing argument value")?;
        match flag.as_str() {
            "--output" => output = value.into(),
            "--rates" => {
                rates = value
                    .split(',')
                    .map(str::parse)
                    .collect::<std::result::Result<_, _>>()?
            }
            "--voices" => {
                voices = value
                    .split(',')
                    .map(str::parse)
                    .collect::<std::result::Result<_, _>>()?
            }
            "--kind" => {
                kinds = match value.as_str() {
                    "held" => vec![Kind::Held],
                    "release-return" => vec![Kind::ReleaseReturn],
                    "both" => vec![Kind::Held, Kind::ReleaseReturn],
                    _ => return Err("Unknown kind".into()),
                }
            }
            "--hidden-seconds" => hidden = value.parse()?,
            "--settling-ms" => settling_ms = value.parse()?,
            _ => return Err(format!("Unknown option {flag}").into()),
        };
        i += 2;
    }
    if rates.is_empty()
        || voices.is_empty()
        || rates.iter().any(|x| ![44100, 48000, 96000].contains(x))
        || voices.iter().any(|x| ![0, 1, 6, 12, 32].contains(x))
        || !hidden.is_finite()
        || !(0.05..=5.0).contains(&hidden)
        || !settling_ms.is_finite()
        || !(0.0..=500.0).contains(&settling_ms)
    {
        return Err("Use supported rates/voice counts, hidden duration 0.05..5 seconds, and analysis settling 0..500 ms".into());
    }
    let provenance: Value =
        serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/provenance.json")))?;
    verify_source(&provenance)?;
    fs::create_dir_all(&output)?;
    let mut reports = Vec::new();
    let mut violations = Vec::new();
    for &sr in &rates {
        for &count in &voices {
            for &kind in &kinds {
                let label = format!("{}-{count}voices-{sr}Hz", kind.name());
                let plan = timeline(kind, sr, hidden, settling_ms);
                let switched = render(sr, count, &plan, None);
                let fast = render(sr, count, &plan, Some(CircuitMode::Fast));
                let heavy = render(sr, count, &plan, Some(CircuitMode::Heavy));
                for event in &switched.report.mode_events {
                    if event.allocations.any()
                        || event.held_before != event.held_after
                        || event.active_before != event.active_after
                    {
                        violations.push(format!(
                            "{label}: setter allocation or voice mutation at{}",
                            event.frame
                        ));
                    }
                }
                if !switched.report.finite || !fast.report.finite || !heavy.report.finite {
                    violations.push(format!("{label}: non-finite output"));
                }
                let excerpt_start = plan
                    .audition_anchor
                    .saturating_sub((sr as f64 * 0.100).round() as usize);
                let paths = [
                    format!("{label}-switch.wav"),
                    format!("{label}-stable-fast.wav"),
                    format!("{label}-stable-heavy.wav"),
                ];
                for (file, rendered) in paths.iter().zip([&switched, &fast, &heavy]) {
                    wav(&output.join(file), &rendered.samples[excerpt_start..], sr)?;
                }
                let transients =
                    transient_reports(sr, &plan, &switched.samples, &fast.samples, &heavy.samples);
                let recovery =
                    recovery_report(sr, settling_ms, &plan, &switched.samples, &heavy.samples);
                let tail_gate = released_tail_gate(kind, &plan, &switched.samples, &heavy.samples);
                if tail_gate["applicable"] == true && tail_gate["passed"] != true {
                    violations.push(format!(
                        "{label}: released-tail absolute residual exceeds -60 dBFS or is non-finite"
                    ));
                }
                let activation = activation_observation(
                    sr,
                    settling_ms,
                    &plan,
                    &switched.samples,
                    &heavy.samples,
                );
                let case = json!({"case":label,"sample_rate":sr,"requested_voices":count,"noise_enabled":false,"physical_heavy_rail_sag":"default enabled","wrapper_fast_finish":"excluded; raw DSP engine only","hidden_seconds":hidden,"analysis_settling_ms":settling_ms,"initial_switch_mode":name(plan.initial),"switch":switched.report,"stable_fast":fast.report,"stable_heavy":heavy.report,"transients":transients,"recovery":recovery,"final_activation":activation,"released_tail_gate":tail_gate,"wav_excerpts":{"files":paths,"start_frame":excerpt_start,"sample_alignment":"Identical MIDI/parameter timeline; equal gain; no normalization"}});
                fs::write(
                    output.join(format!("{label}.json")),
                    serde_json::to_vec_pretty(&case)?,
                )?;
                println!(
                    "{label}: switch render allocations={}, stableHeavy allocations={}",
                    case["switch"]["total_render_allocations"]["allocations"],
                    case["stable_heavy"]["total_render_allocations"]["allocations"]
                );
                reports.push(case);
            }
        }
    }
    let source_verification = verify_source(&provenance);
    let source_error = source_verification
        .as_ref()
        .err()
        .map(|error| error.to_string());
    let report = json!({"source_unchanged_during_run":source_verification.is_ok(),"source_verification_error":source_error,"schema":2,"analysis_settling_ms":settling_ms,"purpose":"Mode-switch allocation, trajectory and transient evidence; no CPU speed or inaudibility claim","build":provenance,"host":{"uname":command("uname",&["-a"]),"cpu":command("sysctl",&["-n","machdep.cpu.brand_string"])},"command_line":args,"counter_scope":"Single-thread executable. Counting enabled exactly during set_circuit_mode or render. Engine creation, warmup, notes, parameter setters, file I/O and report work are outside. Counts are allocator requests, not inferred solver iterations.","guard_limit":"Allocator counts identify requests only. Historical Heavy boxed-state replacement allocated during recovery; the in-place restore may recover without allocation. Amplifier diagnostics can reset and are not a durable guard counter. Zero allocations do not establish zero recovery events or safety for all inputs.","audio_limit":"Finite samples, bounded observed peaks or smooth blend reversal do not establish inaudibility. Fast and Heavy are different circuits. Only noise-off released-tail returns use the explicit -60dBFS absolute residual gate over preparation, fade and post-fade. Recovery windows describe deviation from a continuously running Heavy reference, not a pass/fail hearing threshold.","cpu_timing":"Not measured; no quiet-host CPU timing claims from this tool.","violations":violations,"cases":reports});
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    source_verification?;
    if !report["violations"].as_array().unwrap().is_empty() {
        return Err(
            "Mode setter/finite-output/released-tail contract failed; see report.json".into(),
        );
    }
    Ok(())
}
