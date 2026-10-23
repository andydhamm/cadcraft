#!/usr/bin/env bash
set -uo pipefail
out=$(cargo run -q --release -p cadcraft-cli -- run --metric \
  --cmd 'arch.sample {"walls":300,"openings":150}' --cmd 'build3d {}' 2>/dev/null | tail -n 1) || { echo "FAIL: bench failed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1])
assert d.get("meshes",0)>=300, d.get("meshes")
assert d.get("openings",0)>=150, d.get("openings")
ms=d.get("rebuildMs"); assert ms is not None and ms<50.0, f"300 walls + 150 openings took {ms} ms (budget 50)"
print(f"ok: {d['meshes']} meshes / {d['openings']} openings in {ms:.1f} ms")
PY
inc=$(cargo run -q --release -p cadcraft-cli -- run --metric \
  --cmd 'arch.sample {"walls":300,"openings":150}' \
  --cmd 'arch.opening.move {"opening":"o10","offset":1.0}' --cmd 'build3d {}' 2>/dev/null | tail -n 1) || { echo "FAIL: incremental bench failed"; exit 1; }
python3 - "$inc" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1])
assert d.get("openings",0)>=150 and d.get("rebuildMs",1e9)<50.0, d
print("ok: after an opening edit",f"{d['rebuildMs']:.1f} ms")
PY
echo "PASS 09"
