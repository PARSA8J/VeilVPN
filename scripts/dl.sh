#!/bin/bash
cd /home/z/my-project/tools
echo "== start $(date)" > dl.log
curl -sL -o bt.zip https://dl.google.com/android/repository/build-tools_r34-linux.zip && echo "bt ok" >> dl.log
curl -sL -o plat.zip https://dl.google.com/android/repository/platform-34-ext7_r03.zip && echo "plat ok" >> dl.log
curl -sL -o ecj.jar https://repo1.maven.org/maven2/org/eclipse/jdt/ecj/3.33.0/ecj-3.33.0.jar && echo "ecj ok" >> dl.log
curl -sL -o go.tar.gz https://go.dev/dl/go1.22.5.linux-amd64.tar.gz && echo "go ok" >> dl.log
unzip -o -q bt.zip && mkdir -p plat && unzip -o -q plat.zip -d plat "*/android.jar" && tar -xzf go.tar.gz && echo ALL_OK >> dl.log
