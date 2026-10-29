#!/usr/bin/env bash
set -uo pipefail
cargo xtask layers   || { echo "FAIL: layers"; exit 1; }
cargo xtask literals || { echo "FAIL: exhaustive struct literals"; exit 1; }
cargo xtask wasm     || { echo "FAIL: the web build broke"; exit 1; }
cargo xtask assets   || { echo "FAIL: assets"; exit 1; }
cargo test -p cadcraft-ui-egui --lib -- translations_cover_commands_menus_tools_and_snap_labels 2>&1 | tee /tmp/p6i18n.log
grep -q "test result: ok" /tmp/p6i18n.log || { echo "FAIL: uk.tsv missing rows"; exit 1; }
for row in "Camera" "Pick 3D" "Sun" "Walkthrough" "Architecture"; do
  grep -qP "\t$row\t" crates/ui-egui/src/i18n/uk.tsv || { echo "FAIL: uk.tsv has no row for '$row'"; exit 1; }
done
if grep -rn "use wgpu\|use egui" crates/arch3d/src crates/arch/src crates/doc/src/arch.rs >/dev/null 2>&1; then
  echo "FAIL: wgpu/egui below L5"; grep -rn "use wgpu\|use egui" crates/arch3d/src crates/arch/src | head; exit 1; fi
echo "PASS 09"
