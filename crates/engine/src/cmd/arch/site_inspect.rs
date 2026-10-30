//! The site's part of `arch.inspect` (phase 9): `{"site":true}`, `{"terrain":true}` and
//! `{"plants":true}`. Filled in by `p9-cmd-site-inspect`.

use cadcraft_doc::arch::ArchModel;
use serde_json::{Map, Value};

/// Add the requested site keys to `out`. Read-only.
pub(crate) fn inspect_site(model: &ArchModel, p: &Value, out: &mut Map<String, Value>) {
    let _ = (model, p, out);
}
