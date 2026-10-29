#!/usr/bin/env bash
set -uo pipefail
out=$(cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'sun {"lat":51.05,"date":"2026-06-21","time":"solar-noon"}' \
  --cmd 'sun {"lat":51.05,"date":"2026-12-21","time":"solar-noon"}' \
  --cmd 'sun {"lat":51.05,"date":"2026-06-21","time":"06:00"}' 2>/dev/null | tail -n 3)
[ -n "$out" ] || { echo "FAIL: sun produced nothing"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys,math
rows=[json.loads(l) for l in sys.argv[1].splitlines() if l.strip().startswith('{')]
assert len(rows)==3, rows
for r in rows:
    assert r.get("ok") is True, r
    assert 0<=r["azimuth"]<360, r
    d=r["dir"]; assert len(d)==3 and abs(math.sqrt(sum(c*c for c in d))-1)<1e-6, d
assert abs(rows[0]["elevation"]-62.39)<=0.1, f"summer noon elevation {rows[0]['elevation']} != 62.39"
assert abs(rows[0]["azimuth"]-180.0)<=0.5, f"azimuth {rows[0]['azimuth']} != 180"
assert rows[0]["dir"][1]<0 and rows[0]["dir"][2]>0, rows[0]["dir"]
assert abs(rows[1]["elevation"]-15.5)<=0.6, rows[1]
assert rows[2]["azimuth"]<120, f"06:00 must be east, got {rows[2]['azimuth']}"
print("ok: sun", rows[0]["elevation"], rows[0]["azimuth"])
PY
echo "PASS 01"
