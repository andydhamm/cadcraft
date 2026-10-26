//! `arch.sample` — inject a deterministic sample arch model (benchmark fixture).
//!
//! Replaces `Drawing::arch.walls` with a seeded xorshift64* wall chain (same
//! bit pattern as `apps/cadcraft-cli/src/main.rs::Rng`, seed
//! `0x9E37_79B9_7F4A_7C15`). The chain is connected: wall `i` starts where
//! wall `i-1` ended, the heading turns a right angle every `4 + (rng % 3)`
//! walls, length in [3.0, 8.9) drawing units, thickness alternating 0.2/0.1,
//! height 2.6, ids `w1..wN` (handles `arch_handle(i)`).
//!
//! The optional `openings` parameter (default `0`, capped at `MAX_OPENINGS`)
//! deterministically seeds openings on top of the chain, one per wall, using
//! two waves so no separation conflict is ever possible: wave 0 seeds the
//! even 1-based walls and wave 1 the odd ones, each candidate at 40 % of the
//! centre-line length, choosing the hinged door (0.9 m × 2.1 m) when the
//! 1-based index is a multiple of 3 and the sliding window (1.2 m × 1.2 m,
//! sill 0.9 m) otherwise. Walls shorter than 2.0 m and any placement rejected
//! by [`cadcraft_arch::opening::validate_placement`] are skipped, so the
//! command never fails. Openings are pushed through `ArchModel::push_opening`,
//! so ids continue the shared counter after the wall ids. Same seed ⇒ same
//! output.
//!
//! This command *does* mutate the model and calls `touch_arch()`; it is
//! `.noundo()` only because it is a benchmark fixture, not a user-facing edit.

use serde_json::{Value, json};

use crate::Session;
use cadcraft_arch::opening::validate_placement;
use cadcraft_doc::arch::{ArchId, ArchModel, Level, MAX_OPENINGS, Opening, OpeningKind};

/// Seeded xorshift64*, copied bit-for-bit from the CLI's `Rng`.
struct Rng(u64);

impl Rng {
    /// Advance the state and return the next 64-bit word.
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// The next [0.0, 1.0) value (same as the CLI's `next`).
    fn next(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Generate `n` connected walls. The chain is fully determined by `n` and the
/// fixed seed, so two fresh runs with the same `n` produce identical data.
fn generate_walls(n: usize) -> Vec<cadcraft_doc::Wall> {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut walls: Vec<cadcraft_doc::Wall> = Vec::with_capacity(n);
    let mut pos = cadcraft_geom::Vec2::ZERO;
    let mut dir = 0usize; // 0:+x 1:+y 2:-x 3:-y (right turn = clockwise)
    // Right-angle turn every 4 + (rng % 3) walls (4..=6 walls per leg).
    let mut legs_left: usize = (4 + (rng.next_u64() % 3)) as usize;
    let mut k = 0usize; // 1-based wall index

    while walls.len() < n {
        // Length: 3.0 + (rng % 60) as f64 / 10.0 → [3.0, 8.9) in 0.1 steps.
        // Never zero with the seeded generator; if a degenerate value ever
        // comes back, skip it and retry with a fresh one, bounded retries.
        let mut length: f64 = 0.0;
        let mut tried = 0usize;
        while length <= 0.0 || !length.is_finite() || tried < 32 {
            length = 3.0 + ((rng.next() * 60.0).floor() as f64) / 10.0;
            tried += 1;
        }
        if !(length > 0.0) {
            length = 3.0;
        }

        let (dx, dy) = match dir {
            0 => (1.0, 0.0),
            1 => (0.0, 1.0),
            2 => (-1.0, 0.0),
            _ => (0.0, -1.0),
        };
        let end = pos + cadcraft_geom::Vec2::new(dx * length, dy * length);

        walls.push(cadcraft_doc::Wall {
            id: cadcraft_doc::ArchId((k + 1) as u32),
            start: pos,
            end,
            thickness: if (k + 1) % 2 == 1 { 0.2 } else { 0.1 },
            height: 2.6,
            ..cadcraft_doc::Wall::default()
        });

        pos = end;
        k += 1;

        legs_left = legs_left.saturating_sub(1);
        if legs_left == 0 {
            dir = (dir + 1) % 4;
            legs_left = (4 + (rng.next_u64() % 3)) as usize;
        }
    }

    walls
}

/// Minimum wall length (m) for a sample opening to be seeded on a wall.
const MIN_SAMPLE_WALL: f64 = 2.0;

/// One opening attempt to seed: kind, finished width/height, sill, style.
type OpeningSpec = (OpeningKind, f64, f64, f64, &'static str);

/// Deterministically seed up to `max` openings on the given model, one per
/// wall.
///
/// Two waves so no separation conflict is ever possible: wave 0 seeds the
/// even 1-based walls, wave 1 the odd ones (the other parity is skipped with
/// `continue`), and each candidate wall gets at most one opening. Every
/// generated wall is 3.0-8.9 m long and 2.6 m high, so a single 0.9×2.1 door
/// or 1.2×1.2 window (sill 0.9 m) at 40 % of the centre-line always passes
/// [`validate_placement`] (min segment 0.15 m, rough allowance 0.12 m),
/// meaning wave 0 alone yields `floor(n/2)` openings — 150 for 300 walls —
/// and wave 1 doubles the capacity to `n`. `max` is already clamped to
/// `MAX_OPENINGS` in `run_sample`. Returns the number actually placed; never
/// fails.
fn seed_openings(model: &mut ArchModel, max: usize) -> usize {
    const DOOR: OpeningSpec = (OpeningKind::Door, 0.9, 2.1, 0.0, "hinged");
    const WINDOW: OpeningSpec = (OpeningKind::Window, 1.2, 1.2, 0.9, "slider");

    let mut placed = 0usize;
    // Index-based loop: the only thing copied out is `wid` (a `Copy` ArchId)
    // and a plain `f64`, so no borrow of `model.walls` is live when
    // `model.push_opening(o)` mutates the model.
    for wave in 0..2 {
        for i in 0..model.walls.len() {
            if placed >= max {
                return placed;
            }
            // Wave 0: even 1-based walls; wave 1: odd ones.
            if (i + 1) % 2 != wave {
                continue;
            }
            let wid = model.walls[i].id;
            let length = model.walls[i].start.dist(model.walls[i].end);
            if !length.is_finite() || length < MIN_SAMPLE_WALL {
                continue;
            }
            let spec = if (i + 1) % 3 == 0 { DOOR } else { WINDOW };
            let (kind, width, height, sill, style) = spec;
            let offset = 0.4 * length;

            match validate_placement(model, wid, offset, width, sill, height, kind, style, None) {
                Ok(off) => {
                    let mut o = Opening::new(wid, off, width, height, kind);
                    o.sill = sill;
                    o.style = style.to_string();
                    model.push_opening(o);
                    placed += 1;
                }
                Err(_) => {
                    continue;
                }
            }
            if placed >= max {
                return placed;
            }
        }
    }
    placed
}

/// The four 0.2 m exterior walls of the 10×8 m phase-1 house on `level`,
/// centre lines (0,0)→(10,0)→(10,8)→(0,8)→(0,0).
fn house_exterior(level: u32) -> Vec<cadcraft_doc::Wall> {
    let w = |s: (f64, f64), e: (f64, f64), lvl: u32| -> cadcraft_doc::Wall {
        cadcraft_doc::Wall {
            start: cadcraft_geom::Vec2::new(s.0, s.1),
            end: cadcraft_geom::Vec2::new(e.0, e.1),
            base_level: lvl,
            ..cadcraft_doc::Wall::default()
        }
    };
    vec![w((0.0, 0.0), (10.0, 0.0), level), w((10.0, 0.0), (10.0, 8.0), level), w((10.0, 8.0), (0.0, 8.0), level), w((0.0, 8.0), (0.0, 0.0), level)]
}

/// The thickness-0 interior divider (5,0)→(5,8) on `level`.
fn central_divider(level: u32) -> cadcraft_doc::Wall {
    cadcraft_doc::Wall {
        start: cadcraft_geom::Vec2::new(5.0, 0.0),
        end: cadcraft_geom::Vec2::new(5.0, 8.0),
        base_level: level,
        exterior: false,
        wall_type: "Interior".into(),
        thickness: 0.0,
        divider: true,
        ..cadcraft_doc::Wall::default()
    }
}

/// Run `arch.sample` with `{"house": true}` — phase-1 10×8 house on Level 1,
/// `roomdetect` → 1 room.
fn run_house(s: &mut Session, _params: &Value) -> crate::Result<Value> {
    let mut m = cadcraft_doc::ArchModel::default();
    m.levels = vec![cadcraft_doc::Level { index: 1, name: "Level 1".into(), ..cadcraft_doc::Level::default() }];
    for w in house_exterior(1) {
        m.push_wall(w);
    }
    let rooms = cadcraft_arch::room::detect_rooms(&m);
    let placed = rooms.into_iter().map(|r| m.push_room(r)).count();
    let d = s.doc_mut()?;
    d.arch = m;
    d.touch_arch();
    Ok(json!({
        "walls": d.arch.walls.len(),
        "levels": d.arch.levels.len(),
        "rooms": placed,
        "revision": d.arch_revision
    }))
}

/// Run `arch.sample` with `{"house2": true}` — three-storey demo: basement
/// (elevation −2.7) + Level 1 (elevation 0) + Level 2 (elevation 2.7),
/// exterior walls on all three levels, central divider on the basement and
/// Level 2, `roomdetect` on all three levels → 5 rooms.
fn run_house2(s: &mut Session, _params: &Value) -> crate::Result<Value> {
    let mut m = cadcraft_doc::ArchModel::default();
    m.levels = vec![
        cadcraft_doc::Level { index: 0, name: "Basement".into(), elevation: -2.7, ..cadcraft_doc::Level::default() },
        cadcraft_doc::Level { index: 1, name: "Level 1".into(), ..cadcraft_doc::Level::default() },
        cadcraft_doc::Level { index: 2, name: "Level 2".into(), elevation: 2.7, ..cadcraft_doc::Level::default() },
    ];
    for w in house_exterior(0) {
        m.push_wall(w);
    }
    m.push_wall(central_divider(0));
    for w in house_exterior(1) {
        m.push_wall(w);
    }
    for w in house_exterior(2) {
        m.push_wall(w);
    }
    m.push_wall(central_divider(2));
    let rooms = cadcraft_arch::room::detect_rooms(&m);
    let placed = rooms.into_iter().map(|r| m.push_room(r)).count();
    let d = s.doc_mut()?;
    d.arch = m;
    d.touch_arch();
    Ok(json!({
        "walls": d.arch.walls.len(),
        "levels": d.arch.levels.len(),
        "rooms": placed,
        "revision": d.arch_revision
    }))
}

/// Run `arch.sample`: inject a deterministic wall chain into the drawing.
///
/// `walls` defaults to 64, clamped to `1..=MAX_WALLS`; `openings` defaults to
/// 0 and is clamped to `MAX_OPENINGS`. This replaces the existing walls
/// *and* openings, calls `touch_arch()`; it is `.noundo()` because it is a
/// benchmark fixture.
///
/// `{"house": true}` and `{"house2": true}` override the wall chain with a
/// fixed demo (see `run_house` / `run_house2`).
fn run_sample(s: &mut Session, params: &Value) -> crate::Result<Value> {
    if let Some(true) = params.get("house2").and_then(|v| v.as_bool()) {
        return run_house2(s, params);
    }
    if let Some(true) = params.get("house").and_then(|v| v.as_bool()) {
        return run_house(s, params);
    }
    let n = params.get("walls").and_then(|v| v.as_u64()).unwrap_or(64).clamp(1, crate::cmd::arch::MAX_WALLS as u64) as usize;
    let count = params.get("openings").and_then(|v| v.as_u64()).unwrap_or(0).clamp(0, MAX_OPENINGS as u64) as usize;

    let d = s.doc_mut()?;
    d.arch.walls = generate_walls(n);
    // The generated chain hardcodes ids 1..=n; keep the shared counter in
    // step so `push_opening` hands out ids that continue after the walls.
    d.arch.next_id = n as u32;
    d.arch.openings.clear();
    let placed = seed_openings(&mut d.arch, count);
    d.touch_arch();

    Ok(json!({ "walls": n, "openings": placed, "revision": d.arch_revision }))
}

/// Command spec for `arch.sample`.
pub fn specs() -> Vec<crate::cmd::CommandSpec> {
    vec![
        crate::cmd::CommandSpec::new("arch.sample", "Arch Sample", run_sample)
            .menu(&[])
            .noundo()
            .params("{walls?: N, openings?: N, house?: bool, house2?: bool}"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    /// A session with a fresh empty drawing.
    fn fresh_session() -> Session {
        let mut s = Session::empty();
        s.new_drawing(false);
        s
    }

    #[test]
    fn arch_sample_is_deterministic_and_capped() {
        let mut s1 = fresh_session();
        let mut s2 = fresh_session();
        let p = json!({ "walls": 2000 });
        let r1 = s1.execute("arch.sample", &p).unwrap();
        let r2 = s2.execute("arch.sample", &p).unwrap();

        assert_eq!(r1["walls"].as_u64(), Some(2000));
        assert_eq!(r2["walls"].as_u64(), Some(2000));

        // Byte-identical wall data on fresh documents.
        for (a, b) in s1.doc().unwrap().arch.walls.iter().zip(s2.doc().unwrap().arch.walls.iter()) {
            assert_eq!(a, b);
        }

        // Revision was bumped.
        assert!(s1.doc().unwrap().arch_revision > 0);

        // N is capped at MAX_WALLS.
        let mut big = fresh_session();
        let r3 = big.execute("arch.sample", &json!({ "walls": 99999 })).unwrap();
        assert_eq!(r3["walls"].as_u64(), Some(crate::cmd::arch::MAX_WALLS as u64));
        assert_eq!(big.doc().unwrap().arch.walls.len(), crate::cmd::arch::MAX_WALLS);
    }

    #[test]
    fn arch_sample_clamps_a_hostile_wall_count() {
        let mut s = fresh_session();
        let r = s.execute("arch.sample", &json!({ "walls": 999999999999i64 })).unwrap();
        assert_eq!(r["walls"].as_u64(), Some(crate::cmd::arch::MAX_WALLS as u64));
    }

    #[test]
    fn arch_sample_walls_are_connected() {
        let mut s = fresh_session();
        s.execute("arch.sample", &json!({ "walls": 64 })).unwrap();
        let walls = &s.doc().unwrap().arch.walls;
        for i in 0..walls.len().saturating_sub(1) {
            assert_eq!(walls[i].end, walls[i + 1].start, "wall {} end != wall {} start", i, i + 1);
        }
    }

    /// The seed is the only input: two fresh sessions with the same
    /// `walls`/`openings` params must produce byte-identical openings
    /// (ids, hosts, offsets, dimensions, styles, kinds) in the same order.
    ///
    /// `openings` is set to the wave-0 capacity (`floor(N_WALLS / 2)` = 16),
    /// so exactly the even 1-based walls get seeded and wave 1 never runs:
    /// 16 openings, one per even wall 2,4,…,32, wall 1 empty.
    #[test]
    fn arch_sample_places_deterministic_openings() {
        const N_WALLS: usize = 32;
        const N_OPENINGS: usize = 16;
        let p = json!({ "walls": N_WALLS, "openings": N_OPENINGS });

        let mut s1 = fresh_session();
        let mut s2 = fresh_session();
        let r1 = s1.execute("arch.sample", &p).unwrap();
        let r2 = s2.execute("arch.sample", &p).unwrap();
        assert_eq!(r1, r2);

        let o1 = s1.doc().unwrap().arch.openings.clone();
        let o2 = s2.doc().unwrap().arch.openings.clone();
        assert_eq!(o1, o2);

        // 16 openings (even walls 2..=32, one each), ids in push order.
        assert_eq!(o1.len(), 16, "expected 16 seeded openings");
        assert!(o1.iter().all(|o| o.id.0 < N_OPENINGS as u32), "opening ids must be within the opening counter range");
        assert_eq!(o1.iter().map(|o| o.host.0).collect::<Vec<u32>>(), (2..=N_WALLS).step_by(2).map(|i| i as u32).collect::<Vec<u32>>());

        let d = s1.doc().unwrap();
        // Every opening sits at 40 % of its host wall, in the door/window
        // styles the command seeds.
        for o in &o1 {
            let w = d.arch.wall(o.host).expect("opening host missing");
            let len = w.start.dist(w.end);
            assert!((o.offset - 0.4 * len).abs() < 1e-9, "offset of {:?} not 40 % of its wall", o.id);
            if o.style.contains("hinged") {
                assert_eq!(o.kind, OpeningKind::Door);
                assert_eq!((o.width, o.height, o.sill), (0.9, 2.1, 0.0));
            } else {
                assert_eq!(o.style, "slider");
                assert_eq!(o.kind, OpeningKind::Window);
                assert_eq!((o.width, o.height, o.sill), (1.2, 1.2, 0.9));
            }
        }
        // 1st wall (odd): no opening; 2nd: exactly the window; 6th: exactly
        // the door (1-based index 6 is a multiple of 3).
        assert!(d.arch.openings_on(ArchId(1)).is_empty());
        assert_eq!(d.arch.openings_on(ArchId(2)).len(), 1);
        assert_eq!(d.arch.openings_on(ArchId(6)).len(), 1);
    }

    /// `arch.sample {"walls":300,"openings":150}` places exactly the requested
    /// 150 openings: one per wall, every host distinct, and every placement
    /// still legal against the final model.
    #[test]
    fn arch_sample_places_the_requested_opening_count() {
        let mut s = fresh_session();
        let r = s.execute("arch.sample", &json!({ "walls": 300, "openings": 150 })).unwrap();
        assert_eq!(r["openings"].as_u64(), Some(150));
        let d = s.doc().unwrap();
        assert_eq!(d.arch.openings.len(), 150);

        // One per wall: every host is distinct.
        let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for o in &d.arch.openings {
            assert!(seen.insert(o.host.0), "duplicate host wall for opening {:?}", o.id);
        }

        // Every placement is still legal on the final model.
        for o in &d.arch.openings {
            assert!(
                validate_placement(&d.arch, o.host, o.offset, o.width, o.sill, o.height, o.kind, &o.style, Some(o.id)).is_ok(),
                "opening {:?} fails re-validation on the final model",
                o.id
            );
        }
    }

    /// Opening ids are exactly `ArchId(0)..ArchId(149)` in push order for a
    /// full 300-wall / 150-opening sample — this is what makes `o10`
    /// resolvable by `arch.opening.move`.
    #[test]
    fn arch_sample_opening_ids_are_sequential() {
        let mut s = fresh_session();
        s.execute("arch.sample", &json!({ "walls": 300, "openings": 150 })).unwrap();
        let ids: Vec<ArchId> = s.doc().unwrap().arch.openings.iter().map(|o| o.id).collect();
        assert_eq!(ids, (0..150).map(ArchId).collect::<Vec<ArchId>>());
    }

    /// `arch.opening.move {"opening":"o10","offset":1.0}` on a 300-wall sample
    /// succeeds: with one opening per wall and every wall at least 3.0 m long,
    /// offset 1.0 always validates. This is exactly the edit
    /// `tests/acceptance/phase-2/09_opening_rebuild_bench.sh` runs.
    #[test]
    fn arch_sample_opening_move_to_one_metre_is_legal() {
        let mut s = fresh_session();
        s.execute("arch.sample", &json!({ "walls": 300, "openings": 150 })).unwrap();
        s.execute("arch.opening.move", &json!({ "opening": "o10", "offset": 1.0 })).unwrap();
    }

    /// Requesting more openings than walls places at most one per wall and
    /// neither fails nor panics.
    #[test]
    fn arch_sample_more_openings_than_walls_is_capped_by_capacity() {
        let mut s = fresh_session();
        let r = s.execute("arch.sample", &json!({ "walls": 8, "openings": 100 })).unwrap();
        assert_eq!(r["walls"].as_u64(), Some(8));
        let placed = r["openings"].as_u64().unwrap();
        assert!(placed <= 8, "placed {placed} openings on 8 walls");
        assert_eq!(s.doc().unwrap().arch.openings.len(), placed as usize);
    }

    /// Every seeded opening must still pass placement validation against the
    /// final model (rough/finished dimensions, wall height, segment and
    /// separation limits), i.e. the fixture is a set of legal placements.
    #[test]
    fn arch_sample_openings_are_valid_placements() {
        let mut s = fresh_session();
        s.execute("arch.sample", &json!({ "walls": 64, "openings": 16 })).unwrap();
        let d = s.doc().unwrap();
        for o in &d.arch.openings {
            assert!(d.arch.wall(o.host).is_some(), "host wall missing for {:?}", o.id);
            assert!(
                validate_placement(&d.arch, o.host, o.offset, o.width, o.sill, o.height, o.kind, &o.style, Some(o.id),).is_ok(),
                "opening {:?} fails re-validation on the final model",
                o.id
            );
        }
    }

    /// Without the `openings` parameter (or with 0) the model must contain no
    /// openings, exactly as the command behaved before this parameter.
    #[test]
    fn arch_sample_without_openings_is_unchanged() {
        let mut s1 = fresh_session();
        let mut s2 = fresh_session();
        s1.execute("arch.sample", &json!({})).unwrap();
        s2.execute("arch.sample", &json!({ "walls": 24, "openings": 0 })).unwrap();

        assert!(s1.doc().unwrap().arch.openings.is_empty());
        assert_eq!(s1.doc().unwrap().arch.walls.len(), 64);
        assert!(s2.doc().unwrap().arch.openings.is_empty());
        assert_eq!(s2.doc().unwrap().arch.walls.len(), 24);
    }

    /// `{"house":true}` is the phase-1 10×8 house on Level 1: four exterior
    /// walls, one detected room; the existing `{"walls": N}` path is unchanged.
    #[test]
    fn sample_house_is_unchanged() {
        let mut s1 = fresh_session();
        let mut s2 = fresh_session();
        s1.execute("arch.sample", &json!({ "house": true })).unwrap();
        s2.execute("arch.sample", &json!({ "house": true })).unwrap();

        for (a, b) in s1.doc().unwrap().arch.walls.iter().zip(s2.doc().unwrap().arch.walls.iter()) {
            assert_eq!(a, b);
        }
        let d = &s1.doc().unwrap().arch;
        assert_eq!(d.levels.len(), 1);
        assert_eq!(d.levels[0].index, 1);
        assert_eq!(d.levels[0].elevation, 0.0);
        assert_eq!(d.walls.len(), 4);
        assert_eq!(d.rooms.len(), 1);
        assert_eq!(d.rooms[0].level, 1);

        // Existing `{"walls": N}` path still produces the deterministic chain.
        let mut s3 = fresh_session();
        let mut s4 = fresh_session();
        s3.execute("arch.sample", &json!({ "walls": 64 })).unwrap();
        s4.execute("arch.sample", &json!({ "walls": 64 })).unwrap();
        for (a, b) in s3.doc().unwrap().arch.walls.iter().zip(s4.doc().unwrap().arch.walls.iter()) {
            assert_eq!(a, b);
        }
        assert_eq!(s3.doc().unwrap().arch.walls.len(), 64);
        assert!(s3.doc().unwrap().arch.openings.is_empty());
    }

    /// `{"house2":true}` is the three-storey demo: basement (−2.7) + Level 1
    /// (0) + Level 2 (2.7), exterior on all levels, divider on basement and
    /// Level 2, `roomdetect` on all three → 5 rooms (2 + 1 + 2).
    #[test]
    fn sample_house2_has_two_levels_and_five_rooms() {
        let mut s1 = fresh_session();
        let mut s2 = fresh_session();
        s1.execute("arch.sample", &json!({ "house2": true })).unwrap();
        s2.execute("arch.sample", &json!({ "house2": true })).unwrap();

        for (a, b) in s1.doc().unwrap().arch.walls.iter().zip(s2.doc().unwrap().arch.walls.iter()) {
            assert_eq!(a, b);
        }
        for (a, b) in s1.doc().unwrap().arch.rooms.iter().zip(s2.doc().unwrap().arch.rooms.iter()) {
            assert_eq!(a, b);
        }

        let d = &s1.doc().unwrap().arch;
        assert_eq!(d.levels.len(), 3);
        assert_eq!(d.levels[0].index, 0);
        assert_eq!(d.levels[1].index, 1);
        assert_eq!(d.levels[2].index, 2);
        assert_eq!(d.levels[0].elevation, -2.7);
        assert_eq!(d.levels[1].elevation, 0.0);
        assert_eq!(d.levels[2].elevation, 2.7);
        assert_eq!(d.walls.len(), 14);
        assert_eq!(d.rooms.len(), 5);
        let rooms_on = |lvl: u32| d.rooms.iter().filter(|r| r.level == lvl).count();
        assert_eq!(rooms_on(0), 2);
        assert_eq!(rooms_on(1), 1);
        assert_eq!(rooms_on(2), 2);
    }

    /// The four exterior walls of `{"house":true}` must be at the exact
    /// positions specified by the phase-1 design: a closed 10×8 rectangle.
    #[test]
    fn sample_house_wall_positions() {
        let mut s = fresh_session();
        s.execute("arch.sample", &json!({ "house": true })).unwrap();
        let walls = &s.doc().unwrap().arch.walls;
        assert_eq!(walls.len(), 4);
        assert_eq!(walls[0].start, cadcraft_geom::Vec2::new(0.0, 0.0));
        assert_eq!(walls[0].end, cadcraft_geom::Vec2::new(10.0, 0.0));
        assert_eq!(walls[1].start, cadcraft_geom::Vec2::new(10.0, 0.0));
        assert_eq!(walls[1].end, cadcraft_geom::Vec2::new(10.0, 8.0));
        assert_eq!(walls[2].start, cadcraft_geom::Vec2::new(10.0, 8.0));
        assert_eq!(walls[2].end, cadcraft_geom::Vec2::new(0.0, 8.0));
        assert_eq!(walls[3].start, cadcraft_geom::Vec2::new(0.0, 8.0));
        assert_eq!(walls[3].end, cadcraft_geom::Vec2::new(0.0, 0.0));
    }

    /// The 14 walls of `{"house2":true}` must be at the exact positions:
    /// basement (4 exterior + 1 divider), Level 1 (4 exterior),
    /// Level 2 (4 exterior + 1 divider).
    #[test]
    fn sample_house2_wall_positions() {
        let mut s = fresh_session();
        s.execute("arch.sample", &json!({ "house2": true })).unwrap();
        let walls = &s.doc().unwrap().arch.walls;
        assert_eq!(walls.len(), 14);
        // Basement exterior walls (level 0).
        assert_eq!(walls[0].start, cadcraft_geom::Vec2::new(0.0, 0.0));
        assert_eq!(walls[0].end, cadcraft_geom::Vec2::new(10.0, 0.0));
        assert_eq!(walls[0].base_level, 0);
        assert_eq!(walls[1].start, cadcraft_geom::Vec2::new(10.0, 0.0));
        assert_eq!(walls[1].end, cadcraft_geom::Vec2::new(10.0, 8.0));
        assert_eq!(walls[1].base_level, 0);
        assert_eq!(walls[2].start, cadcraft_geom::Vec2::new(10.0, 8.0));
        assert_eq!(walls[2].end, cadcraft_geom::Vec2::new(0.0, 8.0));
        assert_eq!(walls[2].base_level, 0);
        assert_eq!(walls[3].start, cadcraft_geom::Vec2::new(0.0, 8.0));
        assert_eq!(walls[3].end, cadcraft_geom::Vec2::new(0.0, 0.0));
        assert_eq!(walls[3].base_level, 0);
        // Basement divider.
        assert_eq!(walls[4].start, cadcraft_geom::Vec2::new(5.0, 0.0));
        assert_eq!(walls[4].end, cadcraft_geom::Vec2::new(5.0, 8.0));
        assert_eq!(walls[4].base_level, 0);
        assert!(walls[4].divider);
        // Level 1 exterior walls.
        assert_eq!(walls[5].start, cadcraft_geom::Vec2::new(0.0, 0.0));
        assert_eq!(walls[5].end, cadcraft_geom::Vec2::new(10.0, 0.0));
        assert_eq!(walls[5].base_level, 1);
        assert_eq!(walls[8].start, cadcraft_geom::Vec2::new(0.0, 8.0));
        assert_eq!(walls[8].end, cadcraft_geom::Vec2::new(0.0, 0.0));
        assert_eq!(walls[8].base_level, 1);
        // Level 2 exterior walls.
        assert_eq!(walls[9].start, cadcraft_geom::Vec2::new(0.0, 0.0));
        assert_eq!(walls[9].end, cadcraft_geom::Vec2::new(10.0, 0.0));
        assert_eq!(walls[9].base_level, 2);
        assert_eq!(walls[12].start, cadcraft_geom::Vec2::new(0.0, 8.0));
        assert_eq!(walls[12].end, cadcraft_geom::Vec2::new(0.0, 0.0));
        assert_eq!(walls[12].base_level, 2);
        // Level 2 divider.
        assert_eq!(walls[13].start, cadcraft_geom::Vec2::new(5.0, 0.0));
        assert_eq!(walls[13].end, cadcraft_geom::Vec2::new(5.0, 8.0));
        assert_eq!(walls[13].base_level, 2);
        assert!(walls[13].divider);
    }

    /// `{"house":true}` must produce exactly one room with a reasonable
    /// interior area (the 10×8 rectangle minus wall thickness).
    #[test]
    fn sample_house_room_area() {
        let mut s = fresh_session();
        s.execute("arch.sample", &json!({ "house": true })).unwrap();
        let d = s.doc().unwrap();
        assert_eq!(d.arch.rooms.len(), 1);
        let room = &d.arch.rooms[0];
        // Interior area should be roughly (10-0.2) × (8-0.2) = 76.44 m²,
        // but allow for the detection algorithm's calculation method.
        assert!(room.interior_area > 70.0, "room area too small: {}", room.interior_area);
        assert!(room.interior_area < 85.0, "room area too large: {}", room.interior_area);
        // The room must be on Level 1.
        assert_eq!(room.level, 1);
    }

    /// `{"house2":true}` must produce 5 rooms with positive areas,
    /// distributed 2/1/2 across the three levels.
    #[test]
    fn sample_house2_room_areas() {
        let mut s = fresh_session();
        s.execute("arch.sample", &json!({ "house2": true })).unwrap();
        let d = s.doc().unwrap();
        assert_eq!(d.arch.rooms.len(), 5);
        for room in &d.arch.rooms {
            assert!(room.interior_area > 0.0, "room {:?} has non-positive area", room.id);
            // Each room is at most half the 10×8 footprint.
            assert!(room.interior_area < 80.0, "room {:?} area too large: {}", room.id, room.interior_area);
        }
        // Basement rooms (level 0) should be smaller than the full footprint
        // because of the divider.
        let basement_rooms: Vec<&cadcraft_doc::Room> = d.arch.rooms.iter().filter(|r| r.level == 0).collect();
        assert_eq!(basement_rooms.len(), 2);
        for room in &basement_rooms {
            assert!(room.interior_area < 40.0, "basement room {:?} too large: {}", room.id, room.interior_area);
        }
    }

    /// `{"house":true}` must not create any openings.
    #[test]
    fn sample_house_no_openings() {
        let mut s = fresh_session();
        s.execute("arch.sample", &json!({ "house": true })).unwrap();
        assert!(s.doc().unwrap().arch.openings.is_empty());
    }

    /// `{"house2":true}` must not create any openings.
    #[test]
    fn sample_house2_no_openings() {
        let mut s = fresh_session();
        s.execute("arch.sample", &json!({ "house2": true })).unwrap();
        assert!(s.doc().unwrap().arch.openings.is_empty());
    }

    /// Hostile/extra parameters to `{"house":true}` must be ignored; the
    /// output must be identical to the plain call.
    #[test]
    fn sample_house_hostile_input() {
        let mut s = fresh_session();
        s.execute("arch.sample", &json!({ "house": true, "walls": 100, "openings": 50, "extra": "garbage" })).unwrap();
        let d = s.doc().unwrap();
        assert_eq!(d.arch.walls.len(), 4);
        assert_eq!(d.arch.rooms.len(), 1);
        assert!(d.arch.openings.is_empty());
        assert_eq!(d.arch.levels.len(), 1);
    }

    /// Hostile/extra parameters to `{"house2":true}` must be ignored; the
    /// output must be identical to the plain call.
    #[test]
    fn sample_house2_hostile_input() {
        let mut s = fresh_session();
        s.execute("arch.sample", &json!({ "house2": true, "walls": 100, "openings": 50, "extra": 3.14 })).unwrap();
        let d = s.doc().unwrap();
        assert_eq!(d.arch.walls.len(), 14);
        assert_eq!(d.arch.rooms.len(), 5);
        assert!(d.arch.openings.is_empty());
        assert_eq!(d.arch.levels.len(), 3);
    }

    /// `{"house":true}` must bump the arch revision from 0.
    #[test]
    fn sample_house_revision_bumped() {
        let mut s = fresh_session();
        assert_eq!(s.doc().unwrap().arch_revision, 0);
        s.execute("arch.sample", &json!({ "house": true })).unwrap();
        assert!(s.doc().unwrap().arch_revision > 0);
    }

    /// `{"house2":true}` must bump the arch revision from 0.
    #[test]
    fn sample_house2_revision_bumped() {
        let mut s = fresh_session();
        assert_eq!(s.doc().unwrap().arch_revision, 0);
        s.execute("arch.sample", &json!({ "house2": true })).unwrap();
        assert!(s.doc().unwrap().arch_revision > 0);
    }
}
