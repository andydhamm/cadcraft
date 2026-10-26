#!/usr/bin/env bash
# ROOFPLANE draws one plane from polygon + base edge + pitch, with fascia and soffit.
# A ROOFAUTO roof rebuilds when its walls change, and stops once a plane is edited by hand.
set -uo pipefail
W='wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}'
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'roofplane {"points":[[0,0],[6,0],[6,4],[0,4],[0,0]],"base":[0,0,6,0],"pitchRise":6,"pitchRun":12,"thickness":0.25,"fascia":true,"soffit":true}' \
  --cmd 'arch.inspect {"roofs":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: roofplane crashed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
R=json.loads(sys.argv[1])["roofs"]
assert len(R)==1, f"one manual roof expected: {R}"
r=R[0]
assert r.get("manual") is True, f"a ROOFPLANE roof is manual: {r}"
assert len(r["planes"])==1, f"one plane: {r['planes']}"
p=r["planes"][0]
assert abs(p["slope"]-0.5)<1e-12, f"pitch 6/12: {p}"
assert abs(p["projectedArea"]-24.0)<1e-6, f"6x4 projects to 24 m^2: {p}"
assert abs(r["thickness"]-0.25)<1e-12 and r.get("fascia") is True and r.get("soffit") is True, r
PY
b=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'roofplane {"points":[[0,0],[6,0],[6,4],[0,4],[0,0]],"base":[0,0,6,0],"pitchRise":6,"pitchRun":12,"thickness":0.25,"fascia":true,"soffit":true}' \
  --cmd 'build3d {}' 2>/dev/null | tail -n 1) || { echo "FAIL: roofplane build crashed"; exit 1; }
python3 - "$b" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1])
assert d.get("roofs",0)>=1, f"the roof must build meshes: {d}"
assert d["bounds"]["max"][2] > 2.0, f"a pitched plane must rise: {d['bounds']}"
PY
o1=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$W" --cmd 'roofauto {"pitchRise":6,"pitchRun":12}' \
  --cmd 'wall {"points":[[0,8],[12,8]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'arch.inspect {"roofs":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: rebuild run crashed"; exit 1; }
python3 - "$o1" <<'PY' || exit 1
import json,sys
r=json.loads(sys.argv[1])["roofs"][0]
assert abs(r["projectedArea"]-83.64)>1.0, f"the roof must follow the changed walls: {r['projectedArea']}"
assert r.get("handEdited") is not True, f"a rebuilt roof is not hand-edited: {r}"
PY
o2=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$W" --cmd 'roofauto {"pitchRise":6,"pitchRun":12}' \
  --cmd 'roofplane {"edit":"r0p0","pitchRise":12,"pitchRun":12}' \
  --cmd 'wall {"points":[[0,8],[12,8]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'arch.inspect {"roofs":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: hand-edit run crashed"; exit 1; }
python3 - "$o2" <<'PY' || exit 1
import json,sys
r=json.loads(sys.argv[1])["roofs"][0]
assert r.get("handEdited") is True, f"editing a plane must mark the roof: {r}"
assert abs(r["projectedArea"]-83.64)<1e-6, f"a hand-edited roof must not rebuild: {r['projectedArea']}"
PY
echo "PASS 07"
