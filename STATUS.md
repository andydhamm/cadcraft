# arch3d build status

Updated 2026-10-31 01:30 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 9 of 13: Terrain and site — 12 of 25 segments merged · 0 of 12 acceptance checks**

5,438 workspace tests passing at the last merge · 543 segments merged in all · 0 of 13 seats working

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
| 8 | Recognition from plain DXF | done | 10 / 10 | 16.6 h |
| 9 | Terrain and site | building | 0 / 12 | 5.9 h so far · expected done Oct 31 10:50–Nov 01 00:11 UTC |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 9 segments

12 merged, 9 queued, 3 working, 1 gate

| Segment | What | State | Seat |
|---|---|---|---|
| `p9-arch-terrain-surface` | The modified surface as an exact elevation function | gate | claude-sonnet-b |
| `p9-cmd-terrain` | `terrain` and `terrain.delete` commands | working | mai-flash |
| `p9-cmd-terrain-modify` | `terrain.modify`: flat regions, hills, valleys, roads, driveways and sidewalks | working | claude-sonnet-d |
| `p9-cmd-site-inspect` | `arch.inspect` site, terrain and plants | working | claude-sonnet-c |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 31 01:18 | `p9-arch-terrain-tin` | merged | 5,438 pass | mai-flash |
| Oct 31 00:45 | `harden-p9-arch-plant` | merged | 5,419 pass | tturbo-mac2 |
| Oct 31 00:23 | `p9-cmd-site` | merged | 5,408 pass | bonsai-b |
| Oct 30 23:48 | `p9-cmd-plant` | merged | 5,403 pass | mai-flash |
| Oct 30 23:05 | `p9-cmd-wall-retaining` | merged | 5,398 pass | claude-sonnet-a |
| Oct 30 22:34 | `p9-arch-plant` | merged | 5,395 pass | darwin-ws3 |
| Oct 30 22:06 | `p9-arch-terrain-pad` | merged | 5,390 pass | mai-flash |
| Oct 30 21:33 | `p9-arch-site` | merged | 5,385 pass | claude-sonnet-b |
| Oct 30 20:57 | `p9-arch-terrain-csv` | merged | 5,381 pass | claude-sonnet-a |
| Oct 30 17:41 | `harden-p8f-walls-stop-at-crossing-faces` | merged | 5,364 pass | tturbo-mac2 |
| Oct 30 17:32 | `harden-p8f-rooms-arrangement-once` | merged | 5,359 pass | tturbo-mac |
| Oct 30 17:07 | `harden-p8f-orbit-fit-bounding-sphere` | merged | 5,351 pass | tturbo-mac |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | idle | 704 |
| bonsai-b | bonsai2-27b | idle | 663 |
| bonsai-ws2-a | bonsai2-27b | idle | 704 |
| bonsai-ws2-b | bonsai2-27b | gate | 661 |
| claude-sonnet-a | sonnet | idle | 55 |
| claude-sonnet-b | sonnet | gate | 49 |
| claude-sonnet-c | sonnet | agent | 0 |
| claude-sonnet-d | sonnet | agent | 0 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | gate | 559 |
| kimi-k31 | k3.1-pro-highspeed | idle | 98 |
| mai-flash | mai-code-1.1-flash | agent | 27 |
| tturbo-mac | tturbo-ultraheretic | agent | 482 |
| tturbo-mac2 | tturbo-ultraheretic | merging | 305 |
