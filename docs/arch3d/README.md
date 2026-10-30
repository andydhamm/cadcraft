# arch3d — architectural 3D

"Draw a plan, get the house in 3D." arch3d adds a building model to CADCraft: walls, doors, windows, rooms,
floors, levels, roofs, stairs, railings and decks live on the `Drawing` as arch objects, so they undo, autosave,
audit and round-trip through DXF with everything else. From that one model it builds the plan symbols, the 3D
model, elevations, sections and sheets. Arch objects are in metres.

- [USER-GUIDE.md](USER-GUIDE.md) — how to use it, with examples you can run;
- [DESIGN.md](DESIGN.md) — the design log: decisions, crate layout, what each phase built and what is next.

## Commands by phase

Every command is a `CommandSpec` in `crates/engine/src/cmd/arch/` (plus the shared plot commands), reachable
from the Architecture menu, from `execute` / `command_line` and from the CLI as
`cadcraft-cli run --metric --cmd '<id> <json>'`. The JSON form never opens a dialog.

| Phase | What it added | Commands |
|---|---|---|
| 1 | Walls with mitred joins, plan poché, the first 3D view, glTF export | `wall`, `arch.wall.stretch`, `arch.inspect`, `build3d`, `arch.sample`, `view3d` |
| 2 | Openings: doors, windows, garage doors, their plan symbols and holes in the 3D walls | `door`, `window`, `garagedoor`, `arch.opening.move` |
| 3 | Rooms, floors, ceilings and levels | `room`, `roomdetect`, `level` |
| 4 | Automatic roofs: hips and gables over the exterior walls, with overhangs, and hand-edited roof planes | `roofauto`, `roofplane` |
| 5 | Stairs, railings and decks | `stair`, `railing`, `deck` |
| 6 | The 3D view: sun and shadows, display styles, saved cameras, walkthrough, 3D picking | `sun`, `camera`, `walkthrough`, `pick3d` |
| 7 | Linked elevation and section views, placed on sheets and plotted | `elevation`, `section`, `view.delete`, `viewsheet` (then `exportpdf` / `plot`) |
| 8 | Detecting walls, openings and rooms in plain linework (a plan drawn without arch objects), reviewed and accepted as arch objects | `archdetect` (being built) |

## Command line

```
cadcraft-cli run --metric --cmd '<id> <json>' ... [--save out.dxf]
cadcraft-cli render IN.dxf OUT.png [--camera SPEC] [--style STYLE] [--sun SPEC] [--ground] [--adapter auto|fallback] [--size WxH]
cadcraft-cli convert IN.dxf OUT.glb
```

`render --adapter fallback` uses the software renderer, so it works without a GPU.

## Crate map

| Crate | Layer | arch3d contents |
|---|---|---|
| `doc` (`src/arch.rs`) | L1 | the arch model: `ArchModel`, `ArchId`, walls, openings, rooms, levels, roofs, stairs, views |
| `arch` | L2 | wall joins and stretch, room detection, roof generation, sun position; no rendering |
| `arch3d` | L2 | `build` / `build_dirty` → `Scene3d` (pure, incremental), meshing |
| `render` (`src/arch.rs`) | L2 | arch objects → 2D display list (plan symbols, view linework) |
| `io` | L3 | the arch record in DXF, proxy linework on save, glTF writer |
| `engine` (`cmd/arch`) | L4 | the commands above |
| `render3d` | L5 | offscreen wgpu renderer: styles, shadows, headless PNG |
| `ui-egui` | L5 | the 3D pane, walkthrough, picking |
| `apps/cadcraft-cli` | exempt | `run`, `render`, `convert` |

Budgets are tests: a 300-wall full rebuild in under 50 ms, a one-wall `build_dirty` in under 5 ms.
