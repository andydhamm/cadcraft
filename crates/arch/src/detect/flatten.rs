//! Flatten DXF entities into primitives.

use super::{DetectError, DetectOptions, Linework};
use cadcraft_doc::Drawing;

/// Flatten the drawing into linework primitives.
pub fn flatten(_d: &Drawing, _opts: &DetectOptions) -> Result<Linework, DetectError> {
    Ok(Linework::default())
}

/// Extract text from a raw label string.
pub fn label_text(raw: &str) -> String {
    raw.to_string()
}
