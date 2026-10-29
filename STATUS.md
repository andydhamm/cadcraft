# arch3d build status

Updated 2026-10-29 12:30 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 6 of 13: The 3D view, properly — 7 of 15 segments merged · 2 of 9 acceptance checks**

4,814 workspace tests passing at the last merge · 443 segments merged in all · 0 of 11 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | done | 11 / 11 | 52.1 h |
| 5 | Stairs, railings and decks | done | 9 / 9 | 16.1 h |
| 6 | The 3D view, properly | building | 2 / 9 | 7.4 h so far · expected done Oct 30 03:02–Oct 30 21:36 UTC |
| 7 | Generated elevations and sections | planned | — | — |
| 8 | Recognition from plain DXF | planned | — | — |
| 9 | Terrain and site | planned | — | — |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 6 segments

7 merged, 6 queued, 1 gate, 1 working

| Segment | What | State | Seat |
|---|---|---|---|
| `p6-arch3d-pick` | arch3d::pick — BVH ray cast | gate | claude-sonnet-a |
| `p6-r3d-pbr` | PBR pipeline with MSAA | working | claude-sonnet-b |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 29 11:57 | `harden-p6-doc-cameras` | merged | 4,814 pass | tturbo-mac |
| Oct 29 11:23 | `harden-p6-doc-cameras` | refused | 0 pass | tturbo-mac |
| Oct 29 11:23 | `p6-io-cameras` | merged | 4,805 pass | darwin-ws3 |
| Oct 29 10:30 | `p6-cmd-camera` | merged | 4,800 pass | claude-sonnet-b |
| Oct 29 09:13 | `p6-doc-cameras` | merged | 4,793 pass | kimi-k31 |
| Oct 29 08:08 | `harden-p6-arch-sun` | merged | 4,788 pass | tturbo-mac |
| Oct 29 07:59 | `p6-r3d-limits` | merged | 4,781 pass | claude-sonnet-a |
| Oct 29 07:49 | `p6-cmd-sun` | merged | 4,777 pass | claude-sonnet-b |
| Oct 29 06:47 | `p6-arch-sun` | merged | 4,774 pass | darwin-ws3 |
| Oct 29 06:39 | `harden-p1-arch-crate` | merged | 4,767 pass | tturbo-mac2 |
| Oct 29 06:32 | `harden-p5f-inspect-holes-are-rings` | merged | 4,751 pass | tturbo-mac |
| Oct 29 05:27 | `harden-p1-render-arch-hook` | merged | 4,743 pass | tturbo-mac2 |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 609 |
| bonsai-b | bonsai2-27b | agent | 565 |
| bonsai-ws2-a | bonsai2-27b | held | 700 |
| bonsai-ws2-b | bonsai2-27b | held | 650 |
| claude-sonnet-a | sonnet | gate | 33 |
| claude-sonnet-b | sonnet | agent | 26 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | gate | 446 |
| kimi-k31 | k3.1-pro-highspeed | idle | 75 |
| mai-flash | mai-code-1.1-flash | idle | 6 |
| tturbo-mac | tturbo-ultraheretic | agent | 405 |
| tturbo-mac2 | tturbo-ultraheretic | idle | 251 |
