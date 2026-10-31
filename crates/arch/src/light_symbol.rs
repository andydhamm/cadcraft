//! Plan symbols of light fixtures (phase 10), drawn on `E-LITE`. Filled by `p10-arch-light-symbol`.

use cadcraft_doc::arch::LightFixture;
use cadcraft_geom::Vec2;

/// Plan symbol polylines (metres; a closed ring repeats its first point) of the fixture's shape at
/// its position, sized by `crate::light::housing_diameter`.
pub fn plan_symbol(f: &LightFixture) -> Vec<Vec<Vec2>> {
    let _ = f;
    Vec::new()
}
