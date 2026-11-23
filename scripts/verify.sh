#!/usr/bin/env bash
# Prüflauf: bauen, starten, in zwei Breiten ablichten.
# Aufruf: scripts/verify.sh [pc|mobile|beide]

set -uo pipefail
cd /home/kev/grace-blatt-rs

OUT="${OUT:-/home/kev/.hermes/cache/scratch}"
mkdir -p "$OUT"

echo "── bauen ───────────────────────────────────────────"
if ! cargo build -p aesmyra-app 2>&1 | tail -3; then
  echo "Build fehlgeschlagen"; exit 1
fi

if [ ! -f ui/app.js ]; then
  echo "ui/app.js fehlt — Abbruch"; exit 1
fi
if ! node --check ui/app.js; then
  echo "app.js hat einen Syntaxfehler — Abbruch"; exit 1
fi

# Cargo bindet die Anwendung nicht neu, wenn sich nur Dateien unter ui/ ändern:
# die eingebetteten Daten ändern den Metadaten-Hash der Bibliothek nicht.
# Deshalb beide Einstiegspunkte berühren.
touch crates/aesmyra-app/src/lib.rs crates/aesmyra-app/src/main.rs

echo "── starten ─────────────────────────────────────────"
export DISPLAY=:0
export XAUTHORITY="$(ls -t /run/user/1000/xauth_* 2>/dev/null | head -1)"
pkill -f 'target/debug/aesmyra-app' 2>/dev/null
sleep 0.5
WEBKIT_DISABLE_DMABUF_RENDERER=1 ./target/debug/aesmyra-app >"$OUT/aesmyra.log" 2>&1 &
APP_PID=$!
sleep 5

if ! kill -0 "$APP_PID" 2>/dev/null; then
  echo "App beendet sich sofort:"; tail -20 "$OUT/aesmyra.log"; exit 1
fi

echo "── ablichten ───────────────────────────────────────"
shoot() {
  local name="$1" w="$2" h="$3"
  wmctrl -r Aesmyra. -e "0,60,60,$w,$h" 2>/dev/null
  sleep 1.4
  spectacle -b -n -f -o "$OUT/$name.png" 2>/dev/null
  echo "  $OUT/$name.png  ($(identify -format '%wx%h' "$OUT/$name.png" 2>/dev/null))"
}

case "${1:-beide}" in
  pc) shoot verify-pc 1600 1000 ;;
  mobile) shoot verify-mobile 430 900 ;;
  *) shoot verify-pc 1600 1000; shoot verify-mobile 430 900 ;;
esac

echo "── Fenster ─────────────────────────────────────────"
wmctrl -lG | grep -i aesmyra || true
echo "── Protokoll ───────────────────────────────────────"
tail -5 "$OUT/aesmyra.log" 2>/dev/null || true
echo "fertig"
