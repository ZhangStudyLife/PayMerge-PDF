#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "$0")/env.sh"
cd "$PAYMERGE_ROOT"
python3 scripts/prepare_bundle.py
version="$(cargo metadata --locked --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["version"])')"
cargo build --locked --release --target x86_64-pc-windows-gnullvm --bin paymerge-pdf
source_exe="$PAYMERGE_ROOT/target/x86_64-pc-windows-gnullvm/release/paymerge-pdf.exe"
chmod u+rx "$source_exe"
package="$PAYMERGE_ROOT/dist/PayMerge-PDF-$version-windows-x64"
mkdir -p "$package"
cp "$source_exe" "$package/PayMerge-PDF.exe"
chmod u+rx "$package/PayMerge-PDF.exe"
python3 - "$package" <<'PY'
from pathlib import Path
import hashlib, sys
root = Path(sys.argv[1])
assert [p.name for p in root.iterdir()] == ['PayMerge-PDF.exe'], 'Single-file distribution must contain only the EXE'
executable = root / 'PayMerge-PDF.exe'
print('Single EXE:', executable)
print('Bytes:', executable.stat().st_size)
print('SHA256:', hashlib.sha256(executable.read_bytes()).hexdigest())
PY
