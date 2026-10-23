#!/usr/bin/env bash
set -uo pipefail
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'wall {"points":[[0,3],[10,3]],"thickness":0.1}' \
  --cmd 'door {"wall":"w0","offset":3.0,"width":0.9,"height":2.1,"style":"hinged"}' \
  --cmd 'door {"wall":"w1","offset":5.0,"width":2.4,"height":2.1,"style":"sliding"}' \
  --cmd 'window {"wall":"w0","offset":7.0,"width":1.2,"height":1.2,"sill":0.9,"style":"casement"}' \
  --cmd 'window {"wall":"w2","offset":4.0,"width":1.2,"height":1.2,"sill":0.9,"style":"slider"}' \
  --cmd 'window {"wall":"w3","offset":4.0,"width":1.2,"height":1.2,"sill":0.9,"style":"picture"}' \
  --cmd 'window {"wall":"w4","offset":2.0,"width":1.0,"height":1.0,"sill":1.2,"style":"slider"}' \
  --cmd 'arch.inspect {"openings":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: demo failed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
o=json.loads(sys.argv[1]).get("openings",[])
assert len(o)==6, f"expected 6 openings, got {len(o)}"
assert len({x["id"] for x in o})==6, "ids not unique"
for x in o:
    assert x["host"].startswith("w"), x
    assert x["handle"].startswith("a000"), f"no proxy handle {x}"
    assert x["rough"]["width"]>x["width"] and x["rough"]["height"]>x["height"], f"rough must exceed finished: {x}"
    assert 0.0<x["offset"]<12.0, x
k=sorted(x["kind"] for x in o); assert k.count("door")==2 and k.count("window")==4, k
d=[x for x in o if x["kind"]=="door" and x["host"]=="w0"][0]
assert d["style"]=="hinged" and d["hinge"] in ("left","right") and d["swing"] in ("in","out"), d
s=[x for x in o if x["host"]=="w1"][0]; assert s["style"]=="sliding" and abs(s["width"]-2.4)<1e-6, s
assert all(x["sill"]>0.5 for x in o if x["kind"]=="window"), "windows need a sill"
print("ok:",len(o),"openings")
PY
b3=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0]],"thickness":0.2,"height":2.4}' \
  --cmd 'door {"wall":"w0","offset":3.0}' --cmd 'window {"wall":"w0","offset":7.0}' \
  --cmd 'build3d {}' 2>/dev/null | tail -n 1) || { echo "FAIL: build3d failed"; exit 1; }
python3 - "$b3" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1])
assert d.get("openings")==2, f"build3d must report openings built: {d.get('openings')}"
assert d.get("meshes",0)>=6 and d.get("triangles",0)>0, d
print("ok: build3d",d["meshes"],"meshes")
PY
u=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0]],"thickness":0.2,"height":2.4}' \
  --cmd 'door {"wall":"w0","offset":3.0}' --cmd 'window {"wall":"w0","offset":7.0}' \
  --cmd 'undo {"count":1}' --cmd 'arch.inspect {"openings":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: undo failed"; exit 1; }
python3 - "$u" <<'PY' || exit 1
import json,sys
o=json.loads(sys.argv[1]).get("openings",[])
assert len(o)==1 and o[0]["kind"]=="door", f"one undo removes exactly the last opening: {o}"
print("ok: undo removes one opening")
PY
echo "PASS 01"
