#!/usr/bin/env python3
"""Build only standalone release proof, offline. Never run vendor tests/checks."""
import argparse,pathlib,subprocess,os
ROOT=pathlib.Path(__file__).resolve().parent
p=argparse.ArgumentParser();p.add_argument('--work',type=pathlib.Path,default=ROOT/'build');p.add_argument('--full',action='store_true');a=p.parse_args();work=a.work.resolve()
env=dict(os.environ,CARGO_TARGET_DIR=str(work/('target-full' if a.full else 'target')))
cmd=['cargo','build','--offline','--locked','--release','--manifest-path',str(work/'Cargo.toml'),'-p','recovery-proof']
if a.full:cmd+=['--features','recovery-candidate/reference-full-recovery']
subprocess.run(cmd,env=env,check=True)
