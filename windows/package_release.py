"""Create a release ZIP from explicit runtime entries and reviewed license files."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import zipfile

VERSION = '0.7.4'
ROOT_FILES = ['TingziTextService32.dll', 'TingziTextService64.dll', 'TingziSetup32.exe', 'TingziSetup64.exe', 'TingziPad.exe', 'TingziKeyboard.exe', 'TingziLearning.exe', 'TingziVersion.exe', 'InstallTingzi.cmd', 'RemoveTingzi.cmd', 'VoiceShortcuts.ps1', 'README.md', 'tingzi.svg', 'Tingzi.cin', 'phrases.txt', 'McBopomofo-LICENSE.txt']
FLOAT_FILES = ['TingziFloat.exe', 'LICENSE']
# Every licence name that may appear in a dependency's licence expression has been reviewed for redistribution.
LICENSE_WORDS = {'MIT', 'Apache-2.0', 'BSD-3-Clause', 'BSD-2-Clause', 'Unicode-3.0', 'ISC', 'Zlib', 'Unlicense', 'BSL-1.0', 'CDLA-Permissive-2.0', 'MPL-2.0', '0BSD', 'CC0-1.0', 'MIT-0', 'AND', 'OR'}
# Standard texts for dependencies that ship no licence file of their own.
STANDARD_TEXTS = Path(__file__).parent / 'licenses'

def digest(data):
    return hashlib.sha256(data).hexdigest()

def dependency_notices(metadata, entries):
    """Adds each Rust dependency's licence files to `entries` and returns the list for the notices index."""
    own = set(metadata['workspace_members'])
    dependencies = []
    for item in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
        if item['id'] in own:
            continue
        # Older manifests separate alternatives with a slash.
        expression = (item.get('license') or '').replace('/', ' OR ')
        words = set(re.findall(r'[A-Za-z0-9.-]+', expression))
        if not expression or words - LICENSE_WORDS:
            raise ValueError('Unreviewed license: ' + item['name'] + ' ' + expression)
        base = Path(item['manifest_path']).parent
        folder = 'notices/rust/' + item['name'] + '-' + item['version'] + '/'
        included = []
        for path in sorted(base.rglob('*')):
            if path.is_file() and not path.is_symlink() and re.match(r'^(licen[cs]e|copying|notice)([._-]|$)', path.name, re.I):
                entries[folder + path.relative_to(base).as_posix()] = path.read_bytes()
                included.append(folder + path.relative_to(base).as_posix())
        if not included:
            # The crate names its licence but ships no text: state who wrote it and attach the standard text.
            names = sorted(words - {'AND', 'OR'})
            missing = [name for name in names if not (STANDARD_TEXTS / (name + '.txt')).is_file()]
            if missing:
                raise ValueError('No license text: ' + item['name'] + ' (' + ', '.join(missing) + ')')
            note = [item['name'] + ' ' + item['version'], 'License: ' + expression, 'Authors: ' + ('; '.join(item.get('authors') or []) or 'see the source repository'),
                    'Source: ' + (item.get('repository') or 'https://crates.io/crates/' + item['name']), '',
                    'This package ships no licence file. The standard text of each licence it names follows.', '']
            for name in names:
                note.extend(['----- ' + name + ' -----', (STANDARD_TEXTS / (name + '.txt')).read_text(encoding='utf-8'), ''])
            entries[folder + 'NOTICE.txt'] = '\n'.join(note).encode('utf-8')
            included.append(folder + 'NOTICE.txt')
        dependencies.append(dict(name=item['name'], version=item['version'], license=expression, source=item.get('repository') or '', notices=included))
    return dependencies

def package(source, destination, metadata_path):
    source = source.resolve()
    if destination.exists():
        raise ValueError('Destination already exists; choose a new ZIP name.')
    entries = {}
    for name in ROOT_FILES + ['TingziFloat/' + name for name in FLOAT_FILES]:
        path = source / name
        if not path.is_file() or path.is_symlink():
            raise ValueError('Missing regular required file: ' + name)
        entries[name] = path.read_bytes()
    metadata = json.loads(metadata_path.read_text(encoding='utf-8-sig'))
    dependencies = dependency_notices(metadata, entries)
    combined = ['Third-party notices for Tingzi ' + VERSION + '\n', '\n===== TingziFloat/LICENSE =====\n', entries['TingziFloat/LICENSE'].decode('utf-8-sig')]
    for item in dependencies:
        combined.append('\n===== Rust dependency ' + item['name'] + ' ' + item['version'] + ' (' + item['license'] + ') =====\n')
        for name in sorted(item['notices']):
            combined.extend(['\n--- ' + name + ' ---\n', entries[name].decode('utf-8-sig', errors='replace')])
    entries['THIRD-PARTY-NOTICES.txt'] = ''.join(combined).encode('utf-8')
    entries['notices/rust-dependencies.json'] = json.dumps(dependencies, ensure_ascii=False, indent=2).encode()
    entries['RELEASE.txt'] = ('聽字・注音與語音輸入法 ' + VERSION + '\nWindows x64. The voice window needs Microsoft Edge WebView2 Runtime, which Windows 11 includes.\n'
        'InstallTingzi.cmd installs or upgrades the application.\n'
        'Includes public Zhuyin word data from McBopomofo (MIT, see McBopomofo-LICENSE.txt). No voice recordings, local models, accounts or credentials are included.\n'
        'Online recognition uses the third-party codex-asr 0.1.2 library: https://github.com/wangnov/codex-asr\n'
        'The voice window is built from the Rust packages listed in notices/rust-dependencies.json, each unmodified from crates.io; '
        'packages under MPL-2.0 are available in source form there.\n').encode('utf-8')
    for name, data in entries.items():
        for encoding in ('utf-8', 'utf-16le', 'utf-16be'):
            text = data.decode(encoding, errors='ignore')
            if re.search(r'[A-Za-z]:[\\/](?:Users|Documents)[\\/]', text, re.I):
                raise ValueError('Machine-local data detected: ' + name)
    (source / 'THIRD-PARTY-NOTICES.txt').write_bytes(entries['THIRD-PARTY-NOTICES.txt'])
    manifest = dict(product='聽字・注音與語音輸入法', version=VERSION, runtime='Microsoft Edge WebView2 Runtime', files=[dict(path=name, size=len(data), sha256=digest(data)) for name, data in sorted(entries.items())])
    entries['release-manifest.json'] = json.dumps(manifest, ensure_ascii=False, indent=2).encode('utf-8')
    destination.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(destination, 'w', zipfile.ZIP_DEFLATED) as archive:
        for name, data in sorted(entries.items()):
            archive.writestr(name, data)
    with zipfile.ZipFile(destination) as archive:
        assert archive.testzip() is None
        assert set(archive.namelist()) == set(entries)
        for name, data in entries.items():
            assert archive.read(name) == data
    result = dict(zip=destination.name, sha256=digest(destination.read_bytes()), entries=len(entries), bytes=destination.stat().st_size)
    destination.with_suffix('.manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding='utf-8')
    # Written as bytes so the line ending is the same on every system and checksum tools accept the file.
    destination.with_suffix('.sha256').write_bytes((result['sha256'] + '  ' + destination.name + '\n').encode('ascii'))
    print(json.dumps(result, ensure_ascii=False))

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('destination', type=Path)
    parser.add_argument('--cargo-metadata', required=True, type=Path)
    args = parser.parse_args()
    package(args.source, args.destination, args.cargo_metadata)
