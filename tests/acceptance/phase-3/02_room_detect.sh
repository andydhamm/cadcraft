#!/usr/bin/env bash
# Room detection: the phase-1 house's three rooms, plus rooms enclosed by an
# invisible room-divider wall.
# Fixed 2026-10-26: `want` must be ascending because `A` is sorted; the brief
# lists the three areas by room (27.93, 28.3725, 18.6725), not in sorted order,
# so the old check compared a sorted list against an unsorted one and could never pass.
set -uo pipefail
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.7,"exterior":true}' \
  --cmd 'wall {"points":[[0,3],[10,3]],"thickness":0.1}' \
  --cmd 'wall {"points":[[6,3],[6,8]],"thickness":0.1}' \
  --cmd 'roomdetect {}' --cmd 'arch.inspect {"rooms":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: roomdetect demo failed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1]); R=d.get("rooms",[])
assert len(R)==3, f"expected 3 rooms, got {[(r['name'],r['interiorArea']) for r in R]}"
for r in R:
    assert r["id"].startswith("r"), r
    assert r["handle"].startswith("a001"), f"room proxy handle missing: {r}"
    assert len(r["polygon"])>=3 and r["label"], r
    assert r["type"] in ("living","bedroom","kitchen","bath","closet","utility","garage","porch","deck","openBelow","other"), r["type"]
# interior surfaces: south 9.8 x 2.85 = 27.93, north-west 5.85 x 4.85 = 28.3725,
# north-east 3.85 x 4.85 = 18.6725.
A=sorted(round(r["interiorArea"],4) for r in R); want=[18.6725,27.93,28.3725]
assert len(A)==len(want), f"interior areas {A} != {want}"
for a,w in zip(A,want): assert abs(a-w)<1e-3, f"interior areas {A} != {want}"
def inside(poly,p):
    x,y=p; c=False
    for i in range(len(poly)):
        x1,y1=poly[i]; x2,y2=poly[(i+1)%len(poly)]
        if (y1>y)!=(y2>y) and x < (x2-x1)*(y-y1)/(y2-y1)+x1: c=not c
    return c
for r in R: assert inside(r["polygon"],r["label"]), f"label outside polygon: {r['id']} {r['label']}"
print("ok: rooms",A)
PY
out2=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.7,"exterior":true}' \
  --cmd 'wall {"points":[[5,0],[5,8]],"thickness":0.0,"divider":true}' \
  --cmd 'roomdetect {}' --cmd 'arch.inspect {"rooms":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: divider demo failed"; exit 1; }
python3 - "$out2" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1]); R=d.get("rooms",[])
assert len(R)==2, f"a room-divider wall must enclose rooms: {[(r['name'],r['interiorArea']) for r in R]}"
# interior faces span 0.1..9.9 by 0.1..7.9; a zero-thickness divider at x=5 gives 4.9 x 7.8 = 38.22
A=sorted(round(r["interiorArea"],4) for r in R)
assert abs(A[0]-38.22)<1e-3 and abs(A[1]-38.22)<1e-3, f"two 4.9x7.8 halves: {A}"
dv=[w for w in d["walls"] if abs(w["thickness"])<1e-9]
assert dv and "polygon" not in dv[0], f"divider wall must have no poche: {dv}"
print("ok: divider rooms",A)
PY
echo "PASS 02"
