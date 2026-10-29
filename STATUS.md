# arch3d build status

Updated 2026-10-29 15:00 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 6 of 13: The 3D view, properly — 16 of 20 segments merged · 5 of 9 acceptance checks**

4,883 workspace tests passing at the last merge · 454 segments merged in all · 0 of 11 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | done | 11 / 11 | 52.1 h |
| 5 | Stairs, railings and decks | done | 9 / 9 | 16.1 h |
| 6 | The 3D view, properly | building | 5 / 9 | 9.9 h so far · expected done Oct 29 18:54–Oct 30 02:43 UTC |
| 7 | Generated elevations and sections | planned | — | — |
| 8 | Recognition from plain DXF | planned | — | — |
| 9 | Terrain and site | planned | — | — |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 6 segments

16 merged, 2 gate, 2 working

| Segment | What | State | Seat |
|---|---|---|---|
| `p6f-cmd-camera-full-eye` | camera: a full camera takes and reports a 3D eye [x, y, eye height] | gate | darwin-ws3 |
| `p6f-cli-orbit-target` | render --camera: the orbit form takes target=x,y,z | working | bonsai-b |
| `p6f-r3d-shadow-reach` | The sun's shadow map covers the ground the shadows fall on | gate | claude-sonnet-a |
| `p6f-r3d-glass-house` | glass-house style really blends (today it is pixel-identical to shaded) | working | mai-flash |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 29 14:48 | `p6-docs` | merged | 4,883 pass | darwin-ws3 |
| Oct 29 14:45 | `harden-p6-r3d-edges` | merged | 4,882 pass | tturbo-mac2 |
| Oct 29 14:38 | `p6-ui-pane3d` | merged | 4,874 pass | claude-sonnet-a |
| Oct 29 14:27 | `p6-cli-render` | merged | 4,869 pass | claude-sonnet-b |
| Oct 29 14:14 | `harden-p6-r3d-pbr` | merged | 4,863 pass | tturbo-mac2 |
| Oct 29 14:03 | `p6-r3d-edges` | merged | 4,855 pass | claude-sonnet-b |
| Oct 29 13:56 | `p6-arch3d-door-wall` | merged | 4,851 pass | mai-flash |
| Oct 29 13:36 | `p6-cmd-pick3d` | merged | 4,848 pass | claude-sonnet-a |
| Oct 29 13:22 | `p6-r3d-shadow` | merged | 4,842 pass | claude-sonnet-b |
| Oct 29 12:47 | `p6-r3d-pbr` | merged | 4,836 pass | claude-sonnet-b |
| Oct 29 12:42 | `p6-arch3d-pick` | merged | 4,830 pass | claude-sonnet-a |
| Oct 29 11:57 | `harden-p6-doc-cameras` | merged | 4,814 pass | tturbo-mac |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 618 |
| bonsai-b | bonsai2-27b | agent | 574 |
| bonsai-ws2-a | bonsai2-27b | held | 700 |
| bonsai-ws2-b | bonsai2-27b | held | 650 |
| claude-sonnet-a | sonnet | gate | 37 |
| claude-sonnet-b | sonnet | idle | 33 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | gate | 461 |
| kimi-k31 | k3.1-pro-highspeed | idle | 75 |
| mai-flash | mai-code-1.1-flash | agent | 7 |
| tturbo-mac | tturbo-ultraheretic | agent | 411 |
| tturbo-mac2 | tturbo-ultraheretic | agent | 255 |
