#!/usr/bin/env bash
# RAILING: guard 0.914 default, baluster clear gap < 0.1016, probeable as part "railing".
set -uo pipefail
b=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'railing {"path":[[0,0],[4,0]]}' \
  --cmd 'railing {"path":[[0,4],[4,4]],"style":"balusters"}' \
  --cmd 'arch.inspect {"railings":true}' \
  --cmd 'build3d {"probe":[[2,0,0.5],[2,4,0.88]]}' 2>/dev/null | tail -n 1) || { echo "FAIL: railing demo crashed"; exit 1; }
python3 - "$b" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1]); R=d.get("railings",[])
assert len(R)==2, f"two railings: {R}"
assert abs(R[0]["height"]-0.914)<1e-9, f"default guard 0.914: {R[0]['height']}"
assert R[0]["style"]=="panel" and R[1]["style"]=="balusters", R
assert R[1]["balusterGap"]<0.1016, f"baluster clear gap under 101.6 mm: {R[1]['balusterGap']}"
P=d["probe"]
assert P[0] and P[0]["part"]=="railing", f"inside the panel must be railing: {P[0]}"
assert P[1] and P[1]["part"]=="railing", f"inside the handrail must be railing: {P[1]}"
assert d.get("railings",0)==2, f"build3d must count railings: {d.get('railings')}"
print("ok: railings probe and follow the code")
PY
cargo run -q -p cadcraft-cli -- run --metric --cmd 'railing {"path":[[0,0],[4,0]],"height":0.5}' >/dev/null 2>/tmp/p5r.err
grep -qi "railing" /tmp/p5r.err || { echo "FAIL: a 0.5 m guard must be refused"; exit 1; }
echo "PASS 05"
