#!/usr/bin/env bash
set -uo pipefail
cargo xtask layers   || { echo "FAIL: layers"; exit 1; }
cargo xtask literals || { echo "FAIL: exhaustive struct literals"; exit 1; }
cargo xtask wasm     || { echo "FAIL: the web build broke"; exit 1; }
cargo xtask assets   || { echo "FAIL: assets"; exit 1; }
log=$(mktemp); trap 'rm -f "$log"' EXIT
cargo test -p cadcraft-ui-egui --lib -- translations_cover_commands_menus_tools_and_snap_labels >"$log" 2>&1
grep -q "test result: ok" "$log" || { echo "FAIL: uk.tsv missing rows"; tail -20 "$log"; exit 1; }
for row in "Elevation" "Section" "View Sheet" "Delete View" "Architecture"; do
  grep -qP "\t$row\t" crates/ui-egui/src/i18n/uk.tsv || { echo "FAIL: uk.tsv has no row for '$row'"; exit 1; }
done
if grep -rn "use wgpu\|use egui" crates/arch3d/src crates/arch/src crates/render/src crates/doc/src/arch.rs >/dev/null 2>&1; then
  echo "FAIL: wgpu/egui below L5"; grep -rn "use wgpu\|use egui" crates/arch3d/src crates/arch/src crates/render/src | head; exit 1; fi
# Stray files from earlier phases are gone and nothing new of the kind was added.
for f in mid.tmp dxf_write_arch3d_tmp.rs render_lib_arch3d_tmp.rs crates/arch/src/planarr.rs; do
  [ -e "$f" ] && { echo "FAIL: stray file $f is still in the tree"; exit 1; }
done
stray=$(git ls-files | grep -E '(^|/)[^/]*(_tmp\.rs|\.tmp)$' | head -5)
[ -z "$stray" ] || { echo "FAIL: stray files: $stray"; exit 1; }
echo "PASS 09"
