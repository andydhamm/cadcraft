# arch3d build status

Updated 2026-10-30 21:30 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 9 of 13: Terrain and site — 1 of 25 segments merged · 0 of 12 acceptance checks**

5,381 workspace tests passing at the last merge · 531 segments merged in all · 0 of 11 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | done | 11 / 11 | 52.1 h |
| 5 | Stairs, railings and decks | done | 9 / 9 | 16.1 h |
| 6 | The 3D view, properly | done | 9 / 9 | 12.1 h |
| 7 | Generated elevations and sections | done | 9 / 9 | 8.2 h |
| 8 | Recognition from plain DXF | done | 10 / 10 | 16.6 h |
| 9 | Terrain and site | building | 0 / 12 | 1.9 h so far · expected done Oct 31 17:32–Nov 06 19:51 UTC |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 9 segments

17 queued, 3 merging, 2 working, 2 gate, 1 merged

| Segment | What | State | Seat |
|---|---|---|---|
| `p9-geom-cdt` | Constrained Delaunay triangulation (breaklines are edges) | working | bonsai-a |
| `p9-geom-tin-ops` | TIN point location grid, clipping to a ring, contour chaining | working | bonsai-b |
| `p9-arch-terrain-pad` | Building pad and basement cuts | merging | mai-flash |
| `p9-arch-plant` | Plant catalogue, growth and plan symbols | merging | darwin-ws3 |
| `p9-arch-site` | True north, the north arrow and true-to-drawing directions | merging | claude-sonnet-b |
| `p9-cmd-wall-retaining` | Retaining walls in the `wall` command | gate | claude-sonnet-a |
| `p9-small-convert-and-glb-names` | `convert` detection message counts; GLB floor and ceiling node names | gate | bonsai-ws2-a |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 30 20:57 | `p9-arch-terrain-csv` | merged | 5,381 pass | claude-sonnet-a |
| Oct 30 17:41 | `harden-p8f-walls-stop-at-crossing-faces` | merged | 5,364 pass | tturbo-mac2 |
| Oct 30 17:32 | `harden-p8f-rooms-arrangement-once` | merged | 5,359 pass | tturbo-mac |
| Oct 30 17:07 | `harden-p8f-orbit-fit-bounding-sphere` | merged | 5,351 pass | tturbo-mac |
| Oct 30 16:59 | `harden-p8-detect-junctions` | merged | 5,345 pass | tturbo-mac2 |
| Oct 30 16:43 | `p8f-walls-stop-at-crossing-faces` | merged | 5,339 pass | mai-flash |
| Oct 30 16:35 | `harden-p8-ui-review-panel` | merged | 5,336 pass | tturbo-mac |
| Oct 30 16:31 | `p8f-orbit-fit-bounding-sphere` | merged | 5,329 pass | claude-sonnet-b |
| Oct 30 16:22 | `harden-p8-detect-openings` | merged | 5,327 pass | tturbo-mac2 |
| Oct 30 16:01 | `p8f-rooms-arrangement-once` | merged | 5,323 pass | mai-flash |
| Oct 30 15:20 | `harden-p8-cli-convert-detects` | merged | 5,320 pass | tturbo-mac |
| Oct 30 14:48 | `harden-p8-guide-archdetect` | merged | 5,317 pass | tturbo-mac2 |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 696 |
| bonsai-b | bonsai2-27b | agent | 653 |
| bonsai-ws2-a | bonsai2-27b | gate | 700 |
| bonsai-ws2-b | bonsai2-27b | gate | 650 |
| claude-sonnet-a | sonnet | gate | 54 |
| claude-sonnet-b | sonnet | merging | 47 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | merging | 552 |
| kimi-k31 | k3.1-pro-highspeed | idle | 95 |
| mai-flash | mai-code-1.1-flash | merging | 25 |
| tturbo-mac | tturbo-ultraheretic | gate | 478 |
| tturbo-mac2 | tturbo-ultraheretic | gate | 300 |
