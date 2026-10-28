#!/usr/bin/env bash
# 0.9 m to grade uses the same calculator: 5 risers of 0.18 (6 of 0.15 is further from 168.75 mm).
set -uo pipefail
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'deck {"points":[[0,0],[4,0],[4,3],[0,3],[0,0]],"height":0.9}' \
  --cmd 'stair {"start":[2,-0.5],"direction":90,"width":0.9,"rise":0.9}' \
  --cmd 'arch.inspect {"stairs":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: deck stair crashed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
S=json.loads(sys.argv[1]).get("stairs",[])
assert len(S)==1, f"expected one stair: {S}"
s=S[0]
assert s["risers"]==5, f"0.9 m must give 5 risers, got {s['risers']}"
assert abs(s["riserHeight"]-0.18)<1e-9, f"riser 0.18: {s['riserHeight']}"
assert s["treads"]==4 and abs(s["run"]-1.016)<1e-9, f"4 treads, run 1.016: {s}"
print("ok: 5 risers of 0.18 to grade")
PY
echo "PASS 02"
