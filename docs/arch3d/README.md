# arch3d — architectural 3D

"Draw a plan, get the house in 3D." The arch object model lives on `Drawing`, so it
undoes, autosaves, audits and round-trips with everything else. F24 in
[docs/gaps.md](../gaps.md) (AutoCAD 3D: UCS, orbit, solids, 150 menu items) is a
separate concern and is left alone. See [DESIGN.md](DESIGN.md) for the design log
and `docs/gaps.md` (Architecture section) for the phase-by-phase gap list.

## What exists after phase 1

- **Arch object model** (`doc/src/arch.rs`): `ArchModel`, `ArchId`, `Wall`
  (centre-line `start`/`end`, `thickness`, `height`, `base_level`, `to_ceiling`,
  `exterior`, `wall_type`, `join_start`/`join_end`) stored as `Drawing::arch` with
  `arch_revision: u64`; `Drawing::touch_arch()` bumps the revision so the 3D pane
  and `build_dirty` key off it. Undo, autosave, AUDIT and DXF save all see it.
- **Wall joins** (`arch`): `solve_joins(&ArchModel) -> Vec<WallShape>` trims/extends
  centre-line ends to neighbour face-line intersections (L, T, X, angled, collinear;
  mitre/butt, parallel→square), then offsets — pure, deterministic, one pass +
  spatial hash of endpoints. `stretch_wall(model, id, end, to)` gives transitive
  neighbour stretch (BFS over shared endpoints, capped) so wall grip drags stretch
  the whole graph.
- **Plan poché** (`render/src/arch.rs`): arch geometry is expanded into the 2D
  display list at draw time, tagged with proxy handles `Handle(0xA000_0000 + arch_id)`
  (`arch_handle`) so picking, selection, linetypes, colours and plot styles work.
  Proxy LWPOLYLINEs on `A-WALL` are written only on save (with `CADCRAFT` XDATA and
  a `CADCRAFT_ARCH` XRECORD) and are **not** read back when arch data is present —
  one source of truth.
- **Extruded 3D** (`arch3d`): `build(&ArchModel) -> Scene3d` and
  `build_dirty(&ArchModel, &BTreeSet<ArchId>, &Scene3d)` are pure, deterministic
  and incremental. Phase 1 builds wall elevations: 2D face region (minus opening
  rectangles, none yet) → extrude, no 3D CSG. Meshes carry the owning `ArchId` for
  picking and world-space box UVs (u=x, v=z); ear-clipping triangulation in
  `arch3d::mesh`.
- **Orbit view** (`render3d`): offscreen wgpu render into its own colour+depth
  textures, handed back as an image so egui's render pass is untouched (works on
  WebGPU and WebGL2). Headless render returns `Render3dError::NoAdapter` without a
  GPU; the CLI treats that as exit 0 (SKIP), so CI without a GPU stays green.
- **GLB export** (`io::glb`): own GLTF 2.0 writer, no dependency; node names
  `wall-w1`, POSITION min/max on every accessor.

## Commands (JSON form)

Every command is a `CommandSpec` in `engine::cmd::arch`, reachable via `execute`,
`list_commands` and `command_line` — no new MCP tools in phase 1. The JSON form
never opens dialogs; the interactive form drives on-canvas prompts.

| id | Menu | JSON params |
|---|---|---|
| `wall` | Architecture ▸ Wall | `{points: [[x, y], …], thickness?, height?, type?}` |
| `arch.wall.stretch` | Architecture ▸ Wall | `{id?, end: "start"|"end", to: [x, y]}` — transitive neighbour stretch, capped |
| `arch.inspect` | Architecture | `{ids?}` |
| `build3d` | Architecture | `{ids?}` — scene-level output: object/triangle counts, world bounds, `revision` |
| `arch.sample` | Architecture | `{ids?}` |
| `view3d` | Architecture | — (view-only, noundo) |
| `camera` | Architecture | `{mode: "orbit", azimuth?, elevation?, distance?, fov?}` |

## CLI

```
cadcraft-cli convert IN.dxf OUT.glb
cadcraft-cli run … --export out.glb
cadcraft-cli render IN.dxf OUT.png --camera 'orbit,azimuth=…,elevation=…,distance=…,fov=…' [--size WxH]
```

`render` headless returns exit 0 (SKIP) as `NoAdapter` without a GPU adapter, so
CI without a GPU is green.

## Crate map

| Crate | Layer | Phase 1 contents |
|---|---|---|
| `doc` (`src/arch.rs`) | L1 | `ArchModel`, `ArchId`, `Wall`; `Drawing::arch` + `arch_revision`, `touch_arch()` |
| `arch` | L2 | join solver, neighbour stretch (no rendering) |
| `arch3d` | L2 | `build`/`build_dirty` → `Scene3d`, `mesh` (ear-clipping); no `render` dependency |
| `render` (`src/arch.rs`) | L2 | arch geometry → display-list prims, `arch_handle` |
| `io` (`glb.rs`) | L3 | own GLTF 2.0 writer |
| `engine` (`cmd/arch`) | L4 | the seven commands above; arch grips wired in (snaps to wall faces/ends land in phase 2) |
| `render3d` | L5 | offscreen orbit viewport + headless PNG |
| `ui-egui` | L5 | 3D pane presenting the offscreen texture |
| `apps/cadcraft-cli` | exempt | `convert`, `run --export`, `render` |

Budgets are tests: 300-wall full rebuild < 50 ms, one-wall `build_dirty` < 5 ms.
