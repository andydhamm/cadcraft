# arch3d build status

Updated 2026-10-30 04:00 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 8 of 13: Recognition from plain DXF — 2 of 23 segments merged · 0 of 10 acceptance checks**

5,154 workspace tests passing at the last merge · 488 segments merged in all · 0 of 11 seats working

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
| 8 | Recognition from plain DXF | building | 0 / 10 | 0.9 h so far · expected done Oct 30 11:29–Nov 01 02:18 UTC |
| 9 | Terrain and site | planned | — | — |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 8 segments

15 queued, 3 gate, 3 working, 2 merged

| Segment | What | State | Seat |
|---|---|---|---|
| `p8-offset-is-the-centre` | One meaning of an opening's offset: the distance from the wall start to its CENTRE | gate | mai-flash |
| `p8-header-inside-the-wall` | The opening header sits inside the wall, so elevations draw no rectangle above openings | working | claude-sonnet-b |
| `p8-arch-drawings-are-metres` | A drawing with arch objects is a metre drawing: $INSUNITS 6 | gate | claude-sonnet-a |
| `p8-io-opening-proxies-whole-symbol` | Opening proxies on save carry the whole plan symbol (jambs, leaf, swing, frame, glass) | working | bonsai-a |
| `p8-housekeeping-guide-readme-uk` | User guide that runs (phases 6-7), current README, uk.tsv Двері | working | bonsai-b |
| `p8-detect-skeleton` | crates/arch/src/detect: the API, units, and stub submodules | gate | darwin-ws3 |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 30 03:05 | `harden-p7f-viewsheet-view-ids` | merged | 5,154 pass | tturbo-mac2 |
| Oct 30 03:03 | `upstream-sync-f8b09e92e` | merged | 5,149 pass | bonsai-b |
| Oct 30 02:31 | `upstream-sync-303bc97f9` | merged | 5,117 pass | bonsai-b |
| Oct 30 02:20 | `harden-p7-user-guide-runs` | merged | 5,077 pass | tturbo-mac2 |
| Oct 30 01:19 | `p7f-viewsheet-view-ids` | merged | 5,074 pass | mai-flash |
| Oct 30 00:50 | `p7-user-guide-runs` | merged | 5,071 pass | kimi-k31 |
| Oct 30 00:36 | `harden-p6-cmd-sun` | merged | 5,068 pass | tturbo-mac2 |
| Oct 30 00:02 | `harden-p7-arch3d-views` | merged | 5,062 pass | tturbo-mac |
| Oct 29 23:56 | `p7-engine-viewsheet` | merged | 5,056 pass | darwin-ws3 |
| Oct 29 23:42 | `harden-p6f-r3d-shadow-reach` | merged | 5,052 pass | tturbo-mac |
| Oct 29 23:31 | `p7-engine-views` | merged | 5,048 pass | claude-sonnet-b |
| Oct 29 23:22 | `harden-p6f-r3d-shadow-reach` | refused | 5,041 pass | tturbo-mac |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 660 |
| bonsai-b | bonsai2-27b | agent | 618 |
| bonsai-ws2-a | bonsai2-27b | held | 700 |
| bonsai-ws2-b | bonsai2-27b | held | 650 |
| claude-sonnet-a | sonnet | gate | 42 |
| claude-sonnet-b | sonnet | agent | 41 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | gate | 506 |
| kimi-k31 | k3.1-pro-highspeed | idle | 87 |
| mai-flash | mai-code-1.1-flash | gate | 15 |
| tturbo-mac | tturbo-ultraheretic | gate | 444 |
| tturbo-mac2 | tturbo-ultraheretic | idle | 271 |
