# arch3d build status

Updated 2026-10-31 00:00 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 9 of 13: Terrain and site — 6 of 25 segments merged · 0 of 12 acceptance checks**

5,403 workspace tests passing at the last merge · 536 segments merged in all · 0 of 11 seats working

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
| 9 | Terrain and site | building | 0 / 12 | 4.4 h so far · expected done Oct 31 12:46–Nov 01 04:00 UTC |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 9 segments

14 queued, 6 merged, 4 gate, 1 merging

| Segment | What | State | Seat |
|---|---|---|---|
| `p9-geom-cdt` | Constrained Delaunay triangulation (breaklines are edges) | gate | kimi-k31 |
| `p9-geom-tin-ops` | TIN point location grid, clipping to a ring, contour chaining | gate | bonsai-a |
| `p9-cmd-site` | `site` command, and `sun` from the site | merging | bonsai-b |
| `p9-cli-render-sun-from-site` | `render --sun` takes the latitude and true north from the site | gate | bonsai-ws2-b |
| `p9-small-convert-and-glb-names` | `convert` detection message counts; GLB floor and ceiling node names | gate | claude-sonnet-b |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 30 23:48 | `p9-cmd-plant` | merged | 5,403 pass | mai-flash |
| Oct 30 23:05 | `p9-cmd-wall-retaining` | merged | 5,398 pass | claude-sonnet-a |
| Oct 30 22:34 | `p9-arch-plant` | merged | 5,395 pass | darwin-ws3 |
| Oct 30 22:06 | `p9-arch-terrain-pad` | merged | 5,390 pass | mai-flash |
| Oct 30 21:33 | `p9-arch-site` | merged | 5,385 pass | claude-sonnet-b |
| Oct 30 20:57 | `p9-arch-terrain-csv` | merged | 5,381 pass | claude-sonnet-a |
| Oct 30 17:41 | `harden-p8f-walls-stop-at-crossing-faces` | merged | 5,364 pass | tturbo-mac2 |
| Oct 30 17:32 | `harden-p8f-rooms-arrangement-once` | merged | 5,359 pass | tturbo-mac |
| Oct 30 17:07 | `harden-p8f-orbit-fit-bounding-sphere` | merged | 5,351 pass | tturbo-mac |
| Oct 30 16:59 | `harden-p8-detect-junctions` | merged | 5,345 pass | tturbo-mac2 |
| Oct 30 16:43 | `p8f-walls-stop-at-crossing-faces` | merged | 5,339 pass | mai-flash |
| Oct 30 16:35 | `harden-p8-ui-review-panel` | merged | 5,336 pass | tturbo-mac |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | gate | 701 |
| bonsai-b | bonsai2-27b | merging | 660 |
| bonsai-ws2-a | bonsai2-27b | gate | 701 |
| bonsai-ws2-b | bonsai2-27b | gate | 658 |
| claude-sonnet-a | sonnet | idle | 55 |
| claude-sonnet-b | sonnet | gate | 48 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | gate | 557 |
| kimi-k31 | k3.1-pro-highspeed | gate | 96 |
| mai-flash | mai-code-1.1-flash | idle | 26 |
| tturbo-mac | tturbo-ultraheretic | gate | 481 |
| tturbo-mac2 | tturbo-ultraheretic | merging | 304 |
