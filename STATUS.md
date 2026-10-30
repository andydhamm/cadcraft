# arch3d build status

Updated 2026-10-30 11:15 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 8 of 13: Recognition from plain DXF — 17 of 24 segments merged · 2 of 10 acceptance checks**

5,251 workspace tests passing at the last merge · 508 segments merged in all · 0 of 11 seats working

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
| 8 | Recognition from plain DXF | building | 2 / 10 | 8.2 h so far · expected done Oct 30 15:59–Oct 31 01:28 UTC |
| 9 | Terrain and site | planned | — | — |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 8 segments

17 merged, 6 queued, 1 escalate

| Segment | What | State | Seat |
|---|---|---|---|
| `p8-detect-openings` | Doors (swing arcs, sliding leaves) and windows (glass lines between jambs) on wall runs | escalate |  |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 30 11:03 | `p8-detect-junctions` | merged | 5,251 pass | bonsai-a |
| Oct 30 10:56 | `harden-p8-detect-arcs` | merged | 5,244 pass | tturbo-mac |
| Oct 30 10:46 | `p8-detect-score` | merged | 5,237 pass | darwin-ws3 |
| Oct 30 10:13 | `upstream-sync-f536bf406` | merged | 5,232 pass | claude-sonnet-a |
| Oct 30 09:57 | `harden-p8-detect-flatten` | merged | 5,232 pass | tturbo-mac2 |
| Oct 30 09:46 | `p8-detect-walls` | merged | 5,222 pass | claude-sonnet-b |
| Oct 30 09:18 | `p8-detect-rooms` | merged | 5,216 pass | claude-sonnet-a |
| Oct 30 09:07 | `harden-p8-io-opening-proxies-whole-symbol` | merged | 5,213 pass | tturbo-mac |
| Oct 30 09:01 | `p8-detect-arcs` | merged | 5,209 pass | mai-flash |
| Oct 30 08:50 | `p8-detect-walls` | refused | 0 pass | claude-sonnet-b |
| Oct 30 08:50 | `p8-detect-layers` | merged | 5,205 pass | darwin-ws3 |
| Oct 30 08:35 | `p8-detect-flatten` | merged | 5,202 pass | claude-sonnet-a |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 682 |
| bonsai-b | bonsai2-27b | idle | 641 |
| bonsai-ws2-a | bonsai2-27b | held | 700 |
| bonsai-ws2-b | bonsai2-27b | held | 650 |
| claude-sonnet-a | sonnet | idle | 49 |
| claude-sonnet-b | sonnet | idle | 45 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | agent | 530 |
| kimi-k31 | k3.1-pro-highspeed | idle | 92 |
| mai-flash | mai-code-1.1-flash | idle | 21 |
| tturbo-mac | tturbo-ultraheretic | merging | 456 |
| tturbo-mac2 | tturbo-ultraheretic | agent | 278 |
