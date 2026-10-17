#!/bin/bash
# VeilVPN 1.5.1 Windows EXE build (cross-compile from linux/mac or native on windows)
# Usage: scripts/build_exe.sh   (needs Go 1.22+ in PATH or in ./tools/go)
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="${TOOL_GO:-$ROOT/tools/go/bin}:$PATH"

echo "== sync web embeds"
mkdir -p "$ROOT/windows/web"
cp "$ROOT"/shared/index.html "$ROOT"/shared/app.js "$ROOT"/shared/glass.css "$ROOT"/shared/i18n.js "$ROOT"/shared/seed.txt "$ROOT/windows/web/"

echo "== build"
OUT_DIR_ABS="${OUT_DIR:-$ROOT}"
mkdir -p "$OUT_DIR_ABS" && OUT_DIR_ABS="$(cd "$OUT_DIR_ABS" && pwd)"   # absolute: script cd's internally
OUT="$OUT_DIR_ABS/VeilVPN-1.5.1.exe"
cd "$ROOT/windows"
if [ "$(go env GOOS 2>/dev/null)" != "windows" ]; then
  GOOS=windows GOARCH=amd64 go build -trimpath -ldflags "-s -w -H=windowsgui" -o "$OUT" .
else
  go build -trimpath -ldflags "-s -w -H=windowsgui" -o "$OUT" .
fi
ls -la "$OUT"
echo "BUILD OK -> $OUT"
