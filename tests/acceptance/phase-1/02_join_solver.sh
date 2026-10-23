#!/usr/bin/env bash
# The join solver must reach the user through `arch.inspect {poche:true}`: exterior
# corners mitre, interior walls butt at their host's face and never overshoot it.
set -uo pipefail
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'wall {"points":[[0,3],[10,3]],"thickness":0.1,"type":"interior"}' \
  --cmd 'wall {"points":[[6,3],[6,8]],"thickness":0.1,"type":"interior"}' \
  --cmd 'arch.inspect {"poche":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: cli run failed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1])
sh=[(w,w["polygon"]) for w in d.get("walls",[]) if w.get("polygon")]
assert len(sh)==6, f"expected 6 poche polygons, got {len(sh)}"
allx=[]; ally=[]
for w,p in sh:
    assert len(p)>=4 and p[0]!=p[-1], f"open ring: {p}"
    xs=[a[0] for a in p]; ys=[a[1] for a in p]
    assert all(abs(v)<1e9 for v in xs+ys), f"non-finite vertex in {w['id']}"
    assert min(xs)>=-0.100001 and max(xs)<=10.100001, f"{w['id']} escapes envelope x {min(xs)},{max(xs)}"
    assert min(ys)>=-0.100001 and max(ys)<=8.100001, f"{w['id']} escapes envelope y {min(ys)},{max(ys)}"
    allx+=xs; ally+=ys
# the mitred outer corners reach the envelope exactly
assert abs(min(allx)+0.1)<1e-6 and abs(max(allx)-10.1)<1e-6, f"outer x envelope {min(allx)},{max(allx)}"
assert abs(min(ally)+0.1)<1e-6 and abs(max(ally)-8.1)<1e-6, f"outer y envelope {min(ally)},{max(ally)}"

def ring(wid):
    for w,p in sh:
        if w["id"]==wid: return p
    raise AssertionError(f"no polygon for wall {wid}")
def has(p,x,y):
    return any(abs(a[0]-x)<1e-6 and abs(a[1]-y)<1e-6 for a in p)

# the bottom exterior wall mitres into both neighbours (no raw square corners)
bottom=None
for w,p in sh:
    if abs(w["start"][0])<1e-9 and abs(w["start"][1])<1e-9 and abs(w["end"][0]-10)<1e-9 and abs(w["end"][1])<1e-9:
        bottom=p
assert bottom is not None, "bottom wall (0,0)-(10,0) missing"
for c in ((-0.1,-0.1),(10.1,-0.1),(9.9,0.1),(0.1,0.1)):
    assert has(bottom,*c), f"bottom wall not mitred, missing corner {c}: {bottom}"
assert not has(bottom,10.0,0.1) and not has(bottom,10.0,-0.1), f"bottom wall still has raw corners: {bottom}"

# interior walls butt at the exterior faces: strictly inside the inner envelope
int_=[(w,p) for w,p in sh if not w.get("exterior")]
assert len(int_)==2, f"expected 2 interior walls, got {len(int_)}"
for w,p in int_:
    xs=[a[0] for a in p]; ys=[a[1] for a in p]
    assert min(xs)>=0.099999 and max(xs)<=9.900001, f"interior {w['id']} overshoots the inner x envelope: {min(xs)},{max(xs)}"
    assert min(ys)>=0.099999 and max(ys)<=7.900001, f"interior {w['id']} overshoots the inner y envelope: {min(ys)},{max(ys)}"
print("ok: joins mitre/butt inside the envelope")
PY
echo "PASS 02"
