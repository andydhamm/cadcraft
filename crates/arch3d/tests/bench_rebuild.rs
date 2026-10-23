//! Rebuild benchmark: a 300-wall grid must rebuild in < 50 ms (full) and
//! < 5 ms for a single-wall dirty update, measured in release builds.
//!
//! The test is tagged `#[test]` but returns early in debug mode so CI
//! (which runs `cargo test --release`) can enforce the budget while
//! `cargo test` (debug) stays fast.
//!
//! The grid itself is checked in every build mode: a budget measured on the wrong
//! model, or in the wrong unit, passes without testing anything.

use std::time::Instant;

use cadcraft_doc::geom::Vec2;
use cadcraft_doc::{ArchId, ArchModel, Wall};

/// Build a deterministic 300-wall grid of 1 m rooms: 10 rows of 15 horizontal walls and
/// 10 rows of 15 vertical walls, so the solver sees real L, T and X junctions (not just
/// collinear chains).
fn build_300_wall_grid() -> ArchModel {
    let cols = 15u32;
    let rows = 10u32;
    let thickness = 0.15f64;
    let height = 2.7f64;
    let wall = |start: Vec2, end: Vec2, exterior: bool| Wall {
        id: ArchId(0),
        start,
        end,
        thickness,
        height,
        base_level: 0,
        to_ceiling: true,
        exterior,
        wall_type: "Default".to_string(),
        join_start: Default::default(),
        join_end: Default::default(),
        layers: Vec::new(),
    };

    let mut model = ArchModel::default();
    for r in 0..rows {
        for c in 0..cols {
            let (x, y) = (f64::from(c), f64::from(r));
            model.add_wall(wall(Vec2::new(x, y), Vec2::new(x + 1.0, y), r == 0));
            model.add_wall(wall(Vec2::new(x, y), Vec2::new(x, y + 1.0), c == 0));
        }
    }
    model
}

/// 300-wall full `build` must be < 50 ms; single-wall `build_dirty` < 5 ms.
///
/// In debug builds the test returns early (the budget only holds in release),
/// so it is safe to run in any build mode without flaking. (`cfg!(debug)` is not a
/// cfg Rust sets; `debug_assertions` is.)
#[test]
fn three_hundred_walls_rebuild_under_50ms() {
    if cfg!(debug_assertions) {
        println!("three_hundred_walls_rebuild_under_50ms: skipped in debug");
        return;
    }

    let model = build_300_wall_grid();
    assert_eq!(model.walls.len(), 300, "expected 300 walls");

    // --- full build ---
    let t0 = Instant::now();
    let scene = cadcraft_arch3d::build(&model);
    let full_ms = t0.elapsed().as_secs_f64() * 1_000.0;
    println!(
        "full build (300 walls): {:.1} ms, meshes: {}, tris: {}",
        full_ms,
        scene.meshes.len(),
        scene.meshes.iter().map(|m| m.indices.len() / 3).sum::<usize>()
    );

    assert!(full_ms < 50.0, "full build took {full_ms:.1} ms (budget 50 ms)");

    // --- single-wall dirty rebuild ---
    let first_id = scene.meshes.first().map(|m| m.object).expect("no meshes");
    let mut dirty: std::collections::BTreeSet<cadcraft_arch::ArchId> = std::collections::BTreeSet::new();
    dirty.insert(first_id);

    let t1 = Instant::now();
    let _ = cadcraft_arch3d::build_dirty(&model, &dirty, &scene);
    let dirty_ms = t1.elapsed().as_secs_f64() * 1_000.0;
    println!("build_dirty (1 wall):  {:.1} ms", dirty_ms);

    assert!(dirty_ms < 5.0, "build_dirty took {dirty_ms:.1} ms (budget 5 ms)");
}

/// The benchmark model must be what the budget claims: 300 walls forming a grid with
/// corner, tee and cross junctions, every wall solved and extruded.
#[test]
fn benchmark_grid_has_real_junctions() {
    use cadcraft_arch::join::{Join, solve_joins};
    let model = build_300_wall_grid();
    assert_eq!(model.walls.len(), 300);
    let shapes = solve_joins(&model);
    assert_eq!(shapes.len(), 300, "every grid wall must produce a poché ring");
    let joins: Vec<Join> = shapes.iter().flat_map(|s| s.joins.iter().copied()).collect();
    assert!(joins.iter().any(|j| matches!(j, Join::L(_))), "no L joins in the grid");
    assert!(joins.iter().any(|j| matches!(j, Join::T(_))), "no T joins in the grid");
    assert_eq!(cadcraft_arch3d::build(&model).meshes.len(), 300);
}

/// `elapsed` in milliseconds: 1.5 s must read as 1500, not 1.5 (the old
/// `as_millis() / 1000` turned a 50 ms budget into a 50 s one).
#[test]
fn elapsed_is_measured_in_milliseconds() {
    let d = std::time::Duration::from_millis(1500);
    assert!((d.as_secs_f64() * 1_000.0 - 1500.0).abs() < 1e-9);
}
