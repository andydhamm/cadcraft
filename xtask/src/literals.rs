//! Shared-struct literal check: `cargo xtask literals`.
//!
//! The arch3d swarm builds in parallel segments. When one segment builds a
//! shared struct with an exhaustive literal (`Wall { start, end, .. every field }`)
//! and another segment adds a field, the first stops compiling after the merge
//! (`error[E0063]: missing field ...`). Outside its defining file, a guarded type
//! must be built with `Type { .., ..Default::default() }` or its constructor,
//! and matched with a `..` rest pattern, so adding a field breaks nothing.
//!
//! This is a source scan, not a parser: comments and string literals are
//! blanked, then every `Type {` that is not a declaration, `impl` header or
//! return type is checked for a top-level `..` after `{` or `,`.

use std::path::{Path, PathBuf};

/// A type whose literals are checked outside its defining file.
pub struct Guard {
    /// Type name as written in a literal.
    pub name: &'static str,
    /// Workspace-relative file that defines the type (literals there are allowed).
    pub defined_in: &'static str,
    /// How to build it instead.
    pub fix: &'static str,
}

/// The guarded types: shared structs that more than one segment builds.
pub const GUARDED: &[Guard] = &[
    Guard { name: "Wall", defined_in: "crates/doc/src/arch.rs", fix: "Wall::new(start, end, thickness) or Wall { .., ..Default::default() }" },
    Guard { name: "Level", defined_in: "crates/doc/src/arch.rs", fix: "Level { .., ..Default::default() }" },
    Guard {
        name: "Opening",
        defined_in: "crates/doc/src/arch.rs",
        fix: "Opening::new(host, offset, width, height, kind) or Opening { .., ..Default::default() }",
    },
    Guard { name: "Room", defined_in: "crates/doc/src/arch.rs", fix: "Room { .., ..Default::default() }" },
    Guard { name: "ArchModel", defined_in: "crates/doc/src/arch.rs", fix: "ArchModel { .., ..Default::default() }" },
    Guard { name: "WallType", defined_in: "crates/doc/src/arch.rs", fix: "WallType { .., ..Default::default() }" },
    Guard { name: "WallLayer", defined_in: "crates/doc/src/arch.rs", fix: "WallLayer { .., ..Default::default() }" },
    Guard { name: "Scene3d", defined_in: "crates/arch3d/src/lib.rs", fix: "Scene3d { .., ..Default::default() }" },
    Guard { name: "Mesh", defined_in: "crates/arch3d/src/mesh.rs", fix: "Mesh::new(material, object) or Mesh { .., ..Mesh::new(material, object) }" },
    Guard { name: "WallShape", defined_in: "crates/arch/src/join.rs", fix: "WallShape { .., ..Default::default() }" },
];

/// One exhaustive literal of a guarded type outside its defining file.
#[derive(Debug, PartialEq, Eq)]
pub struct Violation {
    pub file: String,
    pub line: usize,
    pub name: &'static str,
    pub fix: &'static str,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: exhaustive `{} {{ .. }}` literal outside its defining file; use {} \
             (or a `..` rest pattern when matching) so a new field does not break this code",
            self.file, self.line, self.name, self.fix
        )
    }
}

/// Blank comments, string and char literals (newlines kept, so line numbers hold).
pub fn strip(src: &str) -> Vec<char> {
    let s: Vec<char> = src.chars().collect();
    let mut out = s.clone();
    let blank = |out: &mut Vec<char>, from: usize, to: usize| {
        for c in out.iter_mut().take(to).skip(from) {
            if *c != '\n' {
                *c = ' ';
            }
        }
    };
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        let next = s.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            let end = s[i..].iter().position(|&c| c == '\n').map_or(s.len(), |p| i + p);
            blank(&mut out, i, end);
            i = end;
        } else if c == '/' && next == Some('*') {
            let (mut j, mut depth) = (i + 2, 1);
            while j < s.len() && depth > 0 {
                if s[j] == '/' && s.get(j + 1) == Some(&'*') {
                    depth += 1;
                    j += 2;
                } else if s[j] == '*' && s.get(j + 1) == Some(&'/') {
                    depth -= 1;
                    j += 2;
                } else {
                    j += 1;
                }
            }
            blank(&mut out, i, j);
            i = j;
        } else if c == 'r' && (i == 0 || !ident(s[i - 1]) || (s[i - 1] == 'b' && (i < 2 || !ident(s[i - 2])))) && matches!(next, Some('"' | '#')) {
            // Raw string r"..." / r#"..."# (also br"...").
            let mut j = i + 1;
            let mut hashes = 0;
            while s.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if s.get(j) != Some(&'"') {
                i += 1;
                continue;
            }
            j += 1;
            loop {
                match s.get(j) {
                    None => break,
                    Some('"') if (1..=hashes).all(|k| s.get(j + k) == Some(&'#')) => {
                        j += 1 + hashes;
                        break;
                    }
                    _ => j += 1,
                }
            }
            blank(&mut out, i, j);
            i = j;
        } else if c == '"' {
            let mut j = i + 1;
            while j < s.len() && s[j] != '"' {
                j += if s[j] == '\\' { 2 } else { 1 };
            }
            let end = (j + 1).min(s.len());
            blank(&mut out, i, end);
            i = end;
        } else if c == '\'' {
            // Char literal ('x', '\n', '\u{..}') or lifetime ('a).
            if next == Some('\\') {
                let end = s[i + 2..].iter().position(|&c| c == '\'').map_or(s.len(), |p| i + 2 + p + 1);
                blank(&mut out, i, end);
                i = end;
            } else if s.get(i + 2) == Some(&'\'') {
                blank(&mut out, i, i + 3);
                i += 3;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    out
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Index of the last non-whitespace char before `i`.
fn prev_non_ws(s: &[char], i: usize) -> Option<usize> {
    s[..i].iter().rposition(|c| !c.is_whitespace())
}

/// The identifier ending at `end` (inclusive): `(start, text)`.
fn word_ending_at(s: &[char], end: usize) -> (usize, String) {
    let mut start = end + 1;
    while start > 0 && is_ident(s[start - 1]) {
        start -= 1;
    }
    (start, s[start..=end].iter().collect())
}

/// Whether `Name {` at `name_start` is a declaration, `impl` header or type
/// position rather than a literal or pattern.
fn is_type_position(s: &[char], name_start: usize) -> bool {
    // Walk back over the path prefix `a::b::`.
    let mut start = name_start;
    loop {
        let Some(p) = prev_non_ws(s, start) else { return false };
        if p >= 1 && s[p] == ':' && s[p - 1] == ':' {
            let Some(q) = prev_non_ws(s, p - 1) else { return false };
            if !is_ident(s[q]) {
                return false;
            }
            let (ws, word) = word_ending_at(s, q);
            // `Enum::Variant {` or `Self::Variant {`: not our struct.
            if word.chars().next().is_some_and(char::is_uppercase) {
                return true;
            }
            start = ws;
        } else {
            break;
        }
    }
    let Some(mut p) = prev_non_ws(s, start) else { return false };
    // `-> &'a mut Name {`: skip reference sigils back to the arrow.
    loop {
        if s[p] == '&' {
            match prev_non_ws(s, p) {
                Some(q) => p = q,
                None => return false,
            }
        } else if is_ident(s[p]) {
            let (ws, word) = word_ending_at(s, p);
            let lifetime = ws > 0 && s[ws - 1] == '\'';
            if lifetime || word == "mut" {
                match prev_non_ws(s, if lifetime { ws - 1 } else { ws }) {
                    Some(q) => p = q,
                    None => return false,
                }
            } else {
                return matches!(word.as_str(), "struct" | "enum" | "union" | "trait" | "impl" | "for" | "type" | "dyn" | "mod");
            }
        } else if s[p] == '>' {
            // `-> Name {` and `impl<T> Name {` are types; `=> Name {` is a literal.
            return !(p >= 1 && s[p - 1] == '=');
        } else {
            return false;
        }
    }
}

/// Whether the braces opening at `open` contain a top-level `..` rest/base
/// (one that follows `{` or `,`, so a `0..n` field value does not count).
/// Returns `None` when the braces are unbalanced.
fn has_rest(s: &[char], open: usize) -> Option<bool> {
    let mut depth = 0usize;
    let mut found = false;
    let mut j = open;
    while j < s.len() {
        match s[j] {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(found);
                }
            }
            '.' if depth == 1 && s.get(j + 1) == Some(&'.') => {
                if prev_non_ws(s, j).is_some_and(|p| s[p] == ',' || s[p] == '{') {
                    found = true;
                }
                j += 1;
            }
            _ => {}
        }
        j += 1;
    }
    None
}

/// Whether position `i` sits directly in an `enum { .. }` body (a variant
/// declaration such as `enum Tool { Wall { start: Vec2 } }`).
fn is_variant_decl(s: &[char], i: usize) -> bool {
    let text: String = s.iter().collect();
    // Char offsets: `strip` output may hold multi-byte chars, so map byte -> char index.
    let char_at: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
    for (b, _) in text.match_indices("enum") {
        let Ok(k) = char_at.binary_search(&b) else { continue };
        let before = k == 0 || !is_ident(s[k - 1]);
        let after = s.get(k + 4).is_some_and(|c| c.is_whitespace());
        if !before || !after || k > i {
            continue;
        }
        let Some(open) = s[k..].iter().position(|&c| c == '{' || c == ';').map(|p| k + p).filter(|&o| s[o] == '{') else { continue };
        if open >= i {
            continue;
        }
        let mut depth = 0i64;
        for &c in &s[open..i] {
            match c {
                '{' | '(' | '[' => depth += 1,
                '}' | ')' | ']' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                break; // left the enum body
            }
        }
        if depth == 1 {
            return true;
        }
    }
    false
}

/// Whether the (stripped) file declares its own item named `name`.
fn declares(s: &str, name: &str) -> bool {
    ["struct", "enum", "type", "union", "trait"].iter().any(|kw| {
        s.match_indices(kw).any(|(i, _)| {
            let before = s[..i].chars().next_back().is_none_or(|c| !is_ident(c));
            let rest = &s[i + kw.len()..];
            before && rest.starts_with(char::is_whitespace) && rest.trim_start().strip_prefix(name).is_some_and(|r| !r.starts_with(is_ident))
        })
    })
}

/// Line numbers of exhaustive `name { .. }` literals/patterns in `src`.
pub fn exhaustive_literals(src: &str, name: &str) -> Vec<usize> {
    let s = strip(src);
    if declares(&s.iter().collect::<String>(), name) {
        return Vec::new();
    }
    let n: Vec<char> = name.chars().collect();
    let mut lines = Vec::new();
    let mut i = 0;
    while i + n.len() <= s.len() {
        let hit = s[i..i + n.len()] == n[..] && (i == 0 || !is_ident(s[i - 1])) && s.get(i + n.len()).is_none_or(|&c| !is_ident(c));
        if !hit {
            i += 1;
            continue;
        }
        let after = i + n.len();
        let open = s[after..].iter().position(|c| !c.is_whitespace()).map(|p| after + p);
        if let Some(open) = open.filter(|&o| s[o] == '{')
            && !is_type_position(&s, i)
            && !is_variant_decl(&s, i)
            && has_rest(&s, open) == Some(false)
        {
            lines.push(s[..i].iter().filter(|&&c| c == '\n').count() + 1);
        }
        i = after;
    }
    lines
}

/// Check one file (workspace-relative path, with `/` separators).
pub fn check_file(rel: &str, src: &str, guards: &[Guard]) -> Vec<Violation> {
    let mut out = Vec::new();
    for g in guards.iter().filter(|g| g.defined_in != rel) {
        for line in exhaustive_literals(src, g.name) {
            out.push(Violation { file: rel.to_string(), line, name: g.name, fix: g.fix });
        }
    }
    out
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') || name == "target" {
            continue;
        }
        if p.is_dir() {
            rust_files(&p, out);
        } else if name.ends_with(".rs") {
            out.push(p);
        }
    }
}

/// Scan the workspace sources (`crates/`, `apps/`, `tests/`, `examples/`).
pub fn scan(root: &Path, guards: &[Guard]) -> Vec<Violation> {
    let mut files = Vec::new();
    for d in ["crates", "apps", "tests", "examples"] {
        rust_files(&root.join(d), &mut files);
    }
    let mut out = Vec::new();
    for f in files {
        let Ok(src) = std::fs::read_to_string(&f) else { continue };
        let rel = f.strip_prefix(root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
        out.extend(check_file(&rel, &src, guards));
    }
    out
}

/// `--discover`: every named-field `pub struct` declared once under
/// `crates/*/src`, with its exhaustive literals elsewhere. Candidates for
/// [`GUARDED`] are the ones built in another crate or in tests.
fn discover(root: &Path) {
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    let mut decls: Vec<(String, String)> = Vec::new();
    for f in &files {
        let rel = f.strip_prefix(root).unwrap_or(f).to_string_lossy().replace('\\', "/");
        if !rel.contains("/src/") {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(f) else { continue };
        let s: String = strip(&src).into_iter().collect();
        for (i, _) in s.match_indices("pub struct ") {
            let rest = &s[i + "pub struct ".len()..];
            let name: String = rest.chars().take_while(|&c| is_ident(c)).collect();
            let tail = rest[name.len()..].trim_start();
            let generic_end = if tail.starts_with('<') { tail.find('>').map_or(tail, |e| tail[e + 1..].trim_start()) } else { tail };
            if !name.is_empty() && generic_end.starts_with('{') {
                decls.push((name, rel.clone()));
            }
        }
    }
    let mut unique: Vec<(String, String)> = Vec::new();
    for (n, f) in &decls {
        if decls.iter().filter(|(m, _)| m == n).count() == 1 {
            unique.push((n.clone(), f.clone()));
        }
    }
    let guards: Vec<Guard> = unique
        .iter()
        .map(|(n, f)| Guard { name: Box::leak(n.clone().into_boxed_str()), defined_in: Box::leak(f.clone().into_boxed_str()), fix: "" })
        .collect();
    let v = scan(root, &guards);
    for g in &guards {
        let hits: Vec<&Violation> = v.iter().filter(|x| x.name == g.name).collect();
        if hits.is_empty() {
            continue;
        }
        let home = g.defined_in.split("/src/").next().unwrap_or("");
        let foreign: Vec<&str> = hits.iter().filter(|x| !x.file.starts_with(&format!("{home}/src/"))).map(|x| x.file.as_str()).collect();
        let guarded = if GUARDED.iter().any(|x| x.name == g.name) { " [guarded]" } else { "" };
        println!("{:<28} {:>4} literal(s), {:>3} outside its crate's src{guarded}  ({})", g.name, hits.len(), foreign.len(), g.defined_in);
    }
}

pub fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    if args.contains(&"--discover") {
        discover(root);
        return Ok(());
    }
    let v = scan(root, GUARDED);
    if v.is_empty() {
        let names: Vec<&str> = GUARDED.iter().map(|g| g.name).collect();
        println!("OK: no exhaustive literals of shared structs outside their defining files ({}).", names.join(", "));
        Ok(())
    } else {
        for x in &v {
            println!("  - {x}");
        }
        Err(format!(
            "{} exhaustive shared-struct literal(s); build them with `..Default::default()` or the constructor (docs/arch3d/DESIGN.md)",
            v.len()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: &[Guard] = &[Guard { name: "Wall", defined_in: "crates/doc/src/arch.rs", fix: "Wall::new" }];

    fn lines(src: &str) -> Vec<usize> {
        exhaustive_literals(src, "Wall")
    }

    #[test]
    fn exhaustive_literal_flagged() {
        assert_eq!(lines("fn f() {\n    let w = Wall { start, end };\n}"), vec![2]);
        assert_eq!(lines("let w = cadcraft_doc::arch::Wall {\n a: 1 };"), vec![1]);
        assert_eq!(lines("let w = vec![Wall { a: 1, b: 0..3 }];"), vec![1], "a range is not a base expression");
        assert_eq!(lines("match x { _ => Wall { a: 1 } }"), vec![1]);
        assert_eq!(lines("let v = &Wall { a: 1 };"), vec![1]);
    }

    #[test]
    fn exhaustive_pattern_flagged() {
        assert_eq!(lines("let Wall { start, end } = w;"), vec![1]);
    }

    #[test]
    fn rest_and_constructor_pass() {
        assert!(lines("let w = Wall { start, end, ..Default::default() };").is_empty());
        assert!(lines("let w = Wall { start, ..Wall::new(a, b, 0.2) };").is_empty());
        assert!(lines("let w = Wall {\n  start: f(x, y),\n  layers: vec![(a, 1.0)],\n  ..Default::default()\n};").is_empty());
        assert!(lines("let Wall { start, .. } = w;").is_empty());
        assert!(lines("let w = Wall::new(a, b, 0.2);").is_empty());
    }

    #[test]
    fn declarations_and_types_are_not_literals() {
        assert!(lines("impl Default for Wall {\n fn default() -> Self { todo() } }").is_empty());
        assert!(lines("impl Wall { }").is_empty());
        assert!(lines("impl<T> From<T> for Wall { }").is_empty());
        assert!(lines("fn mk() -> Wall { Wall::new(a, b, c) }").is_empty());
        assert!(lines("fn mk(&self) -> &'a mut Wall { &mut self.w }").is_empty());
        assert!(lines("fn mk() -> arch::Wall { g() }").is_empty());
        assert!(lines("let t = Tool::Wall { start };").is_empty(), "enum variant");
        assert!(lines("enum Tool { Line, Wall { start: Vec2 } }").is_empty(), "variant declaration");
        assert_eq!(lines("enum Tool { A }\nfn f() { let w = Wall { a }; }"), vec![2], "a literal after an enum is still a literal");
        assert!(lines("struct Wall { a: f64 }\nlet w = Wall { a: 1.0 };").is_empty(), "file-local type");
    }

    #[test]
    fn comments_and_strings_ignored() {
        assert!(lines("// let w = Wall { a: 1 };\n/* Wall { a } */ let s = \"Wall { a }\";").is_empty());
        assert!(lines("let s = r#\"Wall { a }\"#; let c = '{';").is_empty());
        assert!(lines("/// ```\n/// let w = Wall { a: 1 };\n/// ```\nfn f<'a>(x: &'a str) {}").is_empty());
        assert_eq!(lines("let c = '\"'; let w = Wall { a };"), vec![1], "a quote char literal does not open a string");
    }

    #[test]
    fn longer_names_do_not_match() {
        assert!(lines("let s = WallShape { a }; let t = MyWall { b };").is_empty());
    }

    #[test]
    fn defining_file_is_exempt() {
        let src = "let w = Wall { a: 1 };";
        assert!(check_file("crates/doc/src/arch.rs", src, G).is_empty());
        let v = check_file("crates/engine/src/x.rs", src, G);
        assert_eq!(v.len(), 1);
        let msg = v[0].to_string();
        assert!(msg.contains("crates/engine/src/x.rs:1") && msg.contains("Wall::new"), "{msg}");
    }

    #[test]
    fn guarded_types_exist_in_their_defining_files() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().map(Path::to_path_buf).unwrap_or_default();
        for g in GUARDED {
            let src = std::fs::read_to_string(root.join(g.defined_in)).unwrap_or_default();
            assert!(declares(&src, g.name), "{} is not declared in {}", g.name, g.defined_in);
        }
    }
}
