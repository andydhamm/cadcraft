#!/usr/bin/env bash
set -uo pipefail
cargo xtask layers || { echo "FAIL: cargo xtask layers"; exit 1; }
cargo xtask wasm   || { echo "FAIL: cargo xtask wasm"; exit 1; }
for c in arch arch3d; do
  grep -rnE '^\s*use (wgpu|egui|eframe|winit)|cadcraft-(wgpu|ui-egui)' "crates/$c/src" >/dev/null 2>&1 && { echo "FAIL: crates/$c imports UI crates"; exit 1; }
  grep -q 'deny(clippy::unwrap_used' "crates/$c/src/lib.rs" || { echo "FAIL: crates/$c/lib.rs missing the never-crash deny"; exit 1; }
done
for n in arch arch3d render3d; do grep -q "\"$n\"" xtask/src/layers.rs || { echo "FAIL: $n not in the layer TABLE"; exit 1; }; done
cargo test -q -p cadcraft-arch -p cadcraft-arch3d || { echo "FAIL: arch tests"; exit 1; }
echo "PASS 08"
