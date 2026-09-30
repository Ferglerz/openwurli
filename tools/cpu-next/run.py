#!/usr/bin/env python3
"""Paired heavy/fast CPU and audio study with an immutable fd4f603 reference."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
from reference import TOOL, ROOT, verify

FEATURES = {'cpu-study-preamp-reuse', 'cpu-study-tremolo-zero', 'cpu-study-damper-recurrence', 'cpu-study-amp-transfer', 'cpu-study-heavy-matrices'}


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--engine', choices=['heavy', 'fast'], default='heavy')
    parser.add_argument('--mode', choices=['quick', 'full'], default='quick')
    parser.add_argument('--features', default='')
    parser.add_argument('--runtime-modes', action='store_true', help='Enable both circuit backends only in the candidate and select --engine explicitly.')
    parser.add_argument('--repeats', type=int, default=11)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--require-exact', action='store_true')
    parser.add_argument('--export-all', action='store_true')
    parser.add_argument('--scenario-filter')
    parser.add_argument('--max-peak-dbfs', type=float, default=-60.0)
    parser.add_argument('--max-rms-relative-db', type=float, default=-60.0)
    parser.add_argument('--verify-only', action='store_true')
    parser.add_argument('--list', action='store_true')
    phase = parser.add_mutually_exclusive_group()
    phase.add_argument('--audio-only', action='store_true')
    phase.add_argument('--cpu-only', action='store_true')
    args = parser.parse_args()
    features = sorted(set(filter(None, (s.strip() for s in args.features.split(',')))))
    if set(features) - FEATURES:
        raise SystemExit('Unknown candidate feature(s): '+', '.join(set(features)-FEATURES))
    if args.repeats < 3:
        raise SystemExit('At least three repeats required; default eleven.')
    manifest = verify(generate=True)
    if args.verify_only:
        print('Immutable reference verified:', manifest['revision'])
        return
    if not args.list and args.output is None:
        parser.error('--output is required unless --list or --verify-only')
    if not (TOOL/'Cargo.lock').exists():
        subprocess.run(['cargo','generate-lockfile','--offline','--manifest-path',str(TOOL/'Cargo.toml')],cwd=ROOT,check=True)
    cargo_features = ['engine-'+args.engine]+features
    if args.runtime_modes:cargo_features.append('runtime-modes')
    cmd = ['cargo','run','--offline','--locked','--manifest-path',str(TOOL/'Cargo.toml'),'--release','-p','cpu-next','--no-default-features','--features',','.join(cargo_features),'--','--mode',args.mode,'--repeats',str(args.repeats),'--max-peak-dbfs',str(args.max_peak_dbfs),'--max-rms-relative-db',str(args.max_rms_relative_db)]
    if args.list:
        cmd += ['--list']
    else:
        cmd += ['--output',str(args.output.resolve())]
    for name in ['audio_only','cpu_only','require_exact','export_all']:
        if getattr(args,name):cmd += ['--'+name.replace('_','-')]
    if args.scenario_filter:cmd += ['--scenario-filter',args.scenario_filter]
    if args.list:
        raise SystemExit(subprocess.run(cmd,cwd=ROOT,check=False).returncode)
    if args.output.exists() and any(args.output.iterdir()):
        raise SystemExit('Use an empty output directory; never overwrite recorded evidence.')
    args.output.mkdir(parents=True,exist_ok=True)
    candidate_sources = sorted((ROOT/'crates/openwurli-dsp').rglob('*.rs'))+[ROOT/'crates/openwurli-dsp/Cargo.toml']
    harness_sources = sorted((TOOL/'src').rglob('*.rs'))+[TOOL/name for name in ['build.rs','Cargo.toml','Cargo.lock','reference.py','run.py','reference-manifest.json']]
    baseline_sources = sorted(p for p in (TOOL/'reference').rglob('*') if p.is_file())
    sources = candidate_sources+harness_sources+baseline_sources
    provenance = {
        'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'engine':args.engine,'candidate_features':features,'candidate_runtime_modes':args.runtime_modes,'reference_runtime_modes':False,'candidate_initial_mode':args.engine,'command':cmd,
        'platform':platform.platform(),'architecture':platform.machine(),'rustc':command('rustc','-Vv'),
        'head':command('git','rev-parse','HEAD'),'git_status':command('git','status','--short'),
        'reference_manifest':manifest,'source_sha256':{str(p.relative_to(ROOT)):sha(p) for p in sources},
        'root_cargo_lock_sha256':sha(ROOT/'Cargo.lock'),'harness_cargo_lock_sha256':sha(TOOL/'Cargo.lock'),
        'build_environment':{key:os.environ.get(key,'') for key in ['RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','CARGO_BUILD_TARGET','CARGO_TARGET_DIR','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER']},
        'seed_protocol':'identical ordered MIDI; note*2654435761+monotonic note counter; noise disabled on both engines',
        'reference_isolation':'entire private/public source tree generated from pinned Git; separate crate; matching selected backend; study/runtime features apply only to candidate',
    }
    if sys.platform=='darwin':
        try:provenance['hardware']=command('sysctl','-n','machdep.cpu.brand_string','hw.memsize')
        except subprocess.CalledProcessError as exc:provenance['hardware']={'unavailable':str(exc),'reason':'sandbox restriction'}
        provenance['os_build']=command('sw_vers')
    (args.output/'provenance.json').write_text(json.dumps(provenance,indent=2)+'\n')
    (args.output/'working-tree.patch').write_text(command('git','diff','--','crates/openwurli-dsp','tools/cpu-next')+'\n')
    result=subprocess.run(cmd,cwd=ROOT,check=False)
    after={str(p.relative_to(ROOT)):sha(p) for p in sources}
    provenance['source_unchanged_during_run']=after==provenance['source_sha256']
    provenance['harness_cargo_lock_sha256_after']=sha(TOOL/'Cargo.lock')
    provenance['root_cargo_lock_sha256_after']=sha(ROOT/'Cargo.lock')
    provenance['finished_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat()
    provenance['exit_code']=result.returncode
    (args.output/'provenance.json').write_text(json.dumps(provenance,indent=2)+'\n')
    if not provenance['source_unchanged_during_run']:
        raise SystemExit('Sources changed during measurement: this run is invalid, preserve and repeat after source freeze.')
    raise SystemExit(result.returncode)


if __name__=='__main__':
    main()
