# arch3d build status

Updated 2026-10-31 02:45 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 9 of 13: Terrain and site — 13 of 26 segments merged · 0 of 12 acceptance checks**

5,453 workspace tests passing at the last merge · 545 segments merged in all · 0 of 13 seats working · PAUSED

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
| 9 | Terrain and site | building | 0 / 12 | 7.1 h so far · expected done Oct 31 12:49–Nov 01 03:14 UTC |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 9 segments

13 merged, 6 queued, 4 merging, 3 gate

| Segment | What | State | Seat |
|---|---|---|---|
| `p9-arch-terrain-mesh` | The site mesh: breaklines as edges, an elevation per triangle corner | gate | claude-sonnet-b |
| `p9-arch-retaining` | Retaining walls follow the ground on each side | merging | claude-sonnet-a |
| `p9-cmd-terrain` | `terrain` and `terrain.delete` commands | merging | mai-flash |
| `p9-cmd-terrain-modify` | `terrain.modify`: flat regions, hills, valleys, roads, driveways and sidewalks | merging | claude-sonnet-d |
| `p9-cmd-site-inspect` | `arch.inspect` site, terrain and plants | merging | claude-sonnet-c |
| `p9-arch3d-plants` | Plants in 3D: trunk and crown | gate | bonsai-a |
| `upstream-sync-ba3196e3f` | merge upstream upstream/main up to ba3196e3f (14 commits) | gate | bonsai-b |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 31 02:29 | `p9-arch-terrain-surface` | merged | 5,453 pass | claude-sonnet-b |
| Oct 31 01:59 | `harden-p9-geom-tin-ops` | merged | 5,446 pass | tturbo-mac2 |
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

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | gate | 704 |
| bonsai-b | bonsai2-27b | gate | 665 |
| bonsai-ws2-a | bonsai2-27b | gate | 704 |
| bonsai-ws2-b | bonsai2-27b | gate | 665 |
| claude-sonnet-a | sonnet | merging | 56 |
| claude-sonnet-b | sonnet | gate | 50 |
| claude-sonnet-c | sonnet | merging | 1 |
| claude-sonnet-d | sonnet | merging | 1 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | gate | 562 |
| kimi-k31 | k3.1-pro-highspeed | idle | 98 |
| mai-flash | mai-code-1.1-flash | merging | 28 |
| tturbo-mac | tturbo-ultraheretic | merging | 483 |
| tturbo-mac2 | tturbo-ultraheretic | merging | 306 |
