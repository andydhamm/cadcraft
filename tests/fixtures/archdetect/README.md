# Archdetect Truth Data

This directory contains truth data for scoring the `archdetect` command.

## Truth Format

The truth files (`.truth.json`) describe the expected walls and openings in a drawing.

### Walls
Walls are represented by their **centre line** and **thickness** in metres.
```json
{ "start": [x, y], "end": [x, y], "thickness": 0.2 }
```

### Openings
Openings are represented by their **kind** (`"door"` or `"window"`), **centre** point, and **width** in metres.
```json
{ "kind": "door", "center": [x, y], "width": 0.9 }
```

## Matching Rules

Scoring compares detected candidates against truth using one-to-one greedy matching.

1.  **Walls**: A candidate matches a truth wall if:
    *   The Hausdorff distance between the centre-line segments is `<= max(truth_thickness / 2, 0.05)`.
    *   The absolute difference in thickness is `<= 0.2 * truth_thickness`.
2.  **Openings**: A candidate matches a truth opening if:
    *   The kind matches (Door vs Window).
    *   The distance between centres is `<= 0.15` m.
    *   The absolute difference in width is `<= 0.1 * truth_width`.

Matching is greedy: all valid pairs are sorted by distance (ascending), then by candidate index, then by truth index. The first available pair is taken, marking both candidate and truth as used.

## Corpora

1.  **Synthetic Plans**: Generated from CADCraft models, stripped of arch data, to test precision/recall on clean geometry.
2.  **Apartment**: `examples/apartment.dxf` with hand-written truth (`apartment.truth.json`).
3.  **Real Plans**: Freely licensed real-world plans in `real/` (see `ATTRIBUTION.md`).
