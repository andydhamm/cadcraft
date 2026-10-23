#!/usr/bin/env bash
set -uo pipefail
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
W='wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}'
ins=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$W" \
  --cmd 'door {"wall":"w0","offset":3.0,"width":0.9,"height":2.1,"style":"hinged"}' \
  --cmd 'door {"wall":"w1","offset":5.0,"width":2.4,"style":"sliding"}' \
  --cmd 'window {"wall":"w0","offset":7.0,"width":1.2,"height":1.2,"sill":0.9}' \
  --cmd 'arch.inspect {"openings":true,"symbols":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: inspect failed"; exit 1; }
python3 - "$ins" <<'PY' || exit 1
import json,sys
o=json.loads(sys.argv[1]).get("openings",[])
assert len(o)==3, len(o)
for x in o:
    sym=x.get("symbol"); assert isinstance(sym,list) and sym, f"{x['id']} has no generated symbol"
    for pl in sym: assert len(pl)>=2, f"degenerate polyline in {x['id']}"
    assert x.get("layer") in ("A-DOOR","A-GLAZ"), x.get("layer")
    assert x.get("gap",[0,0])[1]>x.get("gap",[0,0])[0], f"no poché gap for {x['id']}"
d=[x for x in o if x["style"]=="hinged"][0]
assert any(len(pl)>=8 for pl in d["symbol"]), f"hinged door needs a swing arc: {[len(p) for p in d['symbol']]}"
assert len([x for x in o if x["style"]=="sliding"][0]["symbol"])>=2, "sliding door needs leaf lines"
assert len([x for x in o if x["kind"]=="window"][0]["symbol"])>=2, "window needs frame/glass lines"
print("ok: symbols",[len(x["symbol"]) for x in o])
PY
cargo run -q -p cadcraft-cli -- run --metric --cmd "$W" --save "$tmp/w.svg" >/dev/null 2>&1
cargo run -q -p cadcraft-cli -- run --metric --cmd "$W" \
  --cmd 'door {"wall":"w0","offset":3.0,"width":0.9}' \
  --cmd 'window {"wall":"w0","offset":7.0,"width":1.2,"sill":0.9}' \
  --save "$tmp/o.svg" >/dev/null 2>&1 || { echo "FAIL: svg export failed"; exit 1; }
python3 - "$tmp/w.svg" "$tmp/o.svg" <<'PY' || exit 1
import sys,re
a=len(re.findall(r'<(path|polyline|polygon)',open(sys.argv[1]).read()))
b=len(re.findall(r'<(path|polyline|polygon)',open(sys.argv[2]).read()))
assert b>a, f"openings added no plan geometry: {a} vs {b}"
print("ok: svg prims",a,"->",b)
PY
echo "PASS 05"
