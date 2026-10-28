#!/usr/bin/env bash
# Stairs, railings and decks round-trip through DXF (CADCRAFT_ARCH v5) and reach the GLB as named nodes.
set -uo pipefail
tmp=$(mktemp -d)
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.7,"exterior":true}' \
  --cmd 'level {"above":0}' \
  --cmd 'stair {"start":[2,1],"direction":0,"width":1.0,"fromLevel":0,"toLevel":1}' \
  --cmd 'railing {"path":[[0,4],[4,4]]}' \
  --cmd 'deck {"points":[[11,0],[15,0],[15,3],[11,3],[11,0]],"height":0.9}' \
  --save "$tmp/h.dxf" >/dev/null 2>&1 || { echo "FAIL: save crashed"; exit 1; }
cargo run -q -p cadcraft-cli -- run --metric "$tmp/h.dxf" --cmd 'arch.inspect {"stairs":true,"railings":true,"rooms":true}' 2>/dev/null | tail -n 1 > "$tmp/a.json" || { echo "FAIL: reopen crashed"; exit 1; }
cargo run -q -p cadcraft-cli -- convert "$tmp/h.dxf" "$tmp/h.glb" >/dev/null 2>&1 || { echo "FAIL: convert crashed"; exit 1; }
python3 - "$tmp/h.dxf" "$tmp/a.json" "$tmp/h.glb" <<'PY' || exit 1
import json,sys
assert "CADCRAFT_ARCH" in open(sys.argv[1],encoding="utf-8",errors="replace").read(), "arch record dropped"
d=json.load(open(sys.argv[2]))
S=d.get("stairs",[])
assert len(S)==1, f"the stair did not round-trip: {d}"
assert S[0]["risers"]==16 and abs(S[0]["riserHeight"]-0.16875)<1e-9, S[0]
assert abs(S[0]["start"][0]-2.0)<1e-9 and abs(S[0]["width"]-1.0)<1e-9, S[0]
assert S[0]["wellRoom"], f"the stair lost its well room: {S[0]}"
R=d.get("railings",[])
# the drawn railing plus the guard the stair put round its well
guard=[x for x in R if x.get("follows")]
drawn=[x for x in R if not x.get("follows")]
assert len(drawn)==1 and abs(drawn[0]["height"]-0.914)<1e-9, f"drawn railing lost: {R}"
assert [p[0][0] for p in [drawn[0]["path"]]] and abs(drawn[0]["path"][0][0]-0.0)<1e-9, f"railing path lost: {drawn[0]}"
assert len(guard)==1 and abs(guard[0]["height"]-0.914)<1e-9, f"well guard railing lost: {R}"
DK=[x for x in d.get("rooms",[]) if x["type"]=="deck"]
assert len(DK)==1 and abs(DK[0]["deckHeight"]-0.9)<1e-9, f"the deck room was lost: {d.get('rooms')}"
raw=open(sys.argv[3],"rb").read()
assert raw[:4]==b"glTF", "not a glTF binary"
ln=int.from_bytes(raw[12:16],"little"); j=json.loads(raw[20:20+ln])
names={n.get("name","") for n in j.get("meshes",[])} | {n.get("name","") for n in j.get("nodes",[])}
assert [n for n in names if n.startswith("stair-")], f"no stair nodes: {sorted(names)[:20]}"
assert [n for n in names if n.startswith("railing-")], f"no railing nodes: {sorted(names)[:20]}"
assert [n for n in names if n.startswith("deck-")], f"no deck nodes: {sorted(names)[:20]}"
assert any(n.startswith("wall-") for n in names), "walls vanished from the GLB"
print("ok: stairs/railings/decks round-trip and export")
PY
echo "PASS 07"
