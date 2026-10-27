//! Rebuild benchmark: a 300-wall grid must rebuild in < 50 ms (full) and
//! < 5 ms for a single-wall dirty update, measured in release builds.
//!
//! The test is tagged `#[test]` but returns early in debug mode so CI
//! (which runs `cargo test --release`) can enforce the budget while
//! `cargo test` (debug) stays fast.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use cadcraft_arch::join::{Join, solve_joins};
use cadcraft_doc::arch::Opening;
use cadcraft_doc::geom::Vec2;
use cadcraft_doc::{ArchId, ArchModel, Wall};

/// Build a deterministic 300-wall grid: 10 rows x 15 columns.
/// Horizontal walls: 10 * 15 = 150.
/// Vertical walls: 10 * 15 = 150.
/// Total: 300 walls.
fn build_300_wall_grid() -> ArchModel {
    let cols = 15u32;
    let rows = 10u32;
    let cell_w = 1.0;
    let cell_h = 1.0;
    let thickness = 0.15f64;
    let height = 2.7f64;

    let mut model = ArchModel::default();

    // Horizontal walls: 10 rows, 15 columns each
    for r in 0..rows {
        for c in 0..cols {
            let x0 = (c as f64) * cell_w;
            let x1 = (c + 1) as f64 * cell_w;
            let y = (r as f64) * cell_h;
            model.add_wall(Wall {
                start: Vec2::new(x0, y),
                end: Vec2::new(x1, y),
                thickness,
                height,
                to_ceiling: true,
                exterior: r == 0 || r == rows - 1 || c == 0 || c == cols - 1,
                wall_type: "Default".to_string(),
                ..Default::default()
            });
        }
    }

    // Vertical walls: 10 rows, 15 columns each
    for r in 0..rows {
        for c in 0..cols {
            let x = (c as f64) * cell_w;
            let y0 = (r as f64) * cell_h;
            let y1 = (r + 1) as f64 * cell_h;
            model.add_wall(Wall {
                start: Vec2::new(x, y0),
                end: Vec2::new(x, y1),
                thickness,
                height,
                to_ceiling: true,
                exterior: r == 0 || r == rows - 1 || c == 0 || c == cols - 1,
                wall_type: "Default".to_string(),
                ..Default::default()
            });
        }
    }

    model
}

#[test]
fn elapsed_is_measured_in_milliseconds() {
    let d = Duration::from_millis(1500);
    let ms = d.as_secs_f64() * 1000.0;
    assert_eq!(ms, 1500.0);
}

#[test]
fn benchmark_grid_has_real_junctions() {
    let model = build_300_wall_grid();
    assert_eq!(model.walls.len(), 300, "expected 300 walls");

    let joins = solve_joins(&model);
    assert_eq!(joins.len(), 300, "expected 300 join shapes");

    let has_l = joins.iter().any(|j| j.joins.iter().any(|k| matches!(k, Join::L(_))));
    let has_t = joins.iter().any(|j| j.joins.iter().any(|k| matches!(k, Join::T(_))));

    assert!(has_l, "expected at least one L join");
    assert!(has_t, "expected at least one T join");

    let scene = cadcraft_arch3d::build(&model);
    assert_eq!(scene.meshes.len(), 300, "expected 300 meshes");
}

/// 300-wall full `build` must be < 50 ms; single-wall `build_dirty` < 5 ms.
///
/// In debug builds the test returns early (the budget only holds in release),
/// so it is safe to run in any build mode without flaking.
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
    let full_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!(
        "full build (300 walls): {:.1} ms, meshes: {}, tris: {}",
        full_ms,
        scene.meshes.len(),
        scene.meshes.iter().map(|m| m.indices.len() / 3).sum::<usize>()
    );

    assert!(full_ms < 50.0, "full build took {full_ms:.1} ms (budget 50 ms)");

    // --- single-wall dirty rebuild ---
    let first_id = scene.meshes.first().map(|m| m.object).expect("no meshes");
    let mut dirty: BTreeSet<ArchId> = BTreeSet::new();
    dirty.insert(first_id);

    let t1 = Instant::now();
    let _ = cadcraft_arch3d::build_dirty(&model, &dirty, &scene);
    let dirty_ms = t1.elapsed().as_secs_f64() * 1000.0;
    println!("build_dirty (1 wall):  {:.1} ms", dirty_ms);

    assert!(dirty_ms < 5.0, "build_dirty took {dirty_ms:.1} ms (budget 5 ms)");
}

/// 300 walls + 150 openings: full build < 50 ms; one-opening dirty rebuild < 5 ms
/// and identical to a full build of the edited model.
#[test]
fn three_hundred_walls_150_openings_rebuild() {
    if cfg!(debug_assertions) {
        println!("three_hundred_walls_150_openings_rebuild: skipped in debug");
        return;
    }

    // Build 300 walls
    let mut model = build_300_wall_grid();
    assert_eq!(model.walls.len(), 300);

    // Add 150 openings, one per each of the first 150 walls
    for i in 0..150 {
        let mut open = Opening::default();
        open.id = ArchId(1000 + i);
        open.host = ArchId(i);
        open.offset = 0.5;
        open.width = 0.5;
        open.height = 1.5;
        model.openings.push(open);
    }
    assert_eq!(model.openings.len(), 150);

    // Full build
    let t0 = Instant::now();
    let scene = cadcraft_arch3d::build(&model);
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("full build (300 walls, 150 openings): {:.1} ms, meshes: {}, openings: {}", ms, scene.meshes.len(), scene.openings_built);
    assert!(ms < 50.0, "full build took {ms:.1} ms (budget 50 ms)");
    assert!(scene.meshes.len() >= 300, "expected >=300 meshes, got {}", scene.meshes.len());
    assert!(scene.openings_built >= 150, "expected >=150 openings, got {}", scene.openings_built);

    // Edit opening #0
    let open_id = model.openings[0].id;
    let mut edited = model.clone();
    edited.openings[0].width = 1.0;

    let mut dirty: BTreeSet<ArchId> = BTreeSet::new();
    dirty.insert(open_id);

    let t1 = Instant::now();
    let partial = cadcraft_arch3d::build_dirty(&edited, &dirty, &scene);
    let dirty_ms = t1.elapsed().as_secs_f64() * 1000.0;
    println!("build_dirty (1 opening): {:.1} ms", dirty_ms);
    assert!(dirty_ms < 5.0, "build_dirty took {dirty_ms:.1} ms (budget 5 ms)");

    // Compare with full build of the edited model
    let full_edited = cadcraft_arch3d::build(&edited);
    assert_eq!(partial.meshes.len(), full_edited.meshes.len());
    assert_eq!(partial.openings_built, full_edited.openings_built);
    assert_eq!(partial.bounds, full_edited.bounds);
}

#[test]
fn empty_model_builds_cleanly() {
    let model = ArchModel::default();
    let scene = cadcraft_arch3d::build(&model);
    assert_eq!(scene.meshes.len(), 0, "empty model should produce no meshes");
    assert_eq!(scene.openings_built, 0, "empty model should produce no openings");
}

#[test]
fn degenerate_walls_do_not_crash() {
    let mut model = ArchModel::default();

    // Zero-length wall (start == end)
    model.add_wall(Wall {
        start: Vec2::new(0.0, 0.0),
        end: Vec2::new(0.0, 0.0),
        thickness: 0.15,
        height: 2.7,
        to_ceiling: true,
        exterior: false,
        wall_type: "Default".to_string(),
        ..Default::default()
    });

    // Zero-thickness wall
    model.add_wall(Wall {
        start: Vec2::new(0.0, 0.0),
        end: Vec2::new(1.0, 0.0),
        thickness: 0.0,
        height: 2.7,
        to_ceiling: true,
        exterior: false,
        wall_type: "Default".to_string(),
        ..Default::default()
    });

    // Zero-height wall
    model.add_wall(Wall {
        start: Vec2::new(0.0, 1.0),
        end: Vec2::new(1.0, 1.0),
        thickness: 0.15,
        height: 0.0,
        to_ceiling: true,
        exterior: false,
        wall_type: "Default".to_string(),
        ..Default::default()
    });

    // NaN coordinates
    model.add_wall(Wall {
        start: Vec2::new(f64::NAN, 0.0),
        end: Vec2::new(1.0, f64::NAN),
        thickness: 0.15,
        height: 2.7,
        to_ceiling: true,
        exterior: false,
        wall_type: "Default".to_string(),
        ..Default::default()
    });

    // Huge coordinates
    model.add_wall(Wall {
        start: Vec2::new(-1e12, -1e12),
        end: Vec2::new(1e12, 1e12),
        thickness: 0.15,
        height: 2.7,
        to_ceiling: true,
        exterior: false,
        wall_type: "Default".to_string(),
        ..Default::default()
    });

    let scene = cadcraft_arch3d::build(&model);
    assert!(scene.meshes.len() >= 1, "at least some walls should build");
}

#[test]
fn build_dirty_empty_set_is_noop() {
    let model = build_300_wall_grid();
    let scene = cadcraft_arch3d::build(&model);

    let empty_dirty: BTreeSet<ArchId> = BTreeSet::new();
    let rebuilt = cadcraft_arch3d::build_dirty(&model, &empty_dirty, &scene);

    assert_eq!(rebuilt.meshes.len(), scene.meshes.len(), "empty dirty set should not change mesh count");
}

#[test]
fn build_dirty_unknown_id_is_noop() {
    let model = build_300_wall_grid();
    let scene = cadcraft_arch3d::build(&model);

    let mut dirty = BTreeSet::new();
    dirty.insert(ArchId(99999)); // not a real wall ID

    let rebuilt = cadcraft_arch3d::build_dirty(&model, &dirty, &scene);
    assert_eq!(rebuilt.meshes.len(), scene.meshes.len(), "unknown dirty ID should not change mesh count");
}
