//! `cargo xtask literals`: shared structs are never built with an exhaustive struct literal
//! outside the module that defines them.
//!
//! Parallel segments add fields to the arch model and the 3D scene. An exhaustive literal
//! (`Wall { start, end, .., divider }` with no `..rest`) in another crate stops compiling as
//! soon as a field lands (`E0063 missing field`), so callers write
//! `Wall { start, end, ..Default::default() }` or use the type's constructor instead.
//!
//! This is a source scan, not a type check: comments and string literals are blanked, a
//! file that declares its own struct of the same name is skipped, and enum variants
//! (`Kind::Wall { .. }`) are ignored.

use std::path::Path;
use std::process::Command;

/// A struct that must not be built exhaustively outside `file`.
pub struct Guarded {
    pub name: &'static str,
    /// The file (module) that defines the struct, relative to the workspace root.
    pub file: &'static str,
    /// How to build one instead.
    pub fix: &'static str,
}

const DEFAULT: &str = "`..Default::default()`";

/// The guarded types. Add a type here when a second crate (or a test) starts building it.
pub const GUARDED: &[Guarded] = &[
    Guarded { name: "Wall", file: "crates/doc/src/arch.rs", fix: "`..Default::default()` or `Wall::new(start, end, thickness)`" },
    Guarded { name: "Level", file: "crates/doc/src/arch.rs", fix: DEFAULT },
    Guarded { name: "Opening", file: "crates/doc/src/arch.rs", fix: "`..Default::default()` or `Opening::new(host, offset, width, height, kind)`" },
    Guarded { name: "Room", file: "crates/doc/src/arch.rs", fix: DEFAULT },
    Guarded { name: "WallLayer", file: "crates/doc/src/arch.rs", fix: DEFAULT },
    Guarded { name: "WallType", file: "crates/doc/src/arch.rs", fix: DEFAULT },
    Guarded { name: "ArchModel", file: "crates/doc/src/arch.rs", fix: DEFAULT },
    Guarded { name: "WallShape", file: "crates/arch/src/join.rs", fix: "`WallShape::new(id, polygon, centre)` (or `..WallShape::new(..)`)" },
    Guarded { name: "Scene3d", file: "crates/arch3d/src/lib.rs", fix: DEFAULT },
    Guarded { name: "Mesh", file: "crates/arch3d/src/mesh.rs", fix: "`Mesh::new(material, object)` (or `..Mesh::new(..)`)" },
];

/// Replace comments and the contents of string and char literals with spaces, keeping
/// newlines so line numbers survive.
pub fn blank(src: &str) -> String {
    let c: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let keep = |ch: char| if ch == '\n' { '\n' } else { ' ' };
    let mut i = 0;
    while i < c.len() {
        let at = |k: usize| c.get(k).copied().unwrap_or('\0');
        if at(i) == '/' && at(i + 1) == '/' {
            while i < c.len() && c[i] != '\n' {
                out.push(' ');
                i += 1;
            }
        } else if at(i) == '/' && at(i + 1) == '*' {
            let mut depth = 0;
            while i < c.len() {
                if at(i) == '/' && at(i + 1) == '*' {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if at(i) == '*' && at(i + 1) == '/' {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(keep(c[i]));
                    i += 1;
                }
            }
        } else if at(i) == 'r' && (at(i + 1) == '"' || (at(i + 1) == '#' && matches!(at(i + 2), '"' | '#'))) && !ident_char(prev(&c, i)) {
            // Raw string r"..." / r#"..."#.
            let mut j = i + 1;
            let mut hashes = 0;
            while at(j) == '#' {
                hashes += 1;
                j += 1;
            }
            out.push_str(&" ".repeat(j + 1 - i));
            i = j + 1;
            while i < c.len() {
                if c[i] == '"' && (1..=hashes).all(|h| at(i + h) == '#') {
                    out.push_str(&" ".repeat(hashes + 1));
                    i += hashes + 1;
                    break;
                }
                out.push(keep(c[i]));
                i += 1;
            }
        } else if at(i) == '"' {
            out.push(' ');
            i += 1;
            while i < c.len() && c[i] != '"' {
                if c[i] == '\\' {
                    out.push(' ');
                    i += 1;
                }
                if let Some(&ch) = c.get(i) {
                    out.push(keep(ch));
                }
                i += 1;
            }
            out.push(' ');
            i += 1;
        } else if at(i) == '\'' && (at(i + 1) == '\\' || at(i + 2) == '\'') {
            // Char literal ('x', '\n', '\u{..}'); a lifetime ('a) is left alone.
            out.push(' ');
            i += 1;
            while i < c.len() && c[i] != '\'' {
                if c[i] == '\\' {
                    out.push(' ');
                    i += 1;
                }
                if i < c.len() {
                    out.push(' ');
                }
                i += 1;
            }
            out.push(' ');
            i += 1;
        } else {
            out.push(c[i]);
            i += 1;
        }
    }
    out
}

fn ident_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn prev(c: &[char], i: usize) -> char {
    if i == 0 { '\0' } else { c[i - 1] }
}

/// 1-based line numbers of exhaustive `name { .. }` literals (or patterns) in `src`.
pub fn scan(src: &str, name: &str) -> Vec<usize> {
    let s: Vec<char> = blank(src).chars().collect();
    let n: Vec<char> = name.chars().collect();
    let mut hits = Vec::new();
    let mut i = 0;
    while i + n.len() <= s.len() {
        if s[i..i + n.len()] != n[..] || ident_char(prev(&s, i)) || s.get(i + n.len()).is_some_and(|&ch| ident_char(ch)) {
            i += 1;
            continue;
        }
        let mut j = i + n.len();
        while s.get(j).is_some_and(|ch| ch.is_whitespace()) {
            j += 1;
        }
        if s.get(j) != Some(&'{') || !is_literal(&s, i) {
            i += 1;
            continue;
        }
        if !has_rest(&s, j) {
            hits.push(s[..i].iter().filter(|&&ch| ch == '\n').count() + 1);
        }
        i = j + 1;
    }
    hits
}

/// Whether the type name at `i` starts a literal, rather than a declaration (`struct`, `impl`,
/// `for`), a return type (`-> T {`) or an enum variant (`Kind::T {`).
fn is_literal(s: &[char], i: usize) -> bool {
    // Walk back over a `path::` prefix.
    let mut k = i;
    loop {
        let mut b = k;
        while b > 0 && s[b - 1].is_whitespace() {
            b -= 1;
        }
        if b >= 2 && s[b - 1] == ':' && s[b - 2] == ':' {
            let mut e = b - 2;
            while e > 0 && s[e - 1].is_whitespace() {
                e -= 1;
            }
            let mut st = e;
            while st > 0 && ident_char(s[st - 1]) {
                st -= 1;
            }
            if s.get(st).is_some_and(|ch| ch.is_uppercase()) {
                return false; // `Enum::Variant {`
            }
            k = st;
        } else {
            k = b;
            break;
        }
    }
    if k >= 2 && s[k - 1] == '>' && s[k - 2] == '-' {
        return false;
    }
    let mut st = k;
    while st > 0 && ident_char(s[st - 1]) {
        st -= 1;
    }
    let word: String = s[st..k].iter().collect();
    !matches!(word.as_str(), "struct" | "enum" | "union" | "impl" | "for" | "trait" | "mod" | "fn" | "type")
}

/// Whether the brace group opening at `open` has a top-level `..` right after `{` or a `,`.
fn has_rest(s: &[char], open: usize) -> bool {
    let mut depth = 0usize;
    let mut fresh = true; // at the start of a field
    let mut i = open;
    while i < s.len() {
        let ch = s[i];
        match ch {
            '{' | '(' | '[' => {
                depth += 1;
                fresh = depth == 1;
            }
            '}' | ')' | ']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return false;
                }
                fresh = false;
            }
            ',' if depth == 1 => fresh = true,
            '.' if depth == 1 && fresh && s.get(i + 1) == Some(&'.') => return true,
            c if c.is_whitespace() => {}
            _ => {
                if depth == 1 {
                    fresh = false;
                }
            }
        }
        i += 1;
    }
    false
}

/// Whether `src` declares its own struct called `name` (a different, local type).
fn declares(src: &str, name: &str) -> bool {
    let s = blank(src);
    s.match_indices("struct ").any(|(i, _)| {
        let rest = &s[i + "struct ".len()..];
        rest.starts_with(name) && !rest[name.len()..].starts_with(|ch: char| ident_char(ch))
    })
}

/// Violations across `files` (path, source), as printable messages.
pub fn check(files: &[(String, String)]) -> Vec<String> {
    let mut out = Vec::new();
    for g in GUARDED {
        for (path, src) in files {
            if path == g.file || declares(src, g.name) || !src.contains(g.name) {
                continue;
            }
            for line in scan(src, g.name) {
                out.push(format!(
                    "{path}:{line}: `{} {{ .. }}` lists every field outside {}; use {} so a new field does not break this",
                    g.name, g.file, g.fix
                ));
            }
        }
    }
    out
}

pub fn run(root: &Path) -> Result<(), String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(["ls-files", "--cached", "--others", "--exclude-standard", "--", "*.rs"])
        .output()
        .map_err(|e| format!("git ls-files: {e}"))?;
    let mut files = Vec::new();
    for p in String::from_utf8_lossy(&out.stdout).lines() {
        if let Ok(src) = std::fs::read_to_string(root.join(p)) {
            files.push((p.to_owned(), src));
        }
    }
    let mut stale = Vec::new();
    for g in GUARDED {
        let defined = files.iter().any(|(p, s)| p == g.file && declares(s, g.name));
        if !defined {
            stale.push(format!("{} is no longer declared in {} (update xtask/src/literals.rs)", g.name, g.file));
        }
    }
    if !stale.is_empty() {
        return Err(stale.join("\n"));
    }
    let v = check(&files);
    if v.is_empty() {
        println!("literals: {} shared structs, no exhaustive literals outside their modules", GUARDED.len());
        Ok(())
    } else {
        for m in &v {
            println!("  {m}");
        }
        Err(format!(
            "{} exhaustive struct literal(s) of shared types; add `..Default::default()` or use the constructor (docs/arch3d/DESIGN.md)",
            v.len()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exhaustive_literal_is_flagged() {
        assert_eq!(scan("fn f() {\n    let w = Wall { start, end };\n}", "Wall"), vec![2]);
        assert_eq!(scan("let s = cadcraft_arch3d::Scene3d { meshes, bounds };", "Scene3d"), vec![1]);
        assert_eq!(scan("push(Wall {\n  a: vec![X { y }],\n  b: f(1, 2),\n});", "Wall"), vec![1]);
    }

    #[test]
    fn a_literal_with_a_rest_passes() {
        assert!(scan("let w = Wall { start, end, ..Default::default() };", "Wall").is_empty());
        assert!(scan("let w = Wall { start, ..base };", "Wall").is_empty());
        assert!(scan("let w = Wall { ..Wall::new(a, b, 0.2) };", "Wall").is_empty());
        assert!(scan("let Wall { start, .. } = w;", "Wall").is_empty());
    }

    #[test]
    fn a_range_is_not_a_rest() {
        assert_eq!(scan("let w = Wall { ids: 0..3, b };", "Wall"), vec![1]);
    }

    #[test]
    fn declarations_return_types_and_variants_are_not_literals() {
        let src = "pub struct Wall {\n a: u8,\n}\nimpl Wall {\n}\nimpl From<u8> for Wall {\n}\nfn f() -> Wall {\n}\nfn g() -> doc::Wall {\n}\nlet k = Kind::Wall { a };";
        assert!(scan(src, "Wall").is_empty());
    }

    #[test]
    fn longer_names_comments_and_strings_are_ignored() {
        let src = "let t = WallType { a };\nlet u = MyWall { a };\n// Wall { a }\n/* Wall { a } */\nlet s = \"Wall { a }\";\nlet c = '{';\nlet r = r#\"Wall { a }\"#;";
        assert!(scan(src, "Wall").is_empty());
    }

    #[test]
    fn a_lifetime_does_not_start_a_char_literal() {
        assert_eq!(scan("fn f<'a>(x: &'a u8) -> u8 { let w = Wall { a }; 0 }", "Wall"), vec![1]);
    }

    #[test]
    fn the_defining_module_and_local_namesakes_are_exempt() {
        let lit = "let w = Wall { start, end };".to_string();
        let files = vec![
            ("crates/doc/src/arch.rs".to_string(), lit.clone()),
            ("crates/ui-egui/src/x.rs".to_string(), format!("struct Wall {{ a: u8 }}\n{lit}")),
            ("crates/engine/src/y.rs".to_string(), lit.clone()),
        ];
        let v = check(&files);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].starts_with("crates/engine/src/y.rs:1:"), "{}", v[0]);
        assert!(v[0].contains("..Default::default()"), "{}", v[0]);
    }

    #[test]
    fn the_workspace_passes() {
        let root = crate::root();
        assert_eq!(run(&root), Ok(()));
    }
}
