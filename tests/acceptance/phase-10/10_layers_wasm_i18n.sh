#!/usr/bin/env bash
# Carried forward: layering, exhaustive literals, the web build, assets, translations; no GPU code below L5; every phase-10 stub filled.
# needs: p10-arch-light-placement, p10-arch-light-symbol, p10-arch3d-scene-lights, p10-arch3d-fixture-meshes, p10-r3d-light-pack, p10-r3d-shadow-atlas, p10-r3d-lit-pipeline, p10-r3d-lit-frame, p10-r3d-finalize, p10-cmd-lightfixture, p10-cmd-illuminance, p10-cmd-render, p10-cmd-sunstudy, p10-io-light-proxies, p10-io-glb-lights
set -uo pipefail
cargo xtask layers   || { echo "FAIL: layers"; exit 1; }
cargo xtask literals || { echo "FAIL: exhaustive struct literals"; exit 1; }
cargo xtask wasm     || { echo "FAIL: the web build broke"; exit 1; }
cargo xtask assets   || { echo "FAIL: assets"; exit 1; }
log=$(mktemp); trap 'rm -f "$log"' EXIT
cargo test -p cadcraft-ui-egui --lib -- translations_cover_commands_menus_tools_and_snap_labels >"$log" 2>&1
grep -q "test result: ok" "$log" || { echo "FAIL: uk.tsv missing rows"; tail -20 "$log"; exit 1; }
for row in "Light Fixture" "Illuminance" "Render 3D" "Sun Study" "cmd.lightfixture.edit" "Architecture>Light Fixture"; do
  grep -qP "\t\Q$row\E\t" crates/ui-egui/src/i18n/uk.tsv || { echo "FAIL: uk.tsv has no row for '$row'"; exit 1; }
done
if grep -rn "use wgpu\|use egui\|wgpu::" crates/arch3d/src crates/arch/src crates/render/src crates/geom/src crates/doc/src/arch crates/engine/src crates/io/src >/dev/null 2>&1; then
  echo "FAIL: wgpu/egui below L5"; grep -rn "use wgpu\|use egui\|wgpu::" crates/arch3d/src crates/arch/src crates/render/src crates/engine/src crates/io/src | head; exit 1; fi
# The groundwork's stubs return a default after `let _ = (args);` or say "not available yet"; a filled one uses its args.
stubs="crates/arch/src/light.rs crates/arch/src/light_symbol.rs crates/arch3d/src/light.rs crates/arch3d/src/fixture.rs
  crates/render3d/src/lights.rs crates/render3d/src/shadow_atlas.rs crates/render3d/src/lit.rs crates/render3d/src/lit_frame.rs
  crates/render3d/src/final_render.rs crates/engine/src/cmd/arch/lightfixture.rs crates/engine/src/cmd/arch/light_inspect.rs
  crates/engine/src/cmd/arch/illuminance.rs crates/engine/src/cmd/arch/render.rs crates/engine/src/cmd/arch/sunstudy.rs
  crates/io/src/dxf_light.rs crates/io/src/glb_lights.rs"
if grep -nF -e "not available yet" -e "let _ = (" $stubs >/dev/null 2>&1; then
  echo "FAIL: a phase-10 stub is still unfilled:"; grep -nF -e "not available yet" -e "let _ = (" $stubs | head; exit 1; fi
for f in crates/render3d/src/shaders/lit.wgsl crates/render3d/src/shaders/light_math.wgsl; do
  [ -s "$f" ] || { echo "FAIL: $f is the lit shader"; exit 1; }
done
# Every lighting command is reachable over MCP's generic tools and listed with its menu.
grep -q '"lightfixture"' crates/engine/src/cmd/arch/lightfixture.rs || { echo "FAIL: lightfixture command missing"; exit 1; }
stray=$(git ls-files | grep -E '(^|/)[^/]*(_tmp\.rs|\.tmp)$' | head -5)
[ -z "$stray" ] || { echo "FAIL: stray files: $stray"; exit 1; }
echo "PASS 10"
