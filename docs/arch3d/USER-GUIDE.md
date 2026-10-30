# arch3d user guide

arch3d turns a plan into a building: walls, doors, windows, rooms and roofs live in the drawing, and the 3D
model, the elevations, the sections and the sheets are built from them. This guide walks through the 3D views
(phase 6) and the drawings made from the model (phase 7).

Every example below is a shell block you can paste into a terminal at the repository root. Each block is
self-contained: it builds the house it needs and writes its outputs (DXF, PNG, PDF) to the current directory.
The command-line form is always the same:

- `cargo run -q -p cadcraft-cli -- run --metric --cmd '<id> <json>' ... [--save out.dxf]` runs commands in a new
  metric drawing, one `--cmd` per command, in order, and prints each command's JSON reply on its own line;
- `cargo run -q -p cadcraft-cli -- render IN.dxf OUT.png [options]` renders the 3D model of a saved drawing;
- `cargo run -q -p cadcraft-cli -- convert IN.dxf OUT.glb` exports the 3D model as glTF.

Lengths are metres. Arch objects are referred to by id (`w0` is the first wall, `v0` the first view).

## The house

The examples use a 10 × 8 m house: one exterior wall loop 0.2 m thick and 2.4 m high, a hinged door in the
south wall, a sliding window in the north wall, the room found from the walls and a 6/12 gable roof.

```sh
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'door {"wall":"w0","offset":3.0,"width":0.9,"height":2.1,"style":"hinged"}' \
  --cmd 'window {"wall":"w2","offset":4.0,"width":1.2,"height":1.2,"sill":0.9,"style":"slider"}' \
  --cmd 'roomdetect {}' \
  --cmd 'roofauto {"pitchRise":6,"pitchRun":12,"overhang":0.5,"walls":[{"wall":"w0","directive":"gable"},{"wall":"w2","directive":"gable"}]}' \
  --cmd 'arch.inspect {"walls":true,"openings":true,"rooms":true}' \
  --save house.dxf
```

## Sun position

`sun` computes the sun's azimuth and elevation for a latitude, a date and a time (`solar-noon` or a clock time
`HH:MM`). The examples use Calgary, 51.05° N. The renderer uses the same sun for shadows (`render --sun`).

```sh
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'sun {"lat":51.05,"date":"2026-12-21","time":"solar-noon"}' \
  --cmd 'sun {"lat":51.05,"date":"2026-06-21","time":"solar-noon"}' \
  --cmd 'sun {"lat":51.05,"date":"2026-06-21","time":"06:00"}'
```

## Saved cameras

`camera` saves named views in the drawing; they survive undo, redo and save. A `full` camera is an eye, a
target and a field of view; an `orbit` camera circles a target at an azimuth, an elevation and a distance.
`{"use":"name"}` makes a saved camera current, `{"list":true}` lists them and `{"delete":"name"}` removes one.

```sh
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'camera {"save":"front","kind":"full","eye":[3,-8,1.6],"target":[3,0,1.05],"fov":60}' \
  --cmd 'camera {"save":"overview","kind":"orbit","azimuth":35,"elevation":30,"distance":25,"target":[5,4,1.2]}' \
  --cmd 'camera {"list":true}' \
  --cmd 'camera {"use":"front"}' \
  --cmd 'camera {"delete":"overview"}' \
  --cmd 'camera {"list":true}' \
  --save cameras.dxf
```

## Walkthrough

In the 3D pane, walkthrough mode moves the eye with W A S D and turns it with the mouse. The `walkthrough`
command is the same step as a calculator: from an eye and a heading (`yaw`, degrees, 0 = north, +Y) it moves
`forward` and `strafe` metres and turns `turn` degrees, and replies with the new eye and yaw. The eye keeps its
height.

```sh
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'walkthrough {"eye":[5,-10,1.6],"yaw":0,"forward":2,"strafe":0,"turn":0}' \
  --cmd 'walkthrough {"eye":[5,-10,1.6],"yaw":0,"forward":0,"strafe":1,"turn":0}' \
  --cmd 'walkthrough {"eye":[5,-10,1.6],"yaw":0,"forward":0,"strafe":0,"turn":90}'
```

## Picking in 3D

`pick3d` casts a ray through one pixel of a camera's image and replies with the arch object it hits (its id and
handle, the same handle the 2D selection uses) and the hit point; a pixel on the sky hits nothing.

```sh
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'door {"wall":"w0","offset":3.0,"width":0.9,"height":2.1,"style":"hinged"}' \
  --cmd 'pick3d {"camera":{"kind":"full","eye":[3,-8,1.6],"target":[3,0,1.05],"fov":60},"size":[480,320],"pixel":[240,160]}' \
  --cmd 'pick3d {"camera":{"kind":"full","eye":[3,-8,1.6],"target":[3,0,1.05],"fov":60},"size":[480,320],"pixel":[8,8]}'
```

## Rendering

`render` draws the 3D model of a saved drawing into a PNG:

- `--camera` takes `orbit,azimuth=A,elevation=E,distance=D[,target=x,y,z][,fov=F]` or
  `full,eye=x,y,z,target=x,y,z[,fov=F]`;
- `--style` is `shaded` (the default), `shaded-edges`, `hidden-line` or `glass-house`;
- `--sun lat=DEG,date=YYYY-MM-DD,time=solar-noon|HH:MM` lights the model from that sun and casts shadows;
- `--ground` draws a ground plane under the model that takes the shadows;
- `--adapter fallback` uses the software renderer, so it works on a machine without a GPU (`auto`, the default,
  prefers a GPU);
- `--size WxH` sets the image size in pixels.

```sh
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'door {"wall":"w0","offset":3.0,"width":0.9,"height":2.1,"style":"hinged"}' \
  --cmd 'window {"wall":"w2","offset":4.0,"width":1.2,"height":1.2,"sill":0.9,"style":"slider"}' \
  --cmd 'roomdetect {}' \
  --cmd 'roofauto {"pitchRise":6,"pitchRun":12,"overhang":0.5,"walls":[{"wall":"w0","directive":"gable"},{"wall":"w2","directive":"gable"}]}' \
  --save house.dxf
cargo run -q -p cadcraft-cli -- render house.dxf overview.png --size 640x400 \
  --camera 'orbit,azimuth=35,elevation=30,distance=25' --style shaded-edges --adapter fallback
cargo run -q -p cadcraft-cli -- render house.dxf winter.png --size 640x400 \
  --camera 'orbit,azimuth=0,elevation=89,distance=40,target=5,4,0' --ground \
  --sun 'lat=51.05,date=2026-12-21,time=solar-noon' --adapter fallback
cargo run -q -p cadcraft-cli -- render house.dxf front.png --size 640x400 \
  --camera 'full,eye=3,-8,1.6,target=3,0,1.05,fov=60' --style hidden-line --adapter fallback
```

## Elevations and sections

`elevation` and `section` make linked 2D views of the model, placed in model space at `insert`. They are
views, not copies: move a window and its views follow. `elevation {"side":"south"}` looks at the south face
(also `east`, `north`, `west`); `section` cuts along the line `from`–`to` and looks the way `look` points.
Views are numbered `v0`, `v1`, … in the order they are made. `arch.inspect {"views":true}` lists every view
with its geometry and its overall width and height dimensions, and `view.delete {"view":"v4"}` removes one.

```sh
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'door {"wall":"w0","offset":3.0,"width":0.9,"height":2.1,"style":"hinged"}' \
  --cmd 'roomdetect {}' \
  --cmd 'roofauto {"pitchRise":6,"pitchRun":12,"overhang":0.5,"walls":[{"wall":"w0","directive":"gable"},{"wall":"w2","directive":"gable"}]}' \
  --cmd 'elevation {"side":"south","insert":[0,-20]}' \
  --cmd 'elevation {"side":"east","insert":[16,-20]}' \
  --cmd 'elevation {"side":"north","insert":[0,-32]}' \
  --cmd 'elevation {"side":"west","insert":[16,-32]}' \
  --cmd 'section {"from":[-1,4],"to":[11,4],"look":[0,1],"insert":[0,-40]}' \
  --cmd 'arch.inspect {"views":true}' \
  --cmd 'view.delete {"view":"v4"}' \
  --cmd 'arch.inspect {"views":true}' \
  --save views.dxf
```

## Sheets and PDF

`viewsheet` places views on a paper-space layout, one viewport per view at the view's scale (1:50 unless the
view says otherwise, so one model metre is 20 paper mm), on an ARCH D sheet by default. The layout plots like
any other: `exportpdf {"layout":"Elevations","path":"elevations.pdf"}` writes it as a PDF. This is the worked
example: the four elevations of the house on an ARCH D landscape sheet at 1:50.

```sh
cargo run -q -p cadcraft-cli -- run --metric \
  --cmd 'wall {"points":[[0,0],[10,0],[10,8],[0,8],[0,0]],"thickness":0.2,"height":2.4,"exterior":true}' \
  --cmd 'door {"wall":"w0","offset":3.0,"width":0.9,"height":2.1,"style":"hinged"}' \
  --cmd 'roomdetect {}' \
  --cmd 'roofauto {"pitchRise":6,"pitchRun":12,"overhang":0.5,"walls":[{"wall":"w0","directive":"gable"},{"wall":"w2","directive":"gable"}]}' \
  --cmd 'elevation {"side":"south","insert":[0,-20]}' \
  --cmd 'elevation {"side":"east","insert":[16,-20]}' \
  --cmd 'elevation {"side":"north","insert":[0,-32]}' \
  --cmd 'elevation {"side":"west","insert":[16,-32]}' \
  --cmd 'viewsheet {"name":"Elevations","views":["v0","v1","v2","v3"],"paper":"ARCH D"}' \
  --cmd 'exportpdf {"layout":"Elevations","path":"elevations.pdf"}' \
  --save sheet.dxf
```

## Detecting a plan

`archdetect` reads a plain DXF plan (walls as parallel face lines, doors from swing arcs or sliding leaves,
windows from glass lines between jambs, rooms named from their labels) and proposes arch objects. AIA/NCS layer
names raise the confidence, they are never required; `INSERT`s are read through; units come from `$INSUNITS` or
the `units` parameter.

A dry run (`{"dryRun":true}`) lists candidates with their confidence and evidence. Accepting candidates
(`{"accept":"all"}` or a list of ids) creates the walls, openings and rooms in one undo step. If the drawing is
not in metres, accepting converts it to metres in the same step.

```sh
cargo run -q -p cadcraft-cli -- run examples/apartment.dxf \
  --cmd 'archdetect {"dryRun":true}' \
  --cmd 'archdetect {"accept":"all"}' \
  --cmd 'arch.inspect {"rooms":true}' \
  --save apartment-3d.dxf
```

The reply for a dry run includes `walls`, `openings` and `rooms` arrays. Each candidate has an `id` (`c0`, `c1`,
...), a `confidence` (0.0–1.0), and `evidence` (layers, handles, rules). The `accept` reply maps candidate ids
to created arch ids (`w0`, `o0`, `r0`).

You can also convert a DXF directly to a GLB, which runs detection first if no arch objects are present:

```sh
cargo run -q -p cadcraft-cli -- convert examples/apartment.dxf apartment.glb
```

The **Detection Review** panel (Architecture > Detection Review) lets you inspect, check and accept candidates
interactively.

## Out of scope

Not part of arch3d so far: SSAO, TAA, cascaded shadows, path tracing, artificial lights, animated walkthrough
paths, and gamepad or 3D-mouse input.
