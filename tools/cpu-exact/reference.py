"""Generate the ignored reference crate from one immutable Git revision.

`--generate` materializes `reference/` from the manifest. `--pin REV` rewrites
the manifest from a full commit SHA; it never touches an existing reference.
"""
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

TOOL = Path(__file__).resolve().parent
ROOT = TOOL.parent.parent
CRATE = 'crates/openwurli-dsp'
NAME = b'name = "openwurli-dsp"'
RENAMED = b'name = "cpu-exact-reference"'


def source(rev, relative):
    original = subprocess.check_output(['git', 'show', f'{rev}:{CRATE}/{relative}'], cwd=ROOT)
    if relative == 'Cargo.toml':
        original = original.replace(NAME, RENAMED, 1)
    return original


def pin(rev):
    if not re.fullmatch('[0-9a-f]{40}', rev):
        raise SystemExit('Reference must be a full immutable Git commit SHA.')
    listing = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', rev, f'{CRATE}/src'], cwd=ROOT, text=True)
    files = ['Cargo.toml'] + [line[len(CRATE) + 1:] for line in listing.splitlines()]
    manifest = {
        'revision': rev,
        'scope': 'complete src tree plus renamed Cargo.toml; generated and ignored',
        'sha256': {relative: hashlib.sha256(source(rev, relative)).hexdigest() for relative in sorted(files)},
    }
    (TOOL/'reference-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')


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
            original = source(rev, relative)
            if hashlib.sha256(original).hexdigest() != expected:
                raise SystemExit('Pinned Git reference/manifest mismatch: ' + relative)
            path = staged/relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(original)
        staged.rename(destination)
    errors = []
    actual = {str(p.relative_to(destination)) for p in destination.rglob('*') if p.is_file()}
    if actual != set(manifest['sha256']):
        errors.append('Generated reference file inventory differs from the immutable manifest.')
    for relative, expected in manifest['sha256'].items():
        path = destination/relative
        if not path.exists() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            errors.append('Generated reference modified/missing: ' + relative)
    if errors:
        raise SystemExit('\n'.join(errors) + '\nKeep edits in the candidate; regenerate references from Git, never bless drift.')
    return manifest


if __name__ == '__main__':
    if '--pin' in sys.argv:
        pin(sys.argv[sys.argv.index('--pin') + 1])
    else:
        verify(generate='--generate' in sys.argv)
