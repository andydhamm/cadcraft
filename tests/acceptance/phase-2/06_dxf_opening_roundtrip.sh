#!/usr/bin/env bash
set -uo pipefail
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
W='wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}'
cargo run -q -p cadcraft-cli -- run --metric --cmd "$W" \
  --cmd 'door {"wall":"w0","offset":3.0,"width":0.9,"height":2.1,"style":"hinged","hinge":"left","swing":"in"}' \
  --cmd 'window {"wall":"w0","offset":7.0,"width":1.2,"height":1.2,"sill":0.9,"style":"casement","recess":0.05}' \
  --cmd 'garagedoor {"wall":"w2","offset":4.0,"width":2.4,"height":2.1}' \
  --save "$tmp/a.dxf" >/dev/null 2>&1 || { echo "FAIL: save failed"; exit 1; }
grep -q 'CADCRAFT_ARCH' "$tmp/a.dxf" || { echo "FAIL: no CADCRAFT_ARCH"; exit 1; }
grep -q 'A-DOOR' "$tmp/a.dxf" || { echo "FAIL: no A-DOOR proxy graphics"; exit 1; }
grep -q 'ARCHOPENING' "$tmp/a.dxf" || { echo "FAIL: no opening proxy xdata marker"; exit 1; }
out=$(cargo run -q -p cadcraft-cli -- run "$tmp/a.dxf" --cmd 'arch.inspect {"openings":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: reopen failed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
o=json.loads(sys.argv[1]).get("openings",[])
assert len(o)==3, f"openings did not round-trip: {len(o)}"
assert sorted(x["kind"] for x in o)==["door","door","window"]
assert [x for x in o if x["style"]=="garage"], "garage style lost"
d0=[x for x in o if x["style"]=="hinged"][0]
assert d0["hinge"]=="left" and d0["swing"]=="in", d0
w0=[x for x in o if x["kind"]=="window"][0]
assert abs(w0["recess"]-0.05)<1e-9 and abs(w0["sill"]-0.9)<1e-9 and w0["host"].startswith("w"), w0
print("ok: 3 openings round-tripped")
PY
cargo run -q -p cadcraft-cli -- run "$tmp/a.dxf" --save "$tmp/b.dxf" >/dev/null 2>&1 || { echo "FAIL: resave failed"; exit 1; }
out2=$(cargo run -q -p cadcraft-cli -- run "$tmp/b.dxf" --cmd 'arch.inspect {"openings":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: double round-trip failed"; exit 1; }
python3 - "$out2" <<'PY' || exit 1
import json,sys
assert len(json.loads(sys.argv[1]).get("openings",[]))==3, "save/reopen duplicated openings"
print("ok: idempotent round-trip")
PY
echo "PASS 06"
