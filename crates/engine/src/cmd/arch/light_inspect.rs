//! The light fixtures' part of `arch.inspect` (phase 10): `{"lights":true}`. Filled in by
//! `p10-cmd-lightfixture`.

use cadcraft_doc::arch::ArchModel;
use serde_json::{Map, Value};

/// Add the `lights` key to `out` when `p` asks for it (`{"lights":true}`). Read-only.
pub(crate) fn inspect_lights(model: &ArchModel, p: &Value, out: &mut Map<String, Value>) {
    let _ = (model, p, out);
}
