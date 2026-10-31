# arch3d build status

Updated 2026-10-31 05:30 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 9 of 13: Terrain and site — 19 of 27 segments merged · 2 of 12 acceptance checks**

5,520 workspace tests passing at the last merge · 554 segments merged in all · 0 of 13 seats working

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
| 9 | Terrain and site | building | 2 / 12 | 9.9 h so far · expected done Oct 31 11:15–Oct 31 21:46 UTC |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 9 segments

19 merged, 3 gate, 3 queued, 2 merging

| Segment | What | State | Seat |
|---|---|---|---|
| `p9-arch-terrain-contours` | Contours of the site mesh, minor and major, with labels | gate | bonsai-ws2-b |
| `p9-arch-terrain-volumes` | Cut and fill, exact on piecewise-linear surfaces | gate | claude-sonnet-b |
| `p9-arch-retaining` | Retaining walls follow the ground on each side | merging | claude-sonnet-a |
| `p9-arch3d-terrain-solid` | The terrain solid with its skirt, and draped paths | gate | claude-sonnet-d |
| `upstream-sync-b22a06826` | merge upstream upstream/main up to b22a06826 (35 commits) | merging | claude-sonnet-c |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 31 05:22 | `p9-arch-terrain-mesh` | merged | 5,520 pass | bonsai-ws2-b |
| Oct 31 05:09 | `upstream-sync-b22a06826` | refused | 5,795 pass | claude-sonnet-c |
| Oct 31 04:49 | `harden-p9-arch-terrain-tin` | merged | 5,514 pass | tturbo-mac2 |
| Oct 31 04:36 | `p9-arch3d-plants` | merged | 5,510 pass | kimi-k31 |
| Oct 31 04:22 | `harden-p9-cmd-site-inspect` | merged | 5,506 pass | tturbo-mac2 |
| Oct 31 04:07 | `p9-cmd-terrain` | merged | 5,501 pass | mai-flash |
| Oct 31 03:47 | `upstream-sync-ba3196e3f` | merged | 5,496 pass | claude-sonnet-b |
| Oct 31 03:31 | `p9-cmd-terrain-modify` | merged | 5,475 pass | claude-sonnet-d |
| Oct 31 03:11 | `p9-cmd-site-inspect` | merged | 5,470 pass | claude-sonnet-c |
| Oct 31 02:53 | `harden-p8-housekeeping-guide-readme-uk` | merged | 5,466 pass | tturbo-mac |
| Oct 31 02:29 | `p9-arch-terrain-surface` | merged | 5,453 pass | claude-sonnet-b |
| Oct 31 01:59 | `harden-p9-geom-tin-ops` | merged | 5,446 pass | tturbo-mac2 |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 707 |
| bonsai-b | bonsai2-27b | agent | 666 |
| bonsai-ws2-a | bonsai2-27b | agent | 706 |
| bonsai-ws2-b | bonsai2-27b | gate | 674 |
| claude-sonnet-a | sonnet | idle | 56 |
| claude-sonnet-b | sonnet | gate | 52 |
| claude-sonnet-c | sonnet | merging | 5 |
| claude-sonnet-d | sonnet | gate | 1 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | gate | 563 |
| kimi-k31 | k3.1-pro-highspeed | idle | 101 |
| mai-flash | mai-code-1.1-flash | idle | 28 |
| tturbo-mac | tturbo-ultraheretic | merging | 489 |
| tturbo-mac2 | tturbo-ultraheretic | merging | 311 |
