# arch3d — design log

Branch `arch3d` (fork of `storytold/cadcraft`, v0.10.0). Running design log for "draw a plan, get the house in 3D". Phases land one commit each, each ending at a ⛔ checkpoint.

---

## Phase 0 — survey

### 0.1 The repo at a glance

- Workspace: `crates/*`, `apps/*`, `xtask`. Edition 2024, `rust-version = 1.90`, licence `MIT OR Apache-2.0`.
- Lints: `unsafe_code = "forbid"`; every production crate root carries `#![deny(clippy::unwrap_used, expect_used, panic, unimplemented, todo, unreachable)]`.
- Layering enforced by `cargo xtask layers` (`xtask/src/layers.rs::TABLE`): L0 `geom`,`color` (`dxf`,`dwg` standalone) → L1 `doc` → L2 `constraints`,`fonts`,`render` → L3 `io` → L4 `engine` → L5 `ui-egui`,`mcp` → apps (exempt). `UI_CRATES` only from L5 up.
- `plan/` is gitignored — absent in a fresh clone.
- Roadmap: pre-alpha, ~38% ready; **"3D modelling and rendering" is 2%, 300–450 h** (`ROADMAP.md`); `docs/gaps.md` **F24** is *AutoCAD* 3D (UCS, orbit, solids, 150 menu items). Our architectural 3D is additive; we keep our own ARCH-* rows and leave F24 alone.

### 0.2 Entities

`doc/src/entity.rs`: `Entity { handle: Handle(u64), common: Common, kind: EntityKind }`. `Common` carries layer/colour/linetype/lineweight/ltscale/transparency/visible/thickness/extrusion/plot style/annotative scales and **`xdata: Vec<RawTag>`** (foreign XDATA is part of the object, so COPY/MIRROR/ARRAY carry it). `EntityKind` is a closed enum plus `Unknown { dxf_type, tags }`.

`doc/src/store.rs`: `EntityStore` is a **copy-on-write chunked store** (1024-`Arc<Entity>` chunks + 64 sharded handle maps) in draw order. Cloning copies pointers; an edit copies one chunk and one shard — whole-drawing undo is cheap.

`Drawing` holds model/paper spaces, tables, layouts and `foreign: ForeignData`.

### 0.3 Commands

Everything user-visible is a `CommandSpec` (`engine/src/cmd/mod.rs`): `{ id (AutoCAD name, lower case), label, menu path, shortcut, aliases, params doc, enabled, run, undoable, interactive: Option<Factory>, transparent }` with `const fn` builders. Each `cmd/*.rs` exposes `pub fn specs()` appended in `command_specs()`.

Two paths, one implementation: **JSON** `Session::execute(id, params)` (control channel, MCP, CLI, scripts; `f64_req`/`size_param`/`point_param`/`targets`/`bad` validate hostile input and never clamp silently) and **interactive** `Session::start` → `Interactive` state machine (`prompt.rs`) fed by `input` / `input_snapped` / `cmdline` (AutoCAD coordinate grammar).

`Session::execute` and every interactive step run under `catch_unwind` (`guard.rs`); an escaped panic restores the drawing and returns `EngineError::Internal`.

### 0.4 Undo

`DocState { doc: Arc<Drawing>, undo/redo: Vec<Snapshot>, … }`; `Snapshot { label, doc, selection }`. Full-drawing snapshot stack, cheap via CoW; `undoctl.rs` implements UNDO marks / BEgin–End / Control.

> **Consequence:** anything that must undo, autosave, audit or round-trip **must live inside `Drawing`**. A side model in `Session`/`DocState` would silently lose undo.

### 0.5 Selection, grips, snaps

- Selection: `DocState.selection` + pickfirst/window/crossing (`select.rs`) + broad-phase index (`spatial.rs`).
- Grips: `EntityKind::grips()`, `grips.rs::stretch_grip` + `GripMode::{Stretch,Move,Rotate,Scale,Mirror}`; `Session::grip_edit` runs a **`grip.*` command** so drags are undoable and MCP-reachable.
- Snaps: `snap.rs` — OSMODE bits, `SnapHit`, `snap/{curves,iso,tracking,viewport}.rs` over `Drawing`+`Space` via the `Prim` abstraction.

### 0.6 Rendering — there was no 3D

`render` (L2) is pure 2D: `build(space) -> DisplayList` of polylines, filled triangles, masks and points tagged with the top-level `Handle`; `fill::triangulate_evenodd` handles holes; `raster.rs` is the CPU path. The GPU path (`ui-egui/src/gpu.rs`, L5) draws the display list through an `egui_wgpu::Callback` inside egui's own pass with `Stencil8` masking — **no depth buffer, no camera, no 3D vertex format, no lighting**. The 3D viewport was genuinely new work.

### 0.7 DXF custom data round-trip

`io/src/dxf_ext.rs`: our data travels as a registered app `CADCRAFT` in entity/table XDATA, and as XRECORDs in the named-object dictionary (`CADCRAFT_CONSTRAINTS`, `CADCRAFT_LAYERSTATES`, …). `dxf_region.rs` shows the **proxy-graphic pattern**: ordinary LWPOLYLINEs any reader understands, with `CADCRAFT` XDATA naming them as parts of one semantic object. Unknown entities/objects/keys are preserved verbatim except our own `CADCRAFT*` keys.

### 0.8 MCP

`mcp/src/tools.rs` exposes a **small generic tool set** (`command_line`, `script`, `execute`, `list_commands`, `inspect_drawing`, `query_entities`, `render`, `find_text`, `new_drawing`, `open`, `save`, `cancel`, plus UI-only tools). `execute` forwards to `Session::execute`; args are checked against the tool schema.

> **Adjustment to the brief:** no MCP tool per arch command. `WALL`, `DOOR`, `ROOFAUTO`, … are MCP-reachable the moment they are `CommandSpec`s. Only scene-level operations that cannot be a command get a tool (none added so far).

---

## Crate layout (as built)

Precedent copied: **`constraints` (L2) — plain data in `doc`, algorithms in a peer crate.**

| Crate | Layer | Contents | Depends on |
|---|---|---|---|
| `crates/doc/src/arch.rs` | L1 | `ArchModel { walls, openings, next_id, levels, types }`, `ArchId(u32)`, `Wall`, `Opening`, `WallType`/`WallLayer`, `RefSide`, `JoinKind`, `Level`; `ARCH_HANDLE_BASE = 0xA000_0000`; caps `MAX_WALLS = 4096`, `MAX_OPENINGS = 8192`, `MIN_WALL_THICKNESS = 1e-4`, `MIN_WALL_LENGTH = 1e-6`. Stored as `Drawing::arch` with `arch_revision` + `touch_arch()`. | `geom`, `color` |
| `crates/arch` | L2 | 2D behaviour, no rendering: `wall`, `graph`, `join` (`solve_joins`, `JoinOverrides`, `stretch_wall`), **`opening` (placement limits, `place_opening`, `auto_panel`, `clamp_recess`, `project_to_wall`)**, **`symbol` (plan symbols)**. Later: rooms, areas, straight skeleton, stairs, `ARCHDETECT`, catalogues. | `geom`, `color`, `doc` |
| `crates/arch3d` | L2 | `build`/`build_dirty` → `Scene3d`; `mesh` (ear clipping, `push_prism_with_holes`), `walls`, **`openings` (frame/casing/glass/leaf/sill/header)**. No `render` dependency. | `geom`, `color`, `doc`, `arch` |
| `crates/render` (extended) | L2 | `arch.rs`: arch plan geometry → `DisplayList`, tagged with the owning `ArchId`'s proxy handle; per-layer bands; **opening holes in the poché**. | `arch` |
| `crates/io` (extended) | L3 | `dxf_arch.rs` (`CADCRAFT_ARCH` XRECORD v2 + save-time proxy LWPOLYLINEs on `A-WALL`/`A-DOOR`/`A-GLAZ` with `CADCRAFT` XDATA), `glb.rs` (own GLTF 2.0 writer). Later: `obj.rs`, `ifc.rs`. | `arch`, `arch3d`, `dxf` |
| `crates/render3d` | L5 | `camera` (`Camera`, `Orbit`, `Orbit::fit`), `pipeline` (`GpuMeshes`, `upload`), `offscreen` (`Offscreen`, `headless_png`), `types` (`Mesh`, `Scene3d`, `Vertex`), `Render3dError::{NoAdapter,Wgpu,Buffer}`. Later: PBR, sun+CSM, SSAO, MSAA, display styles, GPU picking, ortho elevation/section cameras, walkthrough. | `geom`, `color`, `arch3d`, `wgpu` |
| `crates/engine/src/cmd/arch/*` | L4 | `wall`, `wall.stretch`, `inspect`, **`opening` (`door`, `window`, `garagedoor`, `arch.opening.move`)**, `build3d`, `arch.sample`, `view` (`view3d`, `camera`); arch proxy-handle grip routing; **`snap::arch`**. Later: `room`, `roomdetect`, `level`, `roofauto`, `roofplane`, `stair`, `railing`, `deck`, `cabinet`, `archdetect`, `walkthrough`, `elevation`, `section`, `export3d`. | `arch`, `arch3d`, `io` |
| `crates/ui-egui` | L5 | `pane3d.rs` (dockable pane, orbit, revision-gated rebuild, no-adapter placeholder); `ui.show3d`. Later: arch palettes, detection review, selection sync. | `render3d`, `arch3d` |
| `apps/cadcraft-cli` | exempt | `convert IN.dxf OUT.glb`, `run … --export`, `render FILE OUT.png --camera SPEC [--size WxH]`. | all |

`xtask/src/layers.rs::TABLE` has `arch` (L2), `arch3d` (L2), `render3d` (L5); `INTRA_LAYER_ORDER` has `[geom,color]`, `[fonts,render]`, `[arch,render]`, `[arch,arch3d]`, `[arch3d,render3d]`, `[render3d,ui-egui]`.

---

## Decisions

1. **The arch model lives in `Drawing`.** Undo is a whole-`Drawing` snapshot; autosave, AUDIT, RECOVER, plotting and DXF save read `Drawing`. Data in `doc` (L1), algorithms in `arch` (L2), like `constraints`.
2. **One source of truth: the arch object, not its linework.** Plan geometry is generated into the display list at draw time; DXF proxy entities are written **only on save** and not read back when arch data is present. Cost: `snap.rs`/`grips.rs` need arch branches (both now done).
3. **No 3D CSG.** Walls are cut by 2D boolean on the wall *elevation* (face region minus opening rectangles, ear-clipped with holes in `arch3d::mesh`) then extruded. Faster, robust, no geometry kernel. Exercised in phase 2.
4. **Pure Rust, no new heavy dependencies.** GLTF/GLB, OBJ and STEP/IFC get our own writers. Anything added must be MIT/Apache-2.0/BSD/Zlib and wasm-compatible; `plan/adr/0001` rejects GPL/LGPL/AGPL and C++ kernels.
5. **The 3D pane renders offscreen.** egui's pass has no depth buffer we control; rendering into our own colour+depth textures and presenting as an egui image leaves the canvas path untouched and works identically on WebGPU and WebGL2.
6. **MCP stays generic.** Commands are the API.
7. **Incremental rebuild is first-class**: `Drawing::arch_revision` + a dirty `ArchId` set, benchmarked from phase 1 (300-wall house < 50 ms).
8. **Never crash, always deterministic.** Iteration caps, `get()` not `[]`, checked arithmetic, no NaN casts, property/fuzz tests asserting no panic and manifold output.
9. **Additive to AutoCAD parity.** No AutoCAD command changes; arch commands are new ids under the `Architecture` menu section. `docs/gaps.md` has its own ARCH-1…ARCH-13 section; F24 stays as-is.
10. **Proxy handles, not entities.** Arch objects are not `Entity`s; plan geometry is tagged `Handle(0xA000_0000 + arch_id)` so picking/selection work without polluting the entity store or breaking COPY/MIRROR/ARRAY.
11. **Rough vs finished are separate fields** (Chief's rule). The 3D rough opening uses `rough`; the 2D symbol and the 3D frame/casing use finished. `ROUGH_ALLOWANCE` is one constant shared by commands, CLI and future detection.
12. **Placement limits are data in `arch`, not logic in commands.** So `door`/`window`/`garagedoor`, `cadcraft-cli`, MCP and `ARCHDETECT` (phase 8) all enforce the same numbers.

---

## Phase 1 — walls, joins, plan poché, first 3D (landed)

Segments: arch crate, arch3d, glb, render3d, cli, ui pane, tests+bench, docs, doc arch store, render arch hook, wall command + machine, arch inspect, build3d, wall defaults, arch poché, join types, wall graph, join solver (L/T/X), wall stretch, arch ids, arch types, wall fields, validation, layer bands, handles, workspace, stretch command, grip routing, XRECORD r/w, proxy graphics, arch layers, GLB spec layout, crate lints, solved-poché inspect/plan.

**Built.** `ArchModel` on `Drawing::arch` with `arch_revision`/`touch_arch()`; `Wall` with real wall-type build-ups (siding 0.02 / sheathing 0.015 / stud 0.09 / drywall 0.013; interior drywall/stud/drywall) and `Level 1` at 2.4 m. `solve_joins` trims/extends centre-line ends to neighbour face-line intersections (L, T, X, angled, collinear, parallel→square) with per-end `JoinKind` overrides; `stretch_wall` does capped transitive BFS. `render::arch` expands solved rings + per-layer bands into the display list at draw time. `arch3d::build`/`build_dirty` extrude wall elevations, meshes carry `object: ArchId` and world-space box UVs (u=x, v=z). `render3d` renders offscreen and presents as an image; `headless_png` returns `NoAdapter` without a GPU and the CLI maps that to exit 0 (SKIP). `io::glb` writes GLTF 2.0 with node names `wall-w1`. Commands `wall`, `arch.wall.stretch`, `arch.inspect`, `build3d`, `arch.sample`, `view3d`, `camera`; arch proxy handles route into `grips.rs`, so a wall drag runs `arch.wall.stretch` and is undoable/scriptable/MCP-reachable.

**Acceptance (all PASS).** `01_wall_command` · `02_join_solver` · `03_glb_extrusion` · `04_grip_stretch` · `05_plan_and_dxf_roundtrip` · `06_render3d_orbit` · `07_hostile_never_panics` · `08_layers_wasm_no_gpu_leak` · `09_incremental_bench`.

**Tests.** `arch/tests/property_walls.rs` (random graphs: no panic, manifold rings); `arch3d/tests/golden_house.rs` + `fixtures/house_10x8.json` (golden JSON + GLB hash); `arch3d/tests/bench_rebuild.rs` (release-only: 300-wall full rebuild < 50 ms, one-wall `build_dirty` < 5 ms); join unit tests; hostile-input script.

**Phase-1 deferrals.** Main-layer (`WallLayer.is_main`) joins → phase 3+ (Chief joins *through* the structural layer; matters once roofs/floors land). Pony walls (`split_height`, `upper_type`) and `reversed` → later. Arc walls → later (`join_start`/`join_end` already exist, so arc support is additive).

---

## Phase 2 — openings (landed)

Segments: doc-opening-model, arch-placement, arch-plan-symbol, render-openings, arch3d-components, glb-openings, dxf-openings, cmd-inspect-openings, grip-openings, snap-arch, arch-sample-openings, i18n-uk, golden-openings, p2a-holed-prism, p2b-wall-elevation-cut, p2c-scene-openings, uk-arch-labels, opening-commands, opening-move, opening-machines, doc-opening-ids, arch-fit-and-sill, engine-opening-params, engine-inspect-openings, arch3d-opening-parts, io-opening-roundtrip, engine-arch-handles, arch3d-build-counts, open-style-defaults, fit-slide-first, inspect-kind-labels, openings-envelope, openings-perf, sample-honours-openings.

**Model (`doc`, L1).** `Opening` in `ArchModel.openings`, sharing `next_id` with walls — openings undo, autosave, audit and round-trip with the drawing. Host `ArchId`, `offset` from the wall start, finished `width`/`height`/`sill`, separate **rough** dimensions, `recess` into the build-up, `style`, `hinge`/`swing`, frame/casing/sill sizes, `mull_group` reserved. `MAX_OPENINGS = 8192` bounds every loop. Proxy handles `a000…` for picking.

**Placement (`arch::opening`).** Chief's rules as shared constants: `MIN_HINGED_WIDTH 0.3`, `MIN_GARAGE_WIDTH 1.8`, `MIN_WINDOW_SEGMENT 0.15`, `MIN_BAY_SEGMENT 0.725`, `MIN_SEPARATION 0.05`, `ROUGH_ALLOWANCE 0.12`. `validate_placement` / `place_opening(model, o, fit)` reject or auto-resize; `auto_panel` doubles hinged ≥1.2 m and bifold >0.9 m; `clamp_recess` keeps the unit inside the wall; `project_to_wall` maps a click to an offset; `spans_overlap` enforces separation.

**Plan symbols (`arch::symbol` + `render::arch`).** Generated, never drawn: door leaf + swing arc, sliding panels, window glass lines, casing. The wall poché is cut by even-odd holes at draw time — no duplicate linework. Proxy polylines on `A-DOOR`/`A-GLAZ` with `CADCRAFT` XDATA are written on save and dropped on read while `CADCRAFT_ARCH` (v2, accepts v1) is present.

**3D (`arch3d`).** No CSG. `mesh::push_prism_with_holes` triangulates the wall elevation (region minus opening rectangles) and extrudes — decision 3 exercised. `openings::opening_meshes` emits frame, casing, glass, leaf, sill and header prisms, merged per material into one `Mesh` with `object: ArchId`; material names become GLB node names (`door-frame-o0`, `window-glass-o2`); glass uses a `BLEND` material. Components stay inside the host wall band and below the wall height (unit-tested). `Scene3d.openings_built` makes the cut count observable from `build3d`.

**Interaction.** `door` / `window` / `garagedoor` (interactive machine: pick a host wall, then offset or `Width`/`Height`/`Sill`/`Hinge`/`Swing` keywords — one implementation shared with the JSON path, one undo step) and `arch.opening.move`. Opening grips run `arch.opening.move`, so dragging is undoable, scriptable and MCP-reachable. Offsets are measured from the wall start, so stretching the host wall keeps the opening in place. `snap::arch` adds wall faces, wall ends and opening edges — the item carried over from phase 1.

**Acceptance (all PASS).** `01_opening_commands` (6 openings, unique ids, rough > finished, hinge/swing set) · `02_placement_limits` · `03_opening_glb_parts` (node names + blended glass) · `04_drag_along_wall` (2D offset moves, 3D rebuilds, past-the-end refused, undo restores, opening follows a stretched host wall) · `05_plan_symbols` (generated symbols + SVG growth) · `06_dxf_opening_roundtrip` (idempotent) · `07_hostile_openings` · `08_layers_wasm_i18n` · `09_opening_rebuild_bench` (300 walls + 150 openings < 50 ms).

**Phase-2 deferrals from `CHIEF-REFERENCE.md` §2/§3 Phase 2.** Bay/box/bow geometry beyond the plan symbol (defaults recorded: bay 2'-2"/4'-2"/1'-0"/45°, box 4'-2"/1'-6"/90°, bow 5 sections / 3'-9" radius centre / 11.5"); mulled units sharing one casing and label; per-face interior/exterior jamb and casing profiles with `reveal`; hardware hinge counts (interior 2, exterior 3); display states `swing_angle`/`percent_open`/`glass_mode`; window levels + Select-Next-Object; opening schedules; pocket-door pockets cut in the wall; openings on arc walls; automatic exterior/interior door classification (needs rooms → phase 3).

**Doc fix before phase 3:** `docs/gaps.md` ARCH-7 must flip from `open — phase 2` to `done — phase 2`.

---

## Open items / known gaps vs Chief Architect

Rooms/floors/ceilings/levels (ARCH-8); roofs (ARCH-9); stairs/railings/decks (ARCH-10); PBR, sun and shadows, SSAO, display styles, walkthrough, GPU picking synced with 2D (phase 6); HLR elevations and sections (ARCH-11); `ARCHDETECT` (ARCH-12); OBJ and IFC (ARCH-13); arc walls and openings on them; layered reference-side (main-layer) joins; pony walls and `reversed`; wall-top clipping to rafters; framing and structural members; terrain and site contours; electrical/plumbing/HVAC; layout sheets beyond generated elevation sheets; ray-traced "final views"; dormers; curtain walls; dynamic per-edit roof topology; IFC4 alignment beyond the six entity classes; real-file corpus validation of `ARCHDETECT` precision/recall; opening sub-items listed above.

## Phase plan

0 survey ✅ → 1 walls+joins+basic extruded 3D ✅ → 2 openings ✅ → **3 rooms/floors/levels** → 4 automatic roofs → 5 stairs/railings/decks → 6 the 3D view properly → 7 generated elevations and sections → 8 `ARCHDETECT` on plain DXF → 9 glTF/OBJ/IFC export → 10 polish, materials, benchmarks, `docs/arch3d/USER-GUIDE.md`.

## Phase 3 plan (next)

Rooms, floors, ceilings and levels (ARCH-8). `Room` detection from enclosed wall faces including invisible room-divider walls, with holes (courtyards, stair wells) and a point-in-polygon face assignment; area/perimeter rules from `CHIEF-REFERENCE.md` §3 Phase 3 — **Interior** (inner surfaces, includes bay/box/bow), **Standard** (centre of interior walls, outside surface / Main Layer of exterior walls, excludes bay/box/bow), **Living** (sum of Standard for interior room types, excludes Garage/Deck/Porch/Open Below/Attic and rooms with rough ceiling < 1.2 m). Room labels with areas in the display list; floor and ceiling slabs from room polygons with stair-well holes; `Level` gains elevation, floor-to-floor, ceiling height and floor-structure thickness, plus a basement and a second floor; room-divider walls; `room`/`roomdetect`/`level` commands; main-layer (`is_main`) joins so area bases match Chief; door exterior/interior classification becomes available once rooms exist. `uk.tsv` rows in the same segment.
