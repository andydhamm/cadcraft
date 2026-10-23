#!/usr/bin/env bash
set -uo pipefail
W='wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}'
out=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$W" \
  --cmd 'door {"wall":"w0","offset":3.0,"width":0.9,"height":2.1}' \
  --cmd 'arch.opening.move {"opening":"o0","offset":6.5}' \
  --cmd 'arch.inspect {"openings":true}' --cmd 'build3d {}' 2>/dev/null | tail -n 2) || { echo "FAIL: drag failed"; exit 1; }
python3 - "$(echo "$out"|head -n 1)" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1]); o=d.get("openings",[])
assert len(o)==1 and abs(o[0]["offset"]-6.5)<1e-6, f"2D offset not moved: {o}"
assert o[0]["host"]=="w0" and abs(o[0]["width"]-0.9)<1e-9, o
print("ok: 2D offset moved")
PY
python3 - "$(echo "$out"|tail -n 1)" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1])
assert d.get("openings")==1 and d.get("meshes",0)>=6, f"3D did not rebuild: {d}"
print("ok: 3D rebuilt")
PY
cargo run -q -p cadcraft-cli -- run --metric --cmd "$W" \
  --cmd 'door {"wall":"w0","offset":3.0,"width":0.9}' \
  --cmd 'arch.opening.move {"opening":"o0","offset":9.9}' >/dev/null 2>&1 \
  && { echo "FAIL: drag past the wall end must be refused"; exit 1; }
u=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$W" \
  --cmd 'door {"wall":"w0","offset":3.0,"width":0.9}' \
  --cmd 'arch.opening.move {"opening":"o0","offset":6.5}' \
  --cmd 'undo {"count":1}' --cmd 'arch.inspect {"openings":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: undo failed"; exit 1; }
python3 - "$u" <<'PY' || exit 1
import json,sys
o=json.loads(sys.argv[1]).get("openings",[])
assert len(o)==1 and abs(o[0]["offset"]-3.0)<1e-6, f"undo must restore the offset: {o}"
print("ok: undo restores the offset")
PY
s=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$W" \
  --cmd 'door {"wall":"w0","offset":3.0,"width":0.9}' \
  --cmd 'arch.wall.stretch {"wall":"w0","end":"end","to":[12,0]}' \
  --cmd 'arch.inspect {"openings":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: stretch failed"; exit 1; }
python3 - "$s" <<'PY' || exit 1
import json,sys
o=json.loads(sys.argv[1]).get("openings",[])
assert len(o)==1 and abs(o[0]["offset"]-3.0)<1e-6, f"opening must follow its host wall: {o}"
print("ok: opening follows its host wall")
PY
echo "PASS 04"
