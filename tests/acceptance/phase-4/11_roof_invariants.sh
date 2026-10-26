#!/usr/bin/env bash
# Property invariants over random orthogonal footprints, plus golden roof scenes.
cargo test -p cadcraft-geom --test skeleton_roof_property_tests 2>&1 | tee /tmp/p4a.log
grep -q "test result: ok" /tmp/p4a.log || { echo "FAIL: roof skeleton invariants"; exit 1; }
cargo test -p cadcraft-arch --test property_roofs 2>&1 | tee /tmp/p4b.log
grep -q "test result: ok" /tmp/p4b.log || { echo "FAIL: arch roof property tests"; exit 1; }
cargo test -p cadcraft-arch3d --test golden_roofs 2>&1 | tee /tmp/p4c.log
grep -q "test result: ok" /tmp/p4c.log || { echo "FAIL: golden roof scenes"; exit 1; }
grep -q "every_plane_slope_equals_its_pitch" /tmp/p4b.log || { echo "FAIL: the pitch invariant did not run"; exit 1; }
grep -q "interior_edges_are_shared_by_two_planes" /tmp/p4b.log || { echo "FAIL: the shared-edge invariant did not run"; exit 1; }
echo "PASS 11"
