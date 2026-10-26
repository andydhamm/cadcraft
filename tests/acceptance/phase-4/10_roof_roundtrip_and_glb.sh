#!/usr/bin/env bash
# Roofs round-trip through DXF (CADCRAFT_ARCH v4) and reach the GLB as named nodes.
set -uo pipefail
W='wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}'
RA='roofauto {"pitchRise":6,"pitchRun":12,"overhang":0.5,"thickness":0.25}'
tmp=$(mktemp -d)
cargo run -q -p cadcraft-cli -- run --metric --cmd "$W" --cmd "$RA" --save "$tmp/h.dxf" >/dev/null 2>&1 || { echo "FAIL: save crashed"; exit 1; }
cargo run -q -p cadcraft-cli -- run --metric "$tmp/h.dxf" --cmd 'arch.inspect {"roofs":true}' 2>/dev/null | tail -n 1 > "$tmp/a.json" || { echo "FAIL: reopen crashed"; exit 1; }
cargo run -q -p cadcraft-cli -- convert "$tmp/h.dxf" "$tmp/h.glb" >/dev/null 2>&1 || { echo "FAIL: convert crashed"; exit 1; }
python3 - "$tmp/h.dxf" "$tmp/a.json" "$tmp/h.glb" <<'PY' || exit 1
import json,sys
text=open(sys.argv[1],encoding="utf-8",errors="replace").read()
assert "CADCRAFT_ARCH" in text, "the arch record was dropped on save"
d=json.load(open(sys.argv[2])); R=d.get("roofs",[])
assert len(R)==1, f"the roof did not round-trip: {d}"
r=R[0]
assert abs(r["projectedArea"]-103.04)<1e-6, f"geometry changed on round-trip: {r['projectedArea']}"
assert abs(r["overhang"]-0.5)<1e-12 and abs(r["thickness"]-0.25)<1e-12, r
assert len(r["planes"])==4 and r.get("handle"), r
raw=open(sys.argv[3],"rb").read()
assert raw[:4]==b"glTF", "not a glTF binary"
ln=int.from_bytes(raw[12:16],"little"); j=json.loads(raw[20:20+ln])
names={n.get("name","") for n in j.get("meshes",[])} | {n.get("name","") for n in j.get("nodes",[])}
assert [n for n in names if n.startswith("roof-")], f"no roof nodes: {sorted(names)[:20]}"
assert any(n.startswith("wall-") for n in names), "walls vanished from the GLB"
print("ok: roof round-trips and exports")
PY
echo "PASS 10"
