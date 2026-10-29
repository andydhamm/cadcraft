#!/usr/bin/env bash
set -uo pipefail
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'roomdetect {}' --save "$tmp/p.dxf" >/dev/null 2>&1 || { echo "FAIL: run failed"; exit 1; }
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'walkthrough {"eye":[5,-10,1.6],"yaw":0,"forward":2,"strafe":0,"turn":0}' \
  --cmd 'walkthrough {"eye":[5,-10,1.6],"yaw":0,"forward":0,"strafe":1,"turn":0}' \
  --cmd 'walkthrough {"eye":[5,-10,1.6],"yaw":0,"forward":0,"strafe":0,"turn":90}' \
  --cmd 'walkthrough {"eye":[5,-10,1.6],"yaw":0,"forward":1e308,"strafe":0,"turn":0}' 2>/dev/null | tail -n 4)
python3 - "$out" <<'PY' || exit 1
import json,sys
rows=[json.loads(l) for l in sys.argv[1].splitlines() if l.strip().startswith('{')]
assert len(rows)==4, rows
f,s,t,bad=rows
assert f["eye"][1]>-8, f"forward at yaw 0 (+Y) must move north: {f}"
assert abs(f["eye"][0]-5)<1e-9 and abs(f["eye"][2]-1.6)<1e-9, f
assert s["eye"][0]>5.9, f"strafe must move east: {s}"
assert abs(t["yaw"]-90)<1e-9, f"turn must add to yaw: {t}"
assert bad.get("ok") is not True, f"1e308 step must be refused: {bad}"
print("ok: walkthrough")
PY
i=0
for cam in 'orbit,azimuth=35,elevation=30,distance=25' 'full,eye=3,-8,1.6,target=3,0,1.05,fov=60'; do
  i=$((i+1))
  cargo run -q -p cadcraft-cli -- render "$tmp/p.dxf" "$tmp/c$i.png" --size 480x320 \
    --camera "$cam" --adapter fallback 2>"$tmp/c$i.log" \
    || { echo "FAIL: render --camera '$cam'"; head -3 "$tmp/c$i.log"; exit 1; }
done
python3 - "$tmp" <<'PY' || exit 1
import hashlib,os,sys
tmp=sys.argv[1]
h=[hashlib.sha256(open(os.path.join(tmp,f),'rb').read()).hexdigest() for f in ["c1.png","c2.png"]]
assert h[0]!=h[1], "the full camera gave the orbit's image"
for k in ["c1","c2"]: assert 'PROJECTED' in open(os.path.join(tmp,k+'.log')).read(), k
print("ok: full camera renders differently")
PY
echo "PASS 06"
