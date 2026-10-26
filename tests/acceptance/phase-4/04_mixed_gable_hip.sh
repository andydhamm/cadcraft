#!/usr/bin/env bash
# One gable (x=10) + hips elsewhere: three planes, ridge x 4.0..10.1 at z 4.45.
set -uo pipefail
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'roofauto {"pitchRise":6,"pitchRun":12,"walls":[{"wall":"w1","directive":"gable"}]}' \
  --cmd 'arch.inspect {"roofs":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: mixed roof crashed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
r=json.loads(sys.argv[1])["roofs"][0]; P=r["planes"]
assert len(P)==3, f"one gable + hips -> three planes, got {len(P)}: {P}"
E=[e for e in r["edges"] if e["kind"]=="ridge"]; assert len(E)==1, E
e=E[0]; xs=sorted([e["a"][0],e["b"][0]])
assert abs(xs[0]-4.0)<1e-6 and abs(xs[1]-10.1)<1e-6, f"ridge x 4.0..10.1: {e}"
assert abs(e["za"]-4.45)<1e-6 and abs(e["zb"]-4.45)<1e-6, f"ridge z 4.45: {e}"
assert abs(r["projectedArea"]-83.64)<1e-6, r["projectedArea"]
print("ok: mixed gable+hip, ridge", xs)
PY
echo "PASS 04"
