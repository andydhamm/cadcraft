#!/usr/bin/env bash
set -uo pipefail
cargo xtask layers || { echo "FAIL: layers"; exit 1; }
cargo xtask wasm   || { echo "FAIL: wasm"; exit 1; }
for c in arch arch3d; do
  grep -rnE '^\s*use (wgpu|egui|eframe|winit)|cadcraft-(wgpu|ui-egui)' "crates/$c/src" >/dev/null 2>&1 && { echo "FAIL: crates/$c imports UI crates"; exit 1; }
  grep -q 'deny(clippy::unwrap_used' "crates/$c/src/lib.rs" || { echo "FAIL: crates/$c missing never-crash deny"; exit 1; }
done
cargo test -q -p cadcraft-ui-egui i18n >/tmp/i18n.$$ 2>&1
if grep -q 'missing translations' /tmp/i18n.$$; then echo "FAIL: uk.tsv missing rows"; grep -A3 'missing translations' /tmp/i18n.$$|head -20; rm -f /tmp/i18n.$$; exit 1; fi
rm -f /tmp/i18n.$$
cargo test -q -p cadcraft-doc -p cadcraft-arch -p cadcraft-arch3d -p cadcraft-io || { echo "FAIL: crate tests"; exit 1; }
echo "PASS 08"
