#!/usr/bin/env bash
# Hostile stair/railing/deck input never panics and never leaves a half-applied object.
set -uo pipefail
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'stair {}' --cmd 'stair {"start":[0,0],"direction":0,"width":-1,"fromLevel":0,"toLevel":1}' \
  --cmd 'stair {"start":[0,0],"direction":0,"width":1,"fromLevel":99,"toLevel":1}' \
  --cmd 'stair {"start":[0,0],"direction":0,"width":1,"rise":1e308}' --cmd 'stair {"start":[0,0],"direction":0,"width":1,"rise":0}' \
  --cmd 'stair {"start":[0,0],"direction":0,"width":1,"rise":0.01}' --cmd 'stair {"start":[0,0],"direction":0,"width":1,"rise":2.7,"shape":"spiral"}' \
  --cmd 'railing {"path":[]}' --cmd 'railing {"path":[[0,0]]}' --cmd 'railing {"path":[[0,0],[4,0]],"height":1e308}' \
  --cmd 'railing {"path":[[0,0],[4,0]],"style":"glass"}' \
  --cmd 'deck {"points":[[0,0],[1,0]]}' --cmd 'deck {"points":[[0,0],[4,0],[4,3],[0,3],[0,0]],"height":-2}' \
  --cmd 'deck {"points":[[0,0],[4,0],[4,3],[0,3],[0,0]],"height":1e308}' \
  --cmd 'arch.inspect {"stairs":true,"railings":true,"rooms":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: hostile stair run crashed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1])
assert d.get("stairs",[])==[], f"failed stair calls must add nothing: {d.get('stairs')}"
assert d.get("railings",[])==[], f"failed railing calls must add nothing: {d.get('railings')}"
assert d.get("rooms",[])==[], f"failed deck calls must add nothing: {d.get('rooms')}"
print("ok: hostile stair input is refused cleanly")
PY
cargo run -q -p cadcraft-cli -- run --metric --cmd 'stair {"start":[0,0],"direction":0,"width":1,"rise":0.01}' >/dev/null 2>/tmp/p5s.err
grep -qi "stair" /tmp/p5s.err || { echo "FAIL: a 10 mm rise must be refused"; exit 1; }
echo "PASS 08"
