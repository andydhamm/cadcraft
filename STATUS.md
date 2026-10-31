# arch3d build status

Updated 2026-10-31 08:30 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 9 of 13: Terrain and site — 25 of 29 segments merged · 2 of 12 acceptance checks**

5,852 workspace tests passing at the last merge · 563 segments merged in all · 0 of 13 seats working

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
| 9 | Terrain and site | building | 2 / 12 | 12.9 h so far · expected done Oct 31 11:41–Oct 31 19:47 UTC |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 9 segments

25 merged, 2 merging, 2 working

| Segment | What | State | Seat |
|---|---|---|---|
| `p9-cmd-terrain-query` | `terrain.query`, `terrain.contours` and `terrain.volumes` | merging | claude-sonnet-b |
| `p9-render-site-plan` | The site in the plan: contours, labels, boundary, paths, plants, north arrow | working | kimi-k31 |
| `p9-io-site-proxies` | Site proxies on save, the arch record v8 | merging | claude-sonnet-a |
| `upstream-sync-7cee28580` | merge upstream upstream/main up to 7cee28580 (5 commits) | working | claude-sonnet-c |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 31 08:26 | `upstream-sync-a1335895f` | merged | 5,852 pass | claude-sonnet-c |
| Oct 31 07:48 | `harden-p9-arch-retaining` | merged | 5,844 pass | tturbo-mac |
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

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 712 |
| bonsai-b | bonsai2-27b | agent | 672 |
| bonsai-ws2-a | bonsai2-27b | agent | 712 |
| bonsai-ws2-b | bonsai2-27b | agent | 683 |
| claude-sonnet-a | sonnet | merging | 58 |
| claude-sonnet-b | sonnet | merging | 55 |
| claude-sonnet-c | sonnet | agent | 6 |
| claude-sonnet-d | sonnet | idle | 2 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | gate | 577 |
| kimi-k31 | k3.1-pro-highspeed | agent | 103 |
| mai-flash | mai-code-1.1-flash | idle | 28 |
| tturbo-mac | tturbo-ultraheretic | merging | 491 |
| tturbo-mac2 | tturbo-ultraheretic | merging | 314 |
