#!/usr/bin/env bash
# Carried forward: layering, exhaustive literals, the web build, assets, translations; terrain lives in crates/arch.
set -uo pipefail
cargo xtask layers   || { echo "FAIL: layers"; exit 1; }
cargo xtask literals || { echo "FAIL: exhaustive struct literals"; exit 1; }
cargo xtask wasm     || { echo "FAIL: the web build broke"; exit 1; }
cargo xtask assets   || { echo "FAIL: assets"; exit 1; }
log=$(mktemp); trap 'rm -f "$log"' EXIT
cargo test -p cadcraft-ui-egui --lib -- translations_cover_commands_menus_tools_and_snap_labels >"$log" 2>&1
grep -q "test result: ok" "$log" || { echo "FAIL: uk.tsv missing rows"; tail -20 "$log"; exit 1; }
for row in "Site Location" "Terrain" "Terrain Modifier" "Plant" "cmd.terrain.contours" "cmd.plant.species" "Architecture>Terrain"; do
  grep -qP "\t\Q$row\E\t" crates/ui-egui/src/i18n/uk.tsv || { echo "FAIL: uk.tsv has no row for '$row'"; exit 1; }
done
grep -q "zorgglubb" ATTRIBUTION.md || { echo "FAIL: ATTRIBUTION.md must credit the lifted terrain code (zorgglubb/cadcraft)"; exit 1; }
for d in crates/arch/src/terrain crates/geom/src/cdt.rs crates/arch/src/plant.rs; do
  [ -e "$d" ] || { echo "FAIL: $d is where the terrain algorithms live"; exit 1; }
done
if grep -rn "use wgpu\|use egui" crates/arch3d/src crates/arch/src crates/render/src crates/geom/src crates/doc/src/arch >/dev/null 2>&1; then
  echo "FAIL: wgpu/egui below L5"; grep -rn "use wgpu\|use egui" crates/arch3d/src crates/arch/src crates/render/src | head; exit 1; fi
if grep -rn "not available yet" crates/geom/src/cdt.rs crates/arch/src/terrain crates/arch/src/plant.rs crates/arch/src/site.rs >/dev/null 2>&1; then
  echo "FAIL: a phase-9 stub is still unfilled:"; grep -rn "not available yet" crates/arch/src/terrain crates/arch/src/plant.rs crates/arch/src/site.rs | head; exit 1; fi
stray=$(git ls-files | grep -E '(^|/)[^/]*(_tmp\.rs|\.tmp)$' | head -5)
[ -z "$stray" ] || { echo "FAIL: stray files: $stray"; exit 1; }
echo "PASS 11"
