# arch3d build status

Updated 2026-10-30 07:15 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 8 of 13: Recognition from plain DXF — 7 of 23 segments merged · 1 of 10 acceptance checks**

5,181 workspace tests passing at the last merge · 494 segments merged in all · 0 of 11 seats working

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
| 8 | Recognition from plain DXF | building | 1 / 10 | 4.2 h so far · expected done Oct 30 21:49–Nov 01 00:14 UTC |
| 9 | Terrain and site | planned | — | — |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 8 segments

14 queued, 7 merged, 1 gate, 1 escalate

| Segment | What | State | Seat |
|---|---|---|---|
| `p8-io-opening-proxies-whole-symbol` | Opening proxies on save carry the whole plan symbol (jambs, leaf, swing, frame, glass) | gate | kimi-k31 |
| `p8-detect-skeleton` | crates/arch/src/detect: the API, units, and stub submodules | escalate |  |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
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
| Oct 30 01:19 | `p7f-viewsheet-view-ids` | merged | 5,074 pass | mai-flash |
| Oct 30 00:50 | `p7-user-guide-runs` | merged | 5,071 pass | kimi-k31 |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | gate | 666 |
| bonsai-b | bonsai2-27b | agent | 626 |
| bonsai-ws2-a | bonsai2-27b | held | 700 |
| bonsai-ws2-b | bonsai2-27b | held | 650 |
| claude-sonnet-a | sonnet | idle | 44 |
| claude-sonnet-b | sonnet | idle | 42 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | agent | 513 |
| kimi-k31 | k3.1-pro-highspeed | gate | 89 |
| mai-flash | mai-code-1.1-flash | idle | 20 |
| tturbo-mac | tturbo-ultraheretic | agent | 448 |
| tturbo-mac2 | tturbo-ultraheretic | idle | 271 |
