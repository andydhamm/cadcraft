#!/usr/bin/env bash
set -uo pipefail
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'room {"points":[[0,0],[1,0],[1,1]]}' \
  --cmd 'room {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"type":"bogus"}' \
  --cmd 'room {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"level":"Nope"}' \
  --cmd 'room {"points":[[0,0],[1e308,0],[1e308,1e308],[0,1e308],[0,0]]}' \
  --cmd 'room {"points":[]}' --cmd 'room {"points":[[0,0]]}' \
  --cmd 'room {"id":"r999","type":"kitchen"}' --cmd 'room {"id":"r0","ceiling":-5}' \
  --cmd 'level {"above":"Nope"}' --cmd 'level {}' --cmd 'level {"ceiling":1e308}' \
  --cmd 'roomdetect {"dryRun":"yes"}' \
  --cmd 'build3d {"probe":[[0,0,0],[1e308,0,0],[1e308,1e308,1e308]]}' \
  --cmd 'arch.inspect {"rooms":true,"levels":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: hostile run crashed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1])
assert len(d.get("levels",[]))==1, f"failed LEVEL calls must not add levels: {d.get('levels')}"
assert d.get("rooms",[])==[], f"failed ROOM calls must not add rooms: {d.get('rooms')}"
print("ok: no half-applied edits")
PY
out2=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.7,"exterior":true}' \
  --cmd 'room {"points":[[0,0],[10,0],[10,8]]}' --cmd 'arch.inspect {"rooms":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: open polygon run crashed"; exit 1; }
python3 - "$out2" <<'PY' || exit 1
import json,sys
assert json.loads(sys.argv[1]).get("rooms")==[], "an unclosed room polygon must be refused"
print("ok: unclosed room refused")
PY
out3=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.7,"exterior":true}' \
  --cmd 'wall {"points":[[0,3],[10,3]],"thickness":0.1}' \
  --cmd 'roomdetect {}' --cmd 'undo {"count":1}' --cmd 'arch.inspect {"rooms":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: undo run crashed"; exit 1; }
python3 - "$out3" <<'PY' || exit 1
import json,sys
assert json.loads(sys.argv[1]).get("rooms")==[], "ROOMDETECT must be one undo step"
print("ok: roomdetect is undoable")
PY
echo "PASS 08"
