//! Layer role classification.

/// Role of a layer in detection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerRole {
    /// Wall layer.
    Wall,
    /// Door layer.
    Door,
    /// Window layer.
    Window,
    /// Room label layer.
    Room,
    /// Annotation layer.
    Annotation,
    /// Furniture layer.
    Furniture,
    /// Other.
    Other,
}

/// Determine the role of a layer by name.
pub fn layer_role(_name: &str) -> LayerRole {
    LayerRole::Other
}

/// Whether a layer role is allowed for wall faces.
pub fn wall_face_allowed(role: LayerRole) -> bool {
    !matches!(role, LayerRole::Door | LayerRole::Window | LayerRole::Annotation | LayerRole::Furniture)
}

/// Bonus score for a layer role matching the wanted role.
pub fn role_bonus(role: LayerRole, want: LayerRole) -> f64 {
    if role == want { 0.15 } else { 0.0 }
}
