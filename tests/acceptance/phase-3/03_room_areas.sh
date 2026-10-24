#!/usr/bin/env bash
set -uo pipefail
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.7,"exterior":true}' \
  --cmd 'wall {"points":[[0,3],[10,3]],"thickness":0.1}' \
  --cmd 'wall {"points":[[6,3],[6,8]],"thickness":0.1}' \
  --cmd 'roomdetect {}' --cmd 'arch.inspect {"rooms":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: area demo failed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
R=json.loads(sys.argv[1])["rooms"]; assert len(R)==3, len(R)
S=sorted(round(r["standardArea"],4) for r in R); want=[20.91,31.11,31.62]
for a,w in zip(S,want): assert abs(a-w)<1e-3, f"standard areas {S} != {want}"
tot=sum(r["standardArea"] for r in R)
assert abs(tot-83.64)<1e-3, f"standard areas must sum to 10.2*8.2=83.64, got {tot}"
liv=sum(r["standardArea"] for r in R if r["livingIncluded"])
assert abs(liv-83.64)<1e-3, f"all three interior types: living {liv}"
print("ok: standard",S)
PY
out2=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.7,"exterior":true}' \
  --cmd 'wall {"points":[[0,3],[10,3]],"thickness":0.1}' \
  --cmd 'roomdetect {}' --cmd 'room {"id":"r0","type":"garage"}' --cmd 'room {"id":"r1","type":"deck"}' \
  --cmd 'room {"id":"r2","ceiling":1.0}' --cmd 'arch.inspect {"rooms":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: exclusion demo failed"; exit 1; }
python3 - "$out2" <<'PY' || exit 1
import json,sys
by={r["id"]:r for r in json.loads(sys.argv[1])["rooms"]}
for i in ("r0","r1","r2"): assert not by[i]["livingIncluded"], f"{i} must be excluded: {by[i]}"
liv=sum(r["standardArea"] for r in by.values() if r["livingIncluded"])
assert abs(liv)<1e-9, f"nothing left to count: {liv}"
print("ok: living-area exclusions")
PY
out3=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.7,"exterior":true}' \
  --cmd 'wall {"points":[[0,3],[10,3]],"thickness":0.1}' \
  --cmd 'wall {"points":[[6,3],[6,8]],"thickness":0.1}' \
  --cmd 'wall {"points":[[2.5,1],[3.5,1],[3.5,2],[2.5,2],[2.5,1]],"thickness":0.1}' \
  --cmd 'roomdetect {}' --cmd 'arch.inspect {"rooms":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: hole demo failed"; exit 1; }
python3 - "$out3" <<'PY' || exit 1
import json,sys
R=json.loads(sys.argv[1])["rooms"]
assert len(R)==4, f"the loop must add a room of its own: {[(r['name'],r['interiorArea']) for r in R]}"
south=[r for r in R if abs(r["interiorArea"]-26.72)<1e-3]
assert south and south[0]["holes"]==1, f"south room must be 27.93-1.1^2=26.72 with one hole: {[(r['name'],r['interiorArea'],r['holes']) for r in R]}"
small=[r for r in R if abs(r["interiorArea"]-0.81)<1e-3]
assert small and small[0]["holes"]==0, f"inner room must be 0.81: {[(r['name'],r['interiorArea']) for r in R]}"
print("ok: hole + inner room")
PY
echo "PASS 03"
