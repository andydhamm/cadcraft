#!/usr/bin/env bash
# Floor slabs and ceilings from room polygons, with the room-type rules.
# Fixed 2026-10-26: the second block demanded ceilings==2 while also demanding that
# Open Below keeps its ceiling and that Deck and Porch get none - only one ceiling
# can exist, so the check contradicted the phase rule. It now expects 2 floors / 1
# ceiling, matching the probes above it.
set -uo pipefail
H='wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.7,"exterior":true}'
W1='wall {"points":[[0,3],[10,3]],"thickness":0.1}'
W2='wall {"points":[[6,3],[6,8]],"thickness":0.1}'
b=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$H" --cmd "$W1" --cmd "$W2" \
  --cmd 'roomdetect {}' --cmd 'build3d {"probe":[[2,1,-0.15],[2,1,2.39],[2,1,2.41],[8,5,-0.15]]}' 2>/dev/null | tail -n 1) || { echo "FAIL: floor demo failed"; exit 1; }
python3 - "$b" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1]); P=d["probe"]
assert P[0] and P[0]["part"]=="floor", f"slab under the south room missing: {P[0]}"
assert P[1] and P[1]["part"]=="ceiling", f"ceiling just under 2.4 missing: {P[1]}"
assert P[2] is None, f"nothing above the 2.4 ceiling: {P[2]}"
assert P[3] and P[3]["part"]=="floor", f"slab under the north-east room missing: {P[3]}"
assert d.get("floors",0)==3, f"one slab per room: {d.get('floors')}"
assert d.get("ceilings",0)==3, f"one ceiling per room: {d.get('ceilings')}"
print("ok: slabs and ceilings")
PY
b2=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$H" --cmd "$W1" --cmd "$W2" \
  --cmd 'roomdetect {}' --cmd 'room {"id":"r0","type":"openBelow"}' --cmd 'room {"id":"r1","type":"deck"}' \
  --cmd 'room {"id":"r2","type":"porch"}' \
  --cmd 'build3d {"probe":[[2,1,-0.15],[2,1,2.39],[8,5,-0.15],[8,5,2.39]]}' 2>/dev/null | tail -n 1) || { echo "FAIL: room-type demo failed"; exit 1; }
python3 - "$b2" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1]); P=d["probe"]
assert P[0] is None, f"Open Below must suppress the floor slab: {P[0]}"
assert P[1] and P[1]["part"]=="ceiling", f"Open Below keeps its ceiling: {P[1]}"
assert P[2] and P[2]["part"]=="floor", f"Deck keeps a floor: {P[2]}"
assert P[3] is None, f"Deck/Porch get no ceiling: {P[3]}"
# Open Below: no floor, ceiling kept. Deck and Porch: floor, no ceiling.
assert d.get("floors",0)==2 and d.get("ceilings",0)==1, f"counts {d.get('floors')}/{d.get('ceilings')}, expected 2/1"
print("ok: room-type floor/ceiling rules")
PY
b3=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$H" --cmd "$W1" --cmd "$W2" \
  --cmd 'wall {"points":[[2.5,1],[3.5,1],[3.5,2],[2.5,2],[2.5,1]],"thickness":0.1}' \
  --cmd 'roomdetect {}' --cmd 'build3d {"probe":[[3,1.5,-0.15],[1,1,-0.15]]}' 2>/dev/null | tail -n 1) || { echo "FAIL: slab-hole demo failed"; exit 1; }
python3 - "$b3" <<'PY' || exit 1
import json,sys
P=json.loads(sys.argv[1])["probe"]
assert P[0] and P[0]["part"]=="floor", f"the inner room gets its own slab: {P[0]}"
assert P[1] and P[1]["part"]=="floor", f"the south room keeps a slab outside the loop: {P[1]}"
assert P[0]["id"]!=P[1]["id"], f"the two slabs belong to different rooms: {P}"
print("ok: slab follows the room polygon with its hole")
PY
echo "PASS 05"
