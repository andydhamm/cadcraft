# arch3d build status

Updated 2026-10-29 07:45 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 6 of 13: The 3D view, properly — 1 of 14 segments merged · 1 of 9 acceptance checks**

4,774 workspace tests passing at the last merge · 435 segments merged in all · 0 of 13 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | done | 11 / 11 | 52.1 h |
| 5 | Stairs, railings and decks | done | 9 / 9 | 16.1 h |
| 6 | The 3D view, properly | building | 1 / 9 | 2.6 h so far · expected done Oct 30 04:34–Nov 05 14:20 UTC |
| 7 | Generated elevations and sections | planned | — | — |
| 8 | Recognition from plain DXF | planned | — | — |
| 9 | Terrain and site | planned | — | — |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 6 segments

9 queued, 2 working, 2 merging, 1 merged

| Segment | What | State | Seat |
|---|---|---|---|
| `p6-arch3d-camera` | arch3d::camera — orbit + full camera, projection, pixel rays | working | bonsai-a |
| `p6-doc-cameras` | Camera3d on the arch model | working | bonsai-b |
| `p6-cmd-sun` | sun command | merging | claude-sonnet-b |
| `p6-r3d-limits` | Adapter choice + downlevel WebGL2 limits | merging | claude-sonnet-a |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 29 06:47 | `p6-arch-sun` | merged | 4,774 pass | darwin-ws3 |
| Oct 29 06:39 | `harden-p1-arch-crate` | merged | 4,767 pass | tturbo-mac2 |
| Oct 29 06:32 | `harden-p5f-inspect-holes-are-rings` | merged | 4,751 pass | tturbo-mac |
| Oct 29 05:27 | `harden-p1-render-arch-hook` | merged | 4,743 pass | tturbo-mac2 |
| Oct 29 05:17 | `harden-p5f-inspect-holes-are-rings` | refused | 4,747 pass | tturbo-mac |
| Oct 29 04:40 | `harden-p5-arch3d-stairs` | merged | 4,739 pass | tturbo-mac |
| Oct 29 04:33 | `harden-p1-arch-workspace` | merged | 4,732 pass | tturbo-mac2 |
| Oct 29 04:02 | `harden-p1-wall-graph` | merged | 4,727 pass | tturbo-mac2 |
| Oct 29 03:50 | `p5f-inspect-holes-are-rings` | merged | 4,720 pass | kimi-k31 |
| Oct 29 03:40 | `p5f-inspect-holes-are-rings` | refused | 4,720 pass | bonsai-a |
| Oct 29 03:33 | `p5f-inspect-holes-are-rings` | refused | 4,720 pass | darwin-ws3 |
| Oct 29 03:26 | `harden-p5-arch3d-railing-deck` | merged | 4,719 pass | tturbo-mac |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 597 |
| bonsai-b | bonsai2-27b | agent | 548 |
| bonsai-ws2-a | bonsai2-27b | held | 700 |
| bonsai-ws2-b | bonsai2-27b | held | 650 |
| claude-sonnet-a | sonnet | merging | 30 |
| claude-sonnet-b | sonnet | merging | 25 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | agent | 429 |
| kimi-k31 | k3.1-pro-highspeed | idle | 72 |
| mai-flash | mai-code-1.1-flash | idle | 6 |
| ornith-a | ornith15-397b-exl3-35 | held | 29 |
| ornith-b | ornith15-397b-exl3-35 | held | 17 |
| tturbo-mac | tturbo-ultraheretic | gate | 394 |
| tturbo-mac2 | tturbo-ultraheretic | idle | 251 |
