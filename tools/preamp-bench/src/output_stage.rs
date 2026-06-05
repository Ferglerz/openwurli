use openwurli_dsp::power_amp::PowerAmp;
use openwurli_dsp::speaker::Speaker;
use openwurli_dsp::tables;

#[derive(Debug, Clone, Copy)]
pub struct OutputStageRenderConfig {
    pub sample_rate: f64,
    pub volume: f64,
    pub speaker_char: f64,
    pub no_poweramp: bool,
    pub no_rail_sag: bool,
}

pub fn render_output_stage_with_models(
    preamp_output: &[f64],
    config: OutputStageRenderConfig,
) -> Vec<f64> {
    let mut power_amp = PowerAmp::new_at_sample_rate(config.sample_rate);
    if config.no_rail_sag {
        power_amp.set_rail_sag(false);
    }
    let mut speaker = Speaker::new(config.sample_rate);
    speaker.set_character(config.speaker_char);

    let mut out = vec![0.0; preamp_output.len()];
    for i in 0..preamp_output.len() {
        // Audio taper squared rule
        let attenuated = preamp_output[i] * config.volume * config.volume; 
        let amplified = if config.no_poweramp {
            attenuated
        } else {
            power_amp.process(attenuated)
        };
        out[i] = speaker.process(amplified) * tables::POST_SPEAKER_GAIN;
    }
    out
}
