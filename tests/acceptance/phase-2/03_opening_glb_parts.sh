#!/usr/bin/env bash
set -uo pipefail
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'wall {"points":[[0,3],[10,3]],"thickness":0.1,"height":2.4}' \
  --cmd 'door {"wall":"w0","offset":3.0,"width":0.9,"height":2.1,"style":"hinged"}' \
  --cmd 'window {"wall":"w0","offset":7.0,"width":1.2,"height":1.2,"sill":0.9,"style":"casement"}' \
  --cmd 'window {"wall":"w2","offset":4.0,"width":1.2,"height":1.2,"sill":0.9,"style":"slider"}' \
  --save "$tmp/h.dxf" >/dev/null 2>&1 || { echo "FAIL: plan failed"; exit 1; }
cargo run -q -p cadcraft-cli -- convert "$tmp/h.dxf" "$tmp/h.glb" >/dev/null 2>&1 || { echo "FAIL: convert failed"; exit 1; }
test -s "$tmp/h.glb" || { echo "FAIL: empty glb"; exit 1; }
python3 - "$tmp/h.glb" <<'PY' || exit 1
import json,struct,sys
b=open(sys.argv[1],'rb').read()
magic,ver,length=struct.unpack_from('<III',b,0)
assert magic==0x46546C67 and ver==2 and length==len(b), "bad GLB header"
cl,ct=struct.unpack_from('<II',b,12); assert ct==0x4E4F534A
gl=json.loads(b[20:20+cl]); acc=gl["accessors"]
names=[n.get("name","") for n in gl.get("nodes",[])]
j=" ".join(names).lower()
assert any(n.startswith("wall-") for n in names), names[:6]
for want in ("door","window","glass","frame","leaf"):
    assert want in j, f"no {want} node: {names}"
gl_m=[m for m in gl.get("materials",[]) if "glass" in m.get("name","").lower()]
assert gl_m and gl_m[0]["pbrMetallicRoughness"]["baseColorFactor"][3]<0.6, f"glass must blend: {gl_m}"
lo=[1e30]*3; hi=[-1e30]*3
for a in acc:
    if a.get("type")!="VEC3" or "min" not in a: continue
    for i in range(3):
        assert a["min"][i]<=a["max"][i]
        lo[i]=min(lo[i],a["min"][i]); hi[i]=max(hi[i],a["max"][i])
assert abs((hi[0]-lo[0])-10.2)<0.05 and abs((hi[1]-lo[1])-8.2)<0.05, [hi[i]-lo[i] for i in range(3)]
assert abs((hi[2]-lo[2])-2.4)<0.05, hi[2]-lo[2]
for m in gl["meshes"]:
    for p in m["primitives"]:
        assert "POSITION" in p["attributes"]
        if p.get("indices") is not None: assert acc[p["indices"]]["count"]%3==0
print("ok:",len(gl["meshes"]),"meshes",len(gl.get("materials",[])),"materials")
PY
echo "PASS 03"
