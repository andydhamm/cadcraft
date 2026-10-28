#!/usr/bin/env bash
# Stair well: the upper floor is holed where headroom under the slab is under 2.032 m.
# Riser 3 top z=0.50625, slab underside 2.4, headroom 1.89375 < 2.032 -> well x 2.508..5.81, area 3.302.
set -uo pipefail
H='wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.7,"exterior":true}'
H2='wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.7,"exterior":true,"level":"Level 2"}'
ST='stair {"start":[2,1],"direction":0,"width":1.0,"fromLevel":0,"toLevel":1}'
RUNS=(--cmd "$H" --cmd 'level {"above":0}' --cmd "$H2" --cmd 'roomdetect {"level":1}' --cmd "$ST")
b=$(cargo run -q -p cadcraft-cli -- run --metric "${RUNS[@]}" \
  --cmd 'build3d {"probe":[[2.6,1.0,2.538],[2.1,1.0,2.538],[2.6,1.0,2.2],[8,4,2.538]]}' 2>/dev/null | tail -n 1) || { echo "FAIL: well demo crashed"; exit 1; }
python3 - "$b" <<'PY' || exit 1
import json,sys
P=json.loads(sys.argv[1])["probe"]
assert P[0] is None, f"2.032 m above the 3rd riser must be a hole in the Level-2 slab: {P[0]}"
assert P[1] and P[1]["part"]=="floor", f"over riser 1 the slab stays: {P[1]}"
assert P[2] is None, f"the well is open below: {P[2]}"
assert P[3] and P[3]["part"]=="floor", f"the rest of the Level-2 floor is intact: {P[3]}"
print("ok: stair well holed the upper slab")
PY
r=$(cargo run -q -p cadcraft-cli -- run --metric "${RUNS[@]}" \
  --cmd 'arch.inspect {"rooms":true,"stairs":true,"railings":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: well inspect crashed"; exit 1; }
python3 - "$r" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1])
W=[x for x in d.get("rooms",[]) if x["type"]=="openBelow" and x["level"]=="Level 2"]
assert len(W)==1, f"the well must be one Open Below room on Level 2: {[(x['type'],x['level']) for x in d.get('rooms',[])]}"
assert abs(W[0]["interiorArea"]-3.302)<1e-3, f"well area (5.81-2.508)*1.0 = 3.302: {W[0]['interiorArea']}"
big=[x for x in d["rooms"] if x["level"]=="Level 2" and x["type"]!="openBelow"]
assert len(big)==1, f"one enclosing room on Level 2: {[(x['type'],x['level']) for x in d['rooms']]}"
H=big[0].get("holes",[])
assert isinstance(H,list) and len(H)==1, f"the enclosing room gains the well as a hole: {big[0].get('holes')}"
assert abs(H[0][0][0]-2.508)<1e-6, f"the hole is the well ring: {H[0]}"
R=[x for x in d.get("railings",[]) if x.get("level")==1 and x.get("follows")]
assert len(R)==1, f"a guard railing must ring the well: {d.get('railings')}"
assert abs(R[0]["height"]-0.914)<1e-9, f"guards are 0.914 at wells: {R[0]['height']}"
print("ok: well room, hole and guard railing")
PY
echo "PASS 04"
