#!/usr/bin/env bash
set -uo pipefail
H='wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.7,"exterior":true}'
out=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$H" \
  --cmd 'level {"above":"Level 1"}' \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"level":"Level 2","exterior":true}' \
  --cmd 'wall {"points":[[5,0],[5,8]],"thickness":0.1,"level":"Level 2"}' \
  --cmd 'roomdetect {"level":"Level 2"}' --cmd 'arch.inspect {"rooms":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: two-storey demo failed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
R=json.loads(sys.argv[1])["rooms"]
l2=[r for r in R if r["level"]=="Level 2"]
assert len(l2)==2, f"Level 2 must give two rooms: {[(r['name'],r['level'],r['interiorArea']) for r in R]}"
A=sorted(round(r["interiorArea"],4) for r in l2)
assert abs(A[0]-23.66)<1e-3 and abs(A[1]-23.66)<1e-3, f"two 4.85x4.9 halves: {A}"
print("ok: Level-2 rooms",A)
PY
b=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$H" \
  --cmd 'level {"above":"Level 1"}' \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"level":"Level 2","exterior":true}' \
  --cmd 'wall {"points":[[5,0],[5,8]],"thickness":0.1,"level":"Level 2"}' \
  --cmd 'roomdetect {}' \
  --cmd 'build3d {"probe":[[2,1,2.55],[2,1,1.0],[2,1,-0.15],[12,1,2.55]]}' 2>/dev/null | tail -n 1) || { echo "FAIL: probe build failed"; exit 1; }
python3 - "$b" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1]); P=d.get("probe")
assert isinstance(P,list) and len(P)==4, f"build3d must answer every probe: {P}"
assert P[0] and P[0]["part"]=="floor" and P[0]["id"].startswith("r"), f"(2,1,2.55) must hit a Level-2 slab: {P[0]}"
assert abs(P[0]["level"]-2.7)<1e-6, f"must be the Level-2 slab: {P[0]}"
assert P[1] is None, f"(2,1,1.0) is inside the house volume, not a solid: {P[1]}"
assert P[2] and P[2]["part"]=="floor" and abs(P[2]["level"])<1e-9, f"(2,1,-0.15) must hit the Level-1 slab: {P[2]}"
assert P[3] is None, f"(12,1,2.55) is outside the building: {P[3]}"
print("ok: probes",P)
PY
echo "PASS 04"
