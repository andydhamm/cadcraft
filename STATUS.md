# arch3d build status

Updated 2026-10-27 05:00 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 4 of 13: Automatic roofs — 34 of 63 segments merged · 5 of 11 acceptance checks**

3,822 workspace tests passing at the last merge · 254 segments merged in all · 0 of 12 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | building | 5 / 11 | 20.2 h so far · expected done Oct 27 21:07–Oct 28 08:16 UTC |
| 5 | Stairs, railings and decks | planned | — | — |
| 6 | The 3D view, properly | planned | — | — |
| 7 | Generated elevations and sections | planned | — | — |
| 8 | Recognition from plain DXF | planned | — | — |
| 9 | Terrain and site | planned | — | — |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 4 segments

34 merged, 19 queued, 5 replaced, 5 merging, 3 gate, 2 working

| Segment | What | State | Seat |
|---|---|---|---|
| `p4x-wall-mesh-winding` | Wall, floor and ceiling meshes wind outward | working | kimi-k31 |
| `p4r-join-x-crossing` | A wall crossed by another keeps its full length | merging | claude-sonnet-b |
| `p4r-graph-linear-build` | WallGraph::build is linear in the wall count | merging | bonsai-b |
| `p4r-doc-validate` | ArchModel::validate reports bad heights, stale id counters and accepts the built-in wall types | merging | bonsai-a |
| `p4r-grip-wall-stretch` | Wall and opening grip drags are one undo step; a wall has two grips; locked layers refuse | merging | claude-sonnet-a |
| `p4r-wall-cmd-validate` | The wall command validates everything before it touches the drawing | gate | darwin-ws3 |
| `p4r-io-proxy-linework` | Arch proxy polylines survive a file whose arch record is missing, unreadable or newer | gate | bonsai-ws2-b |
| `p4r-render3d-lints` | render3d and arch3d use the workspace lints; the orbit azimuth doc matches the code | merging | ornith-b |
| `p4r-cli-render-args` | cadcraft-cli refuses render sizes, cameras and framing options it cannot honour | working | ornith-a |
| `p4r-cli-glb-wall-plan` | The CLI GLB tests use a real wall plan and their own temp directories | gate | bonsai-ws2-a |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 27 04:56 | `p4r-camera-validate` | merged | 3,822 pass | bonsai-ws2-a |
| Oct 27 04:51 | `p4r-glb-gltf-conformance` | merged | 3,820 pass | ornith-a |
| Oct 27 04:46 | `p4r-stretch-no-collapse` | merged | 3,817 pass | claude-sonnet-a |
| Oct 27 04:40 | `harden-p3-probe-scene-and-required-tests` | merged | 3,816 pass | tturbo-mac |
| Oct 27 04:33 | `p4r-layer-band-clamp` | merged | 3,811 pass | ornith-b |
| Oct 27 04:24 | `p4x-cli-render-draws` | merged | 3,809 pass | ornith-a |
| Oct 27 04:21 | `harden-p3-engine-level-command` | merged | 3,808 pass | tturbo-mac2 |
| Oct 27 04:17 | `p4-docs-roof` | merged | 3,795 pass | claude-sonnet-a |
| Oct 27 04:09 | `p4g-golden-roofs` | merged | 3,795 pass | kimi-k31 |
| Oct 27 03:47 | `harden-p4g-gable-clip-test-names` | merged | 3,793 pass | tturbo-mac |
| Oct 27 03:38 | `upstream-sync-d864a08b0` | merged | 3,788 pass | claude-sonnet-b |
| Oct 27 03:33 | `p4g-gable-clip-test-names` | merged | 3,788 pass | bonsai-ws2-a |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | merging | 480 |
| bonsai-b | bonsai2-27b | merging | 431 |
| bonsai-ws2-a | bonsai2-27b | gate | 617 |
| bonsai-ws2-b | bonsai2-27b | gate | 585 |
| claude-sonnet-a | sonnet | merging | 7 |
| claude-sonnet-b | sonnet | merging | 7 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | gate | 264 |
| kimi-k31 | k3.1-pro-highspeed | agent | 56 |
| ornith-a | ornith15-397b-exl3-35 | agent | 7 |
| ornith-b | ornith15-397b-exl3-35 | merging | 4 |
| tturbo-mac | tturbo-ultraheretic | merging | 283 |
| tturbo-mac2 | tturbo-ultraheretic | merging | 90 |
