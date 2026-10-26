#!/usr/bin/env bash
# Hip 6/12 over the phase-1 house (walls 2.4 m, overhang 0). Footprint -0.1..10.1 x -0.1..8.1.
# Ridge x 4.0..6.0 at y 4.0, z = 2.4 + 4.1*0.5 = 4.45. Four planes: 2 triangles + 2 trapezoids.
set -uo pipefail
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'roofauto {"pitchRise":6,"pitchRun":12}' \
  --cmd 'arch.inspect {"roofs":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: hip demo crashed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1]); R=d.get("roofs",[])
assert len(R)==1, f"expected one roof: {R}"
r=R[0]; P=r.get("planes",[])
assert len(P)==4, f"hip roof needs 4 planes, got {len(P)}"
for p in P:
    assert abs(p["slope"]-0.5)<1e-12, f"plane slope must be 6/12: {p}"
assert sorted(len(p["ring"]) for p in P)==[3,3,4,4], f"2 triangles + 2 trapezoids: {[len(p['ring']) for p in P]}"
E=[e for e in r.get("edges",[]) if e["kind"]=="ridge"]
assert len(E)==1, f"one ridge: {E}"
e=E[0]; xs=sorted([e["a"][0],e["b"][0]])
assert abs(xs[0]-4.0)<1e-6 and abs(xs[1]-6.0)<1e-6, f"ridge x 4.0..6.0: {e}"
assert abs(e["a"][1]-4.0)<1e-6 and abs(e["b"][1]-4.0)<1e-6, f"ridge at y=4.0: {e}"
assert abs(e["za"]-4.45)<1e-6 and abs(e["zb"]-4.45)<1e-6, f"ridge z 4.45: {e}"
assert abs(r["projectedArea"]-83.64)<1e-6, f"planes must tile 10.2x8.2: {r['projectedArea']}"
assert len([e for e in r["edges"] if e["kind"]=="hip"])==4, f"four hips: {r['edges']}"
assert not [e for e in r["edges"] if e["kind"]=="valley"], "no valley on a rectangle"
print("ok: hip 6/12 ridge", xs, e["za"])
PY
echo "PASS 01"
