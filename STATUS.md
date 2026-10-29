# arch3d build status

Updated 2026-10-29 13:45 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 6 of 13: The 3D view, properly — 11 of 16 segments merged · 3 of 9 acceptance checks**

4,848 workspace tests passing at the last merge · 447 segments merged in all · 0 of 11 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | done | 11 / 11 | 52.1 h |
| 5 | Stairs, railings and decks | done | 9 / 9 | 16.1 h |
| 6 | The 3D view, properly | building | 3 / 9 | 8.6 h so far · expected done Oct 29 21:52–Oct 30 09:43 UTC |
| 7 | Generated elevations and sections | planned | — | — |
| 8 | Recognition from plain DXF | planned | — | — |
| 9 | Terrain and site | planned | — | — |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 6 segments

11 merged, 3 queued, 2 merging

| Segment | What | State | Seat |
|---|---|---|---|
| `p6-r3d-edges` | Feature edges for shaded-edges and hidden-line (arch3d::edges) | merging | claude-sonnet-b |
| `p6-arch3d-door-wall` | A wall that holds a door is solid beside and above the door (arch3d walls) | merging | mai-flash |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 29 13:36 | `p6-cmd-pick3d` | merged | 4,848 pass | claude-sonnet-a |
| Oct 29 13:22 | `p6-r3d-shadow` | merged | 4,842 pass | claude-sonnet-b |
| Oct 29 12:47 | `p6-r3d-pbr` | merged | 4,836 pass | claude-sonnet-b |
| Oct 29 12:42 | `p6-arch3d-pick` | merged | 4,830 pass | claude-sonnet-a |
| Oct 29 11:57 | `harden-p6-doc-cameras` | merged | 4,814 pass | tturbo-mac |
| Oct 29 11:23 | `harden-p6-doc-cameras` | refused | 0 pass | tturbo-mac |
| Oct 29 11:23 | `p6-io-cameras` | merged | 4,805 pass | darwin-ws3 |
| Oct 29 10:30 | `p6-cmd-camera` | merged | 4,800 pass | claude-sonnet-b |
| Oct 29 09:13 | `p6-doc-cameras` | merged | 4,793 pass | kimi-k31 |
| Oct 29 08:08 | `harden-p6-arch-sun` | merged | 4,788 pass | tturbo-mac |
| Oct 29 07:59 | `p6-r3d-limits` | merged | 4,781 pass | claude-sonnet-a |
| Oct 29 07:49 | `p6-cmd-sun` | merged | 4,777 pass | claude-sonnet-b |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | idle | 614 |
| bonsai-b | bonsai2-27b | gate | 569 |
| bonsai-ws2-a | bonsai2-27b | held | 700 |
| bonsai-ws2-b | bonsai2-27b | held | 650 |
| claude-sonnet-a | sonnet | idle | 36 |
| claude-sonnet-b | sonnet | merging | 30 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | agent | 458 |
| kimi-k31 | k3.1-pro-highspeed | idle | 75 |
| mai-flash | mai-code-1.1-flash | merging | 7 |
| tturbo-mac | tturbo-ultraheretic | agent | 408 |
| tturbo-mac2 | tturbo-ultraheretic | gate | 253 |
