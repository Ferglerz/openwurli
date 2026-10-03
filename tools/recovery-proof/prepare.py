#!/usr/bin/env python3
"""Unpack frozen DSPs into separate renamed packages. Optional current candidate path."""
import argparse, pathlib, shutil, tarfile, json, hashlib
ROOT=pathlib.Path(__file__).resolve().parent
parser=argparse.ArgumentParser()
parser.add_argument('--work',type=pathlib.Path,default=ROOT/'build')
parser.add_argument('--candidate-source',type=pathlib.Path)
parser.add_argument('--harness-source',type=pathlib.Path,default=ROOT/'src/main.rs')
parser.add_argument('--candidate-archive',type=pathlib.Path,default=ROOT/'candidate-dsp.tar.gz')
args=parser.parse_args(); work=args.work.resolve(); work.mkdir(parents=True,exist_ok=True)
record={}
for role in ['reference','candidate']:
 target=work/role
 if target.exists():shutil.rmtree(target)
 if role=='candidate' and args.candidate_source:
  shutil.copytree(args.candidate_source,target)
 else:
  archive=ROOT/'baseline-dsp.tar.gz' if role=='reference' else args.candidate_archive.resolve()
  known=json.loads((ROOT/'archives-sha256.json').read_text())
  if archive.name in known and hashlib.sha256(archive.read_bytes()).hexdigest()!=known[archive.name]:raise SystemExit('Archive hash mismatch')
  target.mkdir()
  with tarfile.open(archive) as tf:
   for m in tf.getmembers():
    if m.name.startswith('/') or '..' in pathlib.PurePosixPath(m.name).parts or not (m.isdir() or m.isfile()):raise SystemExit('Unsafe archive member')
   tf.extractall(target)
 record[role]={str(p.relative_to(target)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(target.rglob('*')) if p.is_file() and p.name!='source-hashes.json'}
 p=target/'Cargo.toml';s=p.read_text();s=s.replace('name = "openwurli-dsp"',f'name = "recovery-{role}"',1);p.write_text(s)
(work/'src').mkdir(exist_ok=True);shutil.copy2(args.harness_source,work/'src/main.rs');shutil.copy2(ROOT/'Cargo.lock',work/'Cargo.lock')
(work/'Cargo.toml').write_text('''[package]
name="recovery-proof"
version="0.1.0"
edition="2024"
[workspace]
[dependencies]
recovery-reference={path="reference",features=["runtime-models"]}
recovery-candidate={path="candidate",features=["runtime-models"]}
serde_json="1"
[profile.release]
debug=1
''')
(work/'source-hashes.json').write_text(json.dumps(record,indent=2)+'\n')
print(f'Prepared frozen reference and candidate at {work}')
