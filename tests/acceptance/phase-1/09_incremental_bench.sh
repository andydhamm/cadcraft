#!/usr/bin/env bash
# Phase 1: a 300-wall house rebuilds incrementally in well under the phase-10 budget.
set -uo pipefail
out=$(cargo run -q --release -p cadcraft-cli -- run --metric \
  --cmd 'arch.sample {"walls":300}' \
  --cmd 'build3d {}' 2>/dev/null | tail -n 1) || { echo "FAIL: bench run failed"; exit 1; }
python3 - "$out" <<'PY' || exit 1
import json,sys
d=json.loads(sys.argv[1])
assert d.get("meshes",0)>=300, f"expected >=300 meshes, got {d.get('meshes')}"
assert d.get("triangles",0)>0, "no triangles"
ms=d.get("rebuildMs")
assert ms is not None, "build3d must report rebuildMs"
assert ms<50.0, f"full rebuild of 300 walls took {ms} ms (budget 50 ms)"
print(f"ok: {d['meshes']} meshes, {d['triangles']} tris, {ms:.1f} ms")
PY
echo "PASS 09"
