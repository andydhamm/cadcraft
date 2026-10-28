//! Tests that verify the content of docs/gaps.md after p5-docs landed.

use std::fs;

fn gaps() -> String {
    fs::read_to_string("docs/gaps.md").expect("docs/gaps.md must exist")
}

#[test]
fn arch10_marked_done() {
    let content = gaps();
    // ARCH-10 row must say "done — phase 5"
    let row = content
        .lines()
        .find(|l| l.starts_with("| ARCH-10 |"))
        .expect("ARCH-10 row must exist");
    assert!(
        row.contains("done — phase 5"),
        "ARCH-10 must be marked 'done — phase 5', got: {}",
        row
    );
}

#[test]
fn arch13_phase_is_12() {
    let content = gaps();
    let row = content
        .lines()
        .find(|l| l.starts_with("| ARCH-13 |"))
        .expect("ARCH-13 row must exist");
    assert!(
        row.contains("phase 12"),
        "ARCH-13 must reference phase 12, got: {}",
        row
    );
}

#[test]
fn p5_deferrals_listed() {
    let content = gaps();
    // The phase-5 scope must list these deferrals
    let expected = [
        "winders",
        "curved/spiral",
        "ramps",
        "gooseneck",
        "open risers",
        "deck joist",
        "stair soffits",
    ];
    for term in expected {
        assert!(
            content.contains(term),
            "gaps.md must mention deferral '{}'",
            term
        );
    }
}
