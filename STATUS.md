# arch3d build status

Updated 2026-10-31 03:45 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 9 of 13: Terrain and site — 15 of 26 segments merged · 0 of 12 acceptance checks**

5,475 workspace tests passing at the last merge · 548 segments merged in all · 0 of 13 seats working · PAUSED

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
| 9 | Terrain and site | building | 0 / 12 | 8.1 h so far · expected done Oct 31 11:58–Nov 01 00:46 UTC |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 9 segments

15 merged, 6 queued, 4 merging, 1 gate

| Segment | What | State | Seat |
|---|---|---|---|
| `p9-arch-terrain-mesh` | The site mesh: breaklines as edges, an elevation per triangle corner | merging | claude-sonnet-b |
| `p9-arch-retaining` | Retaining walls follow the ground on each side | merging | claude-sonnet-a |
| `p9-cmd-terrain` | `terrain` and `terrain.delete` commands | merging | mai-flash |
| `p9-arch3d-plants` | Plants in 3D: trunk and crown | gate | kimi-k31 |
| `upstream-sync-ba3196e3f` | merge upstream upstream/main up to ba3196e3f (14 commits) | merging | claude-sonnet-b |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 31 03:31 | `p9-cmd-terrain-modify` | merged | 5,475 pass | claude-sonnet-d |
| Oct 31 03:11 | `p9-cmd-site-inspect` | merged | 5,470 pass | claude-sonnet-c |
| Oct 31 02:53 | `harden-p8-housekeeping-guide-readme-uk` | merged | 5,466 pass | tturbo-mac |
| Oct 31 02:29 | `p9-arch-terrain-surface` | merged | 5,453 pass | claude-sonnet-b |
| Oct 31 01:59 | `harden-p9-geom-tin-ops` | merged | 5,446 pass | tturbo-mac2 |
| Oct 31 01:18 | `p9-arch-terrain-tin` | merged | 5,438 pass | mai-flash |
| Oct 31 00:45 | `harden-p9-arch-plant` | merged | 5,419 pass | tturbo-mac2 |
| Oct 31 00:23 | `p9-cmd-site` | merged | 5,408 pass | bonsai-b |
| Oct 30 23:48 | `p9-cmd-plant` | merged | 5,403 pass | mai-flash |
| Oct 30 23:05 | `p9-cmd-wall-retaining` | merged | 5,398 pass | claude-sonnet-a |
| Oct 30 22:34 | `p9-arch-plant` | merged | 5,395 pass | darwin-ws3 |
| Oct 30 22:06 | `p9-arch-terrain-pad` | merged | 5,390 pass | mai-flash |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | idle | 707 |
| bonsai-b | bonsai2-27b | idle | 666 |
| bonsai-ws2-a | bonsai2-27b | idle | 706 |
| bonsai-ws2-b | bonsai2-27b | gate | 668 |
| claude-sonnet-a | sonnet | idle | 56 |
| claude-sonnet-b | sonnet | merging | 52 |
| claude-sonnet-c | sonnet | idle | 1 |
| claude-sonnet-d | sonnet | idle | 1 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | idle | 563 |
| kimi-k31 | k3.1-pro-highspeed | gate | 100 |
| mai-flash | mai-code-1.1-flash | idle | 28 |
| tturbo-mac | tturbo-ultraheretic | agent | 485 |
| tturbo-mac2 | tturbo-ultraheretic | merging | 308 |
