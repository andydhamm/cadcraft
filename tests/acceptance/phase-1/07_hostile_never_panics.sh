#!/usr/bin/env bash
set -uo pipefail
bad=0
run(){ out=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$1" 2>&1); case "$out" in *panicked*|*RUST_BACKTRACE*) echo "FAIL: panic from: $1"; echo "$out"|head -5; bad=1;; esac; }
run 'wall {"points":[]}'
run 'wall {"points":[[0,0]]}'
run 'wall {"points":[[0,0],[0,0]]}'
run 'wall {"points":[[0,0],[null,2]]}'
run 'wall {"points":[[0,0],[1e308,1e308]],"thickness":0.2}'
run 'wall {"points":[[0,0],[10,0]],"thickness":-1}'
run 'wall {"points":[[0,0],[10,0]],"height":"big"}'
run 'wall {"points":[[0,0],[10,0]],"thickness":1e-12}'
run 'wall {"points":[[0,0],[1,0],[2,0],[3,0]],"thickness":0.2,"height":2.4}'
run 'arch.wall.stretch {"wall":"nope","end":"start","to":[0,0]}'
run 'arch.wall.stretch {"wall":"w1","end":"middle","to":[0,0]}'
run 'build3d {}'
python3 -c 'import json;print("wall "+json.dumps({"points":[[i%50,i//50] for i in range(2000)],"thickness":0.2,"height":2.4}))' > /tmp/h.txt
out=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$(cat /tmp/h.txt)" 2>&1) || true
case "$out" in *panicked*) echo "FAIL: panic on 2000-point wall"; bad=1;; esac
rm -f /tmp/h.txt
[ "$bad" = 0 ] && echo "PASS 07" || exit 1
