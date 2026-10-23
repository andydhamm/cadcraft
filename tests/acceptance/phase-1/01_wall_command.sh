#!/usr/bin/env bash
set -uo pipefail
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'wall {"points":[[0,3],[10,3]],"thickness":0.1,"type":"interior"}' \
  --cmd 'wall {"points":[[6,3],[6,8]],"thickness":0.1,"type":"interior"}' \
  --cmd 'arch.inspect {}' 2>/dev/null | tail -n 1) || { echo "FAIL: cli run failed"; exit 1; }
echo "$out" | head -c 300; echo
python3 - "$out" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1])
w=d.get("walls",[])
assert len(w)==6, f"expected 6 wall segments, got {len(w)}"
assert d.get("revision",0)>0, "arch_revision did not advance"
b=d.get("bounds")
assert b and abs(b[0][0]+0.1)<0.01 and abs(b[0][1]+0.1)<0.01, f"bounds wrong: {b}"
assert abs(b[1][0]-10.1)<0.01 and abs(b[1][1]-8.1)<0.01, f"bounds wrong: {b}"
ex=len([x for x in w if x.get("exterior")])
assert ex==4, f"expected 4 exterior walls, got {ex}"
inn=len([x for x in w if not x.get("exterior")])
assert inn==2, f"expected 2 interior walls, got {inn}"
assert all(x.get("thickness",0)>0 and x.get("height",0)>0 for x in w), "thickness/height must be positive"
print("ok: 6 walls, bounds", b)
PY
echo "PASS 01"
