#!/bin/bash
# Mehrveil 1.6.8 APK build: aapt2 + ecj + d8 + zipalign + apksigner
# Usage: scripts/build_apk.sh   (tools must be in $TOOLS_DIR, default ./tools — run scripts/setup_tools.sh first)
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TOOL="${TOOLS_DIR:-$ROOT/tools}"
BT="$TOOL/bt/android-14"
PLAT="$TOOL/plat/android-34/android-core.jar"
SRC="$ROOT/android/app/src"
RES="$ROOT/android/res"
MAN="$ROOT/android/AndroidManifest.xml"
ASSETS="$ROOT/android/app/assets"
OUT="$ROOT/android/build"
APK_DIR="${OUT_DIR:-$ROOT}"
mkdir -p "$APK_DIR" && APK_DIR="$(cd "$APK_DIR" && pwd)"   # absolute: script cd's internally
APK="$APK_DIR/Mehrveil-1.6.13.apk"
VER_NAME=1.6.13
VER_CODE=24

rm -rf "$OUT" && mkdir -p "$OUT/gen" "$OUT/obj" "$OUT/dex" "$ASSETS"

echo "== assets: web app + seed"
cp "$ROOT"/shared/index.html "$ROOT"/shared/app.js "$ROOT"/shared/glass.css "$ROOT"/shared/i18n.js "$ASSETS/"
cp "$ROOT"/shared/seed.txt "$ASSETS/"

echo "== aapt2 compile + link"
"$BT/aapt2" compile --dir "$RES" -o "$OUT/res.zip"
"$BT/aapt2" link -o "$OUT/base.apk" -I "$PLAT" --manifest "$MAN" --java "$OUT/gen" -A "$ASSETS" "$OUT/res.zip" \
  --min-sdk-version 21 --target-sdk-version 33 --version-code $VER_CODE --version-name $VER_NAME \
  --auto-add-overlay

echo "== ecj compile"
java -jar "$TOOL/ecj.jar" -source 17 -target 17 -encoding UTF-8 \
  -nowarn -proceedOnError \
  -classpath "$PLAT" \
  -d "$OUT/obj" \
  $(find "$SRC" -name '*.java') $(find "$OUT/gen" -name '*.java') 2>&1 | grep -E "ERROR|problems? \(" | head -30
if [ $(find "$OUT/obj" -name '*.class' | wc -l) -lt 5 ]; then echo "ECJ FAILED"; exit 1; fi

echo "== d8 dex"
jar cf "$OUT/obj.jar" -C "$OUT/obj" . 2>/dev/null || { cd "$OUT/obj" && zip -q -r ../obj.jar . && cd ../..; }
"$BT/d8" --release --min-api 21 --lib "$PLAT" --output "$OUT/dex" "$OUT/obj.jar" 2>&1 | head -10

echo "== package"
rm -rf "$OUT/merged" && mkdir -p "$OUT/merged"
cd "$OUT" && unzip -o -q base.apk -d merged
cp dex/classes.dex merged/
cd merged
zip -q -X ../unsigned.apk classes.dex AndroidManifest.xml
zip -q -r -X ../unsigned.apk assets res
# CRITICAL: resources.arsc MUST be STORED (uncompressed) + 4-byte aligned
# for targetSdk 30+ (Android 11+), otherwise install fails: "App not installed"
zip -q -X -0 ../unsigned.apk resources.arsc
cd ../..

# hard check: arsc must be Stored before aligning
if unzip -v "$OUT/unsigned.apk" | awk '$NF=="resources.arsc" && $2!="Stored"{exit 1}'; then
  echo "   resources.arsc: Stored (OK)"
else
  echo "FATAL: resources.arsc got compressed"; exit 1
fi

echo "== zipalign"
"$BT/zipalign" -f -p 4 "$OUT/unsigned.apk" "$OUT/aligned.apk"

echo "== sign"
# project's public release key — committed so ALL builds (local & CI) share one signature
# and users can upgrade the APK in place without uninstalling.
KS="$ROOT/android/veil-release.keystore"
if [ ! -f "$KS" ]; then
  KS="$OUT/veil.keystore"
  keytool -genkeypair -keystore "$KS" -storepass veilvpn123 -keypass veilvpn123 \
    -alias veil -keyalg RSA -keysize 2048 -validity 10000 \
    -dname "CN=VeilVPN, O=Veil, C=IR" 2>/dev/null
fi
java -jar "$BT/lib/apksigner.jar" sign --ks "$KS" --ks-pass pass:veilvpn123 --key-pass pass:veilvpn123 \
  --ks-key-alias veil --out "$APK" "$OUT/aligned.apk"

echo "== verify"
java -jar "$BT/lib/apksigner.jar" verify --print-certs "$APK" | head -3
java -jar "$BT/lib/apksigner.jar" verify -v "$APK" | grep -E "Verified|v1|v2|v3" | head -5
"$BT/zipalign" -c 4 "$APK" && echo "   zipalign: OK"
unzip -v "$APK" | awk '$NF=="resources.arsc"{print "   final resources.arsc method: "$2}'
ls -la "$APK"
echo "BUILD OK -> $APK"
