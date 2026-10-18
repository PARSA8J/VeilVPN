#!/bin/bash
# Mehrveil Windows EXE build: Rust core + WebView2 (Chromium) UI shell.
# Toolchain: MSVC — run natively on Windows (GitHub Actions windows-latest)
# or any host with the x86_64-pc-windows-msvc toolchain.
# Usage: scripts/build_exe.sh   (OUT_DIR=dist to change output dir)
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# resolve output dir to ABSOLUTE path BEFORE any cd
OUT_DIR_ABS="${OUT_DIR:-$ROOT}"
mkdir -p "$OUT_DIR_ABS" && OUT_DIR_ABS="$(cd "$OUT_DIR_ABS" && pwd)"

cd "$ROOT/windows-rs"

VER="$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/')"

echo "== build (windows-msvc, release): v$VER"
cargo build --release

cp target/release/veilvpn.exe "$OUT_DIR_ABS/Mehrveil-$VER.exe"
ls -la "$OUT_DIR_ABS/Mehrveil-$VER.exe"
echo "BUILD OK -> $OUT_DIR_ABS/Mehrveil-$VER.exe"
