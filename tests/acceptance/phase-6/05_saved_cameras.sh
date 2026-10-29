#!/usr/bin/env bash
set -uo pipefail
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'camera {"save":"front","kind":"full","eye":[3,-8,1.6],"target":[3,0,1.05],"fov":60}' \
  --cmd 'camera {"save":"overview","kind":"orbit","azimuth":35,"elevation":30,"distance":25,"target":[5,4,1.2]}' \
  --cmd 'camera {"list":true}' --cmd 'camera {"use":"front"}' --save "$tmp/a.dxf" 2>/dev/null | tail -n 2) \
  || { echo "FAIL: camera run failed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
rows=[json.loads(l) for l in sys.argv[1].splitlines() if l.strip().startswith('{')]
lst=rows[-1]
assert lst.get("ok") is True and lst.get("active")=="front", f"use did not set the active camera: {lst}"
names=[c["name"] for c in lst["cameras"]]
assert names==["front","overview"], names
f=[c for c in lst["cameras"] if c["name"]=="front"][0]
assert f["kind"]=="full" and abs(f["fov"]-60)<1e-9 and f["eye"]==[3.0,-8.0,1.6], f
assert f["target"]==[3.0,0.0,1.05], f
o=[c for c in lst["cameras"] if c["name"]=="overview"][0]
assert o["kind"]=="orbit" and abs(o["azimuth"]-35)<1e-9 and abs(o["distance"]-25)<1e-9, o
print("ok: two cameras saved")
PY
out2=$(cargo run -q -p cadcraft-cli -- run "$tmp/a.dxf" --cmd 'camera {"list":true}' 2>/dev/null | tail -n 1) \
  || { echo "FAIL: reopen failed"; exit 1; }
python3 - "$out2" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1])
assert [c["name"] for c in d["cameras"]]==["front","overview"], f"no DXF round-trip: {d}"
f=[c for c in d["cameras"] if c["name"]=="front"][0]
assert f["eye"]==[3.0,-8.0,1.6] and abs(f["fov"]-60)<1e-9, f
assert d.get("active")=="front", d
print("ok: cameras round-trip")
PY
cargo run -q -p cadcraft-cli -- run "$tmp/a.dxf" --cmd 'camera {"delete":"front"}' --cmd 'camera {"list":true}' \
  2>/dev/null | tail -n 1 | python3 -c '
import json,sys
d=json.load(sys.stdin)
assert [c["name"] for c in d["cameras"]]==["overview"], d
assert d.get("active")!="front", d
print("ok: delete")' || { echo "FAIL: camera delete"; exit 1; }
echo "PASS 05"
