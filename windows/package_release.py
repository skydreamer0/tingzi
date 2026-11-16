"""Create a release ZIP from explicit runtime entries and reviewed license files."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import zipfile

ROOT_FILES = ['TingziTextService32.dll', 'TingziTextService64.dll', 'TingziSetup32.exe', 'TingziSetup64.exe', 'TingziPad.exe', 'TingziKeyboard.exe', 'TingziLearning.exe', 'TingziVersion.exe', 'InstallTingzi.cmd', 'RemoveTingzi.cmd', 'VoiceShortcuts.ps1', 'README.md', 'tingzi.svg', 'Tingzi.cin', 'phrases.txt', 'McBopomofo-LICENSE.txt']
FLOAT_FILES = ['TingziFloat.exe', 'TingziFloat.dll', 'TingziFloat.deps.json', 'TingziFloat.runtimeconfig.json', 'NAudio.dll', 'NAudio.Asio.dll', 'NAudio.Core.dll', 'NAudio.Midi.dll', 'NAudio.Wasapi.dll', 'NAudio.WinForms.dll', 'NAudio.WinMM.dll', 'LICENSE', 'NAudio-LICENSE.txt', 'codex-asr.exe', 'codex-asr-LICENSE.txt']
LICENSE_WORDS = {'MIT','Apache-2.0','BSD-3-Clause','BSD-2-Clause','Unicode-3.0','ISC','Zlib','Unlicense','BSL-1.0','CDLA-Permissive-2.0','AND','OR'}


# Upstream NAudio 2.2.1 DLLs contain their own build-time PDB paths.
# Accept only the byte-identical DLLs from the pinned NuGet packages.
NAUDIO_221 = {
    'TingziFloat/NAudio.Asio.dll': 'ca03780217139b37f7f5b6921d59defb8d24988315b16b167a77fa88caa7d00f',
    'TingziFloat/NAudio.Core.dll': 'fcf493fc47a2f478a65303886b975fbdbf714cbb1f2d79f7fce97e4bb16b01a8',
    'TingziFloat/NAudio.dll': 'c001e49d31497f608b14066b0f10304c0ccb7e3b9df9d5fec5cf3196f92961b8',
    'TingziFloat/NAudio.Midi.dll': 'f246e29921797b173b54229685e997a11f9cc388fa1e589c212328abd7a94ebe',
    'TingziFloat/NAudio.Wasapi.dll': '618ef0e49d64e7a66dfe64bbf6ae81705b9d9683d8a9f321e5c3024d666bdf82',
    'TingziFloat/NAudio.WinForms.dll': 'b313f530227fd31fc2b6fa74547f4c8a964cde7696c07624dfc19b693fe9c468',
    'TingziFloat/NAudio.WinMM.dll': '749a01ebbb5edd8b1a03c5263b04de6acadecf52e4cc84d7412bc6e93f180958',
}

def digest(data):
    return hashlib.sha256(data).hexdigest()

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
    dependencies = []
    for item in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
        expression = item.get('license') or ''
        words = set(re.findall(r'[A-Za-z0-9.-]+', expression))
        if not expression or words - LICENSE_WORDS:
            raise ValueError('Unreviewed license: ' + item['name'] + ' ' + expression)
        base = Path(item['manifest_path']).parent
        notices = [p for p in base.rglob('*') if p.is_file() and not p.is_symlink() and re.match(r'^(licen[cs]e|copying|notice)([._-]|$)', p.name, re.I)]
        if not notices:
            raise ValueError('No license text: ' + item['name'])
        included = []
        for path in notices:
            name = 'notices/codex-asr/' + item['name'] + '-' + item['version'] + '/' + path.relative_to(base).as_posix()
            entries[name] = path.read_bytes()
            included.append(name)
        dependencies.append(dict(name=item['name'], version=item['version'], license=expression, notices=included))
    combined = ['Third-party notices for Tingzi 0.6.2\n']
    for name in ['TingziFloat/LICENSE', 'TingziFloat/NAudio-LICENSE.txt', 'TingziFloat/codex-asr-LICENSE.txt']:
        combined.extend(['\n===== ' + name + ' =====\n', entries[name].decode('utf-8-sig')])
    for item in dependencies:
        combined.append('\n===== codex-asr dependency ' + item['name'] + ' ' + item['version'] + ' (' + item['license'] + ') =====\n')
        for name in sorted(item['notices']):
            combined.extend(['\n--- ' + name + ' ---\n', entries[name].decode('utf-8-sig')])
    entries['THIRD-PARTY-NOTICES.txt'] = ''.join(combined).encode('utf-8')
    entries['notices/codex-asr-dependencies.json'] = json.dumps(dependencies, ensure_ascii=False, indent=2).encode()
    entries['RELEASE.txt'] = ('聽字・注音與語音輸入法 0.6.2\nWindows x64. Requires Microsoft .NET 8 Desktop Runtime x64.\nInstallTingzi.cmd installs or upgrades the application.\nIncludes public Zhuyin word data from McBopomofo (MIT, see McBopomofo-LICENSE.txt). No voice recordings, local models, accounts or credentials are included.\nThird-party codex-asr 0.1.2: https://github.com/wangnov/codex-asr\nNAudio 2.2.1: https://github.com/naudio/NAudio\n').encode('utf-8')
    for name, data in entries.items():
        if name in NAUDIO_221:
            if digest(data) != NAUDIO_221[name]:
                raise ValueError('Unexpected NAudio 2.2.1 binary: ' + name)
            continue
        for encoding in ('utf-8', 'utf-16le', 'utf-16be'):
            text = data.decode(encoding, errors='ignore')
            if re.search(r'[A-Za-z]:[\\/](?:Users|Documents)[\\/]', text, re.I):
                raise ValueError('Machine-local data detected: ' + name)
    (source / 'THIRD-PARTY-NOTICES.txt').write_bytes(entries['THIRD-PARTY-NOTICES.txt'])
    manifest = dict(product='聽字・注音與語音輸入法', version='0.6.2', runtime='Microsoft.NETCore.App + Microsoft.WindowsDesktop.App 8.x x64', files=[dict(path=name, size=len(data), sha256=digest(data)) for name, data in sorted(entries.items())])
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
    destination.with_suffix('.sha256').write_text(result['sha256'] + '  ' + destination.name + '\n', encoding='ascii')
    print(json.dumps(result, ensure_ascii=False))

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('destination', type=Path)
    parser.add_argument('--cargo-metadata', required=True, type=Path)
    args = parser.parse_args()
    package(args.source, args.destination, args.cargo_metadata)
