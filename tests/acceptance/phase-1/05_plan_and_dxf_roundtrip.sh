#!/usr/bin/env bash
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
n=len(re.findall(r'<polygon[^>]*fill="(?!none)',s))+len(re.findall(r'<path[^>]*fill="(?!none)',s))
assert n>=6, f"expected filled poché polygons in the plan svg, got {n}"
print("ok:",n,"filled poché rings")
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
