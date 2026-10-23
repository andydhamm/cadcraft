#!/usr/bin/env bash
# The plan shows filled poche (one filled ring per wall, spanning the wall envelope),
# and the arch model round-trips through DXF as CADCRAFT_ARCH + proxy linework.
set -uo pipefail
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'wall {"points":[[0,3],[10,3]],"thickness":0.1}' \
  --save "$tmp/p.dxf" --export "$tmp/p.svg" >/dev/null 2>&1 || { echo "FAIL: run failed"; exit 1; }
test -s "$tmp/p.svg" || { echo "FAIL: no svg"; exit 1; }
python3 - "$tmp/p.svg" <<'PY' || exit 1
import re,sys
s=open(sys.argv[1]).read()
filled=re.findall(r'<path[^>]*fill="(?!none)[^"]*"[^>]*?d="([^"]*)"',s)
n=len(filled)+len(re.findall(r'<polygon[^>]*fill="(?!none)',s))
assert n>=5, f"expected a filled poche ring per wall in the plan svg, got {n}"
pts=[]
for d in filled:
    pts+= [float(v) for v in re.findall(r'-?\d+\.?\d*(?:[eE][-+]?\d+)?',d)]
assert len(pts)>=8, f"filled rings carry no coordinates ({len(pts)} numbers)"
xs=pts[0::2]; ys=pts[1::2]
w=max(xs)-min(xs); h=max(ys)-min(ys)
assert abs(w-10.2)<0.05 and abs(h-8.2)<0.05, f"filled poche envelope {w:.3f}x{h:.3f}, expected 10.2x8.2"
print("ok:",n,"filled poche rings, envelope %.2fx%.2f"%(w,h))
PY
info=$(cargo run -q -p cadcraft-cli -- run "$tmp/p.dxf" --cmd 'arch.inspect {}' 2>/dev/null | tail -n 1) || { echo "FAIL: reopen failed"; exit 1; }
python3 - "$info" "$tmp/p.dxf" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1])
assert len(d.get("walls",[]))==5, f"arch model did not round-trip: {len(d.get('walls',[]))} walls"
assert d["walls"][0].get("thickness",0)>0, "thickness lost"
txt=open(sys.argv[2],encoding='utf-8',errors='replace').read()
assert 'CADCRAFT_ARCH' in txt, "no CADCRAFT_ARCH XRECORD in the DXF"
n=txt.count('LWPOLYLINE')
assert n>=5, f"expected proxy LWPOLYLINE graphics so other programs see linework ({n})"
print("ok: round-trip,",n,"proxy polylines")
PY
echo "PASS 05"
