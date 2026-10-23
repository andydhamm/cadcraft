#!/usr/bin/env bash
set -uo pipefail
W='wall {"points":[[0,0],[10,0]],"thickness":0.2,"height":2.4}'
fails=0
try(){ cargo run -q -p cadcraft-cli -- run --metric --cmd "$W" --cmd "$1" >/dev/null 2>/tmp/e.$$; local rc=$?
  grep -qE 'panicked|attempt to|unsafe' /tmp/e.$$ && { echo "FAIL: panic on $1"; head -3 /tmp/e.$$; fails=$((fails+1)); }
  [ $rc -eq 101 ] && { echo "FAIL: abort on $1"; fails=$((fails+1)); }; }
for c in 'door {"wall":"w0","offset":null}' 'door {"wall":"w0","offset":[1,2,3]}' \
  'door {"wall":"w0","offset":3,"width":-1}' 'door {"wall":"w0","offset":3,"width":0}' \
  'window {"wall":"w0","offset":3,"sill":-5}' 'window {"wall":"w0","offset":3,"style":"bogus"}' \
  'door {"wall":"w0","offset":3,"style":"bogus"}' 'garagedoor {"wall":"w0","offset":3,"width":null}' \
  'door {"wall":"","offset":3}' 'door {}' 'door {"wall":"w0","offset":1e308,"width":1e308}' \
  'arch.opening.move {"opening":"o99","offset":1}' 'arch.opening.move {"opening":"o0","offset":"x"}' \
  'arch.opening.move {}' 'door {"wall":"w0","offset":3,"swingAngle":1e9}' \
  'window {"wall":"w0","offset":3,"height":1e-12}' 'door {"wall":"w0","offset":3,"recess":1e9}'; do try "$c"; done
rm -f /tmp/e.$$
[ $fails -ne 0 ] && { echo "FAIL: $fails hostile cases"; exit 1; }
big=$(cargo run -q -p cadcraft-cli -- run --metric --cmd 'arch.sample {"walls":300,"openings":200}' --cmd 'build3d {}' 2>/dev/null | tail -n 1) || { echo "FAIL: sample+build failed"; exit 1; }
python3 - "$big" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1])
assert d.get("openings",0)>0 and d.get("triangles",0)>0, d
print("ok: sample",d.get("openings"),"openings")
PY
echo "PASS 07"
