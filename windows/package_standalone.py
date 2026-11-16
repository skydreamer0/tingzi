"""Package the standalone RC from the explicit standalone build and public data."""
import hashlib
import json
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parent.parent
BUILD = ROOT / '.dev-data/standalone-rc2'
PACKAGE = BUILD / 'package'
BINS = ['TingziStandaloneService64.dll', 'TingziStandaloneService32.dll',
        'TingziStandaloneSetup64.exe', 'TingziStandaloneSetup32.exe', 'TingziStandaloneLearning.exe']
SCRIPTS = ['InstallStandalone.ps1', 'RemoveStandalone.ps1']

def sha(data):
    return hashlib.sha256(data).hexdigest()

def main():
    provenance = json.loads((ROOT / 'windows/standalone-data/provenance.json').read_text(encoding='utf-8'))
    files = {name: (PACKAGE / name).read_bytes() for name in BINS}
    files.update({name: (ROOT / 'windows' / name).read_bytes() for name in SCRIPTS})
    files['README.txt'] = (ROOT / 'windows/standalone-readme.txt').read_bytes()
    with zipfile.ZipFile(ROOT / 'windows/standalone-data/public-data.zip') as archive:
        if set(archive.namelist()) != {item['path'] for item in provenance['files']}:
            raise ValueError('Unexpected public data entries')
        for item in provenance['files']:
            data = archive.read(item['path'])
            if sha(data) != item['sha256']:
                raise ValueError('Public data digest mismatch')
            files[item['path']] = data
    files['data-provenance.json'] = json.dumps(provenance, ensure_ascii=False, indent=2).encode('utf-8')
    for name in BINS:
        data = files[name]
        if 'TingziFloat'.encode('utf-16le') in data or bytes.fromhex('2bc6e96a5443224cb14d0616e4afd0bd') in data:
            raise ValueError('Vendor or float reference in standalone binary ' + name)
    manifest = {'product': 'Tingzi standalone Zhuyin RC', 'version': '0.1.0-rc2',
                'files': [{'path': name, 'sha256': sha(data), 'bytes': len(data)} for name, data in sorted(files.items())]}
    files['release-manifest.json'] = json.dumps(manifest, ensure_ascii=False, indent=2).encode('utf-8')
    for name, data in files.items():
        (PACKAGE / name).write_bytes(data)
    output = BUILD / 'TingziStandalone-0.1.0-rc2.zip'
    with zipfile.ZipFile(output, 'w', zipfile.ZIP_DEFLATED) as archive:
        for name, data in sorted(files.items()):
            info = zipfile.ZipInfo(name, (2026, 11, 16, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data)
    with zipfile.ZipFile(output) as archive:
        assert archive.testzip() is None
        assert set(archive.namelist()) == set(files)
        for name, data in files.items():
            assert archive.read(name) == data
    result = {'candidate': str(output), 'sha256': sha(output.read_bytes()), 'files': len(files)}
    output.with_suffix('.sha256').write_text(result['sha256'] + '  ' + output.name + '\n', encoding='ascii')
    output.with_suffix('.manifest.json').write_text(json.dumps(manifest, indent=2), encoding='utf-8')
    print(json.dumps(result))

if __name__ == '__main__':
    main()
