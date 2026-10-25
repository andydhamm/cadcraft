#!/usr/bin/env bash
set -uo pipefail
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.7,"exterior":true}' \
  --cmd 'wall {"points":[[0,3],[10,3]],"thickness":0.1}' \
  --cmd 'wall {"points":[[6,3],[6,8]],"thickness":0.1}' \
  --cmd 'roomdetect {}' --cmd 'room {"id":"r0","name":"Living"}' \
  --cmd 'arch.inspect {"rooms":true,"labels":true}' --save "$tmp/p.dxf" >/dev/null 2>&1 || { echo "FAIL: label demo failed"; exit 1; }
out=$(cargo run -q -p cadcraft-cli -- run "$tmp/p.dxf" --cmd 'arch.inspect {"rooms":true,"labels":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: reopen failed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1]); R=d.get("rooms",[])
assert len(R)==3, f"rooms lost through DXF: {len(R)}"
by={r["id"]:r for r in R}
assert by["r0"]["name"]=="Living", f"room name lost: {by['r0']}"
assert abs(by["r0"]["interiorArea"]-27.93)<1e-3, by["r0"]
for r in R:
    assert r["labelText"] and "m2" in r["labelText"], f"label must carry the area: {r}"
    assert len(r["labelPoint"])==2, r
print("ok: labels", [r["labelText"] for r in R])
PY
out2=$(cargo run -q -p cadcraft-cli -- run "$tmp/p.dxf" --save "$tmp/q.dxf" --cmd 'arch.inspect {"rooms":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: re-save failed"; exit 1; }
out3=$(cargo run -q -p cadcraft-cli -- run "$tmp/q.dxf" --cmd 'arch.inspect {"rooms":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: reopen 2 failed"; exit 1; }
python3 - "$out2" "$out3" <<'PY' || exit 1
import json,sys
key=lambda s:[(r["id"],r["name"],r["type"],round(r["interiorArea"],6),r["level"]) for r in sorted(json.loads(s)["rooms"],key=lambda r:r["id"])]
a=key(sys.argv[1]); b=key(sys.argv[2])
assert a==b, f"rooms changed on a second save/read:\n{a}\n{b}"
print("ok: rooms round-trip idempotently")
PY
python3 - "$tmp/p.dxf" <<'PY' || exit 1
import sys
t=open(sys.argv[1],errors="ignore").read()
assert "CADCRAFT_ARCH" in t, "no arch record written"
assert "A-ROOM" in t, "no A-ROOM proxy layer for room labels"
print("ok: proxy linework present")
PY
echo "PASS 07"
