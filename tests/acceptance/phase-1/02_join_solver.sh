#!/usr/bin/env bash
set -uo pipefail
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'wall {"points":[[0,3],[10,3]],"thickness":0.1}' \
  --cmd 'wall {"points":[[6,3],[6,8]],"thickness":0.1}' \
  --cmd 'arch.inspect {"poché":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: cli run failed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1])
sh=[(w,w["polygon"]) for w in d.get("walls",[]) if w.get("polygon")]
assert len(sh)==6, f"expected 6 poché polygons, got {len(sh)}"
for w,p in sh:
    assert len(p)>=4 and p[0]!=p[-1], f"open ring: {p}"
    xs=[a[0] for a in p]; ys=[a[1] for a in p]
    assert all(abs(v)<1e9 for v in xs+ys), f"non-finite vertex in {w['id']}"
    assert min(xs)>=-0.100001 and max(xs)<=10.100001, f"{w['id']} escapes envelope x {min(xs)},{max(xs)}"
    assert min(ys)>=-0.100001 and max(ys)<=8.100001, f"{w['id']} escapes envelope y {min(ys)},{max(ys)}"
int_=[(w,p) for w,p in sh if not w.get("exterior")]
assert len(int_)==2, f"expected 2 interior walls, got {len(int_)}"
for w,p in int_:
    xs=[a[0] for a in p]; ys=[a[1] for a in p]
    assert min(xs)>=0.099999 and max(xs)<=10.100001, f"interior {w['id']} overshoots x: {min(xs)},{max(xs)}"
    assert min(ys)>=2.999999 and max(ys)<=8.100001, f"interior {w['id']} overshoots y: {min(ys)},{max(ys)}"
print("ok: joins mitre/butt inside the envelope")
PY
echo "PASS 02"
