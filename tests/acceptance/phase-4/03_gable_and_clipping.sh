#!/usr/bin/env bash
# Gables on x=0 (w3) and x=10 (w1): two planes, ridge -0.1..10.1 at z 4.45.
# Wall-to-roof clipping proved with probes: the gable wall reaches 4.45 at y=4, eave walls stay 2.4.
set -uo pipefail
W='wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}'
RA='roofauto {"pitchRise":6,"pitchRun":12,"walls":[{"wall":"w1","directive":"gable"},{"wall":"w3","directive":"gable"}]}'
out=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$W" --cmd "$RA" --cmd 'arch.inspect {"roofs":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: gable demo crashed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
r=json.loads(sys.argv[1])["roofs"][0]; P=r["planes"]
assert len(P)==2, f"two gables -> two planes, got {len(P)}"
E=[e for e in r["edges"] if e["kind"]=="ridge"]; assert len(E)==1, E
e=E[0]; xs=sorted([e["a"][0],e["b"][0]])
assert abs(xs[0]+0.1)<1e-6 and abs(xs[1]-10.1)<1e-6, f"ridge -0.1..10.1: {e}"
assert abs(e["za"]-4.45)<1e-6, f"ridge z 4.45: {e}"
PY
b=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$W" --cmd "$RA" \
  --cmd 'build3d {"probe":[[10.0,4.0,3.0],[10.0,0.5,3.0],[5.0,0.0,2.5],[0.0,4.0,3.0],[5.0,4.0,4.6]]}' 2>/dev/null | tail -n 1) || { echo "FAIL: gable build crashed"; exit 1; }
python3 - "$b" <<'PY' || exit 1
import json,sys
g1,g2,e1,g3,sky=json.loads(sys.argv[1])["probe"]
assert g1 and g1["part"]=="wall", f"gable wall must reach z=3 at y=4: {g1}"
assert g3 and g3["part"]=="wall", f"the other gable wall too: {g3}"
assert g2 is None, f"gable wall must NOT rise at y=0.5: {g2}"
assert e1 is None, f"eave wall must stay at 2.4: {e1}"
assert sky and sky["part"]=="roof", f"above the ridge must be roof: {sky}"
print("ok: gable walls clipped to the roof underside")
PY
echo "PASS 03"
