#!/usr/bin/env bash
set -uo pipefail
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'level {"name":"Basement","below":"Level 1","ceiling":2.4,"structure":0.3}' \
  --cmd 'level {"above":"Level 1"}' \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"level":"Level 2"}' \
  --cmd 'arch.inspect {"levels":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: level demo failed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1]); L=d.get("levels",[])
assert len(L)==3, f"expected 3 levels, got {len(L)}: {L}"
names=[l["name"] for l in L]
assert names==["Basement","Level 1","Level 2"], f"levels must be ordered bottom-up: {names}"
by={l["name"]:l for l in L}
assert abs(by["Level 1"]["elevation"]-0.0)<1e-9
assert abs(by["Level 2"]["elevation"]-2.7)<1e-6, f"Level 2 = 0+2.4+0.3: {by['Level 2']}"
assert abs(by["Basement"]["elevation"]+2.7)<1e-6, f"basement one storey below: {by['Basement']}"
for n in names:
    l=by[n]
    assert abs(l["floorToFloor"]-(l["ceiling"]+l["structure"]))<1e-9, f"floorToFloor must be derived: {l}"
    assert abs(l["ceiling"]-2.4)<1e-9 and abs(l["structure"]-0.3)<1e-9, f"defaults: {l}"
w=[x for x in d["walls"] if x["level"]==2]
assert w, "wall not placed on Level 2"
assert abs(w[0]["height"]-2.7)<1e-6, f"height defaults to derived floor-to-floor: {w[0]}"
print("ok: levels", names)
PY
b=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'level {"above":"Level 1"}' \
  --cmd 'wall {"points":[[0,0],[10,0]],"thickness":0.2,"level":"Level 2"}' \
  --cmd 'build3d {}' 2>/dev/null | tail -n 1) || { echo "FAIL: build3d failed"; exit 1; }
python3 - "$b" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1])
assert abs(d["bounds"]["min"][2]-2.7)<1e-3, f"Level-2 wall must start at z=2.7: {d['bounds']}"
assert abs(d["bounds"]["max"][2]-5.4)<1e-3, f"Level-2 wall top must be 5.4: {d['bounds']}"
print("ok: wall built at its level elevation")
PY
t=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0]],"thickness":0.2,"toCeiling":true}' \
  --cmd 'build3d {}' 2>/dev/null | tail -n 1) || { echo "FAIL: to_ceiling build failed"; exit 1; }
python3 - "$t" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1])
assert abs(d["bounds"]["max"][2]-2.4)<1e-3, f"to_ceiling wall must top at 2.4: {d['bounds']}"
print("ok: to_ceiling honoured")
PY
echo "PASS 01"
