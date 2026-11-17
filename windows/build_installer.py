"""Build 聽字-Setup-<version>-x64.exe from a release ZIP made by package_release.py.

The installer carries the ZIP unchanged, unpacks it and runs InstallTingzi.cmd from it, so the
registration and upgrade rules stay in one place. Needs Inno Setup 6.3 or later (ISCC.exe).
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import zipfile

SCRIPT = Path(__file__).parent / 'installer' / 'Tingzi.iss'

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def find_compiler(given):
    candidates = [given] if given else []
    candidates += [shutil.which('ISCC'), shutil.which('iscc')]
    for base in (os.environ.get('ProgramFiles(x86)'), os.environ.get('ProgramFiles'), os.environ.get('LOCALAPPDATA') and os.path.join(os.environ['LOCALAPPDATA'], 'Programs')):
        if base:
            candidates.append(os.path.join(base, 'Inno Setup 6', 'ISCC.exe'))
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return Path(candidate)
    raise SystemExit('Inno Setup 6 (ISCC.exe) was not found; install it or pass --iscc.')

def unpack(release, folder):
    """Extracts the ZIP and checks every file against its release manifest, so the installer holds exactly what was reviewed."""
    with zipfile.ZipFile(release) as archive:
        for name in archive.namelist():
            # Entries are written by package_release.py, but a path that leaves the folder is refused anyway.
            if name.startswith(('/', '\\')) or '..' in Path(name).parts or ':' in name:
                raise SystemExit('Unsafe entry in the ZIP: ' + name)
        archive.extractall(folder)
    manifest = json.loads((folder / 'release-manifest.json').read_text(encoding='utf-8'))
    listed = {item['path'] for item in manifest['files']}
    present = {path.relative_to(folder).as_posix() for path in folder.rglob('*') if path.is_file()} - {'release-manifest.json'}
    if listed != present:
        raise SystemExit('The ZIP does not match its manifest: ' + ', '.join(sorted(listed ^ present)))
    for item in manifest['files']:
        if digest(folder / item['path']) != item['sha256']:
            raise SystemExit('Checksum mismatch: ' + item['path'])
    return manifest['version']

def build(release, output, compiler):
    with tempfile.TemporaryDirectory(prefix='tingzi-installer-') as staging:
        payload = Path(staging) / 'payload'
        payload.mkdir()
        # Checked here so a damaged or hand-edited ZIP never becomes an installer.
        version = unpack(release, payload)
        output.mkdir(parents=True, exist_ok=True)
        target = output / ('聽字-Setup-' + version + '-x64.exe')
        if target.exists():
            raise SystemExit('Destination already exists; remove it or choose another folder: ' + str(target))
        subprocess.run([str(compiler), '/Qp', '/DAppVersion=' + version, '/DPayloadZip=' + str(release.resolve()), '/DOutputDir=' + str(output.resolve()), str(SCRIPT.resolve())], check=True)
    if not target.is_file():
        raise SystemExit('ISCC did not write ' + target.name)
    sha = digest(target)
    # Written as bytes so the line ending is the same on every system and checksum tools accept the file.
    target.with_name(target.name + '.sha256').write_bytes((sha + '  ' + target.name + '\n').encode('utf-8'))
    print(json.dumps(dict(installer=target.name, version=version, sha256=sha, bytes=target.stat().st_size), ensure_ascii=False))

if __name__ == '__main__':
    # The installer's name is Chinese; a Windows console may not default to UTF-8.
    sys.stdout.reconfigure(encoding='utf-8')
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('release', type=Path, help='release ZIP from package_release.py')
    parser.add_argument('output', type=Path, help='folder for the installer')
    parser.add_argument('--iscc', help='path to ISCC.exe')
    args = parser.parse_args()
    build(args.release, args.output, find_compiler(args.iscc))
