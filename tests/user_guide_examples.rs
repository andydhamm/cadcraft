use std::fs;
use std::path::Path;

fn read_user_guide() -> String {
    let path = Path::new("docs/arch3d/USER-GUIDE.md");
    fs::read_to_string(path).expect("USER-GUIDE.md should exist")
}

fn extract_sh_blocks(content: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut in_block = false;
    let mut current = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```sh") {
            in_block = true;
            current.clear();
        } else if trimmed == "```" && in_block {
            in_block = false;
            blocks.push(current.trim().to_string());
        } else if in_block {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }
    }

    blocks
}

#[test]
fn user_guide_examples_parse() {
    let content = read_user_guide();
    let blocks = extract_sh_blocks(&content);

    assert!(!blocks.is_empty(), "Should have at least one sh block");

    for block in &blocks {
        let first_line = block.lines().next().expect("block should have a line");
        assert!(
            first_line.starts_with("cadcraft"),
            "Example should start with cadcraft: {}",
            first_line
        );
    }
}

#[test]
fn user_guide_has_all_phase6_commands() {
    let content = read_user_guide();

    assert!(content.contains("sun"), "Should document sun command");
    assert!(content.contains("camera"), "Should document camera command");
    assert!(content.contains("walkthrough"), "Should document walkthrough command");
    assert!(content.contains("pick3d"), "Should document pick3d command");
    assert!(content.contains("render"), "Should document render command");
}

#[test]
fn user_guide_render_flags_complete() {
    let content = read_user_guide();

    assert!(content.contains("--style"), "Should document --style flag");
    assert!(content.contains("--sun"), "Should document --sun flag");
    assert!(content.contains("--ground"), "Should document --ground flag");
    assert!(content.contains("--adapter"), "Should document --adapter flag");
    assert!(content.contains("--camera"), "Should document --camera flag");
}

#[test]
fn user_guide_out_of_scope_list() {
    let content = read_user_guide();

    assert!(content.contains("SSAO"), "Should mention SSAO as out of scope");
    assert!(content.contains("TAA"), "Should mention TAA as out of scope");
    assert!(
        content.contains("Cascaded Shadows"),
        "Should mention Cascaded Shadows as out of scope"
    );
    assert!(
        content.contains("Path Tracing"),
        "Should mention Path Tracing as out of scope"
    );
    assert!(
        content.contains("Artificial Lights"),
        "Should mention Artificial Lights as out of scope"
    );
    assert!(
        content.contains("Walkthrough Animation"),
        "Should mention Walkthrough Animation as out of scope"
    );
    assert!(
        content.contains("Gamepad"),
        "Should mention Gamepad as out of scope"
    );
}
