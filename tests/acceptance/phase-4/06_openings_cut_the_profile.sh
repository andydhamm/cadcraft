#!/usr/bin/env bash
set -uo pipefail
W='wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}'
out=$(cargo run -q -p cadcraft-cli -- run --metric --cmd "$W" \
  --cmd 'window {"wall":"w2","offset":6.0,"width":0.9,"height":1.2,"sill":0.9}' \
  --cmd 'build3d {"probe":[[4.0,8.0,1.5],[4.0,8.0,0.5],[4.0,8.0,2.3],[5.0,8.0,1.5]]}' 2>/dev/null | tail -n 1) || { echo "FAIL: window probe run crashed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
in_hole,below,above,side=json.loads(sys.argv[1])["probe"]
assert not (in_hole and in_hole["part"]=="wall"), f"inside the rough opening must find no wall: {in_hole}"
assert below and below["part"]=="wall", f"below the sill must find the wall: {below}"
assert above and above["part"]=="wall", f"above the header must find the wall: {above}"
assert side and side["part"]=="wall", f"beside the opening must find the wall: {side}"
print("ok: the wall is really cut")
PY
cargo test -p cadcraft-arch3d --test hlr_section -- a_window_beyond_a_section_shows_through_its_wall --exact 2>&1 | tee /tmp/p4hlr.log
grep -q "test result: ok. 1 passed" /tmp/p4hlr.log || { echo "FAIL: formerly ignored HLR test did not pass"; exit 1; }
grep -q "ignore" crates/arch3d/tests/hlr_section.rs && { echo "FAIL: the test is still #[ignore]d"; exit 1; }
echo "PASS 06"
