# arch3d build status

Updated 2026-10-29 22:30 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 7 of 13: Generated elevations and sections — 8 of 12 segments merged · 1 of 9 acceptance checks**

5,029 workspace tests passing at the last merge · 476 segments merged in all · 0 of 11 seats working

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
| 7 | Generated elevations and sections | building | 1 / 9 | 5.3 h so far · expected done Oct 30 04:51–Oct 30 14:32 UTC |
| 8 | Recognition from plain DXF | planned | — | — |
| 9 | Terrain and site | planned | — | — |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 7 segments

8 merged, 2 queued, 1 gate, 1 working

| Segment | What | State | Seat |
|---|---|---|---|
| `p7-arch3d-views` | View generator: elevations and sections from the HLR/section kernel, with overall dimensions | gate | claude-sonnet-b |
| `p7-engine-viewsheet` | viewsheet: linked views on a layout at their scale (ARCH D at 1:50) | working | darwin-ws3 |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 29 22:24 | `harden-p7-hlr-kinds` | merged | 5,029 pass | tturbo-mac2 |
| Oct 29 22:22 | `p7-roof-eave-at-the-wall` | merged | 5,025 pass | kimi-k31 |
| Oct 29 22:06 | `p7-render-views` | merged | 5,021 pass | claude-sonnet-b |
| Oct 29 21:40 | `p7-io-views` | merged | 5,015 pass | mai-flash |
| Oct 29 21:32 | `p7-hlr-kinds` | merged | 5,011 pass | claude-sonnet-a |
| Oct 29 19:22 | `harden-p7-render-arch-in-viewports` | merged | 5,001 pass | tturbo-mac2 |
| Oct 29 19:18 | `harden-p7-ui-pick-selects` | merged | 4,996 pass | tturbo-mac |
| Oct 29 18:52 | `harden-p6f-r3d-glass-house` | merged | 4,991 pass | tturbo-mac |
| Oct 29 18:43 | `p7-ui-pick-selects` | merged | 4,982 pass | bonsai-a |
| Oct 29 18:32 | `p7-render-arch-in-viewports` | merged | 4,980 pass | claude-sonnet-a |
| Oct 29 18:21 | `p7-handles-one-form` | merged | 4,976 pass | claude-sonnet-b |
| Oct 29 18:09 | `harden-p6-cmd-camera` | merged | 4,972 pass | tturbo-mac2 |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 644 |
| bonsai-b | bonsai2-27b | gate | 604 |
| bonsai-ws2-a | bonsai2-27b | held | 700 |
| bonsai-ws2-b | bonsai2-27b | held | 650 |
| claude-sonnet-a | sonnet | idle | 41 |
| claude-sonnet-b | sonnet | gate | 38 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | agent | 488 |
| kimi-k31 | k3.1-pro-highspeed | idle | 85 |
| mai-flash | mai-code-1.1-flash | idle | 14 |
| tturbo-mac | tturbo-ultraheretic | agent | 433 |
| tturbo-mac2 | tturbo-ultraheretic | idle | 266 |
