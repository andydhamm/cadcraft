#!/usr/bin/env bash
# The stair is a probeable solid, not just triangles.
set -uo pipefail
b=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.7,"exterior":true}' \
  --cmd 'level {"above":0}' \
  --cmd 'stair {"start":[2,1],"direction":0,"width":1.0,"fromLevel":0,"toLevel":1}' \
  --cmd 'build3d {"probe":[[2.1,1.0,0.1],[2.6,1.0,0.1],[5.7,1.0,2.6],[2.1,1.0,1.0]]}' 2>/dev/null | tail -n 1) || { echo "FAIL: stair build crashed"; exit 1; }
python3 - "$b" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1]); P=d["probe"]
assert d.get("stairs",0)==1, f"build3d must count the stair: {d.get('stairs')}"
assert P[0] and P[0]["part"]=="stair", f"inside step 1 must be stair: {P[0]}"
assert P[1] and P[1]["part"]=="stair", f"inside step 3 must be stair: {P[1]}"
assert P[3] is None, f"the air above step 1 is empty: {P[3]}"
assert P[2] is None, f"nothing beyond the flight top: {P[2]}"
print("ok: stair is probeable")
PY
echo "PASS 03"
