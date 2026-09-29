#![allow(dead_code)]
include!(concat!(env!("OUT_DIR"), "/historical_modules.rs"));
mod tables {
    pub use openwurli_dsp::tables::*;
}
use openwurli_dsp::{reed::ModalReed as Cached, tables as params};
use std::{hint::black_box, time::Instant};

// TEMPORARY: DELETE AFTER CPU/AUDIO STUDY. See ../README.md.
fn main() {
    macro_rules! checksum {
        ($kind:ty) => {{
            let mut reed = <$kind>::new(
                261.63,
                &[1.0, 6.267, 17.547, 34.386, 56.842, 85.1, 119.3],
                &[1.0, 0.3, 0.1, 0.0, 0.0, 0.0, 0.0],
                &[4.5, 18.0, 40.0, 80.0, 120.0, 180.0, 240.0],
                0.002,
                0.8,
                44_100.0,
                42,
            );
            let mut output = vec![0.0f64; 8192];
            reed.render(&mut output);
            reed.start_damper(60, 44_100.0);
            reed.render(&mut output);
            output
                .iter()
                .map(|x| x.to_bits())
                .fold(0u64, |acc, bits| acc.wrapping_mul(16_777_619) ^ bits)
        }};
    }
    let scalar_checksum = checksum!(scalar::ModalReed);
    let simd_checksum = checksum!(simd::ModalReed);
    let post_checksum = checksum!(Cached);
    assert_eq!(scalar_checksum, simd_checksum);
    assert_eq!(scalar_checksum, post_checksum);
    println!("C4 checksums: scalar={scalar_checksum} simd={simd_checksum} post={post_checksum}");
    let mut count = 0u64;
    let velocities = [0.0, 0.001, 0.3, 0.8, 0.999, 1.0];
    for sr in [44_100.0, 48_000.0, 88_200.0, 96_000.0] {
        for note in 33..=96u8 {
            let p = params::note_params(note);
            for (vi, &vel) in velocities.iter().enumerate() {
                let seed = (note as u32)
                    .wrapping_mul(2654435761)
                    .wrapping_add(vi as u32);
                let onset = openwurli_dsp::hammer::onset_ramp_time(vel, p.fundamental_hz);
                let mut a = scalar::ModalReed::new(
                    p.fundamental_hz,
                    &p.mode_ratios,
                    &p.mode_amplitudes,
                    &p.mode_decay_rates,
                    onset,
                    vel,
                    sr,
                    seed,
                );
                let mut b = simd::ModalReed::new(
                    p.fundamental_hz,
                    &p.mode_ratios,
                    &p.mode_amplitudes,
                    &p.mode_decay_rates,
                    onset,
                    vel,
                    sr,
                    seed,
                );
                let mut c = Cached::new(
                    p.fundamental_hz,
                    &p.mode_ratios,
                    &p.mode_amplitudes,
                    &p.mode_decay_rates,
                    onset,
                    vel,
                    sr,
                    seed,
                );
                // Irregular boundaries straddle every jitter/renormalization/ramp state.
                for phase in 0..3 {
                    if phase > 0 {
                        a.start_damper(note, sr);
                        b.start_damper(note, sr);
                        c.start_damper(note, sr);
                    }
                    for &n in &[0, 1, 15, 16, 17, 31, 127, 257, 1023, 2049, 4096] {
                        let mut x = vec![0.123; n];
                        let mut y = x.clone();
                        let mut z = x.clone();
                        a.render(&mut x);
                        b.render(&mut y);
                        c.render(&mut z);
                        for (i, ((x, y), z)) in x.iter().zip(&y).zip(&z).enumerate() {
                            assert_eq!(
                                x.to_bits(),
                                y.to_bits(),
                                "scalar/SIMD sr={sr} note={note} vel={vel} phase={phase} block={n} offset={i}"
                            );
                            assert_eq!(
                                x.to_bits(),
                                z.to_bits(),
                                "scalar/cache sr={sr} note={note} vel={vel} phase={phase} block={n} offset={i}"
                            );
                            count += 1;
                        }
                    }
                }
            }
        }
    }
    println!(
        "Bit-identical samples per comparison: {count}; scenarios: 1536; render phases: attack/sustain, damping, repeated note-off"
    );
    if std::env::args().any(|x| x == "--audio-only") {
        return;
    }
    macro_rules! bench {
        ($kind:ty) => {{
            let p = params::note_params(60);
            let mut voices: Vec<_> = (0..32)
                .map(|i| {
                    <$kind>::new(
                        p.fundamental_hz * (1.0 + i as f64 / 64.0),
                        &p.mode_ratios,
                        &p.mode_amplitudes,
                        &p.mode_decay_rates,
                        0.002,
                        0.7,
                        48_000.0,
                        i,
                    )
                })
                .collect();
            let mut out = [0.0; 256];
            let start = Instant::now();
            for k in 0..750 {
                for v in &mut voices {
                    out.fill(0.0);
                    v.render(black_box(&mut out));
                    black_box(&out);
                }
                black_box(k);
            }
            start.elapsed().as_secs_f64() * 1000.0
        }};
    }
    let mut measurements = [Vec::new(), Vec::new(), Vec::new()];
    for round in 0..12 {
        for item in 0..3 {
            let k = (item + round) % 3;
            let ms = match k {
                0 => bench!(scalar::ModalReed),
                1 => bench!(simd::ModalReed),
                _ => bench!(Cached),
            };
            if round > 1 {
                measurements[k].push(ms);
            }
        }
    }
    let mut records = Vec::new();
    for (label, mut x) in ["scalar", "simd", "post"].iter().zip(measurements) {
        let raw = x
            .iter()
            .map(|v| format!("{v:.6}"))
            .collect::<Vec<_>>()
            .join(",");
        x.sort_by(f64::total_cmp);
        let median = (x[4] + x[5]) / 2.0;
        println!(
            "{label}: median_ms={median:.3} min_ms={:.3} max_ms={:.3} repeats={}",
            x[0],
            x[9],
            x.len()
        );
        records.push(format!("\"{label}\":{{\"median_ms\":{median:.6},\"min_ms\":{:.6},\"max_ms\":{:.6},\"raw_ms\":[{raw}]}}", x[0], x[9]));
    }
    let args = std::env::args().collect::<Vec<_>>();
    if let Some(index) = args.iter().position(|a| a == "--json") {
        let path = args.get(index + 1).expect("--json requires a path");
        std::fs::write(path, format!("{{\"samples_per_comparison\":{count},\"scenarios\":1536,\"mismatched_samples\":0,\"timing\":{{{}}}}}\n",records.join(","))).unwrap();
    }
}
