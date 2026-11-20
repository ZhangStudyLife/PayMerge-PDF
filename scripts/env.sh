#!/usr/bin/env bash
# Source this script before invoking cargo on the server.
PAYMERGE_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
export PAYMERGE_ROOT
export RUSTUP_HOME="$PAYMERGE_ROOT/.tools/rustup"
export CARGO_HOME="$PAYMERGE_ROOT/.tools/cargo"
export CARGO_TARGET_DIR="$PAYMERGE_ROOT/target"
export XDG_CACHE_HOME="$PAYMERGE_ROOT/.cache"
export TMPDIR="$PAYMERGE_ROOT/.tmp"
export PATH="$CARGO_HOME/bin:$PAYMERGE_ROOT/.tools/llvm-mingw/bin:$PATH"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER="$PAYMERGE_ROOT/.tools/llvm-mingw/bin/x86_64-w64-mingw32-clang"
export CC_x86_64_pc_windows_gnullvm="$PAYMERGE_ROOT/.tools/llvm-mingw/bin/x86_64-w64-mingw32-clang"
export AR_x86_64_pc_windows_gnullvm="$PAYMERGE_ROOT/.tools/llvm-mingw/bin/llvm-ar"
export PAYMERGE_PDFIUM_DIR="$PAYMERGE_ROOT/.tools/pdfium-linux/lib"
export PYTHONDONTWRITEBYTECODE=1
mkdir -p "$TMPDIR" "$XDG_CACHE_HOME"
