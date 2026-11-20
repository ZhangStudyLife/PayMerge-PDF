#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "$0")/env.sh"
cd "$PAYMERGE_ROOT"
download() {
    local url="$1" file="$2" sha="$3"
    if [[ ! -f "$file" ]] || ! echo "$sha  $file" | sha256sum --check --status; then
        curl --fail --silent --show-error --location --continue-at - --retry 3 --connect-timeout 20 --max-time 1800 "$url" -o "$file"
        echo "$sha  $file" | sha256sum --check
    fi
}
mkdir -p .cache/downloads .tools assets
if [[ ! -x "$CARGO_HOME/bin/rustup" ]]; then
    installer="$PAYMERGE_ROOT/.cache/downloads/rustup-init"
    curl --fail --location --retry 3 https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init.sha256 -o "$installer.sha256"
    download https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init "$installer" "$(cut -d' ' -f1 "$installer.sha256")"
    chmod +x "$installer"
    "$installer" -y --no-modify-path --profile minimal --default-toolchain 1.100.0
fi
rustup component add rustfmt clippy --toolchain 1.100.0
rustup target add x86_64-pc-windows-gnullvm --toolchain 1.100.0
if [[ ! -x .tools/llvm-mingw/bin/x86_64-w64-mingw32-clang ]]; then
    archive="$PAYMERGE_ROOT/.cache/downloads/llvm-mingw.tar.xz"
    download https://github.com/mstorsjo/llvm-mingw/releases/download/20261117/llvm-mingw-20261117-ucrt-ubuntu-22.04-x86_64.tar.xz "$archive" e7a15a49a2ea95e9f3d8a9d0cfabe5052a4b10fb4777cc54de5a8c27b64bb701
    mkdir -p .tools/llvm-mingw
    tar -xJf "$archive" --strip-components=1 -C .tools/llvm-mingw
fi
for platform in linux win; do
    if [[ "$platform" == linux ]]; then
        sha=e1c3ce6e270cb5e0c20816b379ad9f2c1309972697c505f508de92bf5e7d6659
    else
        sha=9a964261760cff52570aabf038ab346d3a5034dd8b30d0d5fe8861235d356b8b
    fi
    archive="$PAYMERGE_ROOT/.cache/downloads/pdfium-$platform.tgz"
    download "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/8125/pdfium-$platform-x64.tgz" "$archive" "$sha"
    mkdir -p ".tools/pdfium-$platform"
    tar -xzf "$archive" -C ".tools/pdfium-$platform"
done
python3 scripts/setup_font.py
rustc --version
cargo --version
"$CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER" --version | head -n 1
