use openwurli_dsp::dsp::oversampler::Oversampler;
use openwurli_dsp::circuit::preamp::PreampModel;
use openwurli_dsp::circuit::tremolo::Tremolo;

pub struct PreampRenderBlock<'a> {
    preamp: &'a mut dyn PreampModel,
    oversampler: Option<Oversampler>,
    tremolo: Option<Tremolo>,
}

impl<'a> PreampRenderBlock<'a> {
    pub fn new(
        preamp: &'a mut dyn PreampModel,
        use_oversampling: bool,
        tremolo: Option<Tremolo>,
    ) -> Self {
        Self {
            preamp,
            oversampler: use_oversampling.then(Oversampler::new),
            tremolo,
        }
    }

    pub fn process_sample(&mut self, input: f64) -> f64 {
        let (preamp, tremolo) = (&mut self.preamp, &mut self.tremolo);
        if let Some(os) = self.oversampler.as_mut() {
            let mut up = [0.0f64; 2];
            os.upsample_2x(&[input], &mut up);
            let processed = [
                process_native(preamp, tremolo, up[0]),
                process_native(preamp, tremolo, up[1]),
            ];
            let mut down = [0.0f64; 1];
            os.downsample_2x(&processed, &mut down);
            down[0]
        } else {
            process_native(preamp, tremolo, input)
        }
    }

    pub fn process_buffer(&mut self, input: &[f64], output: &mut [f64]) {
        for (out, &x) in output.iter_mut().zip(input.iter()) {
            *out = self.process_sample(x);
        }
    }
}

fn process_native(
    preamp: &mut &mut dyn PreampModel,
    tremolo: &mut Option<Tremolo>,
    input: f64,
) -> f64 {
    if let Some(tremolo) = tremolo.as_mut() {
        preamp.set_ldr_resistance(tremolo.process());
    }
    preamp.process_sample(input)
}
