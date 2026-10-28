#[cfg(test)]
mod tests {
    use std::fs;

    fn readme() -> String {
        fs::read_to_string("docs/arch3d/README.md")
            .expect("README.md should exist")
    }

    fn gaps() -> String {
        fs::read_to_string("docs/gaps.md")
            .expect("gaps.md should exist")
    }

    /// The README must list all seven Phase 1 commands.
    #[test]
    fn readme_lists_all_phase1_commands() {
        let doc = readme();
        for cmd in [
            "wall",
            "arch.wall.stretch",
            "arch.inspect",
            "build3d",
            "arch.sample",
            "view3d",
            "camera",
        ] {
            assert!(doc.contains(cmd), "README missing command: {cmd}");
        }
    }

    /// The README must show the three CLI subcommands with their forms.
    #[test]
    fn readme_shows_cli_forms() {
        let doc = readme();
        assert!(doc.contains("cadcraft-cli convert"), "missing convert form");
        assert!(doc.contains("cadcraft-cli run"), "missing run form");
        assert!(doc.contains("cadcraft-cli render"), "missing render form");
        assert!(doc.contains("--camera"), "missing --camera flag");
    }

    /// The README crate map must cover all Phase 1 crates.
    #[test]
    fn readme_crate_map_complete() {
        let doc = readme();
        for crate_name in ["doc", "arch", "arch3d", "render", "io", "engine", "render3d", "ui-egui"] {
            assert!(
                doc.contains(crate_name),
                "crate map missing crate: {crate_name}"
            );
        }
    }

    /// gaps.md must mark ARCH-1 through ARCH-6 as done (Phase 1).
    #[test]
    fn gaps_marks_phase1_arch_rows_done() {
        let doc = gaps();
        for row in ["ARCH-1", "ARCH-2", "ARCH-3", "ARCH-4", "ARCH-5", "ARCH-6"] {
            let start = doc.find(row).expect("missing row");
            let row_text = &doc[start..];
            let end = row_text.find('\n').expect("row should end");
            assert!(
                row_text[..end].contains("done"),
                "{row} should be marked done"
            );
        }
    }

    /// gaps.md must mark later phases as open.
    #[test]
    fn gaps_marks_future_arch_rows_open() {
        let doc = gaps();
        for row in ["ARCH-7", "ARCH-8", "ARCH-9", "ARCH-10", "ARCH-11", "ARCH-12", "ARCH-13"] {
            let start = doc.find(row).expect("missing row");
            let row_text = &doc[start..];
            let end = row_text.find('\n').expect("row should end");
            assert!(
                row_text[..end].contains("open"),
                "{row} should be marked open"
            );
        }
    }

    /// F24 (AutoCAD 3D) must remain untouched and not be marked done.
    #[test]
    fn gaps_f24_untouched() {
        let doc = gaps();
        let start = doc.find("F24").expect("F24 row missing");
        let row_text = &doc[start..];
        let end = row_text.find('\n').expect("row should end");
        assert!(
            !row_text[..end].contains("done"),
            "F24 should not be marked done"
        );
    }
}
