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

    #[test]
    fn mixed_xref_separators() {
        // When both | and $ appear, the last separator of either kind wins
        assert_eq!(layer_role("PLAN|XREF$A-WALL"), LayerRole::Wall);
        assert_eq!(layer_role("PLAN$XREF|A-DOOR"), LayerRole::Door);
        assert_eq!(layer_role("A$B|C$A-GLAZ"), LayerRole::Window);
        assert_eq!(layer_role("X|Y$Z|A-ROOM"), LayerRole::Room);
    }

    #[test]
    fn wall_face_allowed_by_role() {
        assert!(wall_face_allowed(LayerRole::Wall));
        assert!(wall_face_allowed(LayerRole::Room));
        assert!(wall_face_allowed(LayerRole::Other));
        assert!(!wall_face_allowed(LayerRole::Door));
        assert!(!wall_face_allowed(LayerRole::Window));
        assert!(!wall_face_allowed(LayerRole::Annotation));
        assert!(!wall_face_allowed(LayerRole::Furniture));
    }

    #[test]
    fn role_bonus_matches() {
        assert_eq!(role_bonus(LayerRole::Wall, LayerRole::Wall), 0.15);
        assert_eq!(role_bonus(LayerRole::Door, LayerRole::Door), 0.15);
        assert_eq!(role_bonus(LayerRole::Window, LayerRole::Window), 0.15);
        assert_eq!(role_bonus(LayerRole::Room, LayerRole::Room), 0.15);
        assert_eq!(role_bonus(LayerRole::Annotation, LayerRole::Annotation), 0.15);
        assert_eq!(role_bonus(LayerRole::Furniture, LayerRole::Furniture), 0.15);
        assert_eq!(role_bonus(LayerRole::Other, LayerRole::Other), 0.15);
    }

    #[test]
    fn role_bonus_no_match() {
        assert_eq!(role_bonus(LayerRole::Wall, LayerRole::Door), 0.0);
        assert_eq!(role_bonus(LayerRole::Door, LayerRole::Wall), 0.0);
        assert_eq!(role_bonus(LayerRole::Window, LayerRole::Room), 0.0);
        assert_eq!(role_bonus(LayerRole::Other, LayerRole::Wall), 0.0);
        assert_eq!(role_bonus(LayerRole::Wall, LayerRole::Other), 0.0);
        assert_eq!(role_bonus(LayerRole::Annotation, LayerRole::Furniture), 0.0);
    }

    #[test]
    fn case_insensitive_layer_names() {
        assert_eq!(layer_role("a-wall"), LayerRole::Wall);
        assert_eq!(layer_role("A-Wall"), LayerRole::Wall);
        assert_eq!(layer_role("a-door"), LayerRole::Door);
        assert_eq!(layer_role("A-Door"), LayerRole::Door);
        assert_eq!(layer_role("a-glaz"), LayerRole::Window);
        assert_eq!(layer_role("A-Glaz"), LayerRole::Window);
        assert_eq!(layer_role("a-furn"), LayerRole::Furniture);
        assert_eq!(layer_role("a-anno"), LayerRole::Annotation);
        assert_eq!(layer_role("a-room"), LayerRole::Room);
        assert_eq!(layer_role("A-ROOM"), LayerRole::Room);
    }

    #[test]
    fn double_and_triple_separators() {
        assert_eq!(layer_role("A--WALL"), LayerRole::Wall);
        assert_eq!(layer_role("A-WALL--DOOR"), LayerRole::Door);
        assert_eq!(layer_role("A---WALL"), LayerRole::Wall);
        assert_eq!(layer_role("-WALL-"), LayerRole::Wall);
        assert_eq!(layer_role("_WALL_"), LayerRole::Wall);
        assert_eq!(layer_role(" WALL "), LayerRole::Wall);
        assert_eq!(layer_role("A-WALL-_-DOOR"), LayerRole::Door);
    }

    #[test]
    fn both_door_and_window_words() {
        // When both door and window words appear, Door wins
        assert_eq!(layer_role("A-DOOR-WINDOW"), LayerRole::Door);
        assert_eq!(layer_role("A-WINDOW-DOOR"), LayerRole::Door);
        assert_eq!(layer_role("DOOR_AND_WINDOW"), LayerRole::Door);
        assert_eq!(layer_role("A-DR-WDW"), LayerRole::Door);
    }

    #[test]
    fn european_and_partition_words() {
        assert_eq!(layer_role("MUR"), LayerRole::Wall);
        assert_eq!(layer_role("WAND"), LayerRole::Wall);
        assert_eq!(layer_role("PRHT"), LayerRole::Wall);
        assert_eq!(layer_role("A-PRHT"), LayerRole::Wall);
        assert_eq!(layer_role("A-MUR"), LayerRole::Wall);
        assert_eq!(layer_role("A-WAND"), LayerRole::Wall);
        assert_eq!(layer_role("mur-interieur"), LayerRole::Wall);
    }

    #[test]
    fn layer_names_with_numbers() {
        assert_eq!(layer_role("A-WALL-1"), LayerRole::Wall);
        assert_eq!(layer_role("WALL2"), LayerRole::Other);
        assert_eq!(layer_role("A-DOOR-01"), LayerRole::Door);
        assert_eq!(layer_role("A-GLAZ-2"), LayerRole::Window);
        assert_eq!(layer_role("WALL-123"), LayerRole::Wall);
        assert_eq!(layer_role("1-WALL"), LayerRole::Wall);
    }

    #[test]
    fn single_unrecognized_words() {
        assert_eq!(layer_role("W"), LayerRole::Other);
        assert_eq!(layer_role("D"), LayerRole::Other);
        assert_eq!(layer_role("X"), LayerRole::Other);
        assert_eq!(layer_role("A"), LayerRole::Other);
        assert_eq!(layer_role("B"), LayerRole::Other);
    }

    #[test]
    fn long_layer_names() {
        assert_eq!(layer_role("A-WALL-STRUCTURAL-LOAD-BEARING-EXT"), LayerRole::Wall);
        assert_eq!(layer_role("A-DOOR-HDWD-SINGLE-ACTIVE-32X80"), LayerRole::Door);
        assert_eq!(layer_role("A-GLAZ-FIXED-CURTAIN-WALL-ALUM"), LayerRole::Window);
        assert_eq!(layer_role("A-ANNO-DIMS-GENERAL-1_4"), LayerRole::Annotation);
    }
}
