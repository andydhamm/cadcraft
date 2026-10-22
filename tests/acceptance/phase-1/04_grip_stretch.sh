#!/usr/bin/env bash
set -uo pipefail
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'arch.wall.stretch {"wall":"w4","end":"start","to":[0,4]}' \
  --cmd 'arch.inspect {}' 2>/dev/null | tail -n 1) || { echo "FAIL: stretch run failed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1]); w=d.get("walls",[])
assert w, "no walls"
m=[x for x in w if abs(x["start"][1]-4.0)<1e-6 or abs(x["end"][1]-4.0)<1e-6]
assert len(m)>=2, f"stretching a corner must move both neighbours, moved {len(m)}"
print("ok:",len(m),"walls follow the corner")
PY
out2=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'arch.wall.stretch {"wall":"w4","end":"start","to":[0,4]}' \
  --cmd 'undo {"count":1}' --cmd 'arch.inspect {}' 2>/dev/null | tail -n 1) || { echo "FAIL: undo run failed"; exit 1; }
python3 - "$out2" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1])
ys=[a for x in d.get("walls",[]) for a in (x["start"][1],x["end"][1])]
assert len(d.get("walls",[]))==4, f"undo lost walls: {len(d.get('walls',[]))}"
assert not any(abs(y-4.0)<1e-6 for y in ys), f"undo did not restore the plan: {ys}"
print("ok: undo restores walls")
PY
echo "PASS 04"
