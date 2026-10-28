# arch3d — design log

Branch `arch3d` (fork of `storytold/cadcraft`, v0.10.0). Running log for "draw a plan, get the house in 3D". One commit per phase, each ending at a ⛔ checkpoint.

---

## Phase 0 — survey

- Workspace `crates/*`, `apps/*`, `xtask`; edition 2024, rust 1.90, MIT OR Apache-2.0. `unsafe_code = forbid`; every crate root denies unwrap/expect/panic/todo.
- Layers (`xtask/src/layers.rs`): L0 `geom`,`color` (`dxf`,`dwg` standalone) → L1 `doc` → L2 `constraints`,`fonts`,`render` → L3 `io` → L4 `engine` → L5 `ui-egui`,`mcp` → apps.
- Roadmap: 3D is 2%; `docs/gaps.md` F24 is *AutoCAD* 3D — untouched; our work is additive with its own ARCH-* rows.
- **Entities:** `Entity{handle,kind,common}`; `Common.xdata` travels with COPY/MIRROR/ARRAY. `EntityStore` is copy-on-write (1024-`Arc` chunks + 64 sharded handle maps) so whole-drawing undo is cheap.
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
| `doc/src/arch.rs` | L1 | `ArchModel{walls,openings,rooms,levels,roofs,types,next_id,next_room_id,next_roof_id}`, `ArchId`, `Wall`, `Opening`, `Room`, `RoomType`, `Level`, `WallType`, `Roof`, `RoofPlane`, `RoofEdge`; `ARCH_HANDLE_BASE=0xA000_0000`, `ROOM_HANDLE_OFFSET=0x10000`, `ROOF_HANDLE_OFFSET=0x18000`; caps `MAX_WALLS 4096`, `MAX_OPENINGS 8192`, `MAX_ROOMS 1024`, `MIN_ROOM_AREA 0.05`, `MAX_ROOF_EDGES 256`. On `Drawing::arch` with `arch_revision`/`touch_arch()`. | geom,color |
| `arch` | L2 | `wall`,`graph`,`join`(`solve_joins`,`face_lines`,`stretch_wall`),`opening`,`symbol`,`room`(detection),`poly`(area/perimeter/`offset_polygon` per-edge),`area`(interior/standard/living),`label`(`label_point`),**`roof`(footprint, skeleton driver, `RoofError`)**, **`roofedge`(hip/valley/eave/ridge/gable classification)**. | geom,color,doc |
| `arch3d` | L2 | `build`/`build_dirty`→`Scene3d`; `mesh`(ear clip, `push_prism_with_holes`, profile prism), `walls`(elevation profiles + holes), `openings`, `floors`, **`roof`(planes, fascia, soffit)**, `probe`+`ray`; `edges`/`hlr`/`section` (assisted, phases 6–7). | geom,color,doc,arch |
| `render` | L2 | `arch.rs`: arch geometry → DisplayList tagged with proxy handles; layer bands; opening holes; room labels on `A-ROOM`; **roof plan on `A-ROOF`**. | arch |
| `io` | L3 | `dxf_arch.rs` (`CADCRAFT_ARCH` XRECORD **v4**, accepts 1–4, + save-time proxies on `A-WALL`/`A-DOOR`/`A-GLAZ`/`A-ROOM`/`A-ROOF`), `glb.rs` (own GLTF 2.0). | arch,arch3d,dxf |
| `render3d` | L5 | `camera`(`Orbit`,`Orbit::fit`,`projected_scene_bounds`), `pipeline`, `light`, `offscreen`(`create`/`with_device`/`encode`/`render`/`read_png`/`headless_instance`), `gputest`, `types`. | geom,color,arch3d,wgpu |
| `engine/src/cmd/arch/*` | L4 | `wall`,`wall.stretch`,`inspect`,`opening`,`build3d`,`sample`,`view`,`room`,`roomdetect`,`level`,**`roofauto`,`roofplane`**; grip routing; `snap::arch`. | arch,arch3d,io |
| `ui-egui` | L5 | `pane3d.rs` (dockable, orbit, revision-gated rebuild, show-3D toggle, no-adapter placeholder). | render3d,arch3d |
| `cadcraft-cli` | — | `convert`, `run --export`, `render FILE OUT.png --camera` (prints `PROJECTED`, exit 3 `NO_ADAPTER`). | all |

## Decisions

1. **Arch model lives in `Drawing`** (undo is a whole-`Drawing` snapshot). Data in `doc`, algorithms in `arch`.
2. **One source of truth: the arch object, not its linework.** Plan geometry generated into the display list at draw time; DXF proxies written only on save, not read back when arch data is present.
3. **No 3D CSG.** Walls cut by 2D boolean on the wall *elevation profile*, then extruded; slabs/ceilings are the same trick on room polygons; roof planes are prisms over their plan ring.
4. **Pure Rust, no new heavy deps.** Own GLTF/OBJ/STEP writers; MIT/Apache-2.0/BSD/Zlib, wasm-compatible; no C++ kernels.
5. **The 3D pane renders offscreen** into its own colour+depth textures and presents as an image — egui's pass has no depth buffer we control; identical on WebGPU and WebGL2.
6. **MCP stays generic.** Commands are the API.
7. **Incremental rebuild first-class:** `arch_revision` + dirty `ArchId` set.
8. **Never crash, always deterministic:** caps, `get()`, checked arithmetic, property/fuzz tests.
9. **Additive to AutoCAD parity**; arch commands are new ids under `Architecture`.
10. **Proxy handles, not entities:** `0xA000_0000 + id`, rooms `+0x10000`, roofs `+0x18000`; `grips.rs::arch_id_from_handle` is the single disambiguator.
11. **Rough vs finished separate** (`ROUGH_ALLOWANCE` one constant).
12. **Placement limits and area rules are data in `arch`**, not logic in commands, so commands/CLI/MCP/`ARCHDETECT` agree.
13. **Shared structs** are built outside their declaring file only as `Type{required,..Default::default()}` or via a constructor; `cargo xtask literals` enforces it.
14. **Derived, never stored,** when a quantity is a sum of others (`Level::floor_to_floor()` = ceiling + structure; the stored field was removed).
15. **Probing, not counting, proves geometry.** `build3d {"probe":…}` is the acceptance observable; any new 3D feature must add probeable solids, not just triangles.
16. **Roofs are generated, then owned.** A `ROOFAUTO` roof is regenerated while its walls change; `roofplane {edit}` sets `hand_edited` and the roof keeps its last planes. Generation is all-or-nothing — an error means no roof at all, never a partial one.

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

**Phase-3 gaps.** Out of scope by the brief: stair wells (5), ceilings under the roof (4, landed), room boxes / Build House, other-floor reference, dynamic defaults, sloped/vaulted ceilings, stem walls, split levels, moldings. Also: `is_main` joins missing (standard area measures to the exterior *outer face*; Chief's alternative basis is the Main Layer — one function to change); rooms not drag-editable; exterior/interior door classification not wired; `crates/arch/src/planarr.rs` is an empty orphan to delete; `docs/gaps.md` ARCH-13 says "phase 9" (exports are 12); `uk.tsv` has "Дверь" for Door.

## Open items vs Chief Architect

Stairs/railings/decks and stair wells (ARCH-10); PBR, sun+shadows, SSAO, display styles, walkthrough, GPU picking synced with 2D (6); HLR elevations/sections (ARCH-11); `ARCHDETECT` (ARCH-12); OBJ and IFC (ARCH-13); arc walls and their openings; main-layer joins and the standard-area basis; pony walls, `reversed`; framing; terrain and contours; electrical/plumbing/HVAC; layout sheets; ray-traced final views; dormers/crickets/roof returns/skylights/gutters; interactive roof-plane joining; curtain walls; dynamic roof topology; IFC4 beyond six entity classes; `ARCHDETECT` corpus validation; phase-2 opening sub-items (mullions, jambs, hardware, display states, schedules); room boxes, reference display, dynamic defaults, split levels, moldings; door classification; housekeeping (delete `planarr.rs` and the two `*_tmp.rs` scratch files at the repo root, fix ARCH-13's phase number, `uk.tsv` "Дверь").

## Phase plan

0 ✅ → 1 ✅ → 2 ✅ → 3 ✅ → 4 ✅ → **5 stairs/railings/decks** → 6 the 3D view properly → 7 elevations/sections → 8 `ARCHDETECT` → 9 terrain/site → 10 lighting → 11 ray-traced Finalize → 12 exports → 13 polish.

## Building blocks added outside the swarm (assisted)

- `geom::skeleton` (2026-10-25): `straight_skeleton(outer,holes)`, `inset`/`outset` mitred offsets, `check` invariants. Each input edge sweeps one face; node `time` is distance to that edge.
- `geom::roof_skeleton(outer,holes,slopes)` and `weighted_skeleton(..,weights)`: per-edge pitch and gable ends (`f64::INFINITY` = vertical face); node `time` is roof height, each face is the plane over its edge. Phase 4 is built on this.
- `render3d` drawing and the 3D pane (2026-10-25): `Offscreen` owns colour+depth on its own device (`create`) or the window's (`with_device`); `encode`/`render` draw depth-tested with one directional light + ambient, one uniform slot per mesh, albedo from `material_albedo`; flat shading from each triangle's own plane (`cross(dpdy,dpdx)`) because arch3d boxes share corners with averaged normals and inconsistent winding; `read_rgba`/`read_png` bounded by `READBACK_TIMEOUT`; `PROJECTED` from `projected_scene_bounds`. The pane syncs per document and `arch_revision`, redraws only when scene/camera/size changed. **Headless uses `Backends::PRIMARY` (`headless_instance`)** — concurrent GL/EGL starts hung test binaries for 900 s on cgyws2; GPU tests share one device (`gputest::shared_device`). wgpu moved 29 → 30 to match egui-wgpu.
- **Feature edges, HLR, section cuts (2026-10-25, phases 6–7).** Pure CPU, deterministic, in `arch3d`; tests `crates/arch3d/tests/hlr_section.rs`.
  - **`edges`.** `feature_edges(mesh, CREASE_DEG)` / `scene_feature_edges`: weld at 1e-5 m, then edges used by one triangle (`Boundary`), >2 (`NonManifold`), or two faces meeting over 20° (`Crease`); planar triangulation diagonals never appear. With a view direction, `Silhouette` edges are added.
  - **`hlr`.** `OrthoView::new(dir,up)` / `::elevation([x,y])`; `visible_lines(scene,view) -> Vec<Line2>` tagged with `ArchId`. Exact: depth difference is linear along an edge, so the hidden part is one interval; covering is closed (`EPS_SHEET`) so diagonals never leak; a triangle hides only what is over `EPS_DEPTH` (0.1 mm) behind its plane plus its slope across that band. `merge_collinear` joins runs of the same object only.
  - **`section`.** `Plane::new(point,normal)`; `section_cut(scene,plane,view) -> SectionCut{rings,lines}` — crossings computed once per welded edge and chained, so closed solids give CCW closed rings (open surfaces give lines); a corner exactly on the plane counts as in front. `section_view(scene,plane,up)` = cut + `visible_lines` beyond, each object's cut face hiding what lies inside its poché.
  - **What the HLR exposed:** walls were not cut by openings (fixed in phase 4, and `a_window_beyond_a_section_shows_through_its_wall` is now un-ignored and passing); the header is flush with both wall faces, so now that walls are cut it draws as a rectangle above every opening — narrow it or drop it (phase 7); casings are clamped inside the wall band instead of standing proud by `casing_depth`; `Opening::new` gives sill 0 to windows (the `window` command defaults 0.9), so tests set `sill` themselves.

## Phase 4 — automatic roofs (landed)

**Data (`doc::arch`).** `Roof{id, level, pitch_rise, pitch_run, overhang, thickness, fascia, soffit, manual, hand_edited, planes, edges}` on `ArchModel.roofs`, own `next_roof_id`, handle window `ROOF_HANDLE_OFFSET = 0x18000`. `RoofPlane{ring, slope, projected_area, base_edge, eave_z}`; `RoofEdge{kind: Eave|Ridge|Hip|Valley|GableEnd, a, b, z_a, z_b}` so `arch.inspect` answers roof geometry with no mesh. Per-wall `roof_directive` (hip / full gable), `roof_pitch`, `roof_overhang`, `eave_height`. All new fields defaulted (the phase-1/2 merge-rejection class).

**Generator (`arch::roof` 1856 lines, `arch::roofedge` 1280 lines).** Footprint from the **outer faces of the solved poché** of the top level's exterior walls — the same `solve_joins` call the plan uses, so the roof matches the drawing, not the raw centre lines. Largest closed component only; collinear edges merged first; ≤256 edges; `geom::roof_skeleton(outer, holes, slopes)` with node `time` = roof height and `f64::INFINITY` = a vertical gable end; overhang by running the skeleton on `geom::outset` and keeping the band. Hip vs valley by footprint convexity (convex vertex → hip, reflex → valley). **All-or-nothing:** any `geom::check` failure, non-finite node, interior-crossing wall or over-cap input returns `RoofError` and writes nothing.

**Rebuild policy.** A `ROOFAUTO` roof regenerates when its walls change (`build_dirty` marks the roof from the wall's neighbours); `roofplane {edit}` sets `hand_edited` and wall edits then leave the roof's planes alone.

**Wall-to-roof clipping, no CSG.** `arch3d::walls` builds the wall's **elevation profile** first (u = along, v = height): a rectangle, or a pentagon when a roof plane's `base_edge` lies on that wall's outer face; then each hosted opening's **rough** rect is clipped into that profile as a hole; then the profile is extruded between the wall's two faces (`mesh::push_profile_prism`). This closes the phase-2 defect: openings really cut the wall now — `build3d {"probe":[[4.0,8.0,1.5]]}` finds no wall inside a 0.9 × 1.2 window (sill 0.9) at x = 4 on the north wall, `[4.0,8.0,0.5]` finds it; `hlr_section.rs::a_window_beyond_a_section_shows_through_its_wall` is un-ignored and passing (check 06 greps that no `ignore` remains).

**3D / GLB / DXF / plan.** `arch3d::roof` emits each plane as a solid (underside through the top outer edge of its eave wall, thickness measured **vertically** above that surface — `p4k`), plus a fascia band at the eave edge and a flat soffit back to the wall; `Scene3d.roofs` counts them and `part_of` maps roof/fascia/soffit → `roof`. GLB nodes `roof-r0p0`, `roof-r0-fascia`, `roof-r0-soffit` with their own PBR material (chunk padding fixed in `p4x-glb-chunk-padding`). DXF: `CADCRAFT_ARCH` **v4** (accepts 1–4) plus save-time `A-ROOF` proxy linework dropped on read while the record is present. `render::arch` draws a roof plan on `A-ROOF` (plane outlines, ridges, hips, valleys) honouring layer state; the 3D pane shows roofs and has a show-3D toggle.

**Commands / observables.** `roofauto {level?, pitch:[r,r], overhang?, thickness?, directive?:{wall|all:"hip"|"gable"}, dryRun?}` and `roofplane {polygon, baseEdge, pitch, thickness?, overhang?, id?, edit?}` (object and array argument forms both accepted); `arch.inspect {"roofs":true}`; `build3d` `roofs` count. One `touch_arch()` per command → one undo step. `uk.tsv` rows landed in the same segments.

**Numbers locked.** Hip 6/12 → 4 planes (2 trapezoids, 2 triangles), ridge x 4.0…6.0 at y 4.0, z = 2.4 + 4.1 × 0.5 = **4.45**; 4/12 → **3.766667**; 12/12 → **6.5**. Two gables (x = 0, x = 10) → 2 planes, ridge −0.1…10.1 at 4.45, gable walls reach 4.45 at y = 4 while eave walls stay 2.4. One gable + one hip → 3 planes, ridge x 4.0…10.1. Overhang 0.5 on a hip → projected area **103.04 m²** = 11.2 × 9.2. Square → pyramid. L `(0,0)(10,0)(10,8)(6,8)(6,12)(0,12)` → 1 valley; T → 2.

**Invariants (property tests).** `arch/tests/property_roofs.rs` + `roof_property.rs`, `geom/tests/skeleton_roof_property_tests.rs`, `arch3d/tests/golden_roofs.rs`: on an all-hip orthogonal footprint every footprint vertex starts exactly one sloped edge (hip at convex, valley at reflex), every footprint edge has exactly one plane, every plane's slope equals its pitch to 1e-9, every interior roof edge is shared by exactly two planes, projected areas sum to footprint (+ overhang band) area.

**Review sweep (`p4r-*`, 34 segments).** Wiring roofs through the existing paths surfaced real defects, all fixed: join solver (X crossings, T junctions, mitre limit, own-kind exclusion, raw-ring fallback, box prefilter, tidy), linear graph build, layer-band clamp, `wall.stretch` that never collapses a wall and keeps its openings when the start end moves, wall-command validation and bands/joins, camera validation, `Scene3d` finite-f32 guard, mesh side normals, dirty-neighbour propagation, reserved arch handles, arch layers created on first use, GLB/GLTF conformance, CLI render args, `render3d` lints, the pane's show-3D toggle, `render::arch` layer state, and an honest rebuild benchmark (`rebuilt` ≤ 10 % of objects, so a fast full rebuild cannot pass) with a golden house fixture.

**Acceptance (all PASS).** `01_roofauto_hip` · `02_roofauto_pitches` · `03_gable_and_clipping` · `04_mixed_gable_hip` · `05_overhang` · `06_openings_cut_the_profile` · `07_roofplane_and_rebuild` · `08_hostile_roofs` · `09_layers_wasm_i18n` · `10_roof_roundtrip_and_glb` · `11_roof_invariants`. Hostile-input and layers/wasm/i18n checks carried forward; `cargo xtask wasm` still builds.

**Phase-4 gaps.** Out of scope by the brief: dormers, crickets, Dutch/half-hip gables, roof returns, attic and knee walls, "roof cuts wall at bottom", roofs over lower-level bump-outs, skylights, gutters, interactive joining of planes, curved walls under a roof. Also still open: the header drawn flush above every opening (phase-7 HLR note), `is_main` joins, arc walls and their openings, roof planes over a lower level's bump-out, per-plane material.

## Phase 5 plan (next)

**Stairs, railings and decks (ARCH-10).**
- `arch::stair` calculator as data, not command logic: riser count *n* = the integer whose riser height (rise ÷ *n*) is closest to **168.75 mm** without exceeding the max riser; tread depth = min tread unless given; defaults (IRC) max riser 0.196 m, min tread 0.254 m, min headroom 2.032 m, all configurable per template. Straight, L and U with a landing at least as deep as the stair is wide.
- `STAIR {start, direction, width, fromLevel, toLevel, shape?}`; `RAILING` (balusters or solid panel; handrail 0.864–0.965 m above the nosing line, guards 0.914 m at landings and wells, baluster clear gap < 0.1016 m); `DECK` as a `RoomType::Deck` (floor, no ceiling) with its own height above grade and a board surface; deck-to-grade stairs reuse the same calculator.
- **Stair wells** (moved here from phase 3): a stair between two levels creates an `OpenBelow` room on the upper level over the flight where headroom < minimum, with a guard railing round it.
- **Numbers:** L1 0 → L2 2.7 → **16 risers of 0.16875 m, 15 treads of 0.254 m, run 3.81 m**; deck 4 × 3 m at 0.9 m → **5 risers of 0.18 m** (6 × 0.15 is further from 168.75 mm); a probe 2.032 m above the top of the 3rd riser (z 2.538, inside the Level-2 slab's 2.4…2.7) finds **no** floor slab.
- Carry the hostile-input and layers/wasm/i18n checks; `uk.tsv` rows in the same segment; new shared-struct fields get defaults; stairs must be probeable (`part: "stair"`, `"railing"`, `"deck"`).
- Out of scope: winders, curved/spiral/wrapped stairs, ramps, gooseneck and smooth rail transitions, open risers, deck joist and beam framing.

## Phase 5 plan

**Phase 5 — stairs, railings, decks.**

Data (`doc::arch`): `Stair{id,start,direction(rad),width,from_level,to_level,rise,shape,risers,riser_height,tread_depth,treads,run,landing_depth,well,well_room,layer}` and `Railing{id,path,level,height,style,thickness,baluster_gap,post_spacing,follows,layer}`, both on `ArchModel` with their own counters; `Room.deck_height: Option<f64>`. Handle windows `STAIR_HANDLE_OFFSET 0x20000` (`s{n}`), `RAILING_HANDLE_OFFSET 0x28000` (`ra{n}`); `grips.rs::arch_id_from_handle` gains both arms. All new fields defaulted, so `cargo xtask literals` stays clean.

The calculator (`arch::stair`) is data, not command logic: *n* minimises |rise/n − 0.16875| subject to rise/n ≤ 0.196, searched from ⌈rise/maxRiser⌉ over a bounded 64-wide window; tread = min tread (0.254) unless given; landing ≥ width. `well_start` = the first riser whose top is more than `slab_underside − 2.032` up, and the well begins one tread earlier. `arch::railing` owns the height bands (handrail 0.864–0.965, guard ≥ 0.914) and baluster spacing (< 0.1016 clear). Commands validate through these functions and refuse — never clamp.

3D (`arch3d::stairs`, `arch3d::railing`): the flight is one closed stepped prism (profile in flight coordinates, extruded across the width) — no CSG, the same trick as walls. Railings always carry a top handrail bar so a probe anywhere under the rail resolves. Deck rooms build a board slab whose top is `level.elevation + deck_height` (0.05 thick, material `deck`) instead of the grade slab. `Scene3d` gains `stairs`/`railings`/`decks`; `part_of` maps stair/railing/deck ahead of floor/wall. Stair wells are holes: the stair's `well` polygon is added to the upper level's floor-slab holes and becomes an `OpenBelow` room (no slab) ringed by a guard `Railing` with `follows = stair id`.

Numbers locked: 2.7 m → 16 risers × 0.16875, 15 treads × 0.254, run 3.81; 0.9 m → 5 risers × 0.18, run 1.016; well x 2.508…5.81, area 3.302, probe (2.6,1,2.538) empty while (2.1,1,2.538) and (8,4,2.538) hit the Level-2 slab; deck 4×3 at 0.9 probes `deck` at 0.88, nothing at 2.39 or −0.15.

Plumbing: `arch.inspect {stairs,railings}` + `deckHeight`/`hasFloor` on rooms; `build3d` counts; CADCRAFT_ARCH **v5** (accepts 1–5, clears stairs/railings for ≤4); GLB nodes `stair-s{n}`, `railing-ra{n}`, `deck-r{n}`; plan linework on new `A-STAIR`/`A-RAIL` layers; `uk.tsv` rows in the same segment.

**Phase-5 gaps (vs Chief):** winders, curved/spiral/wrapped stairs, ramps, gooseneck and smooth rail transitions, open risers, stair soffits and stringer detail, deck joist/beam framing, railings that follow the nosing line exactly (we use a straight top rail), stair railings on the flight itself (only well/landing guards are generated), multi-flight L/U geometry is stored but only `straight` is meshed, stairs under a roof, stairs in elevations/sections (phase 7), stairs from `ARCHDETECT` (phase 8), stairs in IFC (phase 12).
