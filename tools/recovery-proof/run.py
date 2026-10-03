#!/usr/bin/env python3
"""Run frozen proof. CPU phase must run alone, after all builds/audio stop."""
import argparse,pathlib,hashlib,json,platform,os,datetime,subprocess,time
ROOT=pathlib.Path(__file__).resolve().parent
p=argparse.ArgumentParser();p.add_argument('phase',choices=['audio','audio-full','cpu']);p.add_argument('--work',type=pathlib.Path,default=ROOT/'build');p.add_argument('--out',type=pathlib.Path,required=True);p.add_argument('--repeats',type=int,default=5);p.add_argument('--filter',default='');a=p.parse_args();work=a.work.resolve();a.out.mkdir(parents=True,exist_ok=False)
def hashes():
 paths=[]
 for name in ['reference','candidate','src']:paths+=list((work/name).rglob('*'))
 paths+=[work/'Cargo.toml',work/'Cargo.lock']
 return {str(f.relative_to(work)):hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted(paths) if f.is_file()}
start=hashes();binary=work/('target-full' if a.phase=='audio-full' else 'target')/'release/recovery-proof';env=dict(os.environ,RECOVERY_CASE_FILTER=a.filter)
meta={'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'platform':platform.platform(),'rustc':subprocess.check_output(['rustc','-Vv'],text=True),'phase':a.phase,'filter':a.filter,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'features':['default legacy-power-amp','runtime-models']+(['recovery-candidate/reference-full-recovery'] if a.phase=='audio-full' else []),'source_hashes':start,'snapshot_source_hashes':json.loads((work/'source-hashes.json').read_text()),'harness_sha256':{str(f.relative_to(ROOT)):hashlib.sha256(f.read_bytes()).hexdigest() for f in [ROOT/'prepare.py',ROOT/'build.py',ROOT/'run.py',ROOT/'src/main.rs']},'build_env':{k:os.environ.get(k,'') for k in ['RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','CARGO_BUILD_TARGET','RUSTC_WRAPPER']}}
with (a.out/'log.txt').open('w') as log:
 t=time.monotonic();result=subprocess.run([str(binary),a.phase,str(a.out/'report.json'),str(a.repeats)],stdout=log,stderr=subprocess.STDOUT,env=env);meta.update(elapsed_seconds=time.monotonic()-t,exit_code=result.returncode,sources_unchanged=start==hashes(),finished_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
(a.out/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n');print((a.out/'log.txt').read_text())
if not meta['sources_unchanged']:raise SystemExit('Frozen sources changed during run')
raise SystemExit(result.returncode)
