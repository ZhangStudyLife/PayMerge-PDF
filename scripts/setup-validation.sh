#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "$0")/env.sh"
cd "$PAYMERGE_ROOT"
# Use the server's existing pip to bootstrap a project-local validation environment.
python3 -m venv --without-pip --system-site-packages .tools/validation
.tools/validation/bin/python -m pip install --cache-dir .cache/pip -r requirements-validation.txt
