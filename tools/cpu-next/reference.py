"""Generate the complete ignored reference crate from one immutable Git revision."""
import hashlib
import json
from pathlib import Path
import re
import subprocess

TOOL = Path(__file__).resolve().parent
ROOT = TOOL.parent.parent


def verify(generate=False):
    manifest = json.loads((TOOL/'reference-manifest.json').read_text())
    rev = manifest['revision']
    if not re.fullmatch('[0-9a-f]{40}', rev):
        raise SystemExit('Reference must be a full immutable Git commit SHA.')
    destination = TOOL/'reference'
    if generate and not destination.exists():
        staged = TOOL/'reference.staging'
        if staged.exists():
            raise SystemExit('Interrupted reference generation: inspect reference.staging before retrying.')
        staged.mkdir()
        for relative, expected in manifest['sha256'].items():
            original = subprocess.check_output(['git', 'show', f'{rev}:crates/openwurli-dsp/{relative}'], cwd=ROOT)
            if relative == 'Cargo.toml':
                original = original.replace(b'name = "openwurli-dsp"', b'name = "cpu-next-reference"', 1)
            if hashlib.sha256(original).hexdigest() != expected:
                raise SystemExit('Pinned Git reference/manifest mismatch: '+relative)
            path = staged/relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(original)
        staged.rename(destination)
    errors = []
    actual_files = {str(p.relative_to(destination)) for p in destination.rglob('*') if p.is_file()}
    if actual_files != set(manifest['sha256']):
        errors.append('Generated reference file inventory differs from the immutable manifest.')
    for relative, expected in manifest['sha256'].items():
        path = destination/relative
        if not path.exists() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            errors.append('Generated reference modified/missing: '+relative)
    if errors:
        raise SystemExit('\n'.join(errors)+'\nKeep source edits in the candidate; regenerate historical references from Git, never bless drift.')
    return manifest


if __name__ == '__main__':
    import sys
    verify(generate='--generate' in sys.argv)
