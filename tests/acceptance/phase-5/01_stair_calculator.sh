#!/usr/bin/env bash
# L1 at 0, L2 at 2.7 -> 16 risers of 0.16875, 15 treads of 0.254, run 3.81.
set -uo pipefail
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.7,"exterior":true}' \
  --cmd 'level {"above":0}' \
  --cmd 'stair {"start":[2,1],"direction":0,"width":1.0,"fromLevel":0,"toLevel":1}' \
  --cmd 'arch.inspect {"stairs":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: stair demo crashed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1]); S=d.get("stairs",[])
assert len(S)==1, f"expected one stair: {d}"
s=S[0]
assert s["risers"]==16, f"2.7 m must give 16 risers, got {s['risers']}"
assert abs(s["riserHeight"]-0.16875)<1e-9, f"riser 0.16875: {s['riserHeight']}"
assert s["treads"]==15 and abs(s["treadDepth"]-0.254)<1e-9, f"15 treads of 0.254: {s}"
assert abs(s["run"]-3.81)<1e-9, f"run 3.81: {s['run']}"
assert abs(s["rise"]-2.7)<1e-9 and abs(s["width"]-1.0)<1e-9 and s["shape"]=="straight", s
assert s.get("id","").startswith("s") and s.get("handle"), f"no id/handle: {s}"
print("ok: 16 risers of 0.16875, run 3.81")
PY
echo "PASS 01"
