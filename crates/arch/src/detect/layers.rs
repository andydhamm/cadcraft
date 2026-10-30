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
///
/// Uses AIA CAD Layer Guidelines / US NCS major groups.
/// Names are upper-cased and split on `-`, `_`, ` `, `|`, `$`.
/// If the name contains an XREF separator (`|` or `$`), the part after the last one is used.
/// If both a wall word and a door/window word appear, door/window wins.
pub fn layer_role(name: &str) -> LayerRole {
    let upper = name.to_uppercase();

    // Handle XREF-bound names: "PLAN$0$A-WALL" or "PLAN|A-WALL"
    // We take the part after the last separator if present.
    let effective_name = if let Some(idx) = upper.rfind('|') {
        &upper[idx + 1..]
    } else if let Some(idx) = upper.rfind('$') {
        &upper[idx + 1..]
    } else {
        &upper
    };

    // Split into words
    let words: Vec<&str> = effective_name.split(|c| c == '-' || c == '_' || c == ' ' || c == '|' || c == '$').filter(|s| !s.is_empty()).collect();

    let mut has_wall = false;
    let mut has_door = false;
    let mut has_window = false;
    let mut has_room = false;
    let mut has_annotation = false;
    let mut has_furniture = false;

    for word in words {
        match word {
            "WALL" | "WALLS" | "PRHT" | "MUR" | "WAND" => has_wall = true,
            "DOOR" | "DOORS" | "DR" => has_door = true,
            "GLAZ" | "WIND" | "WINDOW" | "WINDOWS" | "WDW" => has_window = true,
            "AREA" | "ROOM" | "SPCE" | "SPACE" => has_room = true,
            "ANNO" | "DIMS" | "DIM" | "TEXT" | "NOTE" | "SYMB" | "IDEN" | "TTLB" | "GRID" | "HATC" | "PATT" | "DEFPOINTS" => has_annotation = true,
            "FURN" | "EQPM" | "FIXT" | "APPL" | "CASE" | "CASEWORK" | "PLMB" | "CLNG" => has_furniture = true,
            _ => {}
        }
    }

    // Precedence: Door/Window > Wall > Room > Annotation > Furniture > Other
    if has_door || has_window {
        if has_door && !has_window {
            return LayerRole::Door;
        }
        if has_window && !has_door {
            return LayerRole::Window;
        }
        // If both, ambiguous, but usually specific. Let's pick Door if both?
        // Spec says "door or window wins". If both, it's a mixed layer.
        // Usually A-DOOR and A-GLAZ are separate. If a layer has both words, it's weird.
        // Let's default to Door if both present? Or maybe check order?
        // The spec says "When both a wall word and a door or window word appear, the door or window wins".
        // It doesn't specify Door vs Window. I'll return Door if both present, as doors are more structural?
        // Actually, let's look at the words. If "A-DOOR-WINDOW", maybe it's a door with a window.
        // I'll prioritize Door.
        return LayerRole::Door;
    }

    if has_wall {
        return LayerRole::Wall;
    }

    if has_room {
        return LayerRole::Room;
    }

    if has_annotation {
        return LayerRole::Annotation;
    }

    if has_furniture {
        return LayerRole::Furniture;
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aia_layer_names_get_their_roles() {
        assert_eq!(layer_role("A-WALL"), LayerRole::Wall);
        assert_eq!(layer_role("A-DOOR"), LayerRole::Door);
        assert_eq!(layer_role("A-GLAZ"), LayerRole::Window);
        assert_eq!(layer_role("A-FURN"), LayerRole::Furniture);
        assert_eq!(layer_role("A-ANNO"), LayerRole::Annotation);
        assert_eq!(layer_role("A-DIMS"), LayerRole::Annotation);
        assert_eq!(layer_role("A-HATC"), LayerRole::Annotation);
        assert_eq!(layer_role("A-ROOM"), LayerRole::Room);

        // NCS variants
        assert_eq!(layer_role("WALL"), LayerRole::Wall);
        assert_eq!(layer_role("DOOR"), LayerRole::Door);
        assert_eq!(layer_role("GLAZ"), LayerRole::Window);
        assert_eq!(layer_role("AREA"), LayerRole::Room);
        assert_eq!(layer_role("TEXT"), LayerRole::Annotation);
        assert_eq!(layer_role("FURN"), LayerRole::Furniture);

        // Mixed words: Door/Window wins over Wall
        assert_eq!(layer_role("A-WALL-DOOR"), LayerRole::Door);
        assert_eq!(layer_role("A-WALL-WINDOW"), LayerRole::Window);
        assert_eq!(layer_role("WALL_DOOR"), LayerRole::Door);
    }

    #[test]
    fn unknown_and_zero_layers_are_other() {
        assert_eq!(layer_role("0"), LayerRole::Other);
        assert_eq!(layer_role("Defpoints"), LayerRole::Annotation); // Spec says Defpoints is Annotation
        assert_eq!(layer_role("Layer1"), LayerRole::Other);
        assert_eq!(layer_role("Random"), LayerRole::Other);
        assert_eq!(layer_role(""), LayerRole::Other);
    }

    #[test]
    fn xref_bound_names_keep_their_role() {
        assert_eq!(layer_role("PLAN$0$A-WALL"), LayerRole::Wall);
        assert_eq!(layer_role("PLAN|A-DOOR"), LayerRole::Door);
        assert_eq!(layer_role("XREF$1$A-GLAZ"), LayerRole::Window);
        assert_eq!(layer_role("BUILDING|A-WALL"), LayerRole::Wall);
        assert_eq!(layer_role("A$B$C$A-ROOM"), LayerRole::Room);
    }
}
