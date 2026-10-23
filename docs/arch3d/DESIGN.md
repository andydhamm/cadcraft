# arch3d — design log

Branch `arch3d` (fork of `storytold/cadcraft`, v0.10.0). This file is the running design log for the
"draw a plan, get the house in 3D" feature. Phases land one commit each, each ending at a ⛔ checkpoint.

---

## Phase 0 — survey

### 0.1 The repo at a glance

- Workspace: `crates/*`, `apps/*`, `xtask`. Edition 2024, `rust-version = 1.90`, licence `MIT OR Apache-2.0`.
- Workspace lints: `unsafe_code = "forbid"`; every production crate root carries
  `#![deny(clippy::unwrap_used, expect_used, panic, unimplemented, todo, unreachable)]`.
- Layering is enforced by `cargo xtask layers` (`xtask/src/layers.rs::TABLE`):
  L0 `geom`,`color` (`dxf`,`dwg` standalone) → L1 `doc` → L2 `constraints`,`fonts`,`render` →
  L3 `io` → L4 `engine` → L5 `ui-egui`,`mcp` → apps (exempt). `UI_CRATES`
  (`egui`,`eframe`,`egui-wgpu`,`wgpu`,`winit`,`rfd`,…) may only be used from **L5 up**.
  `INTRA_LAYER_ORDER` lists the allowed same-layer chains.
- `plan/` (STATUS.md, execution-plan.md, ADRs) is gitignored — it does not exist in a fresh clone.
- Roadmap position: pre-alpha, ~38% ready; **"3D modelling and rendering" is 2%, 300–450 h**
  (`ROADMAP.md`), and `docs/gaps.md` **F24** is the row: "No 3D: UCS, 3D views, orbit, solids,
  surfaces, meshes, sections, visual styles, materials, lights, rendering (150 menu items)".
  F24 is *AutoCAD* 3D. Our architectural 3D is additive and must not be conflated with it; we add
  our own gaps row(s) and leave F24 alone.

### 0.2 Entities

`crates/doc/src/entity.rs`: `Entity { handle: Handle(u64), common: Common, kind: EntityKind }`.
`Common` = layer, colour, linetype, lineweight, ltscale, transparency, visible, thickness,
extrusion, plot style, annotative scales, and **`xdata: Vec<RawTag>`** (foreign extended data is
part of the object, so COPY/MIRROR/ARRAY carry it). `EntityKind` is a closed enum (Line, Circle,
Arc, LwPolyline, Spline, Text, MText, Insert, Dimension, MLine, Hatch, Viewport, Table, …) plus
`Unknown { dxf_type, tags }` for anything we do not model.

`crates/doc/src/store.rs`: `EntityStore` is a **copy-on-write chunked store** (chunks of 1024
`Arc<Entity>` + 64 sharded handle maps) in draw order. Cloning a store (an undo snapshot) copies
pointers only; an edit copies one chunk and one shard. This is why whole-drawing undo is cheap.

`Drawing` holds model/paper spaces, tables (layers, linetypes, styles, dimstyles, block records),
layouts, and `foreign: ForeignData` (other programs' objects, extension dictionaries, xdata,
reactors, colour books — `crates/doc/src/foreign.rs`).

### 0.3 Commands

Everything user-visible is a `CommandSpec` (`crates/engine/src/cmd/mod.rs`):
`{ id (AutoCAD name, lower case), label, menu path, shortcut, aliases, params doc, enabled,
run: fn(&mut Session,&Value)->Result<Value>, undoable, interactive: Option<Factory>,
transparent }` with `const fn` builders (`.menu() .key() .alias() .params() .interactive() .noundo()`).
Each `cmd/*.rs` module exposes `pub fn specs() -> Vec<CommandSpec>` and is appended in
`command_specs()`; `find_command` / `resolve_alias` resolve typed names.

Two execution paths, one implementation:
- **JSON form**: `Session::execute(id, params)` — no dialogs, used by control channel, MCP, CLI,
  scripts. Param helpers (`f64_req`, `size_param`, `point_param`, `targets`, `bad`) validate
  hostile input and never clamp silently.
- **Interactive form**: `Session::start` → `Interactive` state machine (`crates/engine/src/prompt.rs`)
  returning `Prompt { message, keywords, default, accept: Accept, base, corner, grip, picker,
  distance }`, fed by `Session::input(Input)` / `input_snapped(SnapHit)` / `cmdline(text)`
  (which parses the AutoCAD coordinate grammar: `x,y`, `@dx,dy`, `@d<a`, direct distance entry).

`Session::execute` and every interactive step run under `catch_unwind` (`crates/engine/src/guard.rs`);
an escaped panic restores the drawing and returns `EngineError::Internal`.

### 0.4 Undo

`DocState { doc: Arc<Drawing>, undo: Vec<Snapshot>, redo: Vec<Snapshot>, undo_ctl, selection,
revision, space, tiles, views, … }`; `Snapshot { label, doc: Arc<Drawing>, selection }`. Undo is a
**full-drawing snapshot stack** made cheap by the CoW store; `undoctl.rs` implements UNDO marks /
BEgin–End groups / Control. `Session::undo`/`redo` swap the `Arc`.

> **Consequence for arch3d:** anything that must undo, autosave, audit or round-trip **must live
> inside `Drawing`**. A side model held only in `Session`/`DocState` would silently lose undo.

### 0.5 Selection, grips, snaps

- Selection: `DocState.selection: Vec<Handle>` + `Session::selection()`; pickfirst/window/crossing
  logic in `crates/engine/src/select.rs`; a broad-phase index in `crates/engine/src/spatial.rs`.
- Grips: `EntityKind::grips() -> Vec<Vec2>`; `crates/engine/src/grips.rs` implements
  `stretch_grip(kind, index, to) -> Option<EntityKind>` and the multi-functional grip modes
  (`GripMode::{Stretch,Move,Rotate,Scale,Mirror}`). The UI calls `Session::grip_edit`, which runs a
  **`grip.*` command** so grip drags are undoable and reachable from scripts/MCP.
  `grip_dims.rs` feeds Dynamic Input dimensions.
- Snaps: `crates/engine/src/snap.rs` — OSMODE bits, `SnapHit { point, mode, name, viewport,
  deferred }`, plus `snap/{curves,iso,tracking,viewport}.rs`. Snaps work over `Drawing` + `Space`
  through the `Prim` abstraction, with deferred tangent/perpendicular resolution.

### 0.6 Rendering — there is no 3D

`crates/render` (L2) is **pure 2D**: `build(space) -> DisplayList` of world-space polylines,
filled triangles, masks and points, each tagged with its top-level `Handle` and resolved colour
(`DPrim`). Linetypes, hatches, text, block expansion, dimensions, mlines and plot styles are
expanded here. `fill::triangulate_evenodd` triangulates polygons with holes. `raster.rs` draws the
same list on the CPU (PNG export, plot preview, image tests).

The GPU path is `crates/ui-egui/src/gpu.rs` (L5): the display list is turned into instanced vertex
buffers and drawn through an **`egui_wgpu::Callback` paint callback inside egui's own render pass**,
with a `Stencil8` buffer used for wipeout masking. Pipelines are line/tri/quad/mask; there is
**no depth buffer, no camera, no 3D vertex format, no lighting**. CPU fallback exists when no wgpu
render state is installed. So the 3D viewport is genuinely new work.

### 0.7 DXF custom data round-trip

`crates/io/src/dxf_ext.rs` is the pattern: CADCraft's own data travels as
- a **registered application `CADCRAFT`** in entity/table XDATA (`1001 CADCRAFT`, then `1000 <tag>`
  and typed groups) — used for dimension associativity, gradients, hatch origin, style fonts, …;
- **XRECORDs in the named-object dictionary**: `CADCRAFT_CONSTRAINTS`, `CADCRAFT_LAYERSTATES`,
  `CADCRAFT_PLOTSTYLES`, `CADCRAFT_ANNOTATION`, `CADCRAFT_HANDSEED`, `CADCRAFT_MLEADERSTYLE`;
- `crates/io/src/dxf_region.rs` shows the **proxy-graphic pattern**: write ordinary LWPOLYLINEs that
  any reader understands, and attach `CADCRAFT` XDATA naming them as parts of one semantic object.

Unknown entities/objects/keys are preserved verbatim (`dxf_raw.rs`, `EntityKind::Unknown`,
`ForeignData`), except our own `CADCRAFT*` keys. This is exactly the mechanism arch objects need.

### 0.8 MCP

`crates/mcp/src/tools.rs` deliberately exposes a **small generic tool set**, not one tool per
command: `command_line`, `script`, `execute`, `list_commands`, `inspect_drawing`, `query_entities`,
`render`, `find_text`, `new_drawing`, `open`, `save`, `cancel`, plus UI-only `screenshot`,
`ui_inspect`, `ui_click`. `execute` forwards to the control method `engine.execute`, i.e. to
`Session::execute`. Arguments are checked against the tool's JSON schema (`check_args`).
`crates/mcp/src/backend.rs` documents the method list; `cadcraft-cli mcp` serves it.

> **Adjustment to the brief:** we do **not** add one MCP tool per arch command. `WALL`, `DOOR`,
> `ROOFAUTO`, … become MCP-reachable the moment they are `CommandSpec`s (via `execute` /
> `list_commands` / `command_line`). We add only 2–3 genuinely new tools that need scene-level
> output (`build3d`, `render3d`, `detect_walls`) — and only if a command cannot express them.

---

## Crate layout (as built)

The precedent copied is **`constraints` (L2): plain data in `doc`, algorithms in a peer crate.**

| Crate | Layer | Contents (phase 1 state) | Depends on |
|---|---|---|---|
| `crates/doc/src/arch.rs` | L1 | **The arch object model as data**: `ArchModel { walls, next_id, levels, types }`, `ArchId(u32)`, `Wall`, `WallType`/`WallLayer`, `RefSide`, `JoinKind`, `Level`; `ARCH_HANDLE_BASE = 0xA000_0000` + `arch_handle`/`is_arch_handle`; caps `MAX_WALLS = 4096`, `MIN_WALL_THICKNESS = 1e-4`, `MIN_WALL_LENGTH = 1e-6`. Stored as `Drawing::arch` with `arch_revision: u64` and `touch_arch()`. Serde, defaults, validation. | `geom`, `color` |
| `crates/arch` | L2 (peer of `constraints`) | **All 2D behaviour, no rendering**: `wall` (offsets, polygon, bounds, `wall_layer_polygons`), `graph` (wall graph), `join` (`solve_joins`/`solve_joins_with` + `JoinOverrides`, `face_lines`, `ring_from_corners`, `stretch_wall`). Later: opening placement limits, room detection, area rules, straight-skeleton roof generator, stair calculator, plan symbols, `ARCHDETECT`, catalogues. Re-exports the `doc` model types. | `geom`, `color`, `doc` |
| `crates/arch3d` | L2 | `build(&ArchModel) -> Scene3d`, `build_dirty(&ArchModel, &BTreeSet<ArchId>, &Scene3d)`; `mesh` (ear clipping), `walls` (extrusion). Later: openings, floors/ceilings, roofs + wall-top clipping, stairs/decks/cabinets, `hlr.rs`. | `geom`, `color`, `doc`, `arch` |
| `crates/render` (extended) | L2 | `arch.rs`: arch plan geometry → `DisplayList` prims tagged with the owning `ArchId`'s proxy handle, including per-layer bands. | `arch` |
| `crates/io` (extended) | L3 | `dxf_arch.rs` (`CADCRAFT_ARCH` XRECORD + save-time proxy LWPOLYLINEs with `CADCRAFT` XDATA), `glb.rs` (own GLTF 2.0 writer). Later: `obj.rs`, `ifc.rs` (own STEP writer). | `arch`, `arch3d`, `dxf` |
| `crates/render3d` | L5 | `camera` (`Camera`, `Orbit`, `Orbit::fit`), `pipeline` (`GpuMeshes`, `upload`), `offscreen` (`Offscreen`, `headless_png`), `types` (`Mesh`, `Scene3d`, `Vertex`), `Render3dError::{NoAdapter,Wgpu,Buffer}`. Later: PBR + sun + CSM + SSAO + MSAA, display styles, GPU picking, orthographic elevation/section cameras, walkthrough. | `geom`, `color`, `arch3d`, `wgpu` |
| `crates/engine/src/cmd/arch/*` | L4 | `wall`, `arch.wall.stretch`, `arch.inspect`, `build3d`, `arch.sample`, `view3d`, `camera`; arch proxy-handle grip routing. Later: `door`, `window`, `garagedoor`, `room`, `roomdetect`, `level`, `roofauto`, `roofplane`, `stair`, `railing`, `deck`, `cabinet`, `archdetect`, `walkthrough`, `elevation`, `section`, `export3d`, plus arch-aware snapping. | `arch`, `arch3d`, `io` |
| `crates/ui-egui` | L5 | `pane3d.rs` (dockable 3D pane, orbit camera, revision-gated rebuild, no-adapter placeholder); `ui.show3d` menu item. Later: arch palettes, detection review UI, selection sync. | `render3d`, `arch3d` |
| `apps/cadcraft-cli` | exempt | `convert IN.dxf OUT.glb`, `run … --export OUT.glb`, `render FILE OUT.png --camera SPEC [--size WxH]`. Later: `ARCHDETECT` on import, IFC/OBJ. | all |

`xtask/src/layers.rs::TABLE` now has `arch` (L2), `arch3d` (L2), `render3d` (L5), and
`INTRA_LAYER_ORDER` has `[geom,color]`, `[fonts,render]`, `[arch,render]`, `[arch,arch3d]`,
`[arch3d,render3d]`, `[render3d,ui-egui]`.

---

## Decisions

1. **The arch model lives in `Drawing`.** Undo is a whole-`Drawing` snapshot; autosave, AUDIT,
   RECOVER, plotting and DXF save all read `Drawing`. A model held in `Session` would not undo.
   Data in `doc` (L1), algorithms in `arch` (L2), exactly like `constraints`.
2. **One source of truth: the arch object, not its linework.** Plan geometry is generated at draw
   time into the display list (`render::arch`), and DXF proxy entities are written **only on save**
   and not read back when arch data is present. Cost: `snap.rs` and `grips.rs` need arch branches
   (grips done in phase 1; **snapping to wall faces/ends is still open**). This is Chief's behaviour
   and avoids stale duplicate geometry.
3. **No 3D CSG.** Walls are cut by 2D boolean on the wall *elevation* (face region minus opening
   rectangles, reusing `render::fill::triangulate_evenodd`'s algorithm, reimplemented in
   `arch3d::mesh` so `arch3d` does not depend on `render`) then extruded. Faster, robust, and keeps
   us out of needing a geometry kernel.
4. **Pure Rust, no new heavy dependencies.** GLTF/GLB, OBJ and STEP/IFC get our own writers
   (they are plain text/binary containers). Anything added must be MIT/Apache-2.0/BSD/Zlib and
   wasm-compatible; `plan/adr/0001` rejects GPL/LGPL/AGPL and C++ kernels outright.
5. **The 3D pane renders offscreen.** egui's render pass has no depth buffer we control; rendering
   into our own colour+depth textures and presenting the result as an egui image keeps the existing
   canvas path untouched and works identically on WebGPU and WebGL2.
6. **MCP stays generic.** Commands are the API. Only scene-level operations that cannot be
   expressed as a command get a tool. Phase 1 added none.
7. **Incremental rebuild is a first-class requirement**, not a later optimisation: `Drawing::arch_revision`
   plus a dirty `ArchId` set, with a benchmark harness from phase 1 (target: 300-wall house < 50 ms).
8. **Never crash, always deterministic.** The straight skeleton, join solver and detection code take
   hostile input (random DXF linework). Iteration caps, `get()` not `[]`, checked arithmetic, no NaN
   casts, and property/fuzz tests that assert no panic and manifold output.
9. **Additive to AutoCAD parity.** No AutoCAD command changes; arch commands are new ids under their
   own `Architecture` menu section. `docs/gaps.md` has a new "Architecture (arch3d)" section
   (ARCH-1…ARCH-13); F24 stays as-is.
10. **Proxy handles, not entities.** Arch objects are not `Entity`s. Plan geometry is tagged
    `Handle(0xA000_0000 + arch_id)` so picking/selection work without polluting the entity store or
    breaking COPY/MIRROR/ARRAY semantics that assume entities.

## Phase 1 — walls, joins, plan poché, first 3D (landed)

Merged segments: arch crate, arch3d, glb, render3d, cli, ui pane, tests+bench, docs, doc arch store,
render arch hook, wall command core + machine, arch inspect, build3d cmd, wall defaults, arch poché
list, join types, wall graph, join solver (L/T, X), wall stretch, arch ids, arch types, wall fields,
validation, layer bands, handles, workspace, stretch command, grip routing, XRECORD read/write,
proxy graphics, arch layers, GLB spec layout, crate lints + wall close, solved-poché inspect/plan.

### What was built

- **Model (`doc`, L1).** `ArchModel { walls, next_id, levels, types }` on `Drawing::arch` with
  `arch_revision: u64` + `touch_arch()`. `Wall { start, end, id, base_level, to_ceiling, exterior,
  wall_type, join_start, join_end, thickness, height, layers }`; `JoinKind = Auto | Mitre | Butt |
  Square`; `RefSide`; `WallLayer`/`WallType` with real build-ups (exterior siding 0.02 / sheathing
  0.015 / stud 0.09 / drywall 0.013; interior drywall / stud / drywall); `Level 1` default at 2.4 m
  floor-to-floor. Proxy identity `ARCH_HANDLE_BASE = 0xA000_0000 + arch_id`.
- **Joins (`arch`).** `solve_joins(&ArchModel) -> Vec<WallShape>`: one deterministic pass plus a
  spatial hash of endpoints — trim/extend centre-line ends to neighbour face-line intersections for
  L, T, X, angled, collinear and parallel (→ square), with per-end `JoinKind` override and
  `JoinOverrides`. `stretch_wall(model, id, end, to) -> Vec<ArchId>`: transitive BFS over shared
  endpoints, capped at `MAX_WALLS`, so dragging one wall stretches its graph.
- **Plan poché (`render::arch`).** Solved wall rings plus per-layer bands expanded into the display
  list at draw time, tagged with the proxy handle, so linetypes, colours, plot styles and picking
  work unchanged. AIA/NCS layers (`A-WALL`, `A-DOOR`, `A-GLAZ`, `A-ROOF`, `A-STAIR`) added to new
  drawings.
- **3D (`arch3d`).** `build` / `build_dirty` → `Scene3d`; wall elevation extruded, no CSG; meshes
  carry `object: ArchId` for picking and world-space box UVs (u=x, v=z); ear-clipping in
  `arch3d::mesh`. `render` is deliberately **not** a dependency of `arch3d` (both are L2; `render`
  pulls fonts/text machinery the 3D path must not carry).
- **Viewport (`render3d`, L5).** Offscreen colour+depth targets, orbit camera with `Orbit::fit`,
  presented as an image; `headless_png` returns `NoAdapter` without a GPU and the CLI maps that to
  exit 0 (SKIP) so GPU-less CI stays green. `ui-egui::pane3d` rebuilds only when `arch_revision`
  advances and shows a placeholder instead of a blank pane when there is no adapter.
- **Persistence (`io`).** `CADCRAFT_ARCH` XRECORD (version-tagged JSON, chunked like
  `CADCRAFT_CONSTRAINTS`) plus save-time proxy LWPOLYLINEs on `A-WALL` with `CADCRAFT` XDATA, never
  read back when the XRECORD is present. Own GLTF 2.0 writer in `io::glb` (node names `wall-w1`,
  POSITION min/max on every accessor).
- **Commands.** `wall` (Architecture ▸ Wall, alias `wal`, interactive machine with
  `Type`/`Thickness`/`Height`/`Close` keywords delegating to the JSON path — one implementation,
  one undo step), `arch.wall.stretch`, `arch.inspect`, `build3d`, `arch.sample`, `view3d`, `camera`.
  Arch proxy handles route into `grips.rs`/`gripcmds.rs`, so a wall grip drag runs
  `arch.wall.stretch` and is undoable, scriptable and MCP-reachable.
- **CLI.** `convert IN.dxf OUT.glb`, `run … --export OUT.glb`,
  `render FILE OUT.png --camera 'orbit,azimuth=…,elevation=…,distance=…,fov=…' [--size WxH]`.

### Acceptance (all PASS)

`01_wall_command` · `02_join_solver` · `03_glb_extrusion` · `04_grip_stretch` ·
`05_plan_and_dxf_roundtrip` · `06_render3d_orbit` · `07_hostile_never_panics` ·
`08_layers_wasm_no_gpu_leak` · `09_incremental_bench`.

### Tests

`arch/tests/property_walls.rs` (random wall graphs: never panic, rings stay manifold);
`arch3d/tests/golden_house.rs` + `fixtures/house_10x8.json` (golden model JSON + GLB hash);
`arch3d/tests/bench_rebuild.rs` (release-only: 300-wall grid full rebuild < 50 ms, one-wall
`build_dirty` < 5 ms); join unit tests (L, T, X, angled, collinear, parallel→square, overrides);
`07_hostile_never_panics.sh` drives malformed JSON and degenerate geometry through the commands.

### Phase 1 deltas and deferrals from `CHIEF-REFERENCE.md` §1/§3 Phase 1

- **Main Layer (`WallLayer.is_main`)** — deferred to phase 2. The solver still joins at the centre
  line; Chief joins "through" the main (structural) layer, which matters for bearing points once
  roofs and floors land. `RefSide` and `WallType.layers` are already in the data, so this is a
  solver change plus a data field, not a rework.
- **Pony walls (`split_height`, `upper_type`)** and the **`reversed` flag** — deferred to phase 2;
  both are pure data additions that do not change the join topology.
- **Arc walls** — deferred; phase 1 walls are centre-line segments only, and `join_start`/`join_end`
  already exist so arc support is additive.

### Verification item carried into phase 2

`crates/ui-egui/src/i18n/tests.rs::translations_cover_commands_menus_tools_and_snap_labels` requires
a `uk.tsv` row for every command label and menu path. Phase 1 introduced `Wall`, `Stretch Wall`,
`Build 3D`, `3D View`, `Arch Sample` and the `Architecture` menu group. The i18n test is **not**
covered by acceptance script 08 (which runs only `arch`/`arch3d`), so **run
`cargo test -p cadcraft-ui-egui i18n` first thing in phase 2** and add any missing rows before
further label churn.

## Open items / known gaps vs Chief Architect

Openings and generated plan symbols (ARCH-7); rooms/floors/ceilings/levels (ARCH-8); roofs (ARCH-9);
stairs/railings/decks (ARCH-10); PBR, sun and shadows, SSAO, display styles, walkthrough, GPU picking
synced with 2D (phase 6); HLR elevations and sections (ARCH-11); `ARCHDETECT` (ARCH-12); OBJ and IFC
(ARCH-13); arc walls; layered reference-side (main-layer) joins; pony walls and `reversed`; snapping
to wall faces/ends; wall-top clipping to rafters; framing and structural members; terrain and site
contours; electrical/plumbing/HVAC; layout sheets beyond generated elevation sheets; ray-traced
"final views"; dormers; curtain walls; dynamic per-edit roof topology changes; IFC4 alignment beyond
the six entity classes; real-file corpus validation of `ARCHDETECT` precision/recall.

## Phase plan

0 survey ✅ → 1 walls+joins+basic extruded 3D ✅ → 2 openings → 3 rooms/floors/levels →
4 automatic roofs → 5 stairs/railings/decks → 6 the 3D view properly → 7 generated elevations and
sections → 8 `ARCHDETECT` on plain DXF → 9 glTF/OBJ/IFC export → 10 polish, materials, benchmarks,
`docs/arch3d/USER-GUIDE.md`.

## Phase 2 plan (next)

Openings land as `Opening` data in `doc` (host wall id, offset along the centre line, width, height,
sill/header, style, swing side, frame/casing/sill sizes) with **rough vs finished** dimensions and
`recess_depth` from the Chief reference, placement limits validated in `arch`, generated 2D plan
symbols in `render::arch`, and 3D cutouts by 2D boolean on the wall elevation plus frame/glass/sash/
leaf/casing meshes in `arch3d`. Commands `door`, `window`, `garagedoor` (click a wall to place, grips
slide along it), arch-aware snapping to wall faces and ends, and the `uk.tsv` rows for every new
label and menu path in the same segment.
