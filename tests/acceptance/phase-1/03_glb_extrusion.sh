#!/usr/bin/env bash
set -uo pipefail
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'wall {"points":[[0,3],[10,3]],"thickness":0.1}' \
  --cmd 'wall {"points":[[6,3],[6,8]],"thickness":0.1}' \
  --save "$tmp/house.dxf" >/dev/null 2>&1 || { echo "FAIL: plan build failed"; exit 1; }
cargo run -q -p cadcraft-cli -- convert "$tmp/house.dxf" "$tmp/house.glb" >/dev/null 2>&1 || { echo "FAIL: convert to .glb failed"; exit 1; }
test -s "$tmp/house.glb" || { echo "FAIL: empty .glb"; exit 1; }
python3 - "$tmp/house.glb" <<'PY' || exit 1
import json,struct,sys
b=open(sys.argv[1],'rb').read()
magic,ver,length=struct.unpack_from('<III',b,0)
assert magic==0x46546C67 and ver==2, f"not glTF v2 GLB ({magic:#x},v{ver})"
assert length==len(b), f"GLB length {length} != file {len(b)}"
cl,ct=struct.unpack_from('<II',b,12); assert ct==0x4E4F534A, "first chunk is not JSON"
gl=json.loads(b[20:20+cl])
meshes=gl.get('meshes',[]); acc=gl.get('accessors',[])
names=' '.join(n.get('name','') for n in gl.get('nodes',[]))
assert len(meshes)>=6, f"expected >=6 wall meshes, got {len(meshes)}"
assert 'wall' in names.lower(), f"nodes not named after walls: {names[:200]}"
assert gl.get('buffers') and gl['buffers'][0].get('byteLength',0)>0, "no buffer data"
lo=[1e30]*3; hi=[-1e30]*3
for a in acc:
    if a.get('type')!='VEC3' or 'min' not in a: continue
    for i in range(3):
        assert a['min'][i]<=a['max'][i], f"degenerate accessor"
        lo[i]=min(lo[i],a['min'][i]); hi[i]=max(hi[i],a['max'][i])
print('bbox',[round(hi[i]-lo[i],3) for i in range(3)])
assert abs((hi[0]-lo[0])-10.2)<0.05 and abs((hi[1]-lo[1])-8.2)<0.05, "plan envelope wrong"
assert abs((hi[2]-lo[2])-2.4)<0.05, f"z span {hi[2]-lo[2]:.3f}, expected 2.4"
assert abs(lo[2])<1e-6, f"walls must sit on level 0 (z min {lo[2]})"
for m in meshes:
    for p in m['primitives']:
        assert 'POSITION' in p['attributes'], "primitive without POSITION"
        ic=p.get('indices')
        if ic is not None: assert acc[ic].get('count',0)%3==0, "index count not a multiple of 3"
print('ok:',len(meshes),'meshes')
PY
echo "PASS 03"
