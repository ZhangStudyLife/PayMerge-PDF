"""Build compressed, embedded resources; no runtime files are shipped next to the EXE."""
import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile
import zlib

ROOT = Path(__file__).resolve().parents[1]
destination = ROOT / '.cache/bundle'
destination.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix='embedded-licenses-', dir=ROOT / '.tmp') as directory:
    package = Path(directory)
    subprocess.run(['python3', str(ROOT / 'scripts/package_licenses.py'), str(package)], check=True)
    for source in sorted((ROOT / 'assets').glob('OFL-*.txt')):
        shutil.copyfile(source, package / 'licenses' / source.name)
    notices = []
    for source in sorted((package / 'licenses').rglob('*')):
        if source.is_file():
            raw = source.read_bytes()
            try:
                text = raw.decode('utf-8')
            except UnicodeDecodeError:
                # A bundled upstream notice uses a legacy Western copyright character.
                text = raw.decode('cp1252')
            notices.append('\n' + '=' * 72 + '\n' + source.relative_to(package / 'licenses').as_posix() + '\n' + '=' * 72 + '\n' + text)
    files = {
        'pdfium': (ROOT / '.tools/pdfium-win/bin/pdfium.dll').read_bytes(),
        'font': (ROOT / 'assets/NotoSansSC.ttf').read_bytes(),
        'notices': '\n'.join(notices).encode('utf-8'),
    }
    for name, content in files.items():
        compressed = zlib.compress(content, 9)
        target = destination / (name + '.bin')
        if not target.exists() or target.read_bytes() != compressed:
            target.write_bytes(compressed)
    digest = hashlib.sha256(files['pdfium']).hexdigest() + '\n'
    digest_file = destination / 'pdfium.sha256'
    if not digest_file.exists() or digest_file.read_text() != digest:
        digest_file.write_text(digest)
print('Embedded PDFium, font and third-party notices prepared')
