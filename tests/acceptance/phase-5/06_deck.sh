#!/usr/bin/env bash
# DECK: a Deck room (floor, no ceiling) with a board surface at its own height above grade.
set -uo pipefail
DECK=(--cmd 'deck {"points":[[0,0],[4,0],[4,3],[0,3],[0,0]],"height":0.9,"name":"Rear deck"}')
i=$(cargo run -q -p cadcraft-cli -- run --metric "${DECK[@]}" --cmd 'arch.inspect {"rooms":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: deck inspect crashed"; exit 1; }
b=$(cargo run -q -p cadcraft-cli -- run --metric "${DECK[@]}" --cmd 'build3d {"probe":[[2,1.5,0.88],[2,1.5,2.39],[2,1.5,-0.15]]}' 2>/dev/null | tail -n 1) || { echo "FAIL: deck demo crashed"; exit 1; }
python3 - "$i" "$b" <<'PY' || exit 1
import json,sys
RM=[x for x in json.loads(sys.argv[1]).get("rooms",[]) if x["type"]=="deck"]
assert len(RM)==1, f"the deck is a Deck room: {[(x['type'],x['name']) for x in json.loads(sys.argv[1]).get('rooms',[])]}"
assert abs(RM[0]["deckHeight"]-0.9)<1e-9 and abs(RM[0]["interiorArea"]-12.0)<1e-6, RM[0]
assert RM[0]["hasFloor"] is True, f"a deck has a floor: {RM[0]}"
d=json.loads(sys.argv[2]); P=d["probe"]
assert P[0] and P[0]["part"]=="deck", f"the board surface at 0.9 must probe as deck: {P[0]}"
assert P[1] is None, f"a Deck gets no ceiling: {P[1]}"
assert P[2] is None, f"a deck with its own height replaces the grade slab: {P[2]}"
assert d.get("decks",0)==1, f"build3d must count decks: {d.get('decks')}"
print("ok: deck room with a board surface at 0.9")
PY
echo "PASS 06"
