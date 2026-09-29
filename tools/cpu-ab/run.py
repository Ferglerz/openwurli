#!/usr/bin/env python3
"""Validate temporary baselines, record provenance, then run one controlled A/B."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
from generate_baselines import materialize

TOOL = Path(__file__).resolve().parent
ROOT = TOOL.parent.parent


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def verify():
    manifest = json.loads((TOOL / 'baseline-manifest.json').read_text())
    failures = []
    for relative, refs in manifest['shared'].items():
        actual = command('git', 'hash-object', relative)
        for baseline, expected in refs.items():
            if actual != expected:
                failures.append(f'{baseline} shared source changed: {relative}')
    for relative, expected in manifest['snapshot_sha256'].items():
        if hashlib.sha256((TOOL / relative).read_bytes()).hexdigest() != expected:
            failures.append(f'baseline snapshot modified: {relative}')
    if failures:
        raise SystemExit('\n'.join(failures) + '\nSnapshot the old shared module from its baseline git ref before continuing. Never update hashes to hide a source change.')
    return manifest


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--verify-only', action='store_true')
    p.add_argument('--mode', choices=['quick', 'full'], default='quick')
    p.add_argument('--repeats', type=int, default=7)
    p.add_argument('--output', type=Path, default=ROOT / 'target/cpu-ab-results')
    p.add_argument('--experimental-circuit-lut', action='store_true')
    p.add_argument('--require-exact', action='store_true')
    p.add_argument('--export-all', action='store_true')
    phase = p.add_mutually_exclusive_group()
    phase.add_argument('--audio-only', action='store_true')
    phase.add_argument('--cpu-only', action='store_true')
    p.add_argument('--max-peak-dbfs', type=float)
    p.add_argument('--max-rms-relative-db', type=float)
    args = p.parse_args()
    if not args.verify_only:
        materialize()
    manifest = verify()
    if args.verify_only:
        return
    if not (TOOL/'Cargo.lock').exists():
        subprocess.run(['cargo', 'generate-lockfile', '--offline', '--manifest-path', str(TOOL/'Cargo.toml')], cwd=ROOT, check=True)
    if args.output.exists() and any(args.output.iterdir()):
        raise SystemExit('Output directory must be empty: preserve each evidence run separately.')
    args.output.mkdir(parents=True, exist_ok=True)
    cmd = ['cargo', 'run', '--offline', '--locked', '--manifest-path', str(TOOL/'Cargo.toml'), '--release', '-p', 'cpu-ab']
    if args.experimental_circuit_lut:
        cmd += ['--features', 'experimental-circuit-lut']
    cmd += ['--', '--mode', args.mode, '--repeats', str(args.repeats), '--output', str(args.output.resolve())]
    if args.require_exact:
        cmd += ['--require-exact']
    if args.export_all:
        cmd += ['--export-all']
    if args.audio_only:
        cmd += ['--audio-only']
    if args.cpu_only:
        cmd += ['--cpu-only']
    for key in ('max_peak_dbfs', 'max_rms_relative_db'):
        if getattr(args, key) is not None:
            cmd += ['--' + key.replace('_', '-'), str(getattr(args, key))]
    tracked_sources = sorted((ROOT/'crates/openwurli-dsp').rglob('*.rs')) + sorted(f for f in TOOL.rglob('*.rs') if 'target' not in f.relative_to(TOOL).parts) + [ROOT/'crates/openwurli-dsp/Cargo.toml', TOOL/'Cargo.toml', TOOL/'run.py', TOOL/'generate_baselines.py', TOOL/'baseline-manifest.json', TOOL/'Cargo.lock']
    provenance = {
        'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'command': cmd, 'platform': platform.platform(), 'architecture': platform.machine(),
        'rustc': command('rustc', '-Vv'), 'head': command('git', 'rev-parse', 'HEAD'),
        'git_status': command('git', 'status', '--short'), 'baseline_manifest': manifest,
        'rustflags': os.environ.get('RUSTFLAGS', ''),
        'cargo_encoded_rustflags': os.environ.get('CARGO_ENCODED_RUSTFLAGS', ''),
        'cargo_build_target': os.environ.get('CARGO_BUILD_TARGET', ''),
        'harness_cargo_lock_sha256_before': hashlib.sha256((TOOL/'Cargo.lock').read_bytes()).hexdigest(),
        'cargo_lock_sha256': hashlib.sha256((ROOT/'Cargo.lock').read_bytes()).hexdigest(),
        'source_sha256': {str(f.relative_to(ROOT)): hashlib.sha256(f.read_bytes()).hexdigest() for f in tracked_sources},
        'seed_protocol': 'WurliEngine note index * 2654435761 + monotonically increasing note counter; identical ordered MIDI; legacy preamp noise disabled',
    }
    if sys.platform == 'darwin':
        try:
            provenance['hardware'] = command('sysctl', '-n', 'machdep.cpu.brand_string', 'hw.memsize')
        except subprocess.CalledProcessError as e:
            provenance['hardware'] = {'unavailable': str(e), 'reason': 'sandbox sysctl restriction'}
        provenance['os_build'] = command('sw_vers')
    (args.output/'provenance.json').write_text(json.dumps(provenance, indent=2)+'\n')
    (args.output/'working-tree.patch').write_text(command('git', 'diff', '--', 'crates/openwurli-dsp', 'tools/cpu-ab')+'\n')
    result = subprocess.run(cmd, cwd=ROOT, check=False)
    after = {relative: hashlib.sha256((ROOT/relative).read_bytes()).hexdigest() for relative in provenance['source_sha256']}
    provenance['source_unchanged_during_run'] = after == provenance['source_sha256']
    provenance['harness_cargo_lock_sha256'] = hashlib.sha256((TOOL/'Cargo.lock').read_bytes()).hexdigest()
    provenance['cargo_lock_sha256_after'] = hashlib.sha256((ROOT/'Cargo.lock').read_bytes()).hexdigest()
    provenance['finished_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    provenance['exit_code'] = result.returncode
    (args.output/'provenance.json').write_text(json.dumps(provenance, indent=2)+'\n')
    if not provenance['source_unchanged_during_run']:
        raise SystemExit('DSP/harness sources changed during measurement; discard this run and repeat.')
    raise SystemExit(result.returncode)


if __name__ == '__main__':
    main()
