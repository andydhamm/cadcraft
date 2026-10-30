#!/usr/bin/env bash
# Carried forward: layering, exhaustive literals, the web build, assets, translations; detection lives in crates/arch.
set -uo pipefail
cargo xtask layers   || { echo "FAIL: layers"; exit 1; }
cargo xtask literals || { echo "FAIL: exhaustive struct literals"; exit 1; }
cargo xtask wasm     || { echo "FAIL: the web build broke"; exit 1; }
cargo xtask assets   || { echo "FAIL: assets (every new file under tests/fixtures/archdetect/real/ needs its ATTRIBUTION.md row)"; exit 1; }
log=$(mktemp); trap 'rm -f "$log"' EXIT
cargo test -p cadcraft-ui-egui --lib -- translations_cover_commands_menus_tools_and_snap_labels >"$log" 2>&1
grep -q "test result: ok" "$log" || { echo "FAIL: uk.tsv missing rows"; tail -20 "$log"; exit 1; }
for row in "Detect from Linework" "Architecture>Detect from Linework" "cmd.archdetect" "Detection Review" "Accept Selected"; do
  grep -qP "\t\Q$row\E\t" crates/ui-egui/src/i18n/uk.tsv || { echo "FAIL: uk.tsv has no row for '$row'"; exit 1; }
done
[ -d crates/arch/src/detect ] || { echo "FAIL: detection belongs in crates/arch/src/detect/ (2D algorithms live in crates/arch)"; exit 1; }
if grep -rn "use wgpu\|use egui" crates/arch3d/src crates/arch/src crates/render/src crates/doc/src/arch.rs >/dev/null 2>&1; then
  echo "FAIL: wgpu/egui below L5"; grep -rn "use wgpu\|use egui" crates/arch3d/src crates/arch/src crates/render/src | head; exit 1; fi
stray=$(git ls-files | grep -E '(^|/)[^/]*(_tmp\.rs|\.tmp)$' | head -5)
[ -z "$stray" ] || { echo "FAIL: stray files: $stray"; exit 1; }
echo "PASS 09"
