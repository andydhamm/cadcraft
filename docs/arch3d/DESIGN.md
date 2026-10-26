# arch3d — design log

Branch `arch3d` (fork of `storytold/cadcraft`, v0.10.0). Running log for "draw a plan, get the house in 3D". One commit per phase, each ending at a ⛔ checkpoint.

---

## Phase 0 — survey

- Workspace `crates/*`, `apps/*`, `xtask`; edition 2024, rust 1.90, MIT OR Apache-2.0. `unsafe_code = forbid`; every crate root denies unwrap/expect/panic/todo.
- Layers (`xtask/src/layers.rs`): L0 `geom`,`color` (`dxf`,`dwg` standalone) → L1 `doc` → L2 `constraints`,`fonts`,`render` → L3 `io` → L4 `engine` → L5 `ui-egui`,`mcp` → apps.
- Roadmap: 3D is 2%; `docs/gaps.md` F24 is *AutoCAD* 3D — untouched; our work is additive with its own ARCH-* rows.
- **Entities:** `Entity{handle,common,kind}`; `Common.xdata` travels with COPY/MIRROR/ARRAY. `EntityStore` is copy-on-write (1024-`Arc` chunks + 64 sharded handle maps) so whole-drawing undo is cheap.
- **Commands:** everything user-visible is a `CommandSpec` (`engine/src/cmd/mod.rs`); one implementation, two paths — JSON `Session::execute` (MCP/CLI/scripts, hostile input validated, never clamped silently) and interactive `Session::start` → `Interactive`. Both under `catch_unwind` (`guard.rs`).
- **Undo:** full-`Drawing` snapshot stack, cheap via CoW. *Consequence:* anything that must undo/autosave/audit/round-trip lives inside `Drawing`.
- **Selection/grips/snaps:** `select.rs`, `spatial.rs`, `grips.rs::stretch_grip` (drags run a `grip.*` command, so undoable and MCP-reachable), `snap.rs` OSMODE bits.
- **Rendering was 2D only:** `render::build(space) -> DisplayList`; the GPU path draws it through an `egui_wgpu::Callback` with `Stencil8` — no depth buffer, no camera, no lighting. The 3D viewport was new work.
- **DXF custom data:** `CADCRAFT` app in XDATA + XRECORDs in the named-object dictionary; `dxf_region.rs` shows the proxy-graphic pattern (ordinary polylines + XDATA naming them one object). Foreign data preserved verbatim.
- **MCP:** a small generic tool set; `execute` forwards to `Session::execute`. *Adjustment:* no MCP tool per arch command — commands are the API.

## Crate layout (as built)

Precedent: **`constraints` (L2) — data in `doc`, algorithms in a peer crate.**

| Crate | L | Contents | Deps |
|---|---|---|---|
| `doc/src/arch.rs` | L1 | `ArchModel{walls,openings,rooms,next_id,next_room_id,levels,types}`, `ArchId`, `Wall`, `Opening`, `Room`, `RoomType`, `Level`, `WallType`; `ARCH_HANDLE_BASE=0xA000_0000`, `ROOM_HANDLE_OFFSET=0x10000`; caps `MAX_WALLS 4096`, `MAX_OPENINGS 8192`, `MAX_ROOMS 1024`, `MIN_ROOM_AREA 0.05`. On `Drawing::arch` with `arch_revision`/`touch_arch()`. | geom,color |
| `arch` | L2 | `wall`,`graph`,`join`(`solve_joins`,`face_lines`,`stretch_wall`),`opening`,`symbol`,`room`(detection),`poly`(area/perimeter/`offset_polygon` per-edge),`area`(interior/standard/living),`label`(`label_point`). | geom,color,doc |
| `arch3d` | L2 | `build`/`build_dirty`→`Scene3d`; `mesh`(ear clip, `push_prism_with_holes`), `walls`, `openings`, `floors`, `probe`+`ray`; `edges`/`hlr`/`section` (assisted, phases 6–7). | geom,color,doc,arch |
| `render` | L2 | `arch.rs`: arch geometry → DisplayList tagged with proxy handles; layer bands; opening holes; room labels on `A-ROOM`. | arch |
| `io` | L3 | `dxf_arch.rs` (`CADCRAFT_ARCH` XRECORD v2 + save-time proxies on `A-WALL`/`A-DOOR`/`A-GLAZ`/`A-ROOM`), `glb.rs` (own GLTF 2.0). | arch,arch3d,dxf |
| `render3d` | L5 | `camera`(`Orbit`,`Orbit::fit`,`projected_scene_bounds`), `pipeline`, `light`, `offscreen`(`create`/`with_device`/`encode`/`render`/`read_png`/`headless_instance`), `gputest`, `types`. | geom,color,arch3d,wgpu |
| `engine/src/cmd/arch/*` | L4 | `wall`,`wall.stretch`,`inspect`,`opening`,`build3d`,`sample`,`view`,**`room`,`roomdetect`,`level`**; grip routing; `snap::arch`. | arch,arch3d,io |
| `ui-egui` | L5 | `pane3d.rs` (dockable, orbit, revision-gated rebuild, no-adapter placeholder). | render3d,arch3d |
| `cadcraft-cli` | — | `convert`, `run --export`, `render FILE OUT.png --camera` (prints `PROJECTED`, exit 3 `NO_ADAPTER`). | all |

## Decisions

1. **Arch model lives in `Drawing`** (undo is a whole-`Drawing` snapshot). Data in `doc`, algorithms in `arch`.
2. **One source of truth: the arch object, not its linework.** Plan geometry generated into the display list at draw time; DXF proxies written only on save, not read back when arch data is present.
3. **No 3D CSG.** Walls cut by 2D boolean on the wall *elevation*, then extruded; slabs/ceilings are the same trick on room polygons.
4. **Pure Rust, no new heavy deps.** Own GLTF/OBJ/STEP writers; MIT/Apache-2.0/BSD/Zlib, wasm-compatible; no C++ kernels.
5. **The 3D pane renders offscreen** into its own colour+depth textures and presents as an image — egui's pass has no depth buffer we control; identical on WebGPU and WebGL2.
6. **MCP stays generic.** Commands are the API.
7. **Incremental rebuild first-class:** `arch_revision` + dirty `ArchId` set.
8. **Never crash, always deterministic:** caps, `get()`, checked arithmetic, property/fuzz tests.
9. **Additive to AutoCAD parity**; arch commands are new ids under `Architecture`.
10. **Proxy handles, not entities:** `0xA000_0000 + id`, rooms `+ ROOM_HANDLE_OFFSET`; `grips.rs::arch_id_from_handle` is the single disambiguator.
11. **Rough vs finished separate** (`ROUGH_ALLOWANCE` one constant).
12. **Placement limits and area rules are data in `arch`**, not logic in commands, so commands/CLI/MCP/`ARCHDETECT` agree.
13. **Shared structs** are built outside their declaring file only as `Type{required,..Default::default()}` or via a constructor; `cargo xtask literals` enforces it.
14. **Derived, never stored,** when a quantity is a sum of others (`Level::floor_to_floor()` = ceiling + structure; the stored field was removed).
15. **Probing, not counting, proves geometry.** `build3d {"probe":…}` is the acceptance observable; any new 3D feature must add probeable solids, not just triangles.

## Phase 1 — walls, joins, poché, first 3D (landed)

`solve_joins` trims/extends centre-line ends to neighbour face-line intersections (L/T/X/angled/collinear, per-end overrides); `stretch_wall` capped transitive BFS; `render::arch` expands solved rings + layer bands; `arch3d::build` extrudes elevations with `object: ArchId` and box UVs; `io::glb` writes GLTF 2.0 (`wall-w1`); wall grips run `arch.wall.stretch`. Checks 01–09 PASS. Deferrals: `is_main` joins, pony walls, `reversed`, arc walls.

## Phase 2 — openings (landed)

`Opening` on `ArchModel.openings` (host, offset, finished + rough dims, recess, style, hinge/swing, frame/casing/sill). `arch::opening` holds Chief's limits as constants (`MIN_HINGED_WIDTH 0.3`, `MIN_GARAGE_WIDTH 1.8`, `MIN_WINDOW_SEGMENT 0.15`, `MIN_SEPARATION 0.05`, `ROUGH_ALLOWANCE 0.12`) with `validate_placement`/`place_opening`/`auto_panel`/`clamp_recess`/`project_to_wall`. Generated plan symbols; poché cut by even-odd holes. `mesh::push_prism_with_holes` triangulates the elevation minus opening rectangles; `openings::opening_meshes` emits frame/casing/glass/leaf/sill/header merged per material (GLB node names, blended glass). `door`/`window`/`garagedoor`/`arch.opening.move`; grips slide openings. Checks 01–09 PASS. Deferrals: bay/box/bow geometry, mullions, jambs/reveals, hardware, display states, schedules, pocket doors, arc-wall openings, exterior/interior classification.

## Phase 3 — rooms, floors, ceilings, levels (landed)

**Carry-overs closed.** (a) `render3d` draws: colour+depth targets, one directional light + ambient, flat shading from each triangle's own plane (arch3d boxes share corners with averaged normals and inconsistent winding, which speckled faces); meshes re-upload on scene-key change; `render` exits **3** with `NO_ADAPTER` and prints `PROJECTED x0,y0,x1,y1` from `projected_scene_bounds`; check 06 rewritten (flat fill fails, foreground fraction in (0.05,0.85), bbox within 3 px, second camera changes pixels, top-down covers more than oblique). (b) `wall_span(model,wall)->Option<(base,top)>` uses the level elevation and honours `to_ceiling`. (c) `build3d {"probe":[…]}` (≤1024 finite points) → `{id,part,level}` or null, via `arch3d::ray` (Möller–Trumbore specialised to +Z — the first version tested z-ranges, never saw a horizontal face, and reported a point inside a slab as empty); `part_of` maps material names to wall/floor/ceiling/glass/frame/casing/leaf/sill/header/roof.

**Levels.** `Level{index,name,elevation,ceiling_height,structure}`; `floor_to_floor()` derived (ceiling + structure), stored field removed (check 09 greps for it and fails if present). Defaults 2.4 + 0.3 = 2.7. `level {above|below|index|name|ceiling|structure}`; basements renumber walls; auto-naming follows the neighbour; wall height defaults to floor-to-floor.

**Rooms.** `Room{id,name,room_type,level,polygon,holes,ceiling_height,floor_height,interior_area,standard_area}` with own `next_room_id` and `room_handle`/`room_label`/`is_room_handle`. `RoomType` (Living, Bedroom, Kitchen, Bath, Closet, Utility, Garage, Porch, Deck, OpenBelow, Other) with `is_interior`/`has_floor`/`has_ceiling`. `detect_rooms`: per level, non-divider walls solved on a temporary model (identical joins to the plan), poché rings closed, `divider` walls contribute centre lines; seeds are edge-midpoint left-normal offsets plus the centroid; faces traced with `geom::region::boundary_at`, normalised CCW, holes from islands, filtered by `MIN_ROOM_AREA` and a thin-strip guard, deduped by rounded (centroid, area), capped by `MAX_ROOMS`. A face inside a face is both a hole and its own room.

**Areas (`arch::area`).** `interior_area` = region area; `standard_area` offsets each edge to its wall's reference line (`edge_standard_offset`: outer face for exterior walls, centre line for interior, 0 for dividers) with `poly::offset_polygon(pts, dist, outward)` per-edge distances, holes inward, falling back to interior when degenerate; `perimeter(Interior|Standard|Exterior)`; `living_area` = sum of standard over interior types with effective ceiling ≥ `MIN_LIVING_CEILING` 1.2; `refresh_areas`.

**3D.** `floors::floor_slab` (top exactly at level elevation, down by structure) and `ceiling_surface` (at ceiling height) — prisms with holes, `Mesh::level_elevation` set, `None` for `!has_floor()`/`!has_ceiling()`; `Scene3d.floors`/`.ceilings` counts, equal in full and incremental builds (regression-tested).

**Labels/grips/DXF.** `label_point` inside polygon+holes; `render::arch` shapes name+area on `A-ROOM` tagged with the room handle; `dxf_arch` writes save-time `A-ROOM` proxies with `CADCRAFT-ROOM` XDATA, dropped on read while `CADCRAFT_ARCH` is present; room grips resolve to `ArchTarget::Room` and return the label point, drags a silent no-op. `arch.inspect {rooms,labels,levels}` reports id/handle/name/type/level/polygon/holes/areas/livingIncluded/ceiling/label.

**Commands.** `level`, `room` (closed ring ≥4 distinct simple points, or edit by id — name/type/heights only, validated before any write, no-op refused), `roomdetect {level?,dryRun?}` (replace-per-level, dryRun never calls `doc_mut()`, one `touch_arch()` → one undo step, `truncated` reported), `build3d {probe}`, `arch.sample {house2:true}` (basement −2.7, L1 0, L2 2.7, 5 rooms). `uk.tsv` rows in the same segments.

**Numbers locked.** Interior 27.93 / 28.3725 / 18.6725; standard 31.62 / 31.11 / 20.91 = 83.64 = 10.2 × 8.2; closed 0.1 m loop → south 26.72 with one hole + a 0.81 m² room, each with its own slab (probes give different ids); Level 2 at 2.7 with `(5,0)–(5,8)` → two rooms of 37.83; probes hit a Level-2 slab at (2,1,2.55), nothing at (2,1,1.0), Level-1 slab at (2,1,-0.15), nothing at (12,1,2.55). Room-type rules: Open Below → no floor, ceiling kept; Deck/Porch → floor, no ceiling.

**Acceptance (all PASS).** `01_levels_command` · `02_room_detect` · `03_room_areas` · `04_levels_two_storey` · `05_floors_ceilings_3d` · `06_render3d_real` · `07_room_labels_and_roundtrip` · `08_hostile_rooms` · `09_layers_wasm_i18n`. Four checks were broken as written and fixed (02 sorted-vs-unsorted, 03 edited a room never drawn, 05 contradictory ceiling rules, 06 passed on a flat fill). **Lesson: run every check against a stub before locking it.**

**Phase-3 gaps.** Out of scope by the brief: stair wells (5), ceilings under the roof (4), room boxes / Build House, other-floor reference, dynamic defaults, sloped/vaulted ceilings, stem walls, split levels, moldings. Also: `is_main` joins missing (standard area measures to the exterior *outer face*; Chief's alternative basis is the Main Layer — one function to change); rooms not drag-editable; exterior/interior door classification not wired; `crates/arch/src/planarr.rs` is an empty orphan to delete; `docs/gaps.md` ARCH-13 says "phase 9" (exports are 12); `uk.tsv` has "Дверь" for Door.

## Open items vs Chief Architect

Roofs (ARCH-9); stairs/railings/decks and stair wells (ARCH-10); PBR, sun+shadows, SSAO, display styles, walkthrough, GPU picking synced with 2D (6); HLR elevations/sections (ARCH-11); `ARCHDETECT` (ARCH-12); OBJ and IFC (ARCH-13); **walls still not cut by openings in 3D** (4); arc walls and their openings; main-layer joins and the standard-area basis; pony walls, `reversed`; wall-top clipping to rafters; framing; terrain and contours; electrical/plumbing/HVAC; layout sheets; ray-traced final views; dormers; curtain walls; dynamic roof topology; IFC4 beyond six entity classes; `ARCHDETECT` corpus validation; phase-2 opening sub-items; room boxes, reference display, dynamic defaults, split levels, moldings; door classification.

## Phase plan

0 ✅ → 1 ✅ → 2 ✅ → 3 ✅ → **4 roofs** → 5 stairs/railings/decks → 6 the 3D view properly → 7 elevations/sections → 8 `ARCHDETECT` → 9 terrain/site → 10 lighting → 11 ray-traced Finalize → 12 exports → 13 polish.

## Phase 4 plan (next)

Automatic roofs (ARCH-9), built on the assisted `geom::roof_skeleton(outer,holes,slopes)` / `weighted_skeleton` (each edge sweeps one face; node `time` is roof height; `f64::INFINITY` = a vertical gable face).
- `ROOFAUTO` over the top level's exterior walls: outer-face footprint, collinear edges merged, ≤256 edges, event/iteration caps, **on failure return an error and build no roof**.
- Per-wall directives (hip/full gable, pitch, overhang, eave height) as data on `Wall`; rebuilds while walls change until a plane is hand-edited. Plane underside passes through the top outer edge of its eave wall; thickness above that surface; fascia band + flat soffit. `ROOFAUTO` also draws one plane from polygon + base edge + pitch.
- **Wall-to-roof clipping without CSG:** build the elevation profile first (pentagon for a gable), cut the openings' rough openings into that same profile, triangulate with holes, extrude. Closes the phase-2 defect; un-ignore `hlr_section.rs::a_window_beyond_a_section_shows_through_its_wall`; probe `[4.0,8.0,1.5]` finds no wall in a 0.9 × 1.2 window (sill 0.9) at x = 4 on the north wall, `[4.0,8.0,0.5]` finds it.
- Numbers (walls 2.4 m, overhang 0): hip 6/12 → four planes, ridge x 4.0…6.0 at y 4.0, z = 2.4 + 4.1 × 0.5 = **4.45**; 4/12 → 3.7667, 12/12 → 6.5. Gables on x=0 and x=10 → two planes, ridge −0.1…10.1 at 4.45, gable walls reach 4.45 at y=4, eave walls stay 2.4. One gable + one hip → three planes, ridge x 4.0…10.1. Overhang 0.5 on a hip → projected areas sum to **11.2 × 9.2 = 103.04 m²**.
- Invariants: one sloped edge per footprint vertex (hip convex, valley reflex); one plane per footprint edge; slope equals pitch to 1e-9; interior edges shared by exactly two planes; projected areas sum to footprint + overhang band. L footprint `(0,0)(10,0)(10,8)(6,8)(6,12)(0,12)` has one valley, a T two.
- Carry the hostile-input and layers/wasm/i18n checks; `uk.tsv` rows in the same segment; new shared-struct fields get defaults.

## Building blocks added outside the swarm (assisted)

- `geom::skeleton` (2026-10-25): `straight_skeleton(outer,holes)`, `inset`/`outset` mitred offsets, `check` invariants. Each input edge sweeps one face; node `time` is distance to that edge.
- `geom::roof_skeleton(outer,holes,slopes)` and `weighted_skeleton(..,weights)`: per-edge pitch and gable ends (`f64::INFINITY` = vertical face); node `time` is roof height, each face is the plane over its edge. Build phase 4 on this.
- `render3d` drawing and the 3D pane (2026-10-25): `Offscreen` owns colour+depth on its own device (`create`) or the window's (`with_device`); `encode`/`render` draw depth-tested with one directional light + ambient, one uniform slot per mesh, albedo from `material_albedo`; flat shading from each triangle's own plane (`cross(dpdy,dpdx)`) because arch3d boxes share corners with averaged normals and inconsistent winding; `read_rgba`/`read_png` bounded by `READBACK_TIMEOUT`; `PROJECTED` from `projected_scene_bounds`. The pane syncs per document and `arch_revision`, redraws only when scene/camera/size changed. **Headless uses `Backends::PRIMARY` (`headless_instance`)** — concurrent GL/EGL starts hung test binaries for 900 s on cgyws2; GPU tests share one device (`gputest::shared_device`). wgpu moved 29 → 30 to match egui-wgpu.

## Assisted building block (2026-10-25): feature edges, HLR, section cuts (phases 6–7)

Pure CPU, deterministic, in `arch3d`; tests `crates/arch3d/tests/hlr_section.rs`.
- **`edges`.** `feature_edges(mesh, CREASE_DEG)` / `scene_feature_edges`: weld at 1e-5 m, then edges used by one triangle (`Boundary`), >2 (`NonManifold`), or two faces meeting over 20° (`Crease`); planar triangulation diagonals never appear. With a view direction, `Silhouette` edges are added.
- **`hlr`.** `OrthoView::new(dir,up)` / `::elevation([x,y])`; `visible_lines(scene,view) -> Vec<Line2>` tagged with `ArchId`. Exact: depth difference is linear along an edge, so the hidden part is one interval; covering is closed (`EPS_SHEET`) so diagonals never leak; a triangle hides only what is over `EPS_DEPTH` (0.1 mm) behind its plane plus its slope across that band. `merge_collinear` joins runs of the same object only.
- **`section`.** `Plane::new(point,normal)`; `section_cut(scene,plane,view) -> SectionCut{rings,lines}` — crossings computed once per welded edge and chained, so closed solids give CCW closed rings (open surfaces give lines); a corner exactly on the plane counts as in front. `section_view(scene,plane,up)` = cut + `visible_lines` beyond, each object's cut face hiding what lies inside its poché.

**What the HLR exposed (unfixed):** walls are not cut by openings (phase 4 fixes it); the header is flush with both wall faces, so once walls are cut it draws as a rectangle above every opening — narrow it or drop it; casings are clamped inside the wall band instead of standing proud by `casing_depth`; `Opening::new` gives sill 0 to windows (the `window` command defaults 0.9), so tests set `sill` themselves.

## Phase 4 plan

**Phase 4 — automatic roofs (ARCH-9).**

- **Data** (`doc::arch`): `Roof{id,level,pitch_rise,pitch_run,overhang,thickness,fascia,soffit,manual,hand_edited,planes,edges}` on `ArchModel.roofs` with its own `next_roof_id` and handle window `ROOF_HANDLE_OFFSET=0x18000`; per-wall `roof_directive`/`roof_pitch`/`roof_overhang`/`eave_height`. `RoofPlane` stores the plan ring + slope + projected area + base edge; `RoofEdge` is classified Eave/Ridge/Hip/Valley/GableEnd with both z's, so `arch.inspect` answers roof geometry without rebuilding meshes.
- **Generator** (`arch::roof`): outer-face footprint from the *solved* poché (so joins are identical to the plan), collinear edges merged, ≤256 edges, then `geom::roof_skeleton(outer, holes, slopes)` — node `time` is roof height, each face is the plane over its edge, `f64::INFINITY` slope is a vertical gable end. Overhang runs the skeleton on `geom::outset`. **Failure is all-or-nothing**: any `geom::check` failure, non-finite node or over-cap returns `RoofError` and writes nothing — never a partial roof.
- **Rebuild policy**: a `ROOFAUTO` roof is regenerated when its walls change; `roofplane {edit}` sets `hand_edited`, after which wall edits stop rebuilding it (the roof keeps its last planes).
- **Wall-to-roof clipping without CSG**: the wall's *elevation profile* (wall coords u=along, v=height) is built first — a rectangle, or a pentagon when a roof plane's `base_edge` lies on the wall's outer face — then each hosted opening's **rough** rect is clipped into it as a hole, then the profile is extruded between the wall's two faces. Same 2D-boolean-then-extrude rule as phases 1–3.
- **Phase-2 defect closed**: openings now really cut the wall. `build3d {"probe":[[4,8,1.5]]}` finds no wall inside a 0.9×1.2 window at x=4 on the north wall, `[4,8,0.5]` finds it; `hlr_section.rs::a_window_beyond_a_section_shows_through_its_wall` is un-ignored and passing (check 06 also greps that no `ignore` remains).
- **Numbers locked**: hip 6/12 → 4 planes, ridge x 4.0…6.0 at y 4.0, z 4.45; 4/12 → 3.766667; 12/12 → 6.5; two gables → 2 planes, ridge −0.1…10.1, gable walls reach 4.45 at y=4 while eave walls stay 2.4; gable+hip → 3 planes, ridge x 4.0…10.1; overhang 0.5 → projected 103.04 = 11.2×9.2; L → 1 valley, T → 2.
- **Observables**: `arch.inspect {"roofs":true}` (planes, edges, projected area, handEdited) and `build3d` `roofs` count + probe part `roof` (fascia/soffit map to `roof`). No new MCP tool.
- **Round-trip**: `CADCRAFT_ARCH` v4 (accepts 1–4); save-time `A-ROOF` proxy linework dropped on read while the record is present; GLB nodes `roof-r0p0`, `roof-r0-fascia`, `roof-r0-soffit` with their own PBR material.
- **Phase-4 gaps** (out of scope by the brief): dormers, crickets, Dutch/half-hip gables, roof returns, attic/knee walls, "roof cuts wall at bottom", roofs over lower-level bump-outs, skylights, gutters, interactive joining of planes, curved walls under a roof. Also still open: the header drawn flush above every opening (phase 7 HLR note), `is_main` joins, arc walls and their openings.
