#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "$0")/env.sh"
cd "$PAYMERGE_ROOT"
python3 scripts/prepare_bundle.py
cargo fmt --all --check
cargo test --locked --no-default-features
cargo clippy --locked --no-default-features --all-targets -- -D warnings
cargo clippy --locked --target x86_64-pc-windows-gnullvm --all-targets -- -D warnings
