//! `cargo xtask literals`: shared arch/3D structs are never built with an exhaustive struct
//! literal outside the module that defines them.
//!
//! Why: the arch3d swarm builds in parallel segments. A segment that writes
//! `Scene3d { meshes, bounds, revision, openings_built, floors, ceilings }` compiles on its own,
//! then stops compiling (`E0063: missing field`) as soon as another segment merges a new field into
//! `Scene3d`. A literal that ends in `..Default::default()`, or a constructor such as `Mesh::new`,
//! keeps compiling when fields are added.
//!
//! The check is a source scan, not a type-checker. A struct is *shared* when it is a `pub` struct
//! with named fields declared under [`SHARED_ROOTS`]. Any `Name { .. }` literal of such a struct, in
//! a file other than the one that declares it, must contain a `..base` tail. The declaring file
//! (including its inline `#[cfg(test)]` modules) is exempt: the compiler already points at every
//! such site when a field is added.
//!
//! Not reported: patterns (`let Name { a, b } = x`, match arms), enum variants (`Kind::Name { .. }`),
//! `impl`/`-> Name {` headers, anything in comments or strings, and literals in a crate that declares
//! its own, different struct with the same name. To protect more structs, add their source file or
//! directory to [`SHARED_ROOTS`].

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

/// Source files or directories (workspace-relative, `/` separators) whose `pub` named-field structs
/// are shared. A directory ends with `/`.
const SHARED_ROOTS: &[&str] = &[
    "crates/arch/src/",
    "crates/arch3d/src/",
    "crates/render3d/src/",
    "crates/doc/src/arch.rs",
    "crates/render/src/arch.rs",
    "crates/io/src/dxf_arch.rs",
    "crates/io/src/glb.rs",
    "crates/engine/src/cmd/arch/",
];

/// Workspace directories scanned for literals (tests, benches and examples included).
const SCAN_DIRS: &[&str] = &["crates", "apps"];

/// One Rust source file.
pub struct Source {
    /// Workspace-relative path with `/` separators.
    pub path: String,
    pub text: String,
}

/// An exhaustive literal of a shared struct outside its defining file.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Violation {
    pub path: String,
    pub line: usize,
    pub name: String,
    /// The file that declares the struct.
    pub home: String,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: `{} {{ .. }}` spells out every field; `{}` is declared in {}", self.path, self.line, self.name, self.name, self.home)
    }
}

/// What a scan found.
pub struct Report {
    /// `(struct name, declaring file)` for every shared struct.
    pub shared: Vec<(String, String)>,
    pub violations: Vec<Violation>,
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Replace comments, string literals and char literals with spaces (newlines are kept), so a scan
/// for braces and identifiers cannot be fooled by text. Lifetimes (`'a`) are left alone.
fn blank_noncode(src: &str) -> Vec<char> {
    let c: Vec<char> = src.chars().collect();
    let n = c.len();
    let mut out = c.clone();
    let blank = |out: &mut Vec<char>, from: usize, to: usize| {
        for slot in out.iter_mut().take(to.min(n)).skip(from) {
            if *slot != '\n' {
                *slot = ' ';
            }
        }
    };
    let mut i = 0;
    while i < n {
        match c[i] {
            '/' if c.get(i + 1) == Some(&'/') => {
                let mut j = i;
                while j < n && c[j] != '\n' {
                    j += 1;
                }
                blank(&mut out, i, j);
                i = j;
            }
            '/' if c.get(i + 1) == Some(&'*') => {
                let mut depth = 1;
                let mut j = i + 2;
                while j < n && depth > 0 {
                    if c[j] == '/' && c.get(j + 1) == Some(&'*') {
                        depth += 1;
                        j += 2;
                    } else if c[j] == '*' && c.get(j + 1) == Some(&'/') {
                        depth -= 1;
                        j += 2;
                    } else {
                        j += 1;
                    }
                }
                blank(&mut out, i, j);
                i = j;
            }
            'r' if (i == 0 || !is_ident(c[i - 1]) || (c[i - 1] == 'b' && (i < 2 || !is_ident(c[i - 2])))) && raw_string_end(&c, i).is_some() => {
                let end = raw_string_end(&c, i).unwrap_or(n);
                blank(&mut out, i, end);
                i = end;
            }
            '"' => {
                let mut j = i + 1;
                while j < n && c[j] != '"' {
                    if c[j] == '\\' {
                        j += 1;
                    }
                    j += 1;
                }
                blank(&mut out, i, j + 1);
                i = j + 1;
            }
            '\'' => {
                if c.get(i + 1) == Some(&'\\') {
                    // Escaped char literal: '\n', '\'', '\u{1F600}'.
                    let mut j = i + 3;
                    while j < n && c[j] != '\'' {
                        j += 1;
                    }
                    blank(&mut out, i, j + 1);
                    i = j + 1;
                } else if i + 2 < n && c[i + 2] == '\'' && c[i + 1] != '\'' {
                    blank(&mut out, i, i + 3);
                    i += 3;
                } else {
                    i += 1; // a lifetime or loop label
                }
            }
            _ => i += 1,
        }
    }
    out
}

/// If a raw string literal (`r"…"`, `r#"…"#`) starts at `i`, the index just past its end.
fn raw_string_end(c: &[char], i: usize) -> Option<usize> {
    let mut j = i + 1;
    let mut hashes = 0;
    while c.get(j) == Some(&'#') {
        hashes += 1;
        j += 1;
    }
    if c.get(j) != Some(&'"') {
        return None;
    }
    j += 1;
    while j < c.len() {
        if c[j] == '"' && (0..hashes).all(|k| c.get(j + 1 + k) == Some(&'#')) {
            return Some(j + 1 + hashes);
        }
        j += 1;
    }
    Some(c.len())
}

fn skip_ws(c: &[char], mut i: usize) -> usize {
    while i < c.len() && c[i].is_whitespace() {
        i += 1;
    }
    i
}

/// Index of the last non-whitespace char before `i`.
fn prev_nonws(c: &[char], i: usize) -> Option<usize> {
    let mut k = i;
    while k > 0 {
        k -= 1;
        if !c[k].is_whitespace() {
            return Some(k);
        }
    }
    None
}

/// The identifier that ends at `end` (exclusive).
fn word_ending_at(c: &[char], end: usize) -> String {
    let mut s = end;
    while s > 0 && is_ident(c[s - 1]) {
        s -= 1;
    }
    c[s..end].iter().collect()
}

/// `(start, end)` of every identifier-like word, in order.
fn words(c: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        if is_ident(c[i]) {
            let s = i;
            while i < c.len() && is_ident(c[i]) {
                i += 1;
            }
            out.push((s, i));
        } else {
            i += 1;
        }
    }
    out
}

/// Index of the `}` matching the `{` at `open`.
fn matching_brace(c: &[char], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (k, ch) in c.iter().enumerate().skip(open) {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether the text before `start` is a `pub` / `pub(crate)` / `pub(in path)` visibility.
fn has_pub_before(c: &[char], start: usize) -> bool {
    let Some(mut p) = prev_nonws(c, start) else {
        return false;
    };
    if c[p] == ')' {
        let mut depth = 0usize;
        loop {
            match c[p] {
                ')' => depth += 1,
                '(' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            if p == 0 {
                return false;
            }
            p -= 1;
        }
        match prev_nonws(c, p) {
            Some(q) => p = q,
            None => return false,
        }
    }
    is_ident(c[p]) && word_ending_at(c, p + 1) == "pub"
}

/// `(name, is_pub)` of every struct declared with named fields (`struct Name { .. }`).
fn named_structs(c: &[char]) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    for (s, e) in words(c) {
        if c[s..e].iter().collect::<String>() != "struct" {
            continue;
        }
        let ns = skip_ws(c, e);
        let mut ne = ns;
        while ne < c.len() && is_ident(c[ne]) {
            ne += 1;
        }
        if ne == ns {
            continue;
        }
        let name: String = c[ns..ne].iter().collect();
        let mut k = skip_ws(c, ne);
        if c.get(k) == Some(&'<') {
            let mut depth = 0usize;
            while k < c.len() {
                match c[k] {
                    '<' => depth += 1,
                    '>' if k > 0 && c[k - 1] != '-' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            k += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                k += 1;
            }
            k = skip_ws(c, k);
        }
        // A `where` clause runs up to the body.
        while k < c.len() && !matches!(c[k], '{' | ';' | '(') {
            k += 1;
        }
        if c.get(k) == Some(&'{') {
            out.push((name, has_pub_before(c, s)));
        }
    }
    out
}

/// Whether a struct literal body ends in a `..base` tail (at the literal's own nesting level).
fn has_rest(body: &[char]) -> bool {
    let mut depth = 0i32;
    let mut i = 0;
    while i < body.len() {
        match body[i] {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '.' if depth == 0 && body.get(i + 1) == Some(&'.') && body.get(i + 2) != Some(&'.') => {
                let before = prev_nonws(body, i);
                if before.is_none_or(|p| body[p] == ',') {
                    return true;
                }
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    false
}

fn line_of(c: &[char], idx: usize) -> usize {
    1 + c.iter().take(idx).filter(|&&ch| ch == '\n').count()
}

/// Words that, right before `Name {`, mean it is a declaration or header rather than a literal.
const NON_LITERAL_BEFORE: &[&str] = &["struct", "enum", "impl", "trait", "for", "mod", "use", "fn", "type", "union", "dyn", "where", "as", "let"];

/// Whether the `Name` at `s`, which is followed by `{`, is an enum variant (`Kind::Name {`,
/// `Self::Name {`) or the end of a header (`impl Name {`, `-> path::Name {`) rather than a literal.
fn not_a_struct_literal(c: &[char], s: usize) -> bool {
    // Walk back over `a::b::` path segments; an uppercase segment is an enum or an associated item.
    let mut start = s;
    while let Some(p) = prev_nonws(c, start) {
        if !(c[p] == ':' && p >= 1 && c[p - 1] == ':') {
            break;
        }
        let Some(q) = prev_nonws(c, p - 1).filter(|&q| is_ident(c[q])) else {
            break;
        };
        let mut seg = q + 1;
        while seg > 0 && is_ident(c[seg - 1]) {
            seg -= 1;
        }
        if c[seg].is_uppercase() {
            return true;
        }
        start = seg;
    }
    match prev_nonws(c, start) {
        Some(p) if c[p] == '>' && p >= 1 && c[p - 1] == '-' => true,
        Some(p) if is_ident(c[p]) => NON_LITERAL_BEFORE.contains(&word_ending_at(c, p + 1).as_str()),
        _ => false,
    }
}

/// Literals `Name { .. }` of any struct in `names`: `(name, line, exhaustive)`.
fn literal_sites(c: &[char], names: &HashSet<&str>) -> Vec<(String, usize, bool)> {
    let mut out = Vec::new();
    for (s, e) in words(c) {
        let word: String = c[s..e].iter().collect();
        if !names.contains(word.as_str()) {
            continue;
        }
        let open = skip_ws(c, e);
        if c.get(open) != Some(&'{') {
            continue;
        }
        if not_a_struct_literal(c, s) {
            continue;
        }
        let Some(close) = matching_brace(c, open) else {
            continue;
        };
        // A pattern: `Name { a, b } = x`, `Some(Name { a }) =>`, `|Name { a }|`, `(Name { a }: Name)`.
        let mut k = close + 1;
        loop {
            k = skip_ws(c, k);
            if matches!(c.get(k), Some(')' | ']')) {
                k += 1;
            } else {
                break;
            }
        }
        let is_pattern = match c.get(k) {
            Some('=') => c.get(k + 1) != Some(&'='),
            Some('|') => c.get(k + 1) != Some(&'|'),
            Some(':') => c.get(k + 1) != Some(&':'),
            Some('@') => true,
            Some('i') => c.get(k + 1) == Some(&'f') && !c.get(k + 2).is_some_and(|&ch| is_ident(ch)),
            _ => false,
        };
        if is_pattern {
            continue;
        }
        out.push((word, line_of(c, s), !has_rest(&c[open + 1..close])));
    }
    out
}

/// `crates/foo` / `apps/foo` for a path under them, else the first path component.
fn crate_of(path: &str) -> String {
    let mut parts = path.split('/');
    match (parts.next(), parts.next()) {
        (Some(a @ ("crates" | "apps")), Some(b)) => format!("{a}/{b}"),
        (Some(a), _) => a.to_string(),
        _ => String::new(),
    }
}

/// Scan `sources` and report exhaustive literals of the structs declared under `roots`.
pub fn check(sources: &[Source], roots: &[&str]) -> Report {
    let code: Vec<(&str, Vec<char>)> = sources.iter().map(|s| (s.path.as_str(), blank_noncode(&s.text))).collect();

    // Every named-field struct, by crate, so a same-named struct elsewhere is not mistaken for ours.
    let mut declared: HashMap<(String, String), BTreeSet<&str>> = HashMap::new();
    let mut shared: Vec<(String, String)> = Vec::new();
    for (path, c) in &code {
        let in_root = roots.iter().any(|r| path.starts_with(r));
        for (name, is_pub) in named_structs(c) {
            declared.entry((crate_of(path), name.clone())).or_default().insert(path);
            if in_root && is_pub && !shared.iter().any(|(n, h)| *n == name && h.as_str() == *path) {
                shared.push((name, (*path).to_string()));
            }
        }
    }
    shared.sort();

    let names: HashSet<&str> = shared.iter().map(|(n, _)| n.as_str()).collect();
    let mut violations = BTreeSet::new();
    for (path, c) in &code {
        for (name, line, exhaustive) in literal_sites(c, &names) {
            if !exhaustive {
                continue;
            }
            let homes: Vec<&str> = shared.iter().filter(|(n, _)| *n == name).map(|(_, h)| h.as_str()).collect();
            if homes.contains(path) {
                continue; // the declaring file
            }
            // This crate declares its own struct of that name (not one of ours): not a literal of ours.
            let own = declared.get(&(crate_of(path), name.clone()));
            if own.is_some_and(|files| files.iter().any(|f| !homes.contains(f))) {
                continue;
            }
            let Some(home) = homes.first() else {
                continue;
            };
            violations.insert(Violation { path: (*path).to_string(), line, name, home: (*home).to_string() });
        }
    }
    Report { shared, violations: violations.into_iter().collect() }
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<Source>) -> Result<(), String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()), // a scan dir that does not exist (partial checkout)
    };
    let mut paths: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if p.is_dir() {
            if name != "target" && !name.starts_with('.') {
                collect(&p, root, out)?;
            }
        } else if name.ends_with(".rs") {
            let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            let rel =
                p.strip_prefix(root).unwrap_or(&p).components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/");
            out.push(Source { path: rel, text });
        }
    }
    Ok(())
}

pub fn run(root: &Path) -> Result<(), String> {
    let mut sources = Vec::new();
    for d in SCAN_DIRS {
        collect(&root.join(d), root, &mut sources)?;
    }
    let report = check(&sources, SHARED_ROOTS);
    if report.violations.is_empty() {
        println!(
            "literals: {} shared structs, {} files scanned, no exhaustive literals outside the declaring file",
            report.shared.len(),
            sources.len()
        );
        return Ok(());
    }
    println!("{} exhaustive literal(s) of shared structs:\n", report.violations.len());
    for v in &report.violations {
        println!("  {v}");
    }
    println!(
        "\nA literal that names every field stops compiling when another segment adds a field to the struct.\n\
         Write `Type {{ required_field, ..Default::default() }}` (derive or implement `Default`) or call a constructor such as\n\
         `Type::new(..)`; if the struct has neither, add one in the file that declares it."
    );
    Err(format!("{} exhaustive literal(s) of shared structs", report.violations.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOTS: &[&str] = &["crates/arch3d/src/"];
    const HOME: &str = "crates/arch3d/src/lib.rs";

    fn src(path: &str, text: &str) -> Source {
        Source { path: path.into(), text: text.into() }
    }

    /// The declaring file, then `other` (a file in `path`), checked together.
    fn violations_in(path: &str, other: &str) -> Vec<Violation> {
        let home =
            "pub struct Scene3d { pub meshes: Vec<u32>, pub floors: usize }\nfn build() -> Scene3d { Scene3d { meshes: vec![], floors: 0 } }\n";
        check(&[src(HOME, home), src(path, other)], ROOTS).violations
    }

    #[test]
    fn flags_an_exhaustive_literal_outside_the_declaring_file() {
        let v = violations_in(
            "crates/render3d/src/pipeline.rs",
            "fn t() {\n    let s = Scene3d {\n        meshes: vec![],\n        floors: 0,\n    };\n}\n",
        );
        assert_eq!(v, vec![Violation { path: "crates/render3d/src/pipeline.rs".into(), line: 2, name: "Scene3d".into(), home: HOME.into() }]);
        let msg = v[0].to_string();
        assert!(msg.contains("crates/render3d/src/pipeline.rs:2") && msg.contains("Scene3d") && msg.contains(HOME), "{msg}");
    }

    #[test]
    fn flags_a_path_qualified_literal_and_a_literal_in_a_tests_dir() {
        assert_eq!(
            violations_in("crates/arch3d/tests/golden.rs", "fn t() { let _ = cadcraft_arch3d::Scene3d { meshes: vec![], floors: 1 }; }").len(),
            1
        );
    }

    #[test]
    fn the_declaring_file_may_build_its_own_struct_exhaustively() {
        let home = "pub struct Scene3d { pub a: u32 }\nfn f() -> Scene3d { Scene3d { a: 1 } }\n#[cfg(test)]\nmod tests { fn g() { let _ = super::Scene3d { a: 2 }; } }\n";
        assert!(check(&[src(HOME, home)], ROOTS).violations.is_empty());
    }

    #[test]
    fn accepts_a_struct_update_tail_and_constructors() {
        for ok in [
            "fn t() { let _ = Scene3d { meshes: vec![], ..Default::default() }; }",
            "fn t() { let _ = Scene3d { floors: 1, ..Scene3d::new() }; }",
            "fn t() { let _ = Scene3d { ..Default::default() }; }",
            "fn t() { let _ = Scene3d::new(); let _ = Scene3d::default(); }",
            "fn t() { let _ = Scene3d { meshes: Wrapper { a: 1, b: 2 }.into(), ..Default::default() }; }",
        ] {
            assert!(violations_in("crates/io/src/glb.rs", ok).is_empty(), "{ok}");
        }
    }

    #[test]
    fn a_tail_inside_a_nested_literal_does_not_count() {
        let v = violations_in("crates/io/src/glb.rs", "fn t() { let _ = Scene3d { meshes: Other { a: 1, ..Default::default() }, floors: 0 }; }");
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn ignores_patterns_variants_headers_and_types() {
        for ok in [
            "fn t(s: Scene3d) { let Scene3d { meshes, floors } = s; }",
            "fn t(s: Scene3d) { match s { Scene3d { meshes, floors } => {} } }",
            "fn t(s: Option<Scene3d>) { if let Some(Scene3d { meshes, floors }) = s {} }",
            "fn t(Scene3d { meshes, floors }: Scene3d) {}",
            "fn t(s: Scene3d) { match s { Scene3d { floors: 0, meshes } if true => {} _ => {} } }",
            "fn t() { let _ = Shape::Scene3d { meshes: 1, floors: 2 }; let _ = Self::Scene3d { a: 1 }; }",
            "impl Scene3d { fn x() {} }\nimpl Clone for Scene3d { fn clone(&self) -> Self { todo!() } }",
            "fn build() -> Scene3d { todo!() }",
            "fn build(m: &M) -> cadcraft_arch3d::Scene3d { todo!() }",
            "impl cadcraft_arch3d::Scene3d { fn x() {} }\nimpl Clone for cadcraft_arch3d::Scene3d { fn clone(&self) -> Self { todo!() } }",
            "fn f(a: &Scene3d) {}",
        ] {
            assert!(violations_in("crates/io/src/glb.rs", ok).is_empty(), "{ok}");
        }
    }

    #[test]
    fn ignores_comments_strings_and_chars() {
        for ok in [
            "// let _ = Scene3d { meshes: vec![], floors: 0 };",
            "/* Scene3d { a: 1 } /* nested */ Scene3d { a: 1 } */ fn t() {}",
            "fn t() { let _ = \"Scene3d { a: 1 }\"; let _ = r#\"Scene3d { a: \"1\" }\"#; }",
            "/// Scene3d { a: 1 }\nfn t() { let _ = '{'; let _ = '\\''; }",
        ] {
            assert!(violations_in("crates/io/src/glb.rs", ok).is_empty(), "{ok}");
        }
    }

    #[test]
    fn only_public_named_structs_under_the_shared_roots_are_protected() {
        let home = "struct Hidden { a: u32 }\npub struct Tuple(u32);\npub struct Unit;\npub(crate) struct Crate { a: u32 }\npub struct G<T: Clone> where T: Copy { pub a: T }\n";
        let free = "pub struct Free { pub a: u32 }\n";
        let user = "fn t() { let _ = Hidden { a: 1 }; let _ = Free { a: 1 }; let _ = Crate { a: 1 }; let _ = G { a: 1 }; }";
        let r = check(&[src(HOME, home), src("crates/engine/src/free.rs", free), src("crates/io/src/user.rs", user)], ROOTS);
        let flagged: Vec<&str> = r.violations.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(flagged, vec!["Crate", "G"]);
        let shared: Vec<&str> = r.shared.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(shared, vec!["Crate", "G"]);
    }

    #[test]
    fn a_same_named_struct_in_another_crate_is_not_ours() {
        let theirs = "pub struct Scene3d { pub x: u32 }\nfn t() { let _ = Scene3d { x: 1 }; }";
        let r = check(
            &[
                src(HOME, "pub struct Scene3d { pub a: u32 }"),
                src("crates/ui-egui/src/scene.rs", theirs),
                src("crates/ui-egui/src/b.rs", "fn t() { let _ = Scene3d { x: 1 }; }"),
            ],
            ROOTS,
        );
        assert!(r.violations.is_empty(), "{:?}", r.violations);
    }

    #[test]
    fn has_rest_only_counts_a_tail_at_the_literals_own_level() {
        let chars = |s: &str| s.chars().collect::<Vec<_>>();
        assert!(has_rest(&chars("a: 1, ..base")));
        assert!(has_rest(&chars(" ..Default::default() ")));
        assert!(!has_rest(&chars("a: 0..5")));
        assert!(!has_rest(&chars("a: S { b: 1, ..d }")));
        assert!(!has_rest(&chars("a: f(x, ..y)")));
        assert!(!has_rest(&chars("a: 1, b: 2")));
    }

    #[test]
    fn blanking_keeps_offsets_and_lifetimes() {
        let src = "fn f<'a>(x: &'a str) { let c = '{'; let s = \"}\"; } // {\n";
        let blanked: String = blank_noncode(src).into_iter().collect();
        assert_eq!(blanked.chars().count(), src.chars().count());
        assert_eq!(blanked.matches('{').count(), 1);
        assert_eq!(blanked.matches('}').count(), 1);
        assert!(blanked.contains("<'a>") && blanked.contains("&'a str"));
    }

    #[test]
    fn crate_of_groups_by_crate_directory() {
        assert_eq!(crate_of("crates/arch3d/src/lib.rs"), "crates/arch3d");
        assert_eq!(crate_of("apps/cadcraft-cli/tests/x.rs"), "apps/cadcraft-cli");
        assert_eq!(crate_of("xtask/src/main.rs"), "xtask");
    }
}
