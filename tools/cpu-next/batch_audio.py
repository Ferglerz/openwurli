#!/usr/bin/env python3
"""Shard the unchanged audio matrix; never use parallel jobs for CPU timing."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import subprocess
import sys

TOOL = Path(__file__).resolve().parent
ROOT = TOOL.parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--engine', choices=['fast', 'heavy'], required=True)
    parser.add_argument('--features', default='')
    parser.add_argument('--runtime-modes', action='store_true')
    parser.add_argument('--jobs', type=int, default=4)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    assert 1 <= args.jobs <= 8
    assert not args.output.exists(), 'Never overwrite evidence'
    args.output.mkdir(parents=True)
    base = [sys.executable, str(TOOL/'run.py'), '--engine', args.engine,
            '--features', args.features, '--mode', 'full', '--audio-only', '--require-exact']
    if args.runtime_modes:
        base.append('--runtime-modes')
    coverage = json.loads(subprocess.check_output(base + ['--list'], cwd=ROOT, text=True))
    cases = coverage['audio_cases']
    names = [case['name'] for case in cases]
    assert len(set(names)) == len(names)
    groups = [names[i::args.jobs] for i in range(args.jobs)]
    for group in groups:
        assert {name for name in names if any(part in name for part in group)} == set(group)
    plan = {'engine': args.engine, 'features': args.features,
            'runtime_modes': args.runtime_modes, 'audio_only': True,
            'jobs': args.jobs, 'expected_cases': names, 'groups': groups,
            'script_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    (args.output/'batch-plan.json').write_text(json.dumps(plan, indent=2)+'\n')

    def run(index):
        out = args.output/f'shard-{index}'
        cmd = base + ['--scenario-filter', ','.join(groups[index]), '--output', str(out)]
        with (args.output/f'shard-{index}.log').open('w') as log:
            result = subprocess.run(cmd, cwd=ROOT, stdout=log, stderr=log)
        print(f'{args.engine} audio shard {index}: exit {result.returncode}', flush=True)
        return result.returncode

    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        exits = list(pool.map(run, range(args.jobs)))
    seen = []
    summaries = []
    for index in range(args.jobs):
        path = args.output/f'shard-{index}'
        if not (path/'report.json').exists():
            summaries.append({'shard': index, 'missing_report': True})
            continue
        report = json.loads((path/'report.json').read_text())
        provenance = json.loads((path/'provenance.json').read_text())
        seen += [item['scenario']['name'] for item in report['audio']]
        summaries.append({'shard': index, 'samples': report['audio_samples_per_pair'],
                          'cases': report['audio_scenarios'],
                          'exact': report['all_audio_exact'],
                          'bounds': report['all_audio_within_bounds'],
                          'diagnostics_exact': report['power_amp_diagnostics_exact'],
                          'source_unchanged': provenance['source_unchanged_during_run']})
    complete = len(seen) == len(names) and set(seen) == set(names)
    passed = complete and all(code == 0 for code in exits)
    summary = {'complete_unique_coverage': complete, 'passed': passed,
               'case_count': len(seen), 'samples_per_pair': sum(s.get('samples', 0) for s in summaries),
               'shards': summaries, 'exit_codes': exits,
               'cpu_timing_performed': False}
    (args.output/'batch-summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    print(json.dumps(summary, indent=2))
    raise SystemExit(0 if passed else 1)


if __name__ == '__main__':
    main()
