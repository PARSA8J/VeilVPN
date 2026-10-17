#!/bin/bash
# VeilVPN 1.6.1 Windows EXE build (Rust native, cross-compile from linux/mac or native on windows)
# Needs: rustup target x86_64-pc-windows-gnu + llvm-mingw (linux) or mingw toolchain.
#   rustup target add x86_64-pc-windows-gnu
# Usage: scripts/build_exe.sh   (OUT_DIR=dist to change output dir)
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# resolve output dir to ABSOLUTE path BEFORE any cd
OUT_DIR_ABS="${OUT_DIR:-$ROOT}"
mkdir -p "$OUT_DIR_ABS" && OUT_DIR_ABS="$(cd "$OUT_DIR_ABS" && pwd)"

cd "$ROOT/windows-rs"

# toolchain hints (llvm-mingw or mingw-w64 in PATH)
if ! command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1 && [ -d "$ROOT/tools/llvm-mingw/bin" ]; then
  export PATH="$ROOT/tools/llvm-mingw/bin:$PATH"
fi

echo "== build (windows-gnu, release)"
export RUSTFLAGS="${RUSTFLAGS} -L $HOME/tools/fakelibs"
cargo build --release --target x86_64-pc-windows-gnu

cp target/x86_64-pc-windows-gnu/release/veilvpn.exe "$OUT_DIR_ABS/VeilVPN-1.6.1.exe"
ls -la "$OUT_DIR_ABS/VeilVPN-1.6.1.exe"
echo "BUILD OK -> $OUT_DIR_ABS/VeilVPN-1.6.1.exe"
