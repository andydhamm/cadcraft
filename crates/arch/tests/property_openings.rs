//! Property / fuzz tests for architectural opening placement and plan symbols.
//!
//! Seeded random wall graphs with random (including hostile: NaN / Inf / negative)
//! openings. `validate_placement`, `place_opening`, `opening_symbol` and
//! `poché_gaps` must **never panic**, symbols must stay finite and inside the host
//! wall's poché envelope (expanded for the door swing), and poché gaps must stay
//! within `[0, wall_length]` and never overlap.
//!
//! No external dependencies: a tiny deterministic LCG drives the "random" input so
//! the tests are reproducible and the workspace builds without new crates.

use std::f64::consts::PI;

use cadcraft_arch::opening::{place_opening, validate_placement};
use cadcraft_arch::symbol::{opening_symbol, poché_gaps};
use cadcraft_arch::wall::wall_bounds;
use cadcraft_arch::{ArchModel, Wall};
use cadcraft_doc::ArchId;
use cadcraft_doc::arch::{Hinge, Opening, OpeningKind, Swing};
use cadcraft_geom::Vec2;

/// A tiny deterministic LCG so the "random" input is reproducible (no `rand` dep).
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg { state: seed.wrapping_add(0x9E3779B97F4A7C15) }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.state
    }

    /// Uniform in `[0.0, 1.0)`.
    fn f64(&mut self) -> f64 {
        (self.next() as f64) / 18446744073709551616.0
    }

    /// Uniform in `[lo, hi)`.
    fn in_range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.f64()
    }
}

/// A 10 × 8 m rectangle plus one interior wall. Returns the model and the list
/// of host-wall ids (the four exterior walls and the interior wall).
fn house_model() -> (ArchModel, Vec<ArchId>) {
    let mut model = ArchModel::default();
    let south = model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
    let east = model.push_wall(Wall::new(Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), 0.2));
    let north = model.push_wall(Wall::new(Vec2::new(0.0, 8.0), Vec2::new(10.0, 8.0), 0.2));
    let west = model.push_wall(Wall::new(Vec2::new(0.0, 8.0), Vec2::new(0.0, 0.0), 0.2));
    let interior = model.push_wall(Wall::new(Vec2::new(5.0, 0.0), Vec2::new(5.0, 8.0), 0.2));
    (model, vec![south, east, north, west, interior])
}

/// A 10 × 8 m house with a hinged door (south), a sliding door (east) and two
/// windows (north), all placed validly (non-overlapping, in range).
fn house_with_openings() -> (ArchModel, Vec<ArchId>) {
    let (mut model, hosts) = house_model();
    let south = hosts[0];
    let east = hosts[1];
    let north = hosts[2];

    let door = place_opening(&model, Opening::new(south, 2.0, 0.9, 2.1, OpeningKind::Door), false).unwrap();
    model.push_opening(door);
    let slid = place_opening(&model, Opening::new(east, 3.0, 1.0, 2.0, OpeningKind::Door), false).unwrap();
    model.push_opening(slid);
    let win1 = place_opening(&model, Opening::new(north, 2.0, 0.8, 1.5, OpeningKind::Window), false).unwrap();
    model.push_opening(win1);
    let win2 = place_opening(&model, Opening::new(north, 5.0, 0.8, 1.5, OpeningKind::Window), false).unwrap();
    model.push_opening(win2);

    (model, hosts)
}

#[test]
fn poche_gaps_are_rough_spans_centred_on_the_offset() {
    let mut model = ArchModel::default();
    let wall = model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
    let door = place_opening(&model, Opening::new(wall, 3.0, 0.9, 2.1, OpeningKind::Door), false).unwrap();
    model.push_opening(door);

    let gaps = poché_gaps(&model, wall);
    assert_eq!(gaps.len(), 1, "expected one gap, got {:?}", gaps);
    assert!((gaps[0].0 - 2.49).abs() < 1e-9, "gap start {} != 2.49", gaps[0].0);
    assert!((gaps[0].1 - 3.51).abs() < 1e-9, "gap end {} != 3.51", gaps[0].1);
}

#[test]
fn symbol_spans_exactly_the_poche_gap() {
    let mut model = ArchModel::default();
    let wall = model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
    let door = place_opening(&model, Opening::new(wall, 3.0, 0.9, 2.1, OpeningKind::Door), false).unwrap();
    model.push_opening(door);

    let gap = poché_gaps(&model, wall);
    let sym = opening_symbol(&model, &model.openings[0]).unwrap();
    let jamb_xs: Vec<f64> =
        sym.polylines.iter().filter_map(|p| if p.len() == 2 && (p[0].x - p[1].x).abs() < 1e-9 { Some(p[0].x) } else { None }).collect();
    assert_eq!(jamb_xs.len(), 2, "expected 2 jambs, got {:?}", jamb_xs);
    assert!(
        (jamb_xs.iter().copied().fold(f64::INFINITY, f64::min) - gap[0].0).abs() < 1e-9,
        "symbol start {:?} doesn't match gap {:?}",
        jamb_xs,
        gap
    );
    assert!(
        (jamb_xs.iter().copied().fold(f64::NEG_INFINITY, f64::max) - gap[0].1).abs() < 1e-9,
        "symbol end {:?} doesn't match gap {:?}",
        jamb_xs,
        gap
    );
}

#[test]
fn placement_limits_measure_the_centred_rough_span() {
    let mut model = ArchModel::default();
    let wall = model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
    for (offset, ok) in [(0.5, false), (0.8, true), (9.3, true), (9.5, false)] {
        let res = validate_placement(&model, wall, offset, 0.9, 0.0, 2.1, OpeningKind::Door, "hinged", None);
        assert_eq!(res.is_ok(), ok, "offset {offset} should {}: {:?}", if ok { "succeed" } else { "fail" }, res);
    }
}

#[test]
fn fit_slides_a_centred_opening_inside_the_wall() {
    let mut model = ArchModel::default();
    let wall = model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
    let placed = place_opening(&model, Opening::new(wall, 0.0, 0.9, 2.1, OpeningKind::Door), true).unwrap();
    assert!((placed.offset - 0.66).abs() < 1e-9, "offset {} should be 0.66", placed.offset);
    assert!((placed.width - 0.9).abs() < 1e-9, "width {} should stay 0.9", placed.width);
    assert!((placed.rough_width - 1.02).abs() < 1e-9, "rough width {} should be 1.02", placed.rough_width);
}

/// Seeded random wall graphs with random (including hostile: NaN / Inf / negative)
/// openings. `validate_placement`, `place_opening`, `opening_symbol` and
/// `poché_gaps` must never panic.
#[test]
fn random_openings_never_panic() {
    let mut rng = Lcg::new(0x42);
    let (model, hosts) = house_with_openings();
    let kinds = [OpeningKind::Door, OpeningKind::Window, OpeningKind::GarageDoor, OpeningKind::CasedOpening];
    let styles = ["hinged", "slider", "garage", "cased"];

    for _ in 0..200 {
        let host = hosts[(rng.next() as usize) % hosts.len()];
        let kind = kinds[(rng.next() as usize) % kinds.len()];
        let style = styles[(rng.next() as usize) % styles.len()];
        let offset = rng.in_range(-10.0, 30.0);
        let width = rng.in_range(-3.0, 8.0);
        let sill = rng.in_range(-3.0, 6.0);
        let height = rng.in_range(-3.0, 6.0);

        // Hostile numeric inputs must return Err, never panic.
        let _ = validate_placement(&model, host, offset, width, sill, height, kind, style, None);
        let _ = validate_placement(&model, host, f64::NAN, width, sill, height, kind, style, None);
        let _ = validate_placement(&model, host, offset, f64::INFINITY, sill, height, kind, style, None);
        let _ = validate_placement(&model, host, offset, width, f64::NAN, height, kind, style, None);
        let _ = validate_placement(&model, host, offset, width, sill, f64::INFINITY, kind, style, None);

        // `opening_symbol` on a freshly-constructed (possibly degenerate) opening.
        let o = Opening::new(host, offset, width, height, kind);
        let _ = opening_symbol(&model, &o);

        // `poché_gaps` over every host (including hosts with no openings).
        let _ = poché_gaps(&model, host);

        // `place_opening` with both fit modes.
        let _ = place_opening(&model, o, rng.next() % 2 == 0);
        let _ = place_opening(&model, Opening::new(host, f64::NAN, f64::INFINITY, f64::NAN, kind), false);
    }
}

/// Every generated symbol must be finite and lie within the host wall's poché
/// envelope (expanded by the opening width to allow the door swing arc).
#[test]
fn opening_symbols_stay_inside_the_host_wall() {
    let (model, _hosts) = house_with_openings();
    for o in &model.openings {
        let Some(sym) = opening_symbol(&model, o) else {
            continue;
        };
        let w = model.wall(o.host).unwrap();
        let wb = wall_bounds(&w);
        // The door swing arc has radius `o.width` centred on the centre line, so
        // expand the poché bounds by that amount in every direction.
        let margin = o.width;
        let min_x = wb.min.x - margin;
        let max_x = wb.max.x + margin;
        let min_y = wb.min.y - margin;
        let max_y = wb.max.y + margin;
        for poly in &sym.polylines {
            for p in poly {
                assert!(p.is_finite(), "non-finite symbol vertex at ({}, {})", p.x, p.y);
                assert!(p.x >= min_x - 1e-6 && p.x <= max_x + 1e-6, "symbol vertex x {} outside [{}..{}]", p.x, min_x, max_x);
                assert!(p.y >= min_y - 1e-6 && p.y <= max_y + 1e-6, "symbol vertex y {} outside [{}..{}]", p.y, min_y, max_y);
            }
        }
    }
}

/// Poché gaps must lie within `[0, wall_length]` and never overlap.
#[test]
fn opening_gaps_never_overlap() {
    let (model, hosts) = house_with_openings();
    for host in &hosts {
        let w = model.wall(*host).unwrap();
        let len = w.start.dist(w.end);
        let gaps = poché_gaps(&model, *host);
        for (i, (s, e)) in gaps.iter().enumerate() {
            assert!(s.is_finite() && e.is_finite(), "non-finite gap in host {}", host.0);
            assert!(*s >= -1e-9, "gap {} start {} < 0 in host {}", i, s, host.0);
            assert!(*e <= len + 1e-9, "gap {} end {} > wall length {} in host {}", i, e, len, host.0);
            assert!(s < e, "gap {} is empty or reversed in host {}", i, host.0);
            if i + 1 < gaps.len() {
                let (ns, _) = gaps[i + 1];
                assert!(*e <= ns + 1e-9, "gaps overlap in host {}: [{}..{}] vs [{}..{}]", host.0, s, e, ns, gaps[i + 1].1);
            }
        }
    }
}

/// The poché gap is symmetric around the offset: the offset is the centre of the gap.
#[test]
fn poché_gap_is_symmetric_around_offset() {
    let mut model = ArchModel::default();
    let wall_id = model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
    let o = Opening::new(wall_id, 3.0, 0.9, 2.1, OpeningKind::Door);
    let placed = place_opening(&model, o, false).unwrap();
    model.push_opening(placed);

    let gap = poché_gaps(&model, wall_id);
    assert_eq!(gap.len(), 1, "expected one gap");
    // The gap should be symmetric around the offset
    let gap_centre = (gap[0].0 + gap[0].1) / 2.0;
    assert!((gap_centre - 3.0).abs() < 1e-9, "gap centre {} != offset 3.0", gap_centre);
    // The gap should be exactly rough_width wide
    assert!((gap[0].1 - gap[0].0 - placed.rough_width).abs() < 1e-9, "gap width {} != rough_width {}", gap[0].1 - gap[0].0, placed.rough_width);
}

/// The symbol spans exactly the poché gap for all opening kinds.
#[test]
fn symbol_spans_gap_for_all_opening_kinds() {
    let kinds = [
        (OpeningKind::Door, "hinged", 0.9, 2.1),
        (OpeningKind::Window, "slider", 1.2, 1.2),
        (OpeningKind::GarageDoor, "garage", 2.4, 2.1),
        (OpeningKind::CasedOpening, "cased", 0.5, 2.0),
    ];
    for (kind, style, width, height) in kinds {
        let mut model = ArchModel::default();
        let wall_id = model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
        let mut o = Opening::new(wall_id, 3.0, width, height, kind);
        o.style = style.to_string();
        let placed = place_opening(&model, o, false).unwrap();
        model.push_opening(placed);

        let gap = poché_gaps(&model, wall_id);
        assert_eq!(gap.len(), 1, "expected one gap for {kind:?}");
        let sym = opening_symbol(&model, &model.openings[0]).unwrap();

        if kind == OpeningKind::Door {
            // For doors, check that the jambs span the gap
            let jamb_xs: Vec<f64> =
                sym.polylines.iter().filter_map(|p| if p.len() == 2 && (p[0].x - p[1].x).abs() < 1e-9 { Some(p[0].x) } else { None }).collect();
            assert_eq!(jamb_xs.len(), 2, "expected 2 jambs for {kind:?}, got {:?}", jamb_xs);
            let min_jamb = jamb_xs.iter().copied().fold(f64::INFINITY, f64::min);
            let max_jamb = jamb_xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            assert!((min_jamb - gap[0].0).abs() < 1e-9, "door jamb start {} != gap start {} for {kind:?}", min_jamb, gap[0].0);
            assert!((max_jamb - gap[0].1).abs() < 1e-9, "door jamb end {} != gap end {} for {kind:?}", max_jamb, gap[0].1);
        } else {
            // For non-door openings, the symbol should span exactly the gap
            let mut min_x = f64::INFINITY;
            let mut max_x = f64::NEG_INFINITY;
            for poly in &sym.polylines {
                for p in poly {
                    if p.x < min_x {
                        min_x = p.x;
                    }
                    if p.x > max_x {
                        max_x = p.x;
                    }
                }
            }
            assert!((min_x - gap[0].0).abs() < 1e-9, "symbol min_x {} != gap start {} for {kind:?}", min_x, gap[0].0);
            assert!((max_x - gap[0].1).abs() < 1e-9, "symbol max_x {} != gap end {} for {kind:?}", max_x, gap[0].1);
        }
    }
}

/// Placement limits use the centred rough span: offsets exactly at the boundary are accepted.
#[test]
fn placement_limits_at_exact_centred_boundaries() {
    let mut model = ArchModel::default();
    let wall_id = model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
    // For a 0.9m door (rough 1.02), min_seg = 0.15:
    // min_off = 0.15 + 0.51 = 0.66
    // max_off = 10.0 - 0.15 - 0.51 = 9.34
    let res = validate_placement(&model, wall_id, 0.66, 0.9, 0.0, 2.1, OpeningKind::Door, "hinged", None);
    assert!(res.is_ok(), "offset 0.66 should be accepted: {:?}", res);
    let res = validate_placement(&model, wall_id, 9.34, 0.9, 0.0, 2.1, OpeningKind::Door, "hinged", None);
    assert!(res.is_ok(), "offset 9.34 should be accepted: {:?}", res);
    // Just outside the boundary should be rejected
    let res = validate_placement(&model, wall_id, 0.6599, 0.9, 0.0, 2.1, OpeningKind::Door, "hinged", None);
    assert!(res.is_err(), "offset 0.6599 should be rejected: {:?}", res);
    let res = validate_placement(&model, wall_id, 9.3401, 0.9, 0.0, 2.1, OpeningKind::Door, "hinged", None);
    assert!(res.is_err(), "offset 9.3401 should be rejected: {:?}", res);
}

/// Fit slides an opening to the exact centred boundary.
#[test]
fn fit_slides_to_exact_centred_boundary() {
    let mut model = ArchModel::default();
    let wall_id = model.push_wall(Wall::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.2));
    // Opening at offset 0.0 should be slid to 0.66
    let o = Opening::new(wall_id, 0.0, 0.9, 2.1, OpeningKind::Door);
    let placed = place_opening(&model, o, true).unwrap();
    assert!((placed.offset - 0.66).abs() < 1e-9, "offset {} should be 0.66", placed.offset);
    // Opening at offset 10.0 should be slid to 9.34
    let o = Opening::new(wall_id, 10.0, 0.9, 2.1, OpeningKind::Door);
    let placed = place_opening(&model, o, true).unwrap();
    assert!((placed.offset - 9.34).abs() < 1e-9, "offset {} should be 9.34", placed.offset);
}
