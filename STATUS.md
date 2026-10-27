# arch3d build status

Updated 2026-10-27 06:00 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 4 of 13: Automatic roofs — 42 of 63 segments merged · 5 of 11 acceptance checks**

3,857 workspace tests passing at the last merge · 263 segments merged in all · 0 of 12 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | building | 5 / 11 | 21.2 h so far · expected done Oct 27 16:30–Oct 28 00:06 UTC |
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

42 merged, 14 queued, 5 replaced, 3 merging, 2 gate, 1 escalate, 1 working

| Segment | What | State | Seat |
|---|---|---|---|
| `p4x-wall-mesh-winding` | Wall, floor and ceiling meshes wind outward | escalate |  |
| `p4r-join-x-crossing` | A wall crossed by another keeps its full length | gate | darwin-ws3 |
| `p4r-io-proxy-linework` | Arch proxy polylines survive a file whose arch record is missing, unreadable or newer | merging | ornith-b |
| `p4r-render-arch-layer-state` | Arch wall and opening poche follow their layers' on / frozen state (and no-plot when plotting) | merging | bonsai-a |
| `p4r-build-dirty-neighbours` | build_dirty rebuilds the walls whose joins a dirty wall changed | merging | claude-sonnet-b |
| `p4r-stretch-start-keeps-openings` | Stretching a wall's start keeps its openings where they are | gate | bonsai-ws2-a |
| `p4r-arch-layers-on-first-use` | New drawings have no arch layers; arch commands create theirs on first use | working | bonsai-ws2-b |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 27 05:58 | `harden-p4r-layer-band-clamp` | merged | 3,857 pass | tturbo-mac |
| Oct 27 05:55 | `p4r-cli-render-args` | merged | 3,845 pass | ornith-a |
| Oct 27 05:54 | `p4r-cli-glb-wall-plan` | merged | 3,842 pass | claude-sonnet-a |
| Oct 27 05:53 | `p4r-wall-cmd-validate` | merged | 3,842 pass | bonsai-ws2-b |
| Oct 27 05:48 | `p4r-ui-show3d-toggle` | merged | 3,837 pass | bonsai-b |
| Oct 27 05:47 | `p4r-grip-wall-stretch` | merged | 3,836 pass | bonsai-ws2-a |
| Oct 27 05:44 | `p4r-graph-linear-build` | merged | 3,831 pass | claude-sonnet-b |
| Oct 27 05:08 | `p4r-render3d-lints` | merged | 3,827 pass | ornith-b |
| Oct 27 05:03 | `p4r-doc-validate` | merged | 3,825 pass | bonsai-a |
| Oct 27 04:56 | `p4r-camera-validate` | merged | 3,822 pass | bonsai-ws2-a |
| Oct 27 04:51 | `p4r-glb-gltf-conformance` | merged | 3,820 pass | ornith-a |
| Oct 27 04:46 | `p4r-stretch-no-collapse` | merged | 3,817 pass | claude-sonnet-a |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | merging | 484 |
| bonsai-b | bonsai2-27b | agent | 433 |
| bonsai-ws2-a | bonsai2-27b | gate | 622 |
| bonsai-ws2-b | bonsai2-27b | agent | 587 |
| claude-sonnet-a | sonnet | idle | 8 |
| claude-sonnet-b | sonnet | merging | 9 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | gate | 269 |
| kimi-k31 | k3.1-pro-highspeed | down | 58 |
| ornith-a | ornith15-397b-exl3-35 | agent | 10 |
| ornith-b | ornith15-397b-exl3-35 | merging | 7 |
| tturbo-mac | tturbo-ultraheretic | agent | 284 |
| tturbo-mac2 | tturbo-ultraheretic | agent | 91 |
