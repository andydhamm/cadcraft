# arch3d build status

Updated 2026-10-30 09:15 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 8 of 13: Recognition from plain DXF — 12 of 23 segments merged · 2 of 10 acceptance checks**

5,213 workspace tests passing at the last merge · 501 segments merged in all · 0 of 11 seats working

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
| 8 | Recognition from plain DXF | building | 2 / 10 | 6.2 h so far · expected done Oct 30 17:01–Oct 31 05:24 UTC |
| 9 | Terrain and site | planned | — | — |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 8 segments

12 merged, 6 queued, 3 working, 2 merging

| Segment | What | State | Seat |
|---|---|---|---|
| `p8-detect-walls` | Wall runs from parallel face lines | merging | claude-sonnet-b |
| `p8-detect-junctions` | Snap wall run ends into exact L, T and X junctions | working | bonsai-b |
| `p8-detect-openings` | Doors (swing arcs, sliding leaves) and windows (glass lines between jambs) on wall runs | working | bonsai-a |
| `p8-detect-rooms` | Rooms from the detected walls, named from the labels inside them | merging | claude-sonnet-a |
| `p8-detect-score` | Truth files and one-to-one scoring (tests/fixtures/archdetect) | working | darwin-ws3 |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 30 09:07 | `harden-p8-io-opening-proxies-whole-symbol` | merged | 5,213 pass | tturbo-mac |
| Oct 30 09:01 | `p8-detect-arcs` | merged | 5,209 pass | mai-flash |
| Oct 30 08:50 | `p8-detect-walls` | refused | 0 pass | claude-sonnet-b |
| Oct 30 08:50 | `p8-detect-layers` | merged | 5,205 pass | darwin-ws3 |
| Oct 30 08:35 | `p8-detect-flatten` | merged | 5,202 pass | claude-sonnet-a |
| Oct 30 08:20 | `harden-p8-offset-is-the-centre` | merged | 5,193 pass | tturbo-mac |
| Oct 30 08:02 | `p8-detect-skeleton` | merged | 5,189 pass | darwin-ws3 |
| Oct 30 07:36 | `p8-io-opening-proxies-whole-symbol` | merged | 5,184 pass | kimi-k31 |
| Oct 30 07:13 | `harden-p7-engine-viewsheet` | merged | 5,181 pass | tturbo-mac |
| Oct 30 06:50 | `p8-offset-is-the-centre` | merged | 5,169 pass | mai-flash |
| Oct 30 06:09 | `p8-header-inside-the-wall` | merged | 5,162 pass | claude-sonnet-b |
| Oct 30 05:38 | `p8-engine-convert-to-metres` | merged | 5,160 pass | claude-sonnet-a |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 674 |
| bonsai-b | bonsai2-27b | agent | 632 |
| bonsai-ws2-a | bonsai2-27b | held | 700 |
| bonsai-ws2-b | bonsai2-27b | held | 650 |
| claude-sonnet-a | sonnet | merging | 48 |
| claude-sonnet-b | sonnet | merging | 45 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | agent | 523 |
| kimi-k31 | k3.1-pro-highspeed | idle | 91 |
| mai-flash | mai-code-1.1-flash | idle | 21 |
| tturbo-mac | tturbo-ultraheretic | agent | 451 |
| tturbo-mac2 | tturbo-ultraheretic | agent | 273 |
