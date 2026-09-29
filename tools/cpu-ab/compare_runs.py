#!/usr/bin/env python3
"""Compare optimized native and LUT outputs directly; both runs need --export-all."""
import argparse
import array
import json
import math
from pathlib import Path
import sys


def samples(path):
    a = array.array('f')
    a.frombytes(path.read_bytes())
    if sys.byteorder != 'little':
        a.byteswap()
    return a


def db(value):
    return 20*math.log10(value) if value > 0 else None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('reference', type=Path)
    parser.add_argument('candidate', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    old = json.loads((args.reference/'report.json').read_text())
    new = json.loads((args.candidate/'report.json').read_text())
    if old['experimental_circuit_lut']:
        raise SystemExit('Reference must be a native default run.')
    old_provenance = json.loads((args.reference/'provenance.json').read_text())
    new_provenance = json.loads((args.candidate/'provenance.json').read_text())
    if old_provenance['source_sha256'] != new_provenance['source_sha256']:
        raise SystemExit('Runs must compare exactly the same DSP/harness sources; only features may differ.')
    if not old_provenance['source_unchanged_during_run'] or not new_provenance['source_unchanged_during_run']:
        raise SystemExit('A run was invalidated by source edits.')
    if [a['scenario'] for a in old['audio']] != [a['scenario'] for a in new['audio']]:
        raise SystemExit('Scenario matrices differ.')
    result = []
    for a, b in zip(old['audio'], new['audio']):
        if not a.get('optimized_waveform') or not b.get('optimized_waveform'):
            raise SystemExit('Missing waveforms: repeat both runs with --export-all.')
        ap = args.reference/a['optimized_waveform']
        bp = args.candidate/b['optimized_waveform']
        av, bv = samples(ap), samples(bp)
        if len(av) != len(bv):
            raise SystemExit('Sample counts differ.')
        finite = all(math.isfinite(x) and math.isfinite(y) for x, y in zip(av, bv))
        if not finite:
            raise SystemExit('Nonfinite sample: '+a['scenario']['name'])
        residual = [float(y)-float(x) for x, y in zip(av, bv)]
        peak = max(abs(d) for d in residual)
        rms = math.sqrt(sum(d*d for d in residual)/len(av))
        reference_rms = math.sqrt(sum(float(x)*float(x) for x in av)/len(av))
        # Compare serialized f32 bits, including signed zero.
        abits, bbits = ap.read_bytes(), bp.read_bytes()
        mismatches = sum(abits[i:i+4] != bbits[i:i+4] for i in range(0, len(abits), 4))
        result.append({'scenario':a['scenario'], 'samples':len(av), 'bit_mismatches':mismatches,
                       'peak_residual':peak, 'rms_residual':rms, 'peak_residual_dbfs':db(peak),
                       'rms_residual_dbfs':db(rms), 'residual_relative_db':db(rms/reference_rms) if reference_rms else None})
    cpu = []
    new_cpu = {c['scenario']['name']:c for c in new['cpu']}
    for a in old['cpu']:
        b = new_cpu[a['scenario']['name']]
        av = next(v for v in a['variants'] if v['variant']=='optimized')
        bv = next(v for v in b['variants'] if v['variant']=='optimized')
        ratio = bv['median_ns_per_sample']/av['median_ns_per_sample']
        cpu.append({'scenario':a['scenario'], 'candidate_to_native_median_time_ratio':ratio,
                    'cpu_reduction_percent':100*(1-ratio)})
    args.output.write_text(json.dumps({'reference':str(args.reference.resolve()),'candidate':str(args.candidate.resolve()),
        'note':'CPU runs occurred separately; inspect hardware/load and raw distributions before interpreting small differences.',
        'audio':result, 'cpu':cpu}, indent=2)+'\n')
    print(args.output)


if __name__ == '__main__':
    main()
