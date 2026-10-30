# arch3d build status

Updated 2026-10-30 08:15 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 8 of 13: Recognition from plain DXF — 9 of 23 segments merged · 2 of 10 acceptance checks**

5,189 workspace tests passing at the last merge · 496 segments merged in all · 0 of 11 seats working

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
| 8 | Recognition from plain DXF | building | 2 / 10 | 5.2 h so far · expected done Oct 30 20:06–Oct 31 12:51 UTC |
| 9 | Terrain and site | planned | — | — |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 8 segments

9 merged, 9 queued, 4 gate, 1 working

| Segment | What | State | Seat |
|---|---|---|---|
| `p8-detect-layers` | Layer roles from AIA/NCS names (they raise a score, never required) | gate | darwin-ws3 |
| `p8-detect-arcs` | Arcs from tessellated polylines | gate | mai-flash |
| `p8-detect-flatten` | Flatten a drawing to metric linework: entities, polylines, INSERTs (nested, arrays), labels | gate | claude-sonnet-a |
| `p8-detect-walls` | Wall runs from parallel face lines | working | claude-sonnet-b |
| `p8-detect-junctions` | Snap wall run ends into exact L, T and X junctions | gate | bonsai-b |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 30 08:02 | `p8-detect-skeleton` | merged | 5,189 pass | darwin-ws3 |
| Oct 30 07:36 | `p8-io-opening-proxies-whole-symbol` | merged | 5,184 pass | kimi-k31 |
| Oct 30 07:13 | `harden-p7-engine-viewsheet` | merged | 5,181 pass | tturbo-mac |
| Oct 30 06:50 | `p8-offset-is-the-centre` | merged | 5,169 pass | mai-flash |
| Oct 30 06:09 | `p8-header-inside-the-wall` | merged | 5,162 pass | claude-sonnet-b |
| Oct 30 05:38 | `p8-engine-convert-to-metres` | merged | 5,160 pass | claude-sonnet-a |
| Oct 30 05:09 | `harden-p7-engine-viewsheet` | refused | 0 pass | tturbo-mac |
| Oct 30 04:36 | `p8-arch-drawings-are-metres` | merged | 5,157 pass | claude-sonnet-a |
| Oct 30 03:05 | `harden-p7f-viewsheet-view-ids` | merged | 5,154 pass | tturbo-mac2 |
| Oct 30 03:03 | `upstream-sync-f8b09e92e` | merged | 5,149 pass | bonsai-b |
| Oct 30 02:31 | `upstream-sync-303bc97f9` | merged | 5,117 pass | bonsai-b |
| Oct 30 02:20 | `harden-p7-user-guide-runs` | merged | 5,077 pass | tturbo-mac2 |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 671 |
| bonsai-b | bonsai2-27b | gate | 628 |
| bonsai-ws2-a | bonsai2-27b | held | 700 |
| bonsai-ws2-b | bonsai2-27b | held | 650 |
| claude-sonnet-a | sonnet | gate | 45 |
| claude-sonnet-b | sonnet | agent | 43 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | gate | 517 |
| kimi-k31 | k3.1-pro-highspeed | idle | 91 |
| mai-flash | mai-code-1.1-flash | gate | 20 |
| tturbo-mac | tturbo-ultraheretic | merging | 450 |
| tturbo-mac2 | tturbo-ultraheretic | idle | 271 |
