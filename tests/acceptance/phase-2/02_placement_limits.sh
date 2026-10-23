#!/usr/bin/env bash
set -uo pipefail
W='wall {"points":[[0,0],[10,0]],"thickness":0.2,"height":2.4,"exterior":true}'
fails=0
bad(){ local l="$1"; shift; local o; o=$(cargo run -q -p cadcraft-cli -- run --metric "$@" 2>&1); local rc=$?
  [ $rc -eq 0 ] && { echo "FAIL: $l should be rejected"; fails=$((fails+1)); }
  case "$o" in *panicked*) echo "FAIL: $l panicked"; fails=$((fails+1));; esac; }
bad "wider than wall" --cmd "$W" --cmd 'door {"wall":"w0","offset":5.0,"width":12.0}'
bad "garage below min width" --cmd "$W" --cmd 'garagedoor {"wall":"w0","offset":5.0,"width":1.0}'
bad "window on short wall" --cmd 'wall {"points":[[0,0],[0.12,0]],"thickness":0.2}' --cmd 'window {"wall":"w0","offset":0.06,"width":0.6}'
bad "offset past end" --cmd "$W" --cmd 'door {"wall":"w0","offset":9.9,"width":0.9}'
bad "negative offset" --cmd "$W" --cmd 'door {"wall":"w0","offset":-1.0}'
bad "overlapping" --cmd "$W" --cmd 'door {"wall":"w0","offset":3.0,"width":0.9}' --cmd 'window {"wall":"w0","offset":3.2,"width":0.9}'
bad "unknown host" --cmd "$W" --cmd 'door {"wall":"w99","offset":3.0}'
bad "non-finite width" --cmd "$W" --cmd 'door {"wall":"w0","offset":3.0,"width":1e400}'
bad "hostile types" --cmd "$W" --cmd 'door {"wall":5,"offset":[1,2],"width":"big"}'
bad "missing offset" --cmd "$W" --cmd 'door {"wall":"w0"}'
bad "sill above header" --cmd "$W" --cmd 'window {"wall":"w0","offset":3.0,"sill":2.3,"height":1.0}'
[ $fails -ne 0 ] && { echo "FAIL: $fails limit cases wrong"; exit 1; }

# fit:true on a door far too wide for the wall must auto-resize it.
fit=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$W" \
  --cmd 'door {"wall":"w0","offset":5.0,"width":12.0,"fit":true}' \
  --cmd 'arch.inspect {"openings":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: auto-resize failed"; exit 1; }
python3 - "$fit" <<'PY' || exit 1
import json,sys
o=json.loads(sys.argv[1]).get("openings",[])
assert len(o)==1, o
w=o[0]["width"]
assert 0.3<=w<12.0, f"fit:true must shrink a 12 m door to fit: {w}"
assert o[0]["rough"]["width"]>w, o[0]
print("ok: auto-resized",round(w,3))
PY

# fit:true near the wall end must keep the rough opening inside the wall.
edge=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$W" \
  --cmd 'door {"wall":"w0","offset":0.05,"width":0.9,"fit":true}' \
  --cmd 'arch.inspect {"openings":true,"symbols":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: edge fit failed"; exit 1; }
python3 - "$edge" <<'PY' || exit 1
import json,sys
o=json.loads(sys.argv[1]).get("openings",[])
assert len(o)==1, o
g=o[0].get("gap")
assert g and g[0]>=0.15-1e-6 and g[1]<=10.0-0.15+1e-6, f"rough span must stay inside the wall: {g}"
assert 0.3<=o[0]["width"]<=0.9+1e-9, o[0]
print("ok: rough span kept inside the wall",[round(v,3) for v in g])
PY

sep=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$W" \
  --cmd 'door {"wall":"w0","offset":2.0,"width":0.9}' --cmd 'window {"wall":"w0","offset":4.0,"width":0.9}' \
  --cmd 'arch.inspect {"openings":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: separation failed"; exit 1; }
python3 - "$sep" <<'PY' || exit 1
import json,sys
assert len(json.loads(sys.argv[1]).get("openings",[]))==2, "separated openings must coexist"
print("ok: separation respected")
PY
echo "PASS 02"
