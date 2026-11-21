#!/bin/bash
# Windows UI test of the TMDB lookup (Plex tab) under Wine, against the local
# mock server (tests/mock_tmdb.py). Nothing goes to the internet and no real
# key is used: the INI gets a made-up 32-hex key.
#
#   WINEPREFIX=... DISPLAY=:NN EXE='C:\...\Organimizer.exe' tests/wine_lookup_flow.sh OUTDIR
#
# Needs: wine, xdotool, ImageMagick (import / convert), python3, an X server
# of 1600x1000 on $DISPLAY with a window manager, and Organimizer.ini in the
# prefix set to: mode = plex, width = 1500, height = 940, x = y = 10, the
# Plex source = a folder with the demo files below, tmdb_enabled = true,
# tmdb_key = 0123456789abcdef0123456789abcdef.
#   Alien.1979.1080p.BluRay.x264/{Alien.1979.1080p.BluRay.x264.mkv, alien making of.mkv,
#     Alien.1979.Trailer.mkv, Alien (1979) Featurette - The Beast Within.mkv, *.en.srt}
#   The Expanse/Season2/{Expanse - 2x01 - Safe.mkv, The Expanse S2 E2.mkv,
#     The.Expanse.S02E03.720p.mkv, the expanse s2e5.mkv}
# The app reads the hidden debug overrides below only for 127.0.0.1 / localhost.
# Coordinates are for that window size; egui under Wine repaints late, so
# every step hovers first and waits.
set -eu
OUT=${1:-/tmp/lookup-flow}
PORT=${PORT:-8799}
mkdir -p "$OUT"
HERE=$(cd "$(dirname "$0")" && pwd)
LOG="$OUT/mock.log"
python3 "$HERE/mock_tmdb.py" "$PORT" 2>"$LOG" &
MOCK=$!
trap 'kill $MOCK 2>/dev/null; kill $APP 2>/dev/null || true' EXIT
export WINEDEBUG=-all ORGANIMIZER_TMDB_API=http://127.0.0.1:$PORT/3 ORGANIMIZER_TMDB_IMAGES=http://127.0.0.1:$PORT/t/p
wine "$EXE" >/dev/null 2>&1 &
APP=$!
sleep 15

win() { xdotool search --onlyvisible --name "Organimizer" | head -1; }
# click at 1024x640 preview coordinates (x1.5625)
click() {
  xdotool windowactivate --sync "$(win)" 2>/dev/null || true
  local x=$(( $1 * 25 / 16 )) y=$(( $2 * 25 / 16 ))
  xdotool mousemove $x $y; sleep 1.5; xdotool mousemove $((x + 1)) $y; sleep 1.5
  xdotool mousedown 1; sleep 0.25; xdotool mouseup 1; sleep 3
}
settle() { for i in 1 2 3; do xdotool mousemove $((1250 + i * 5)) 905; sleep 2; done; xdotool key shift; sleep 3; }
shot() { settle; import -window root "$OUT/$1.png"; convert "$OUT/$1.png" -crop 1515x985+0+0 +repage "$OUT/$1.png"; }
expect() { grep -q "$1" "$LOG" || { echo "FAIL: no request $1"; cat "$LOG"; exit 1; }; }

click 911 126; settle                    # Scan
click 50 223; sleep 6; shot results      # Look up: Alien (1979)
expect "GET /3/search/movie 200"
expect "GET /t/p/w92/alien-poster.jpg 200"
click 655 258; sleep 6; settle           # This one: Alien
expect "GET /3/movie/348 200"
click 290 409; settle                    # OK - use these names
click 50 245; sleep 6; settle            # Look up: The Expanse - Season 2
expect "GET /3/search/tv 200"
click 660 323; sleep 6; shot match       # This one: The Expanse
expect "GET /3/tv/63639 200"
expect "GET /3/tv/63639/season/2 200"
click 290 409; shot preview              # OK
if grep -q " 401" "$LOG"; then echo "FAIL: a request went without the key"; exit 1; fi
echo "PASS: lookup flow (mock TMDB), screenshots in $OUT"
