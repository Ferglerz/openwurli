#!/usr/bin/env python3
"""Paired Fast/Heavy runtime-mode proof against an immutable pinned reference.

Audio comparison is sharded across processes; CPU timing always runs alone.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
from reference import TOOL, ROOT, verify

BINARY = TOOL/'target/release/cpu-exact'


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sources():
    candidate = sorted((ROOT/'crates/openwurli-dsp/src').rglob('*.rs')) + [ROOT/'crates/openwurli-dsp/Cargo.toml']
    harness = sorted((TOOL/'src').rglob('*.rs')) + [TOOL/name for name in ['build.rs', 'Cargo.toml', 'Cargo.lock', 'reference.py', 'run.py', 'reference-manifest.json']]
    return candidate + harness


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--engine', choices=['heavy', 'fast'], required=True)
    parser.add_argument('--mode', choices=['quick', 'full'], default='quick')
    parser.add_argument('--shards', type=int, default=os.cpu_count() or 1)
    parser.add_argument('--repeats', type=int, default=11)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--require-exact', action='store_true')
    parser.add_argument('--scenario-filter')
    phase = parser.add_mutually_exclusive_group()
    phase.add_argument('--audio-only', action='store_true')
    phase.add_argument('--cpu-only', action='store_true')
    args = parser.parse_args()
    manifest = verify(generate=True)
    if args.output.exists() and any(args.output.iterdir()):
        raise SystemExit('Use an empty output directory; never overwrite recorded evidence.')
    args.output.mkdir(parents=True, exist_ok=True)
    subprocess.run(['cargo', 'build', '--release', '--locked', '--manifest-path', str(TOOL/'Cargo.toml')], cwd=ROOT, check=True)
    base = [str(BINARY), '--engine', args.engine, '--mode', args.mode, '--repeats', str(args.repeats)]
    if args.require_exact:
        base.append('--require-exact')
    if args.scenario_filter:
        base += ['--scenario-filter', args.scenario_filter]
    before = {str(p.relative_to(ROOT)): sha(p) for p in sources()}
    provenance = {
        'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'engine': args.engine, 'mode': args.mode, 'shards': args.shards,
        'platform': platform.platform(), 'architecture': platform.machine(), 'rustc': command('rustc', '-Vv'),
        'head': command('git', 'rev-parse', 'HEAD'), 'git_status': command('git', 'status', '--short'),
        'reference_manifest': manifest, 'source_sha256': before,
        'build_environment': {key: os.environ.get(key, '') for key in ['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_TARGET', 'RUSTC_WRAPPER']},
        'reference_isolation': 'entire source tree generated from pinned Git; separate crate; both sides compile runtime-models and select the same initial mode',
    }
    (args.output/'working-tree.patch').write_text(command('git', 'diff', '3103753a387424a34eb606a197ebefbe0d8f2e89', '--', 'crates/openwurli-dsp') + '\n')
    failures = []
    if not args.cpu_only:
        procs = []
        for shard in range(args.shards):
            out = args.output/f'audio-{shard}'
            log = open(args.output/f'audio-{shard}.log', 'w')
            procs.append((shard, subprocess.Popen(base + ['--audio-only', '--shard', f'{shard}/{args.shards}', '--output', str(out)], cwd=ROOT, stdout=log, stderr=log)))
        summary = {'exact': True, 'diagnostics_exact': True, 'within_bounds': True, 'finite': True, 'cases': 0, 'nonexact': []}
        for shard, proc in procs:
            code = proc.wait()
            if code:
                failures.append(f'audio shard {shard} exit {code}')
            report = json.loads((args.output/f'audio-{shard}/report.json').read_text())
            summary['exact'] &= bool(report['all_audio_exact'])
            summary['diagnostics_exact'] &= bool(report['power_amp_diagnostics_exact'])
            summary['within_bounds'] &= bool(report['all_audio_within_bounds'])
            summary['finite'] &= bool(report['audio_finite_guards_clear'])
            summary['cases'] += report['audio_scenarios']
            summary['nonexact'] += [row['scenario']['name'] for row in report['audio'] if not row['comparison']['exact']]
        (args.output/'audio-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
        print(json.dumps({k: v for k, v in summary.items() if k != 'nonexact'} | {'nonexact_count': len(summary['nonexact'])}))
    if not args.audio_only:
        code = subprocess.run(base + ['--cpu-only', '--output', str(args.output/'cpu')], cwd=ROOT).returncode
        if code:
            failures.append(f'cpu exit {code}')
        rows = json.loads((args.output/'cpu/cpu.json').read_text())
        for row in rows:
            ratio = next(r for r in row['ratios'] if r['metric'] == 'render_ns_per_sample')
            print(f"{row['scenario']['name']:48} ref {ratio['reference_median']:9.1f} ns/sample  cand {ratio['candidate_median']:9.1f}  reduction {ratio['cpu_reduction_percent']:6.2f}%")
    provenance['source_unchanged_during_run'] = {str(p.relative_to(ROOT)): sha(p) for p in sources()} == before
    provenance['finished_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    provenance['failures'] = failures
    (args.output/'provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')
    if not provenance['source_unchanged_during_run']:
        raise SystemExit('Sources changed during measurement: this run is invalid.')
    if failures:
        raise SystemExit('; '.join(failures))


if __name__ == '__main__':
    main()
