#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "$0")/env.sh"
cd "$PAYMERGE_ROOT"
version="$(cargo metadata --locked --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["version"])')"
cargo build --locked --release --target x86_64-pc-windows-gnullvm --bins
chmod u+rx target/x86_64-pc-windows-gnullvm/release/paymerge-pdf.exe target/x86_64-pc-windows-gnullvm/release/paymerge-updater.exe
package="$PAYMERGE_ROOT/dist/PayMerge-PDF-$version-windows-x64"
# Always assemble a fresh package, so removed dependencies cannot leave stale license files.
package_stage="$(mktemp -d "$TMPDIR/package.XXXXXXXX")"
trap 'rm -rf -- "$package_stage"' EXIT
mkdir -p "$package_stage/licenses"
cp target/x86_64-pc-windows-gnullvm/release/paymerge-updater.exe "$package_stage/PayMergeUpdater.exe"
cp target/x86_64-pc-windows-gnullvm/release/paymerge-pdf.exe "$package_stage/ASC_InvoiceMergeTool.exe"
cp .tools/pdfium-win/bin/pdfium.dll "$package_stage/pdfium.dll"
chmod u+x "$package_stage/ASC_InvoiceMergeTool.exe" "$package_stage/PayMergeUpdater.exe" "$package_stage/pdfium.dll"
cp assets/OFL-NotoSansSC.txt "$package_stage/licenses/"
cp docs/使用说明.md "$package_stage/使用说明.md"
python3 scripts/package_licenses.py "$package_stage"
python3 - "$package_stage" "$package" <<'PY'
import hashlib, pathlib, shutil, sys, zipfile
root, destination = map(pathlib.Path, sys.argv[1:])
manifest = root / 'SHA256SUMS.txt'
files = [p for p in sorted(root.rglob('*')) if p.is_file() and p != manifest]
required = {'ASC_InvoiceMergeTool.exe', 'PayMergeUpdater.exe', 'pdfium.dll', '使用说明.md'}
if not required.issubset({p.relative_to(root).as_posix() for p in files}):
    raise RuntimeError('Required program files disappeared before packaging')
hashes = {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in files}
manifest.write_text(''.join(digest+'  '+name+'\n' for name, digest in hashes.items()), encoding='utf-8')
archive_path = pathlib.Path(str(destination)+'.zip')
archive_stage = root / 'package.zip'
with zipfile.ZipFile(archive_stage, 'w', zipfile.ZIP_DEFLATED) as archive:
    for p in [*files, manifest]:
        archive.write(p, destination.name+'/'+p.relative_to(root).as_posix())
with zipfile.ZipFile(archive_stage) as archive:
    for name, expected in hashes.items():
        if hashlib.sha256(archive.read(destination.name+'/'+name)).hexdigest() != expected:
            raise RuntimeError('A program file changed during packaging: '+name)
archive_stage.replace(archive_path)
pathlib.Path(str(archive_path)+'.sha256').write_text(hashlib.sha256(archive_path.read_bytes()).hexdigest()+'  '+archive_path.name+'\n', encoding='utf-8')
if destination.exists():
    if destination.parent != root.parent.parent/'dist' or destination.is_symlink():
        raise RuntimeError('Refusing to replace a package outside dist')
    shutil.rmtree(destination)
shutil.copytree(root, destination)
for name, expected in hashes.items():
    if hashlib.sha256((destination/name).read_bytes()).hexdigest() != expected:
        raise RuntimeError('Packaged file changed: '+name)
print('Package:', destination)
PY
