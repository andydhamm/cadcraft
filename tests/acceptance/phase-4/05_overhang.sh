#!/usr/bin/env bash
# Overhang 0.5 m on a hip roof: plan-projected plane areas sum to 11.2 x 9.2 = 103.04 m^2.
set -uo pipefail
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'roofauto {"pitchRise":6,"pitchRun":12,"overhang":0.5}' \
  --cmd 'arch.inspect {"roofs":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: overhang demo crashed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
r=json.loads(sys.argv[1])["roofs"][0]
assert abs(r["overhang"]-0.5)<1e-12, f"overhang stored: {r.get('overhang')}"
assert abs(r["projectedArea"]-103.04)<1e-6, f"must be 11.2*9.2=103.04, got {r['projectedArea']}"
s=sum(p["projectedArea"] for p in r["planes"])
assert abs(s-r["projectedArea"])<1e-6, f"plane areas must sum to the roof's: {s} vs {r['projectedArea']}"
assert len(r["planes"])==4, len(r["planes"])
print("ok: overhang 0.5 ->", r["projectedArea"])
PY
echo "PASS 05"
