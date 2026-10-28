#!/usr/bin/env bash
# DECK: a Deck room (floor, no ceiling) with a board surface at its own height above grade.
set -uo pipefail
b=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'deck {"points":[[0,0],[4,0],[4,3],[0,3],[0,0]],"height":0.9,"name":"Rear deck"}' \
  --cmd 'arch.inspect {"rooms":true}' \
  --cmd 'build3d {"probe":[[2,1.5,0.88],[2,1.5,2.39],[2,1.5,-0.15]]}' 2>/dev/null | tail -n 1) || { echo "FAIL: deck demo crashed"; exit 1; }
python3 - "$b" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1]); P=d["probe"]
assert P[0] and P[0]["part"]=="deck", f"the board surface at 0.9 must probe as deck: {P[0]}"
assert P[1] is None, f"a Deck gets no ceiling: {P[1]}"
assert P[2] is None, f"a deck with its own height replaces the grade slab: {P[2]}"
assert d.get("decks",0)==1, f"build3d must count decks: {d.get('decks')}"
RM=[x for x in d.get("rooms",[]) if x["type"]=="deck"]
assert len(RM)==1, f"the deck is a Deck room: {[x['type'] for x in d.get('rooms',[])]}"
assert abs(RM[0]["deckHeight"]-0.9)<1e-9 and abs(RM[0]["interiorArea"]-12.0)<1e-6, RM[0]
print("ok: deck room with a board surface at 0.9")
PY
echo "PASS 06"
