#!/usr/bin/env bash
set -uo pipefail
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'roomdetect {}' --save "$tmp/p.dxf" >/dev/null 2>&1 || { echo "FAIL: run failed"; exit 1; }
cargo run -q -p cadcraft-cli -- render "$tmp/p.dxf" "$tmp/f.png" --size 320x240 --adapter fallback 2>"$tmp/f.log"
[ $? -eq 0 ] || { echo "FAIL: --adapter fallback (no SKIP from phase 6)"; head -5 "$tmp/f.log"; exit 1; }
[ -s "$tmp/f.png" ] || { echo "FAIL: no png from the fallback adapter"; exit 1; }
grep -q '^DEVICE ' "$tmp/f.log" || { echo "FAIL: render must print a DEVICE line"; head -5 "$tmp/f.log"; exit 1; }
grep -q 'limits=downlevel_webgl2' "$tmp/f.log" || { echo "FAIL: not downlevel WebGL2 limits: $(grep DEVICE "$tmp/f.log")"; exit 1; }
cargo run -q -p cadcraft-cli -- render "$tmp/p.dxf" "$tmp/g.png" --adapter nonsense 2>/dev/null
[ $? -ne 0 ] || { echo "FAIL: --adapter nonsense accepted"; exit 1; }
cargo test -p cadcraft-render3d --lib -- device_limits_are_downlevel_webgl2 2>&1 | tee "$tmp/t.log" >/dev/null
grep -q 'test result: ok' "$tmp/t.log" || { echo "FAIL: device_limits_are_downlevel_webgl2"; tail -5 "$tmp/t.log"; exit 1; }
echo "PASS 07"
