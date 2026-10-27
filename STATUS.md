# arch3d build status

Updated 2026-10-27 07:15 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 4 of 13: Automatic roofs — 50 of 63 segments merged · 5 of 11 acceptance checks**

3,887 workspace tests passing at the last merge · 274 segments merged in all · 0 of 12 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | building | 5 / 11 | 22.4 h so far · expected done Oct 27 14:15–Oct 27 19:44 UTC |
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

50 merged, 10 queued, 5 replaced, 2 escalate, 1 working

| Segment | What | State | Seat |
|---|---|---|---|
| `p4r-join-x-crossing` | A wall crossed by another keeps its full length | escalate |  |
| `p4r-mesh-side-normals` | Wall side faces carry horizontal outward normals | working | bonsai-a |
| `p4r-stretch-start-keeps-openings` | Stretching a wall's start keeps its openings where they are | escalate |  |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 27 07:08 | `harden-p4r-scene-finite-f32` | merged | 3,887 pass | tturbo-mac |
| Oct 27 07:05 | `p4x-wall-mesh-winding` | merged | 3,883 pass | bonsai-a |
| Oct 27 07:02 | `p4r-dxf-proxy-solved-rings` | merged | 3,881 pass | ornith-b |
| Oct 27 07:00 | `p4r-arch-handles-reserved-top` | merged | 3,879 pass | claude-sonnet-a |
| Oct 27 06:55 | `p4r-arch-layers-on-first-use` | merged | 3,875 pass | bonsai-ws2-b |
| Oct 27 06:50 | `p4r-arch-handles-reserved-top` | refused | 0 pass | claude-sonnet-a |
| Oct 27 06:49 | `harden-p4r-render-arch-layer-state` | merged | 3,870 pass | tturbo-mac |
| Oct 27 06:45 | `p4r-arch-layers-on-first-use` | refused | 3,872 pass | bonsai-ws2-b |
| Oct 27 06:38 | `p4r-arch-handles-reserved-top` | refused | 3,871 pass | claude-sonnet-a |
| Oct 27 06:34 | `p4x-wall-mesh-winding` | refused | 3,869 pass | bonsai-a |
| Oct 27 06:29 | `harden-p4r-render-arch-layer-state` | refused | 3,870 pass | tturbo-mac |
| Oct 27 06:15 | `p4r-scene-finite-f32` | merged | 3,867 pass | claude-sonnet-b |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 493 |
| bonsai-b | bonsai2-27b | agent | 441 |
| bonsai-ws2-a | bonsai2-27b | idle | 632 |
| bonsai-ws2-b | bonsai2-27b | idle | 596 |
| claude-sonnet-a | sonnet | idle | 12 |
| claude-sonnet-b | sonnet | idle | 10 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | idle | 276 |
| kimi-k31 | k3.1-pro-highspeed | down | 58 |
| ornith-a | ornith15-397b-exl3-35 | agent | 16 |
| ornith-b | ornith15-397b-exl3-35 | idle | 11 |
| tturbo-mac | tturbo-ultraheretic | agent | 289 |
| tturbo-mac2 | tturbo-ultraheretic | agent | 96 |
