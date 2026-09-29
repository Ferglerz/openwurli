use openwurli_dsp::{dk_preamp_legacy::DkPreamp,preamp::PreampModel,tremolo::Tremolo};
use cpu_ab_shipping::{dk_preamp_legacy::DkPreamp as NativePreamp,preamp::PreampModel as NativeModel,tremolo::Tremolo as NativeTremolo};
fn main(){
 let sr=88200.0;
 for amplitude in [0.05,0.5,5.0] {
  let (mut a,mut b)=(NativePreamp::new(sr),DkPreamp::new(sr));
  let(mut ta,mut tb)=(NativeTremolo::new(1.0,sr),Tremolo::new(1.0,sr));
  ta.set_response_multiplier(0.5);tb.set_response_multiplier(0.5);
  let(mut max_rdiff,mut max_pdiff,mut first_large,mut maxa,mut maxb)=(0.0_f64,0.0_f64,None,0.0_f64,0.0_f64);
  for n in 0..(sr*8.0) as usize{
   let(ra,rb)=(ta.process(),tb.process());
   a.set_ldr_resistance(ra);b.set_ldr_resistance(rb);
   let x=amplitude*(n as f64*440.0*std::f64::consts::TAU/sr).sin();
   let(ya,yb)=(a.process_sample(x),b.process_sample(x));
   max_rdiff=max_rdiff.max((ra-rb).abs());max_pdiff=max_pdiff.max((ya-yb).abs());maxa=maxa.max(ya.abs());maxb=maxb.max(yb.abs());
   if first_large.is_none()&&(ya-yb).abs()>1e-6{first_large=Some(n);}
  }
  println!("amplitude={amplitude} max_LDR_ohm_diff={max_rdiff:e} max_preamp_volt_diff={max_pdiff:e} first_over_1uV={first_large:?} max_native_volt={maxa:e} max_candidate_volt={maxb:e}");
 }
}
