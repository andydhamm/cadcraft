# arch3d build status

Updated 2026-10-27 08:30 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 4 of 13: Automatic roofs — 53 of 63 segments merged · 5 of 11 acceptance checks**

3,924 workspace tests passing at the last merge · 280 segments merged in all · 0 of 12 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | building | 5 / 11 | 23.7 h so far · expected done Oct 27 14:12–Oct 27 18:52 UTC |
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

53 merged, 7 queued, 5 replaced, 2 working, 1 merging

| Segment | What | State | Seat |
|---|---|---|---|
| `p4r-join-t-junction` | T joins meet the host face, and a tee drawn as three ends butts the straight pair | working | bonsai-ws2-b |
| `p4r-golden-house-fixture` | The golden house test really loads its JSON fixture | merging | claude-sonnet-b |
| `p4r-mesh-side-normals` | Wall side faces carry horizontal outward normals | working | bonsai-a |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 27 08:29 | `p4r-join-raw-ring-fallback` | merged | 3,924 pass | bonsai-ws2-b |
| Oct 27 08:21 | `p4r-join-x-crossing` | merged | 3,923 pass | bonsai-ws2-a |
| Oct 27 08:18 | `harden-p4r-stretch-start-keeps-openings` | merged | 3,919 pass | tturbo-mac |
| Oct 27 08:16 | `harden-p4r-cli-glb-wall-plan` | merged | 3,914 pass | tturbo-mac2 |
| Oct 27 08:06 | `harden-p4r-dxf-proxy-solved-rings` | merged | 3,907 pass | tturbo-mac |
| Oct 27 08:04 | `p4r-stretch-start-keeps-openings` | merged | 3,891 pass | ornith-b |
| Oct 27 07:08 | `harden-p4r-scene-finite-f32` | merged | 3,887 pass | tturbo-mac |
| Oct 27 07:05 | `p4x-wall-mesh-winding` | merged | 3,883 pass | bonsai-a |
| Oct 27 07:02 | `p4r-dxf-proxy-solved-rings` | merged | 3,881 pass | ornith-b |
| Oct 27 07:00 | `p4r-arch-handles-reserved-top` | merged | 3,879 pass | claude-sonnet-a |
| Oct 27 06:55 | `p4r-arch-layers-on-first-use` | merged | 3,875 pass | bonsai-ws2-b |
| Oct 27 06:50 | `p4r-arch-handles-reserved-top` | refused | 0 pass | claude-sonnet-a |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 497 |
| bonsai-b | bonsai2-27b | idle | 445 |
| bonsai-ws2-a | bonsai2-27b | agent | 641 |
| bonsai-ws2-b | bonsai2-27b | agent | 599 |
| claude-sonnet-a | sonnet | idle | 12 |
| claude-sonnet-b | sonnet | merging | 11 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | agent | 277 |
| kimi-k31 | k3.1-pro-highspeed | idle | 58 |
| ornith-a | ornith15-397b-exl3-35 | idle | 19 |
| ornith-b | ornith15-397b-exl3-35 | idle | 13 |
| tturbo-mac | tturbo-ultraheretic | agent | 294 |
| tturbo-mac2 | tturbo-ultraheretic | merging | 101 |
