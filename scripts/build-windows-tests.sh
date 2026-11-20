#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "$0")/env.sh"
cd "$PAYMERGE_ROOT"
python3 scripts/prepare_bundle.py
cargo test --locked --target x86_64-pc-windows-gnullvm --no-run --message-format=json > .cache/windows-tests-artifacts.jsonl
python3 - <<'PY'
import json
from pathlib import Path
for line in Path('.cache/windows-tests-artifacts.jsonl').read_text().splitlines():
    artifact = json.loads(line)
    if artifact.get('reason') == 'compiler-artifact' and artifact.get('executable'):
        path = Path(artifact['executable'])
        path.chmod(path.stat().st_mode | 0o500)
PY
