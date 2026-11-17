"""Write the third-party notices that go inside Tingzi.app, from the output of `cargo metadata`."""
import json
from pathlib import Path
import sys

# The Windows packager already knows which licences have been reviewed and where each package keeps its text.
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / 'windows'))
from package_release import dependency_notices


def notices(metadata_path, destination):
    metadata = json.loads(metadata_path.read_text(encoding='utf-8-sig'))
    entries = {}
    dependencies = dependency_notices(metadata, entries)
    combined = ['Third-party notices for Tingzi on macOS\n',
                'The app is built from the Rust packages below, each unmodified from crates.io; packages under MPL-2.0 are available in source form there.\n',
                'Online recognition uses the third-party codex-asr library: https://github.com/wangnov/codex-asr\n']
    for item in dependencies:
        combined.append('\n===== Rust dependency ' + item['name'] + ' ' + item['version'] + ' (' + item['license'] + ') =====\n')
        for name in sorted(item['notices']):
            combined.extend(['\n--- ' + name + ' ---\n', entries[name].decode('utf-8-sig', errors='replace')])
    destination.write_text(''.join(combined), encoding='utf-8')
    print(str(len(dependencies)) + ' packages listed in ' + destination.name)


if __name__ == '__main__':
    if len(sys.argv) != 3:
        sys.exit('usage: notices.py <cargo-metadata.json> <THIRD-PARTY-NOTICES.txt>')
    notices(Path(sys.argv[1]), Path(sys.argv[2]))
