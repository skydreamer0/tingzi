"""Verify standalone binaries without registering or selecting a keyboard."""
import hashlib
from pathlib import Path
import subprocess
import uuid
import shutil

ROOT = Path(__file__).resolve().parent.parent
BUILD = ROOT / '.dev-data/standalone-rc2'
RECEIPT = ROOT / '.dev-data/standalone-rc2/receipts'

def main():
    RECEIPT.mkdir(parents=True, exist_ok=True)
    package = BUILD / 'package'
    logs = []
    for arch, bits in [('x64', 64), ('x86', 32)]:
        binary = BUILD / arch
        for name in ['Tingzi.cin', 'phrases.txt']:
            shutil.copyfile(package / name, binary / name)
        dll = package / f'TingziStandaloneService{bits}.dll'
        logs.append(f'{arch} DLL SHA256 {hashlib.sha256(dll.read_bytes()).hexdigest()}\n')
        learning = RECEIPT / ('learning-' + arch + '-' + uuid.uuid4().hex)
        restart = RECEIPT / ('restart-' + arch + '-' + uuid.uuid4().hex)
        commands = [[binary / 'engine-test.exe', package],
                    [binary / 'learning-test.exe', package, learning],
                    [binary / 'restart-test.exe', package, restart, 'train'],
                    [binary / 'restart-test.exe', package, restart, 'verify'],
                    [binary / 'activation-test.exe'],
                    [binary / 'activation-test.exe', dll]]
        for command in commands:
            result = subprocess.run([str(x) for x in command], cwd=ROOT, capture_output=True, text=True, errors='replace')
            logs.append('COMMAND ' + ' '.join(str(x) for x in command) + '\n' + result.stdout + result.stderr + f'EXIT {result.returncode}\n')
            (RECEIPT / 'tests.txt').write_text(''.join(logs), encoding='utf-8')
            print(result.stdout, end='', flush=True)
            if result.returncode:
                raise SystemExit(result.returncode)
    print('PASS both architectures, including separate-process learning and fixed packaged DLLs')

if __name__ == '__main__':
    main()
