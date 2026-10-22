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
> output (`build3d`, `render3d`, `detect_walls`).

---

## Proposed crate layout (adjusted to what was found)

The precedent to copy is **`constraints` (L2): plain data in `doc`, algorithms in a peer crate.**

| Crate | Layer | Contents | Depends on |
|---|---|---|---|
| `crates/doc/src/arch.rs` (+ `arch/`) | L1 | **The arch object model as data**: `ArchModel`, `ArchId`, `Wall`, `WallType`/layers, `Opening`, `Room`, `Level`, `RoofSettings`/`RoofPlane`, `Stair`, `Railing`, `Deck`, `Cabinet`, `Fixture`, `Material`, `Camera3d`. Serde, defaults, validation. Stored as `Drawing::arch` with `arch_revision: u64`. | `geom`, `color` |
| `crates/arch` | L2 (peer of `constraints`) | **All 2D behaviour, no rendering**: wall graph + join solver (L/T/X/angled/collinear, mitre/butt/layer cleanup, neighbour stretch), poché/plan polygons, opening placement limits, room detection (reuses `geom::region::{boundary_at,enclosing_loop,interior_point}`), area/perimeter rules, **straight-skeleton roof generator**, stair code calculator, plan-symbol generation, `ARCHDETECT` (parallel-pair wall finding, door arcs, window mullions, AIA/NCS layer hints), material/door/window/stair defaults, imperial+metric templates. | `geom`, `color`, `doc` |
| `crates/arch3d` | L2 | `fn build(&ArchModel) -> Scene3d`: deterministic, **incremental** (dirty `ArchId` set from `arch_revision`). Wall elevation as 2D region minus opening rectangles → extrude (no 3D CSG); frames/glass/sash/leaf/casing/sill; floors and ceilings from room polygons with stair-well holes; roof planes with thickness, fascia, soffit, overhang; wall-top clipping to rafters; stairs, decks, fences, cabinets. World-space box UVs, normals, per-object `ArchId` for picking. Also `hlr.rs`: hidden-line removal of our own meshes → 2D vectors for elevations/sections. | `geom`, `color`, `doc`, `arch` |
| `crates/render` (extend) | L2 | `arch.rs`: turn `ArchModel` plan geometry into `DisplayList` prims tagged with the owning `ArchId`'s proxy handle, so linetypes, colours, plot styles and picking all work. Needs `INTRA_LAYER_ORDER += ["arch", "render"]`. | `arch` |
| `crates/io` (extend) | L3 | `dxf_arch.rs` (XDATA `CADCRAFT` + `CADCRAFT_ARCH` XRECORD + proxy graphics), `glb.rs` (own GLTF 2.0 writer, no dependency), `obj.rs`, `ifc.rs` (own STEP writer for `IfcWall/IfcWindow/IfcSlab/IfcRoof/IfcStair/IfcSpace`). | `arch`, `arch3d`, `dxf` |
| `crates/render3d` | L5 | wgpu viewport: cameras (orbit, full camera, orthographic elevation/section, walkthrough), PBR + date/time/latitude sun + cascaded shadow maps + SSAO + MSAA, display styles (shaded, shaded-with-edges, hidden-line, x-ray), edge outlines, GPU picking. **Renders offscreen into its own colour+depth targets and hands back a texture** (avoids fighting egui's render pass; works on WebGL2). Headless offscreen renderer for the CLI. | `geom`, `color`, `arch3d`, `wgpu` |
| `crates/engine/src/cmd/arch/*` | L4 | `specs()` for `wall`, `door`, `window`, `garagedoor`, `room`, `roomdetect`, `level`, `roofauto`, `roofplane`, `stair`, `railing`, `deck`, `cabinet`, `archdetect`, `view3d`, `camera`, `walkthrough`, `elevation`, `section`, `export3d`. Plus arch-aware snapping and arch grips (wall grips that stretch neighbours through the join solver). | `arch`, `arch3d`, `io` |
| `crates/ui-egui` | L5 | 3D pane (an `egui_wgpu` callback showing the offscreen texture), arch tool palettes, arch detection review UI, selection sync between panes. `INTRA_LAYER_ORDER += ["render3d", "ui-egui"]`. | `render3d`, `arch3d` |
| `apps/cadcraft-cli` | exempt | `convert plan.dxf out.glb` (runs `archdetect` when the drawing has no arch objects), `render plan.dxf out.png --camera …` headless. | all |

`xtask/src/layers.rs::TABLE` must gain `arch` (L2), `arch3d` (L2) and `render3d` (L5), and the
`INTRA_LAYER_ORDER` chains above, in the same PR that adds each crate.

---

## Decisions

1. **The arch model lives in `Drawing`.** Undo is a whole-`Drawing` snapshot; autosave, AUDIT,
   RECOVER, plotting and DXF save all read `Drawing`. A model held in `Session` would not undo.
   Data in `doc` (L1), algorithms in `arch` (L2), exactly like `constraints`.
2. **One source of truth: the arch object, not its linework.** Plan geometry is generated at draw
   time into the display list (`render::arch`), and DXF proxy entities are written **only on save**
   and not read back when arch data is present. Cost: `snap.rs` and `grips.rs` need arch branches.
   This is Chief's behaviour and avoids stale duplicate geometry.
3. **No 3D CSG.** Walls are cut by 2D boolean on the wall *elevation* (face region minus opening
   rectangles, reusing `render::fill::triangulate_evenodd`) then extruded. Faster, robust, and
   keeps us out of needing a geometry kernel.
4. **Pure Rust, no new heavy dependencies.** GLTF/GLB, OBJ and STEP/IFC get our own writers
   (they are plain text/binary containers). Anything added must be MIT/Apache-2.0/BSD/Zlib and
   wasm-compatible; `plan/adr/0001` rejects GPL/LGPL/AGPL and C++ kernels outright.
5. **The 3D pane renders offscreen.** egui's render pass has no depth buffer we control; rendering
   into our own colour+depth textures and presenting the result as an egui image keeps the existing
   canvas path untouched and works identically on WebGPU and WebGL2.
6. **MCP stays generic.** Commands are the API. Only scene-level operations that cannot be
   expressed as a command get a tool.
7. **Incremental rebuild is a first-class requirement**, not a later optimisation: `Drawing::arch_revision`
   plus a dirty `ArchId` set, with a benchmark harness from phase 1 (target: 300-wall house < 50 ms).
8. **Never crash, always deterministic.** The straight skeleton, join solver and detection code take
   hostile input (random DXF linework). Iteration caps, `get()` not `[]`, checked arithmetic, no NaN
   casts, and property/fuzz tests that assert no panic and manifold output.
9. **Additive to AutoCAD parity.** No AutoCAD command changes; arch commands are new ids with their
   own menu section. `docs/gaps.md` gets a new "Architecture (arch3d)" section; F24 stays as-is.

## Open items / known gaps vs Chief Architect

Framing and structural members; terrain and site contours; electrical/plumbing/HVAC; layout sheets
beyond the generated elevation sheets; ray-traced "final views"; dormers; curtain walls; dynamic
per-edit roof topology changes; IFC4 alignment beyond the six entity classes; real-file corpus
validation of `ARCHDETECT` precision/recall.

## Phase plan

0 survey (this file) → 1 walls+joins+basic extruded 3D → 2 openings → 3 rooms/floors/levels →
4 automatic roofs → 5 stairs/railings/decks → 6 the 3D view properly → 7 generated elevations and
sections → 8 `ARCHDETECT` on plain DXF → 9 glTF/OBJ/IFC export → 10 polish, materials, benchmarks,
`docs/arch3d/USER-GUIDE.md`.
