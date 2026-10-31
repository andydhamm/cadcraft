# arch3d build status

Updated 2026-10-31 07:45 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 9 of 13: Terrain and site — 24 of 28 segments merged · 2 of 12 acceptance checks**

5,834 workspace tests passing at the last merge · 561 segments merged in all · 0 of 13 seats working

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
| 9 | Terrain and site | building | 2 / 12 | 12.1 h so far · expected done Oct 31 11:05–Oct 31 19:36 UTC |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 9 segments

24 merged, 2 gate, 1 working, 1 merging

| Segment | What | State | Seat |
|---|---|---|---|
| `p9-cmd-terrain-query` | `terrain.query`, `terrain.contours` and `terrain.volumes` | gate | claude-sonnet-b |
| `p9-render-site-plan` | The site in the plan: contours, labels, boundary, paths, plants, north arrow | working | bonsai-ws2-a |
| `p9-io-site-proxies` | Site proxies on save, the arch record v8 | gate | claude-sonnet-a |
| `upstream-sync-a1335895f` | merge upstream upstream/main up to a1335895f (19 commits) | merging | claude-sonnet-c |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 31 07:20 | `p9-arch-terrain-contours` | merged | 5,834 pass | kimi-k31 |
| Oct 31 07:08 | `p9-arch-terrain-volumes` | merged | 5,828 pass | claude-sonnet-b |
| Oct 31 06:57 | `p9-arch3d-terrain-solid` | merged | 5,824 pass | claude-sonnet-d |
| Oct 31 06:41 | `upstream-sync-b22a06826` | merged | 5,819 pass | claude-sonnet-c |
| Oct 31 06:25 | `harden-p9-cmd-terrain` | merged | 5,538 pass | tturbo-mac2 |
| Oct 31 06:05 | `harden-p9-cmd-terrain-modify` | merged | 5,531 pass | tturbo-mac |
| Oct 31 05:52 | `p9-arch-retaining` | merged | 5,524 pass | claude-sonnet-a |
| Oct 31 05:22 | `p9-arch-terrain-mesh` | merged | 5,520 pass | bonsai-ws2-b |
| Oct 31 05:09 | `upstream-sync-b22a06826` | refused | 5,795 pass | claude-sonnet-c |
| Oct 31 04:49 | `harden-p9-arch-terrain-tin` | merged | 5,514 pass | tturbo-mac2 |
| Oct 31 04:36 | `p9-arch3d-plants` | merged | 5,510 pass | kimi-k31 |
| Oct 31 04:22 | `harden-p9-cmd-site-inspect` | merged | 5,506 pass | tturbo-mac2 |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | gate | 709 |
| bonsai-b | bonsai2-27b | agent | 670 |
| bonsai-ws2-a | bonsai2-27b | agent | 710 |
| bonsai-ws2-b | bonsai2-27b | merging | 682 |
| claude-sonnet-a | sonnet | gate | 57 |
| claude-sonnet-b | sonnet | gate | 54 |
| claude-sonnet-c | sonnet | merging | 6 |
| claude-sonnet-d | sonnet | idle | 2 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | gate | 576 |
| kimi-k31 | k3.1-pro-highspeed | idle | 103 |
| mai-flash | mai-code-1.1-flash | idle | 28 |
| tturbo-mac | tturbo-ultraheretic | merging | 490 |
| tturbo-mac2 | tturbo-ultraheretic | merging | 314 |
