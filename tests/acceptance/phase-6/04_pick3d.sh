#!/usr/bin/env bash
set -uo pipefail
C='{"kind":"full","eye":[3,-8,1.6],"target":[3,0,1.05],"fov":60}'
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'door {"wall":"w0","offset":3.0,"width":0.9,"height":2.1,"style":"hinged"}' \
  --cmd 'arch.inspect {"openings":true,"walls":true}' \
  --cmd "pick3d {\"camera\":$C,\"size\":[480,320],\"pixel\":[240,160]}" \
  --cmd "pick3d {\"camera\":$C,\"size\":[480,320],\"pixel\":[400,160]}" \
  --cmd "pick3d {\"camera\":$C,\"size\":[480,320],\"pixel\":[8,8]}" 2>/dev/null | tail -n 4)
python3 - "$out" <<'PY' || exit 1
import json,sys
rows=[json.loads(l) for l in sys.argv[1].splitlines() if l.strip().startswith('{')]
assert len(rows)==4, len(rows)
ins,centre,wall,sky=rows
door=[o for o in ins["openings"] if o["host"]=="w0"][0]
walls={w["id"]:w for w in ins["walls"]}
for r in (centre,wall,sky): assert r.get("ok") is True, r
h=centre["hit"]
assert h and h["id"]==door["id"], f"centre pixel must pick the door {door['id']}, got {h}"
assert h["handle"]==door["handle"], f"pick must return the 2D pane's proxy handle: {h} vs {door}"
assert h["part"] in ("leaf","frame","glass","casing","sill"), h["part"]
assert h["distance"]>0 and len(h["point"])==3
w=wall["hit"]
assert w and w["id"]=="w0" and w["part"]=="wall", f"right of centre must pick w0, got {w}"
assert w["handle"]==walls["w0"]["handle"], w
assert sky["hit"] is None, f"the sky must pick nothing: {sky['hit']}"
print("ok: pick", h["id"], h["part"], w["id"], "sky null")
PY
echo "PASS 04"
