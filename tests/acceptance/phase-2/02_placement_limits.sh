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
fit=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$W" \
  --cmd 'door {"wall":"w0","offset":0.05,"width":0.9,"fit":true}' \
  --cmd 'arch.inspect {"openings":true}' 2>/dev/null | tail -n 1) || { echo "FAIL: auto-resize failed"; exit 1; }
python3 - "$fit" <<'PY' || exit 1
import json,sys
o=json.loads(sys.argv[1]).get("openings",[])
assert len(o)==1 and 0.3<=o[0]["width"]<0.9, f"fit:true must shrink to fit: {o}"
print("ok: auto-resized",round(o[0]["width"],3))
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
