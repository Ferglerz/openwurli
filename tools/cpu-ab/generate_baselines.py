"""Materialize ignored reference crates from pinned Git history for one comparison."""
import json
from pathlib import Path
import re
import subprocess

TOOL = Path(__file__).resolve().parent
ROOT = TOOL.parent.parent


def materialize():
    manifest = json.loads((TOOL/'baseline-manifest.json').read_text())
    for name, info in manifest['baselines'].items():
        if not re.fullmatch(r'[0-9a-f]{40}', info['revision']):
            raise SystemExit('Baseline revisions must be immutable full Git SHAs.')
        directory = TOOL/name
        if directory.exists():
            continue  # The verifier checks every retained source against its immutable hash.
        (directory/'src').mkdir(parents=True)
        rev = info['revision']
        def show(relative):
            return subprocess.check_output(['git', 'show', rev+':'+relative], cwd=ROOT).decode()
        cargo = show('crates/openwurli-dsp/Cargo.toml').replace('name = "openwurli-dsp"', f'name = "cpu-ab-{name}"')
        (directory/'Cargo.toml').write_text(cargo)
        lib = show('crates/openwurli-dsp/src/lib.rs')
        for module in re.findall(r'pub mod (\w+);', lib):
            relative = f'crates/openwurli-dsp/src/{module}.rs'
            if not (ROOT/relative).exists():
                relative = f'crates/openwurli-dsp/src/{module}/mod.rs'
            if module in info['mutable_modules']:
                (directory/'src'/f'{module}.rs').write_text(show(relative))
            else:
                lib = lib.replace(f'pub mod {module};', f'#[path = "../../../../{relative}"]\npub mod {module};')
        (directory/'src/lib.rs').write_text('// TEMPORARY BENCHMARK BASELINE: delete after review/sign-off; see ../README.md.\n'+lib)


if __name__ == '__main__':
    materialize()
