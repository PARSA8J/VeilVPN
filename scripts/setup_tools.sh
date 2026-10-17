#!/bin/bash
# Download Android build-tools + platform + ecj + Go into ./tools/
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TOOL="$ROOT/tools"
mkdir -p "$TOOL" && cd "$TOOL"
[ -f bt/android-14/aapt2 ] || { curl -sL -o bt.zip https://dl.google.com/android/repository/build-tools_r34-linux.zip && mkdir -p bt && unzip -o -q bt.zip -d bt; }
[ -f plat/android-34/android.jar ] || { curl -sL -o plat.zip https://dl.google.com/android/repository/platform-34-ext7_r03.zip && mkdir -p plat && unzip -o -q plat.zip -d plat "*/android.jar"; }
# strip java/javax from android.jar to avoid split-package issues with ecj on modern JDKs
[ -f plat/android-34/android-core.jar ] || { cp plat/android-34/android.jar plat/android-34/android-core.jar && zip -q -d plat/android-34/android-core.jar 'java/*' 'javax/*'; }
[ -f ecj.jar ] || curl -sL -o ecj.jar https://repo1.maven.org/maven2/org/eclipse/jdt/ecj/3.33.0/ecj-3.33.0.jar
if ! command -v go >/dev/null && [ ! -f go/bin/go ]; then curl -sL -o go.tar.gz https://go.dev/dl/go1.22.5.linux-amd64.tar.gz && tar -xzf go.tar.gz; fi
echo "tools ready in $TOOL"
