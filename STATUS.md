# arch3d build status

Updated 2026-10-27 06:45 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 4 of 13: Automatic roofs — 46 of 63 segments merged · 5 of 11 acceptance checks**

3,867 workspace tests passing at the last merge · 268 segments merged in all · 0 of 12 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | building | 5 / 11 | 21.9 h so far · expected done Oct 27 15:53–Oct 27 22:43 UTC |
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

46 merged, 11 queued, 5 replaced, 4 working, 2 escalate

| Segment | What | State | Seat |
|---|---|---|---|
| `p4x-wall-mesh-winding` | Wall, floor and ceiling meshes wind outward | working | kimi-k31 |
| `p4r-join-x-crossing` | A wall crossed by another keeps its full length | escalate |  |
| `p4r-dxf-proxy-solved-rings` | Saved wall proxy polylines use the mitred join rings, not overlapping rectangles | working | ornith-b |
| `p4r-arch-handles-reserved-top` | Arch proxy handles live at the top of u64 and never collide with file handles | working | claude-sonnet-a |
| `p4r-stretch-start-keeps-openings` | Stretching a wall's start keeps its openings where they are | escalate |  |
| `p4r-arch-layers-on-first-use` | New drawings have no arch layers; arch commands create theirs on first use | working | bonsai-ws2-b |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 27 06:45 | `p4r-arch-layers-on-first-use` | refused | 3,872 pass | bonsai-ws2-b |
| Oct 27 06:38 | `p4r-arch-handles-reserved-top` | refused | 3,871 pass | claude-sonnet-a |
| Oct 27 06:34 | `p4x-wall-mesh-winding` | refused | 3,869 pass | bonsai-a |
| Oct 27 06:29 | `harden-p4r-render-arch-layer-state` | refused | 3,870 pass | tturbo-mac |
| Oct 27 06:15 | `p4r-scene-finite-f32` | merged | 3,867 pass | claude-sonnet-b |
| Oct 27 06:12 | `harden-p4r-cli-render-args` | merged | 3,866 pass | tturbo-mac |
| Oct 27 06:11 | `p4r-io-proxy-linework` | merged | 3,863 pass | ornith-b |
| Oct 27 06:07 | `p4r-render-arch-layer-state` | merged | 3,861 pass | bonsai-a |
| Oct 27 06:03 | `p4r-build-dirty-neighbours` | merged | 3,859 pass | claude-sonnet-b |
| Oct 27 05:58 | `harden-p4r-layer-band-clamp` | merged | 3,857 pass | tturbo-mac |
| Oct 27 05:55 | `p4r-cli-render-args` | merged | 3,845 pass | ornith-a |
| Oct 27 05:54 | `p4r-cli-glb-wall-plan` | merged | 3,842 pass | claude-sonnet-a |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 490 |
| bonsai-b | bonsai2-27b | agent | 436 |
| bonsai-ws2-a | bonsai2-27b | agent | 630 |
| bonsai-ws2-b | bonsai2-27b | agent | 595 |
| claude-sonnet-a | sonnet | agent | 10 |
| claude-sonnet-b | sonnet | idle | 10 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | idle | 274 |
| kimi-k31 | k3.1-pro-highspeed | agent | 58 |
| ornith-a | ornith15-397b-exl3-35 | agent | 13 |
| ornith-b | ornith15-397b-exl3-35 | agent | 9 |
| tturbo-mac | tturbo-ultraheretic | merging | 287 |
| tturbo-mac2 | tturbo-ultraheretic | agent | 93 |
