"""Generate disposable original-engine crates, with a separate untimed capture hook."""
from pathlib import Path
import subprocess,json,hashlib
ROOT=Path(__file__).resolve().parents[2]
BASE='fd4f603dd8deef7a57d4449826f2c9d5dd53757e'
PREFIX='crates/openwurli-dsp/'
files=subprocess.check_output(['git','ls-tree','-r','--name-only',BASE,'--',PREFIX+'src',PREFIX+'Cargo.toml'],cwd=ROOT,text=True).splitlines()
hashes={}
for name in ['reference','capture']:
 out=Path(__file__).resolve().parent/name
 for f in files:
  data=subprocess.check_output(['git','show',BASE+':'+f],cwd=ROOT)
  hashes[f]=hashlib.sha256(data).hexdigest()
  relative=f[len(PREFIX):]
  text=data.decode()
  if relative=='Cargo.toml': text=text.replace('name = "openwurli-dsp"', 'name = "cpu-profile-'+name+'"')
  if name=='capture' and relative=='src/engine.rs':
   needle='let drive = preamp_out * tables::FIXED_CIRCUIT_DRIVE;'
   assert text.count(needle)==2
   text=text.replace(needle,needle+'\n                    crate::drive_capture::record(drive);')
  if name=='capture' and relative=='src/lib.rs':
   text+='''
// Diagnostic-only capture. Never use this crate for timing.
pub mod drive_capture {
    use std::cell::RefCell;
    thread_local! { static TRACE: RefCell<Option<Vec<f64>>> = const { RefCell::new(None) }; }
    pub fn start(capacity: usize) { TRACE.with(|t| *t.borrow_mut() = Some(Vec::with_capacity(capacity))); }
    pub fn finish() -> Vec<f64> { TRACE.with(|t| t.borrow_mut().take().unwrap()) }
    pub(crate) fn record(value: f64) { TRACE.with(|t| { if let Some(v)=t.borrow_mut().as_mut() { v.push(value); } }); }
}
'''
  p=out/relative;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(text)
(Path(__file__).resolve().parent/'reference-source.json').write_text(json.dumps({'revision':BASE,'sha256':hashes},indent=2)+'\n')
