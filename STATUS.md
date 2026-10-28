# arch3d build status

Updated 2026-10-28 04:30 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 4 of 13: Automatic roofs — 83 of 85 segments merged · 10 of 11 acceptance checks**

4,338 workspace tests passing at the last merge · 363 segments merged in all · 0 of 12 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | building | 10 / 11 | 43.7 h so far · expected done Oct 28 05:59–Oct 28 11:04 UTC |
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

83 merged, 8 replaced, 1 queued, 1 working

| Segment | What | State | Seat |
|---|---|---|---|
| `p4n-footprint-largest-component` | outer_footprint: build the ring by connectivity, per largest component | working | claude-sonnet-b |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 28 04:19 | `p4n-footprint-ring-helpers` | merged | 4,338 pass | claude-sonnet-b |
| Oct 28 03:59 | `harden-p4g-profile-slab-extrude` | merged | 4,333 pass | tturbo-mac2 |
| Oct 28 03:52 | `harden-p4r-grip-wall-stretch` | merged | 4,329 pass | tturbo-mac |
| Oct 28 03:39 | `harden-p2-render-openings` | merged | 4,324 pass | tturbo-mac2 |
| Oct 28 03:30 | `harden-p4g-gable-clip-wire` | merged | 4,319 pass | tturbo-mac |
| Oct 28 03:18 | `harden-p2-opening-commands` | merged | 4,314 pass | tturbo-mac2 |
| Oct 28 03:08 | `harden-p4r-build-dirty-neighbours` | merged | 4,309 pass | tturbo-mac |
| Oct 28 02:44 | `harden-p4f-gable-clip-probe` | merged | 4,304 pass | tturbo-mac |
| Oct 28 02:37 | `harden-p2-opening-move` | merged | 4,295 pass | tturbo-mac2 |
| Oct 28 02:06 | `harden-p2-glb-openings` | merged | 4,288 pass | tturbo-mac2 |
| Oct 28 00:33 | `upstream-sync-3929d94d8` | merged | 4,283 pass | claude-sonnet-a |
| Oct 28 00:19 | `harden-p2-grip-openings` | merged | 4,256 pass | tturbo-mac2 |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 540 |
| bonsai-b | bonsai2-27b | agent | 490 |
| bonsai-ws2-a | bonsai2-27b | held | 700 |
| bonsai-ws2-b | bonsai2-27b | held | 650 |
| claude-sonnet-a | sonnet | idle | 19 |
| claude-sonnet-b | sonnet | agent | 16 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | agent | 360 |
| kimi-k31 | k3.1-pro-highspeed | idle | 68 |
| ornith-a | ornith15-397b-exl3-35 | held | 29 |
| ornith-b | ornith15-397b-exl3-35 | held | 17 |
| tturbo-mac | tturbo-ultraheretic | merging | 352 |
| tturbo-mac2 | tturbo-ultraheretic | agent | 182 |
