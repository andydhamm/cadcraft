# arch3d User Guide — Phase 6

This guide covers the new 3D view features introduced in Phase 6: sun positioning, camera management, walkthrough mode, 3D picking, and enhanced CLI rendering.

## Sun Positioning

The `sun` command calculates the sun's position based on latitude, date, and time. This affects shadows and lighting in the 3D view.

```sh
# Set sun for London (51.05°N) at solar noon on June 21st
cadcraft run --command sun --params '{"lat":51.05,"date":"2026-06-21","time":"solar-noon"}'
```

## Camera Management

Cameras can be saved, listed, used, and deleted. Saved cameras are stored in the drawing and persist through undo/redo and file saves.

### Save a Camera

```sh
# Save the current view as "front"
cadcraft run --command camera --params '{"action":"save","name":"front"}'
```

### List Cameras

```sh
# List all saved cameras
cadcraft run --command camera --params '{"action":"list"}'
```

### Use a Camera

```sh
# Switch to the "front" camera
cadcraft run --command camera --params '{"action":"use","name":"front"}'
```

### Delete a Camera

```sh
# Delete the "front" camera
cadcraft run --command camera --params '{"action":"delete","name":"front"}'
```

## Walkthrough Mode

Walkthrough mode allows WASD navigation in the 3D pane. This is an interactive mode activated in the UI, but can be triggered via command for scripting purposes (though primarily interactive).

```sh
# Enter walkthrough mode (interactive)
cadcraft run --command walkthrough
```

## 3D Picking

The `pick3d` command performs a CPU ray cast against the 3D scene to identify objects under a specific pixel.

```sh
# Pick the object at pixel (100, 100) in a 800x600 view
cadcraft run --command pick3d --params '{"camera":"front","size":[800,600],"pixel":[100,100]}'
```

## CLI Rendering

The `render` command in `cadcraft-cli` has been enhanced with new options for style, sun, ground, adapter, and camera.

### Render with Style

```sh
# Render with shaded style
cadcraft render house.dxf out.png --style shaded
```

### Render with Sun

```sh
# Render with sun from the south
cadcraft render house.dxf out.png --sun '{"lat":51.05,"date":"2026-06-21","time":"solar-noon"}'
```

### Render with Ground

```sh
# Render with a ground plane
cadcraft render house.dxf out.png --ground
```

### Render with Adapter

```sh
# Force use of the software adapter
cadcraft render house.dxf out.png --adapter fallback
```

### Render with Camera

```sh
# Use a saved camera named "front"
cadcraft render house.dxf out.png --camera front
```

## Out of Scope

The following features are explicitly out of scope for Phase 6:
- SSAO (Screen Space Ambient Occlusion)
- TAA (Temporal Anti-Aliasing)
- Cascaded Shadows
- Path Tracing
- Artificial Lights
- Path-based Walkthrough Animation
- Gamepad and 3D Mouse support

## Phase 7: Elevations, Sections and Sheets

Phase 7 adds linked `elevation` and `section` views. Generated view geometry can be queried with `arch.inspect {"views":true}`, and an unwanted view can be removed with `view.delete`.

The `viewsheet` command places views on a layout. The worked example uses four elevations of the phase-1 house with a gable roof on an ARCH D landscape sheet at 1:50. The sheet can then be plotted with `exportpdf {"layout":"Elevations"}`.
