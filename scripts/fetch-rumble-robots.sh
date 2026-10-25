#!/usr/bin/env bash
# Downloads every robot of the RoboRumble 1-vs-1 participant list (about 1250
# classic Robocode robots, ~40 MB) and wraps them into Tank Royale bot
# directories with import-classic-robots.sh.
#
#   scripts/fetch-rumble-robots.sh [output, default bots/rumble]
#
# The robots are other people's code. Classic Robocode runs them in a security
# sandbox; here each one is an ordinary Java program. On Linux the app starts
# bots inside bubblewrap when it is installed (read-only system, home folder
# hidden); elsewhere they run with your permissions.
set -euo pipefail

OUT=${1:-bots/rumble}
CACHE=${XDG_CACHE_HOME:-$HOME/.cache}/claude_tanks/rumble
JARS=$CACHE/jars
mkdir -p "$JARS"
LIST=https://robowiki.net/wiki/RoboRumble/Participants?action=raw

echo "Fetching the participant list..."
curl -fsSL --retry 4 --retry-all-errors --retry-delay 2 -o "$CACHE/participants.txt" "$LIST"
# Lines look like "jk.mega.DrussGT 3.1.16,https://...": save as <class>_<version>.jar.
grep -E '^[^ ,]+ [^,]+,https?://' "$CACHE/participants.txt" | tr -d '\r' |
  awk -F, '{split($1, a, " "); print a[1] "_" a[2] ".jar\t" $2}' > "$CACHE/urls.tsv"
echo "$(wc -l < "$CACHE/urls.tsv") robots listed; downloading (already downloaded ones are kept)..."

fetch() {
  local out=$1/$2
  [ -s "$out" ] && return 0
  if curl -fsSL --max-time 60 --retry 2 --retry-delay 2 -o "$out.part" "$3" &&
    [ "$(head -c 2 "$out.part")" = "PK" ]; then
    mv "$out.part" "$out"
  else
    rm -f "$out.part"
    echo "  failed: $2 ($3)"
  fi
}
export -f fetch
tr '\t' '\n' < "$CACHE/urls.tsv" | xargs -d '\n' -n 2 -P 8 bash -c 'fetch "$0" "$1" "$2"' "$JARS"
echo "$(ls "$JARS"/*.jar | wc -l) robots downloaded."

"$(dirname "$0")/import-classic-robots.sh" "$JARS" "$OUT"
