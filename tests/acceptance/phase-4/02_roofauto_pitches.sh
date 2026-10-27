#!/usr/bin/env bash
set -uo pipefail
for pair in "4 3.766666666666667" "6 4.45" "12 6.5"; do
  set -- $pair; rise=$1; want=$2
  out=$(cargo run -q -p cadcraft-cli -- run --metric \
    --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
    --cmd "roofauto {\"pitchRise\":$rise,\"pitchRun\":12}" \
    --cmd 'arch.inspect {"roofs":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: $rise/12 crashed"; exit 1; }
  python3 - "$out" "$rise" "$want" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1]); rise=float(sys.argv[2]); want=float(sys.argv[3])
R=d.get("roofs",[]); assert len(R)==1, f"{rise}/12 built no roof: {d}"
r=R[0]; E=[e for e in r["edges"] if e["kind"]=="ridge"]
assert len(E)==1, f"{rise}/12: one ridge, got {E}"
e=E[0]
assert abs(e["za"]-want)<1e-6 and abs(e["zb"]-want)<1e-6, f"{rise}/12 ridge z {e['za']} != {want}"
assert len(r["planes"])==4, f"{rise}/12 hip: four planes, got {len(r['planes'])}"
for p in r["planes"]:
    assert abs(p["slope"]-rise/12.0)<1e-12, f"{rise}/12 plane slope {p['slope']}"
print(f"ok: {rise}/12 -> {e['za']}")
PY
done
echo "PASS 02"
