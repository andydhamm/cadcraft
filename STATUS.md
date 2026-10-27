# arch3d build status

Updated 2026-10-27 05:45 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 4 of 13: Automatic roofs — 37 of 63 segments merged · 5 of 11 acceptance checks**

3,831 workspace tests passing at the last merge · 257 segments merged in all · 0 of 12 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | building | 5 / 11 | 20.9 h so far · expected done Oct 27 20:00–Oct 28 06:00 UTC |
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

37 merged, 16 queued, 5 replaced, 5 working, 5 merging

| Segment | What | State | Seat |
|---|---|---|---|
| `p4x-wall-mesh-winding` | Wall, floor and ceiling meshes wind outward | working | kimi-k31 |
| `p4r-join-x-crossing` | A wall crossed by another keeps its full length | working | darwin-ws3 |
| `p4r-grip-wall-stretch` | Wall and opening grip drags are one undo step; a wall has two grips; locked layers refuse | merging | bonsai-ws2-a |
| `p4r-wall-cmd-validate` | The wall command validates everything before it touches the drawing | merging | bonsai-ws2-b |
| `p4r-io-proxy-linework` | Arch proxy polylines survive a file whose arch record is missing, unreadable or newer | working | ornith-b |
| `p4r-cli-render-args` | cadcraft-cli refuses render sizes, cameras and framing options it cannot honour | merging | ornith-a |
| `p4r-cli-glb-wall-plan` | The CLI GLB tests use a real wall plan and their own temp directories | merging | claude-sonnet-a |
| `p4r-ui-show3d-toggle` | View > 3D View toggles back to the drawing | merging | bonsai-b |
| `p4r-render-arch-layer-state` | Arch wall and opening poche follow their layers' on / frozen state (and no-plot when plotting) | working | bonsai-a |
| `p4r-build-dirty-neighbours` | build_dirty rebuilds the walls whose joins a dirty wall changed | working | claude-sonnet-b |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 27 05:44 | `p4r-graph-linear-build` | merged | 3,831 pass | claude-sonnet-b |
| Oct 27 05:08 | `p4r-render3d-lints` | merged | 3,827 pass | ornith-b |
| Oct 27 05:03 | `p4r-doc-validate` | merged | 3,825 pass | bonsai-a |
| Oct 27 04:56 | `p4r-camera-validate` | merged | 3,822 pass | bonsai-ws2-a |
| Oct 27 04:51 | `p4r-glb-gltf-conformance` | merged | 3,820 pass | ornith-a |
| Oct 27 04:46 | `p4r-stretch-no-collapse` | merged | 3,817 pass | claude-sonnet-a |
| Oct 27 04:40 | `harden-p3-probe-scene-and-required-tests` | merged | 3,816 pass | tturbo-mac |
| Oct 27 04:33 | `p4r-layer-band-clamp` | merged | 3,811 pass | ornith-b |
| Oct 27 04:24 | `p4x-cli-render-draws` | merged | 3,809 pass | ornith-a |
| Oct 27 04:21 | `harden-p3-engine-level-command` | merged | 3,808 pass | tturbo-mac2 |
| Oct 27 04:17 | `p4-docs-roof` | merged | 3,795 pass | claude-sonnet-a |
| Oct 27 04:09 | `p4g-golden-roofs` | merged | 3,795 pass | kimi-k31 |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 483 |
| bonsai-b | bonsai2-27b | merging | 432 |
| bonsai-ws2-a | bonsai2-27b | merging | 619 |
| bonsai-ws2-b | bonsai2-27b | merging | 587 |
| claude-sonnet-a | sonnet | merging | 8 |
| claude-sonnet-b | sonnet | agent | 8 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | agent | 267 |
| kimi-k31 | k3.1-pro-highspeed | agent | 58 |
| ornith-a | ornith15-397b-exl3-35 | merging | 9 |
| ornith-b | ornith15-397b-exl3-35 | agent | 6 |
| tturbo-mac | tturbo-ultraheretic | gate | 283 |
| tturbo-mac2 | tturbo-ultraheretic | agent | 90 |
