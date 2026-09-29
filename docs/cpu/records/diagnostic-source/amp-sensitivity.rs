use cpu_ab_shipping::power_amp::PowerAmp;
fn main(){
 let mut a=PowerAmp::new();let mut b=PowerAmp::new();
 for perturbation in [1e-6,1e-8,1e-10,1e-12] {
  let(mut max_diff,mut input_at,mut ya_at,mut yb_at,mut count)=(0.0_f64,0.0_f64,0.0_f64,0.0_f64,0);
  for i in 0..1_000_000{
   let x=-10.0+20.0*i as f64/1_000_000.0;
   let(ya,yb)=(a.process(x),b.process(x+perturbation));let diff=(ya-yb).abs();
   if diff>max_diff {max_diff=diff;input_at=x;ya_at=ya;yb_at=yb;}
   if diff>0.1{count+=1;}
  }
  println!("input_perturbation={perturbation:e} max_output_diff={max_diff:e} at_input={input_at:e} native={ya_at:e} perturbed={yb_at:e} jumps_over_0.1={count}");
 }
}
