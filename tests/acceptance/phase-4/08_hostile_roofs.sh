#!/usr/bin/env bash
# Hostile input never panics and never leaves a partial roof; L and T footprints get the right
# number of valleys; a self-crossing footprint builds no roof at all.
set -uo pipefail
W='wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}'
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'roofauto {}' --cmd 'roofauto {"pitchRise":0,"pitchRun":0}' \
  --cmd 'roofauto {"pitchRise":1e308,"pitchRun":12}' --cmd 'roofauto {"overhang":-5}' \
  --cmd 'roofauto {"overhang":1e308}' --cmd 'roofauto {"walls":[{"wall":"w999","directive":"gable"}]}' \
  --cmd 'roofauto {"walls":[{"wall":"w0","directive":"bogus"}]}' \
  --cmd 'roofplane {"points":[[0,0],[1,0],[1,1]]}' --cmd 'roofplane {"points":[]}' \
  --cmd 'roofplane {"points":[[0,0],[4,0],[4,4],[0,4]],"base":[0,0,0,0],"pitchRise":6,"pitchRun":12}' \
  --cmd 'roofplane {"points":[[0,0],[4,0],[4,4],[0,4]],"base":[9,9,9,9],"pitchRise":6,"pitchRun":12}' \
  --cmd 'arch.inspect {"roofs":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: hostile roof run crashed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
assert json.loads(sys.argv[1]).get("roofs",[])==[], f"failed roof calls must add nothing: {sys.argv[1]}"
PY
L='wall {"points":[[0,0],[10,0],[10,8],[6,8],[6,12],[0,12],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}'
o=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$L" --cmd 'roofauto {"pitchRise":6,"pitchRun":12}' --cmd 'arch.inspect {"roofs":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: L roof crashed"; exit 1; }
python3 - "$o" <<'PY' || exit 1
import json,sys
R=json.loads(sys.argv[1])["roofs"]
assert len(R)==1, f"L footprint built no roof: {sys.argv[1]}"
r=R[0]
v=[e for e in r["edges"] if e["kind"]=="valley"]
assert len(v)==1, f"an L footprint has one valley, got {len(v)}: {v}"
PY
T='wall {"points":[[0,0],[12,0],[12,4],[8,4],[8,8],[4,8],[4,4],[0,4],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}'
o=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$T" --cmd 'roofauto {"pitchRise":6,"pitchRun":12}' --cmd 'arch.inspect {"roofs":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: T roof crashed"; exit 1; }
python3 - "$o" <<'PY' || exit 1
import json,sys
r=json.loads(sys.argv[1])["roofs"][0]
v=[e for e in r["edges"] if e["kind"]=="valley"]
assert len(v)==2, f"a T footprint has two valleys, got {len(v)}: {v}"
PY
o=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[8,0],[8,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'wall {"points":[[4,-2],[4,10]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'roofauto {"pitchRise":6,"pitchRun":12}' --cmd 'arch.inspect {"roofs":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: crossing run crashed"; exit 1; }
python3 - "$o" <<'PY' || exit 1
import json,sys
assert json.loads(sys.argv[1]).get("roofs",[])==[], f"a bad footprint must build no roof: {sys.argv[1]}"
PY
echo "PASS 08"
