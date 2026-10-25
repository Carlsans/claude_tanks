#!/usr/bin/env bash
# Turns classic Robocode robots (robot .jar files, as in Robocode's robots
# folder or the RoboRumble) into Tank Royale bot directories that the
# Robocode tab and `royale` can start. Uses the official robocode-api-bridge,
# compiled here with javac (no Gradle needed).
#
#   scripts/import-classic-robots.sh <folder with robot jars> [output, default bots/classic]
#
# Needs a JDK 11+ (java, javac, jar) and curl. Downloads go to
# ~/.cache/claude_tanks/bridge and are reused.
set -euo pipefail

if [ $# -lt 1 ]; then
  echo "usage: $0 <folder with robot jars> [output folder]" >&2
  exit 1
fi
SRC=$(cd "$1" && pwd)
OUT=${2:-bots/classic}
mkdir -p "$OUT/lib"
OUT=$(cd "$OUT" && pwd)

BRIDGE_COMMIT=07108c333f48dad77237d453974aef0a414de07d
MAVEN=https://repo1.maven.org/maven2
CACHE=${XDG_CACHE_HOME:-$HOME/.cache}/claude_tanks/bridge
mkdir -p "$CACHE"
cd "$CACHE"

fetch() { [ -s "$2" ] || curl -fsSL --retry 4 --retry-all-errors --retry-delay 2 -o "$2" "$1"; }
fetch "$MAVEN/dev/robocode/tankroyale/robocode-tankroyale-bot-api/1.4.0/robocode-tankroyale-bot-api-1.4.0.jar" bot-api.jar
fetch "$MAVEN/org/apache/bcel/bcel/6.13.0/bcel-6.13.0.jar" bcel.jar
fetch "$MAVEN/org/apache/commons/commons-lang3/3.20.0/commons-lang3-3.20.0.jar" commons-lang3.jar
fetch "$MAVEN/commons-io/commons-io/2.22.0/commons-io-2.22.0.jar" commons-io.jar
# Robots written in Kotlin need its standard library, as classic Robocode ships it.
fetch "$MAVEN/org/jetbrains/kotlin/kotlin-stdlib/1.9.24/kotlin-stdlib-1.9.24.jar" kotlin-stdlib.jar
fetch "https://github.com/robocode-dev/robocode-api-bridge/archive/$BRIDGE_COMMIT.tar.gz" bridge.tar.gz

if [ ! -s robocode-api.jar ] || [ ! -d wrapper ]; then
  echo "Compiling the Robocode API bridge..."
  rm -rf src api wrapper && mkdir -p src api wrapper
  tar xzf bridge.tar.gz -C src --strip-components 1
  find src/robocode-api/src/main/java -name '*.java' > api-sources.txt
  javac --release 11 -nowarn -encoding UTF-8 -cp bot-api.jar -d api @api-sources.txt
  (cd api && jar cf ../robocode-api.jar .)
  javac --release 11 -nowarn -encoding UTF-8 -cp robocode-api.jar:bcel.jar -d wrapper \
    src/robots-wrapper/src/main/java/*.java
fi

cp "$SRC"/*.jar "$OUT"/
cp robocode-api.jar bot-api.jar kotlin-stdlib.jar "$OUT/lib/"
(cd "$OUT" && java -cp "$CACHE/wrapper:$CACHE/robocode-api.jar:$CACHE/bcel.jar:$CACHE/commons-lang3.jar:$CACHE/commons-io.jar" Main "$OUT")
echo "Done: point the Robocode tab's bot folder at $OUT"
