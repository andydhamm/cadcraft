# CADCraft arch3d: Chief Architect X18 Behavioral Reference

This document refines the `arch3d` implementation plan based on behavioral observations of Chief Architect X18. It is a **clean-room reference**: it describes *what* the software does (behaviors, defaults, workflows) to inform our own implementation, not *how* it is coded.

**How to use it (2026-10-22).** Written by the lead model (Darwin R3) from the Chief Architect X18 Reference Manual and
Tutorial Guide, which the challenge lists as fair clean-room sources, in its own words. When planning a phase, fold that
phase's items from sections 1 and 3 into the plan where they fit the phase's scope. Section 4 lists deferrals, not bans:
where it disagrees with sections 1–3 (mulled units, snaps, curved stairs), sections 1–3 win. Units: imperial defaults
are given with metric equivalents; CADCraft's own unit handling decides what the user sees.

## 1. What changes in the plan

### Phase 1: Walls and Joins
*   **Add `Main Layer` concept to `WallType`:** The plan currently treats walls as uniform thickness. Chief uses a "Main Layer" (usually the structural core, e.g., studs) as the reference for joins, dimensioning, and roof baselines.
    *   *Why:* Joins must "build through" to the main layer, not just the centerline. This prevents gaps in complex assemblies and ensures correct bearing points for roofs/floors.
    *   *Action:* Add `is_main: bool` to `WallLayer`. Update join solver to intersect at the main layer surface, not just the centerline.
*   **Add `Pony Wall` support:** Walls can have different types above and below a split height.
    *   *Why:* Common for half-walls (e.g., stone base, siding top).
    *   *Action:* Add `split_height: Option<f64>` and `upper_type: WallTypeId` to `Wall`.
*   **Add `Reversed` flag:** Walls need a flag to flip the interior/exterior orientation without changing the type.
    *   *Why:* Users often draw walls in the wrong direction; flipping is easier than redrawing.
    *   *Action:* Add `reversed: bool` to `Wall`.

### Phase 2: Openings
*   **Distinguish `Rough Opening` vs. `Finished Opening`:** The plan mentions cutting openings. Chief distinguishes between the structural cut (rough) and the frame size (finished).
    *   *Why:* The 3D boolean cut must use the *rough* dimensions. The 2D symbol and 3D frame use the *finished* dimensions.
    *   *Action:* Add `rough_width`, `rough_height` to `Opening`.
*   **Add `Recess Depth`:** Openings can be offset from the wall face.
    *   *Why:* Windows are often recessed into the wall assembly (e.g., behind siding).
    *   *Action:* Add `recess_depth: f64` to `Opening`.
*   **Add `Mulled Units`:** Adjacent openings can be grouped.
    *   *Why:* Common for bay windows or door/window combos. They share a single casing and label.
    *   *Action:* Add `mull_group_id: Option<ArchId>` to `Opening`.

### Phase 3: Rooms, Floors, Levels
*   **Add `Room Type` enum:** Rooms are not just "enclosed spaces." They have types (Living, Garage, Deck, Attic, Open Below).
    *   *Why:* Type drives behavior: Garages get slabs, Decks get no ceiling, Open Below suppresses floors.
    *   *Action:* Add `RoomType` enum to `Room`.
*   **Add `Invisible Walls` (Room Dividers):** Walls with 0 thickness used only for room detection.
    *   *Why:* To split large rooms for different finishes or ceiling heights without adding physical walls.
    *   *Action:* Add `is_invisible: bool` to `Wall`. Exclude from 3D mesh and 2D poché, but include in room detection.
*   **Add `Open Below` flag:** Rooms can be open to the floor below.
    *   *Why:* Stairwells, vaulted ceilings.
    *   *Action:* Add `open_below: bool` to `Room`. Suppress floor platform generation for this room.

### Phase 4: Automatic Roofs
*   **Add `Roof Directives` to Walls:** Walls need flags for `Full Gable`, `Knee Wall`, `Hip`, etc.
    *   *Why:* The roof generator needs to know which walls are gable ends vs. hip ends.
    *   *Action:* Add `roof_directive: RoofDirective` enum to `Wall`.
*   **Add `Attic Walls`:** Auto-generated triangular walls between the roof underside and the floor above.
    *   *Why:* To close the space under the roof for insulation/finishes.
    *   *Action:* Implement `generate_attic_walls()` in the roof builder.
*   **Add `Roof Returns`:** Small planes extending past gable ends.
    *   *Why:* Aesthetic and functional (protects wall from rain).
    *   *Action:* Implement `generate_roof_returns()` for gable ends.

### Phase 5: Stairs, Railings, Decks
*   **Add `Walkline` concept:** Tread depth is measured along a walkline, not the edge.
    *   *Why:* Critical for curved/winder stairs.
    *   *Action:* Add `walkline_offset: f64` to `Stair`.
*   **Add `Landing` object:** Landings are distinct objects, not just part of the stair.
    *   *Why:* They can be moved independently and have their own railing logic.
    *   *Action:* Add `Landing` struct.
*   **Add `Railing` types:** Baluster, Solid, Open, Panels.
    *   *Why:* Different visual and structural behaviors.
    *   *Action:* Add `RailingType` enum.

### Phase 6: The 3D View
*   **Add `Display Styles`:** Shaded, Hidden-Line, X-Ray, Clay.
    *   *Why:* Essential for different workflows (drafting vs. presentation).
    *   *Action:* Implement `DisplayStyle` enum in `render3d`.
*   **Add `Hide Camera-Facing Walls`:** Toggle to see interiors from outside.
    *   *Why:* Critical for interior design views.
    *   *Action:* Implement view-dependent culling in `render3d`.

### Phase 7: Generated Elevations and Sections
*   **Add `Auto Dimensions`:** Generate dimension strings for walls and openings.
    *   *Why:* Chief's "Auto Dimensions" is a key productivity feature.
    *   *Action:* Implement `generate_auto_dimensions()` in `arch` crate.
*   **Add `Schedules`:** Tables of rooms, doors, windows.
    *   *Why:* Essential for construction documents.
    *   *Action:* Add `Schedule` object type and rendering logic.

### Phase 8: Recognition (ARCHDETECT)
*   **Add `Layer Mapping`:** Map external DXF layers to internal arch types.
    *   *Why:* `A-WALL` should map to `Wall`, `A-DOOR` to `Door`.
    *   *Action:* Implement `LayerMap` in `ARCHDETECT`.

### Phase 9: Exports
*   **Add `Metadata`:** Objects need custom fields (Code, Manufacturer, Price).
    *   *Why:* For IFC export and schedules.
    *   *Action:* Add `metadata: HashMap<String, String>` to `ArchObject`.

### Phase 10: Polish
*   **Add `Material Regions`:** Localized material overrides on walls/floors.
    *   *Why:* To apply brick to only the bottom half of a wall.
    *   *Action:* Add `MaterialRegion` object.
*   **Add `Moldings`:** Baseboards, crown molding.
    *   *Why:* Essential for interior finishes.
    *   *Action:* Add `Molding` object type.

## 2. UI and workflow

### Drawing Tools
*   **Continuous Chain:** Right-click drag-release to continue drawing walls. Double-click or `Esc` to stop.
*   **Angle Snaps:** Default 15° increments. Angle snaps take priority over object snaps for walls.
*   **Extension Snaps:** Snap to imaginary extension lines of other walls.
*   **Numeric Input:** Support arithmetic (`+`, `-`, `*`, `/`) and mixed units (`1'6"`, `18"`, `1.5'`).

### Editing & Grips
*   **Perpendicular Drag:** Resizing a wall via edge handles moves the entire wall perpendicular to its axis.
*   **Same Wall Type Grip:** Dragging a "Same Wall Type" handle creates a *new* wall of the same type, rather than extending the existing one.
*   **Bumping/Pushing:** Moving objects can "bump" into others, stopping movement or pushing the obstructing object.
*   **Reverse Layers:** Command to flip the interior/exterior orientation of a wall.

### Openings
*   **Auto-Resize:** If space is insufficient, doors/windows automatically shrink to fit the wall segment.
*   **Preview:** An outline follows the mouse; it disappears if placement is invalid.
*   **Elevation Reference:** Place relative to `Absolute`, `From Floor`, `From Finished Floor`, `From Ceiling`, or `From Roof`.
*   **Swing Indicators:** Direction can be configured to point to the Hinge or the Handle.

### Rooms & Levels
*   **Room Boxes:** Draw rectangular "Room Boxes" with a `Function` and `Name`. A "Build House" tool converts these into walls, floors, and ceilings.
*   **Reference Display:** View objects from other floors (e.g., Floor 1 walls while on Floor 2). Reference objects can be snapped to but not selected/edited.
*   **Dynamic Defaults:** Changing a global default (e.g., ceiling height) updates all objects set to "Use Default" but leaves manually overridden objects unchanged.

### 3D View
*   **Display Styles:** Shaded, Hidden-Line, X-Ray, Clay.
*   **Pivot Point:** Configurable to center mouse movement on the camera location or the focal point.
*   **Overlays:** Crosshairs, X/Y/Z axis indicators, sun/moon direction indicators.
*   **Picking:** Option to select the *Room* defined by a wall instead of the wall itself.

### Generated Views
*   **Auto Dimensions:** Generate up to three rows of dimensions (innermost, middle, outermost).
*   **Callouts:** Construction line callouts can be auto-numbered based on position/angle.
*   **Schedules:** Live tables linked to plan objects. Update automatically on object changes.

## 3. Behaviour reference by phase

### Phase 1: Walls and Joins
*   **Wall Types:**
    *   **Main Layer:** Structural core determining join depth, platform bearing, and roof baselines.
    *   **Partition Walls:** Do not build through floor/ceiling platforms.
    *   **Pony Walls:** Split upper/lower types.
    *   **Foundation Walls:** Auto-generate footings.
*   **Join Logic:**
    *   **Build-Through:** Walls build through neighbors until meeting the *Main Layer*.
    *   **Acute Angles:** Intersections < 60° terminate the building wall at a right angle to its own direction.
    *   **Fix Connections:** "Caution" icon for disconnected walls; command to auto-connect nearby ends.
*   **Snapping:**
    *   **Auto-Snap:** Wall ends snap to intersections if center lines are within the larger wall’s width, even if Object Snaps are off.
    *   **Resize About:** Configurable reference line (Outer Surface, Main Layer Outside, Center, etc.) determines anchor points for resizing/snapping.

### Phase 2: Openings
*   **Placement:**
    *   **Minimum Widths:** Hinged/Sliding/Pocket: 12" (300mm). Garage: 6' (1800mm).
    *   **Auto-Paneling:** Hinged/Pocket doors become Double if width ≥ 4' (1200mm). Bifold doors become Double if width > 3' (900mm).
    *   **Minimum Separation:** A global default controls the minimum distance between openings; this value also dictates the width of shared casing in "Mulled" units.
    *   **Wall Segment Limits:** Standard windows require a 6" (150mm) wall segment. Bay/Box/Bow windows require 30" (725mm).
*   **Swing & Hinge Logic:**
    *   **Interaction:** "Hold-and-drag" determines hinge side (L/R) and swing direction (In/Out) from the *exterior* perspective.
    *   **Classification:** Doors are automatically classified as Exterior if they separate an interior room from an exterior room (or building exterior). Interior if between two interior rooms.
*   **Geometry & Components:**
    *   **Recessing:** Openings can be recessed into specific wall layers (e.g., sheathing vs. framing).
    *   **Jamb/Casing:** Separate profiles for Interior/Exterior. Jamb can "Fit to Wall" (auto-depth). Casing has `width`, `depth`, and `reveal` (gap).
    *   **Hardware:** Defaults: Interior hinged = 2 hinges; Exterior hinged = 3 hinges.
    *   **Display States:** Doors/Windows can be shown Open or Closed independently in 2D and 3D. `swing_angle` (0-180°) and `percent_open` control geometry.
    *   **Glass:** `glass_mode` can be Automatic, Opaque, or Transparent.
    *   **Mulled Units:** Windows/doors automatically group if casings touch (same sill elevation). Manual mulling groups objects within 24" (600mm). Components remain separate for scheduling but share a single label/casing.
    *   **Window Levels:** A parameter (0-N) controls plan view stacking and selection priority. Level 0 is selectable; others require "Select Next Object".
    *   **Bay/Box/Bow Defaults:**
        *   **Bay:** 2'-2" front, 4'-2" back, 1'-0" depth, 45° angle.
        *   **Box:** 4'-2" width, 1'-6" depth, 90° angle.
        *   **Bow:** 5 sections, radius centered 3'-9" inside wall, 11.5" depth.
        *   **Area:** Bay/Box/Bow areas are included in "Interior Area" but excluded from "Living Area."

### Phase 3: Rooms, Floors, Levels
*   **Room Detection:**
    *   **Measurement Basis:** Area calculation can be based on the wall's "Main Layer" (structural centerline) or "Surface" (exterior finish).
    *   **Room Boxes:** Users can draw rectangular "Room Boxes" with a `Function` and `Name`. A "Build House" tool converts these into walls, floors, and ceilings.
*   **Levels & Reference:**
    *   **Reference Display:** Allows viewing objects from other floors (e.g., Floor 1 walls while on Floor 2). Reference objects can be snapped to but not selected/edited.
    *   **Grade Level:** A specific elevation marker independent of terrain, used for story pole dimensions.
    *   **Dynamic Defaults:** Changing a global default (e.g., ceiling height) updates all objects set to "Use Default" but leaves manually overridden objects unchanged.
*   **Room Types & Defaults:**
    *   **Garage:** Concrete slab, no finish, stem walls/curbs.
    *   **Deck:** Framed platform, no ceiling/roof.
    *   **Porch:** Slab floor, ceiling, roof.
    *   **Attic:** Requires adjacent walls flagged as "Knee Walls".
    *   **Open Below:** Suppresses floor platform generation (used for stairwells).
*   **Area Metrics:**
    *   **Interior Area:** Measured from inner wall surfaces. Includes bay/box/bow areas.
    *   **Standard Area:** Measured from center of interior walls and outside surface (or Main Layer) of exterior walls. Excludes bay/box/bow areas.
    *   **Living Area:** Sum of Standard Areas for Interior room types. Excludes Exterior/Hybrid types (Garage, Deck, Porch), Open Below, and Attic. Rooms with Rough Ceiling height < 48" (1200mm) are excluded by default.

### Phase 4: Automatic Roofs
*   **Generation:**
    *   **Directives:** Roof generation relies on "roof directives" carried from lower floors to the top floor.
    *   **Auto-Rebuild:** Roofs automatically rebuild when wall geometry changes. Manual editing of roof planes disables auto-rebuild.
    *   **Pitch Display:** Supports "Rise/Run" (e.g., 6:12) and "Degrees" (e.g., 26.5°).
*   **Roof Shapes:** `Hip`, `Full Gable`, `Dutch Gable` (partial gable on hip), `High Shed/Gable`, `Knee Wall` (interior only), `Extend Slope Downward`.
*   **Parameters:** `Pitch`, `Upper Pitch` (for Dutch gable), `Height Upper Pitch Starts`, `Overhang Length`.
*   **Roof Returns:** Auto-generated on gable ends. Parameters: `Length`, `Extend` past overhang, `Gable/Hip/Full` type, `Sloping/Flat`, include `Shadow Boards`, `Ridge Caps`, `Frieze`, `Gutters`.
*   **Wall/Roof Interaction:** `Roof Cuts Wall at Bottom` prevents wall building below intersecting roof plane. `Include Frieze` generates molding at roof line.
*   **Bay/Box/Bow Roof Logic:** Default is Hip roof over bump-out. Options: `Use Existing Roof`, `Extend Existing Roof Over`, `Rectangular Roof Over`. Gable is not automatic.

### Phase 5: Stairs, Railings, Decks
*   **Stair Calculation:** "Best Fit" riser height closest to **6 3/4" (168.75 mm)** that reaches the next level precisely. Rise measured to floor *finish* surfaces; stringers locate to floor *structure*.
*   **Tread Modes:** `Automatic Treads`, `Lock Tread Depth` (count changes with length), or `Lock Number of Treads` (depth changes with length).
*   **Walkline:** Tread depth calculated along a "Walkline" offset from edge (default 12"/300 mm). Critical for curved/winder stairs.
*   **Winders:** Expand to fill gaps between stair side and nearby walls. Requires valid room definition.
*   **Wrapped Stairs:** Two perpendicular sections with identical attributes can "wrap" around a corner if close enough. Edits propagate.
*   **Stringers:** Options: `2 Closed Side`, `1 Center`, or `Custom`. Parameters: `Height Above/Below Tread`.
*   **Open Risers:** Checkbox to remove riser faces or add a skirt underneath.
*   **Landings:**
    *   *Auto Height:* Top Height = 1 riser + floor finish thickness (interior) or thickness (exterior).
    *   *Thickness:* Defaults to 6.75" (169mm) if free-standing, or matches attached stair riser.
    *   *Railing Logic:* Per-edge control: `Automatic` (suppress if connecting to stair/ramp/landing), `No Railing`, `Has Railing`.
    *   *Plan Display:* Option to display landing on floor above (`Automatic` if in "Open Below" room).
*   **Railings:**
    *   *Automatic Openings:* Creates doorway opening in railing when stairs snap to it.
    *   *Types:* `Balusters`, `Open`, `Open with Middle Rail`, `Panels`.
    *   *Heights:* Separate `Stair Rail Height` and `Landing Rail Height`.
    *   *Transitions:* Options for gooseneck and smooth transitions.
    *   *Extensions:* `Extend Top/Bottom End` values define handrail extension past first/last riser.
*   **Stairwells:** Auto button creates `Open Below` room on floor above, enclosed by railings.
*   **Ramps:** Default slope 1:12, max height 30" (760mm). Auto-length calculation.

### Phase 6: The 3D View
*   **Display Styles:**
    *   **Floor Overview:** A 3D mode that shows the current floor and floors below but **omits ceilings** on the current floor to reveal interior layout.
    *   **Vector View:** Hidden-line removal. Optional toggle to suppress curved edge lines for performance.
    *   **Glass House:** X-ray view.
*   **Navigation & UI:**
    *   **Pivot Point:** Configurable to center mouse movement on the camera location or the focal point.
    *   **Overlays:** Supports crosshairs, X/Y/Z axis indicators (floating/fixed), and sun/moon direction indicators.
    *   **Status Bar:** Displays X, Y, Z coordinates, layer, and drawing group.
    *   **Picking:** Option to select the *Room* defined by a wall instead of the wall itself.
    *   **Gamepad/3D Mouse:** Configurable sensitivity, acceleration curves, and axis inversion.

### Phase 7: Generated Elevations & Sections
*   **Construction Lines:** Guide lines drawn in plan view appear in sections/elevations if the line of sight is parallel. Vertical lines in sections appear in plan.
*   **Callouts:** Construction line callouts can be auto-numbered based on position/angle. Collinear lines share numbers.
*   **Auto Exterior Dimensions:** Generates up to three rows:
    1.  **Innermost:** Locates exterior/interior walls and openings.
    2.  **Middle:** Locates exterior/interior walls only.
    3.  **Outermost:** Overall exterior dimension.
    *   **Parameters:** `Line Separation`, `First Line Offset`, `Offset From Wall` (Center/Dimension Layer/Surface), `Reach`, `Minimum Area`.
    *   **State:** If manually edited, marked as "edited" and not overwritten by auto-generation.
*   **Auto Story Pole Dimensions:** Vertical dimension strings and Elevation Markers (e.g., Top of Plate, Ridge).
    *   **Parameters:** `Reach` (percentage across structure), `Line Separation`, `First Line Offset`, `Marks to Include`.
*   **Dimension Formatting:**
    *   **Rounding:** `Grid Rounding` (sum of parts equals whole) vs `Distance Rounding` (accurate segments, sum may vary).
    *   **Units:** Imperial (ft-in, fractional) or Metric. Fraction reduction: GCD or Closest.
    *   **Extension Lines:** `Gap From Marked Object`, `Fixed Proximity` (locks distance).
*   **Annotations:**
    *   **Callouts:** Linked to saved views/layout pages. Text macros insert view names/page numbers.
    *   **Markers:** Level Line, Point, Test Boring. Distinct geometry (center point, line, text).
    *   **Text Styles:** Saved sets of attributes (font, height, color). 3D text can "Face Camera" (billboard) or align with surface.

### Phase 8: Recognition (ARCHDETECT)
*   **Layer Handling:** Importing/detecting objects must dynamically create missing layers (e.g., `A-WALL`) in the target document.
*   **Clipboard:** Copying arch objects between files creates necessary custom layers/styles in the destination if missing.

### Phase 9: Exports
*   *(No specific behavioral notes from manual pages relevant to export formats beyond standard object data.)*

### Phase 10: Polish & Validation
*   **Plan Check:** Validates plans against building practices. Automatically assigns Room Types based on size/contents (e.g., small room + shelf = closet). Highlights errors with navigation.
*   **Materials List:** Supports custom formulas for cost estimation.
*   **UI Preferences:**
    *   **Contextual Toolbar:** Appears only when objects are selected, with buttons changing based on object type.
    *   **Command History:** `Shift+Esc` cycles through the last 100 used commands.
    *   **Line Weights:** Minimum display weight in pixels ensures visibility when zoomed out.

## 4. Not worth copying

*   **Framing Generation:** Full structural framing (studs, joists, trusses) is out of scope for Phases 0–10. Data models (`bearing`, `is_framing`, `header_type`) should be prepared, but generation logic is deferred.
*   **Terrain Mesh Generation:** From elevation points/contours. Terrain modifiers, hills, valleys, roads, driveways, sidewalks.
*   **Thermal Envelope:** Conditioned space detection and orientation logic for energy analysis.
*   **Ray Tracing:** Progressive path tracing for "final views".
*   **Electrical/Plumbing/HVAC:** MEP wiring/splines are **not** part of `arch3d` MVP.
*   **Layout Sheets:** Beyond the generated elevation sheets. Title blocks, revision tables, multi-page printing.
*   **External Links:** Linking layout views to external plan files (XREF style).
*   **Spatial Filtering for Exports:** `MATERIALSCOPE` object or command.
*   **Component Hierarchy:** Detailed component structure for material takeoffs.
*   **Path-Based Walkthrough Animation:** Not in Phase 6 scope.
*   **Artificial Light Sources:** Point, spot, area lights.
*   **Curved/Wrapped Stairs:** Deferred.
*   **Interactive Roof Plane Editing:** Joining/locating intersections.
*   **Dormers and Crickets:** Not in Phase 4.
*   **Detailed Eave/Fascia/Soffit Generation:** Deferred.
*   **Complex Window Types:** Bay/Bow and Mulled Units not in Phase 2 scope.
*   **Layer Mapping UI:** Not detailed.
*   **User-Defined Default Saving:** "Saved Defaults Dialog".
*   **Specific Snap Types:** Object Snaps, Extension Snaps, Wall Intersections.

## 5. Asset and interchange candidates (not from the manuals)

CADCraft arch3d is a personal, non-commercial project (Andy, 2026-10-22); the fork is public, so what is committed is
redistributed and must allow that.


* **Phase 10, fixtures library:** NVIDIA's SimReady assets (`huggingface.co/datasets/nvidia/simready-assets`, OpenUSD,
  2,575 assets) include appliances (dishwasher, refrigerator, toaster, range hood), 2 m and 4 m window blinds, file
  cabinets and beds. Licences are per asset (`<asset>/.metadata/LICENSE.txt`; the ones checked are CC BY 4.0): read
  each one before use, record source and licence in `ATTRIBUTION.md`, and convert the chosen few to glTF outside the
  codebase (CADCraft stays pure Rust, no USD reader). Most of the set is industrial and medical and does not fit.
* **Phase 9, optional OpenUSD export:** NVIDIA SimReady Foundation (`github.com/NVIDIA/simready-foundation`,
  Apache-2.0) specifies semantic labels, physics properties and packaging for simulation-ready USD. A USD export that
  follows it would let CADCraft houses load into Omniverse or Isaac. Only after glTF, OBJ and IFC4 are done.
* **Phase 10, home furniture and fixtures (best fit):** Sweet Home 3D's model libraries
  (`sweethome3d.com/import-models`, ZIP libraries of OBJ/DAE models). Use public-domain sets first (BlendSwap CC-0, 175
  models; Dodecagon), then CC BY sets with attribution (Scopia 500, KatorLegaz 90, BlendSwap CC-BY 135). Free Art
  License sets (Contributions 511, LucaPresidente 64) may go in too, as separate asset files that keep their own licence
  (share-alike covers those models and changes to them, not CADCraft). Check each model's licence, record it in
  `ATTRIBUTION.md`, convert to glTF.
  Sweet Home 3D itself is GPL: behaviour may inform the furniture catalogue UI, its code may not be used.
* **Local only, never in the repo:** sunvage SimHome (`huggingface.co/datasets/sunvage-lab/sunvage-vertens3d-simhome`,
  2,000 whole homes and 12,169 rooms in OpenUSD + glb) is "non-commercial research only", gated, no redistribution
  without written permission. This is a personal, non-commercial project, so it may be used locally once Andy accepts
  its terms: as evaluation data (room detection and area rules against real homes) or as a local furniture source read
  from a folder outside the repository. The fork (`andydhamm/cadcraft`) is public, so none of it is ever committed.
* **Not an asset source:** D5 Render (free Community tier, Windows + RTX) and its D5 Works library (terms not
  published on its download page). At most a check that the phase 9 glTF export renders correctly in it.
