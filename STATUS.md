# arch3d build status

Updated 2026-10-29 18:00 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 7 of 13: Generated elevations and sections — 0 of 12 segments merged · 0 of 9 acceptance checks**

4,967 workspace tests passing at the last merge · 463 segments merged in all · 0 of 11 seats working

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
| 7 | Generated elevations and sections | building | 0 / 9 | 0.8 h so far · expected done Oct 29 23:43–Oct 31 04:21 UTC |
| 8 | Recognition from plain DXF | planned | — | — |
| 9 | Terrain and site | planned | — | — |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 7 segments

7 queued, 4 gate, 1 working

| Segment | What | State | Seat |
|---|---|---|---|
| `p7-handles-one-form` | One JSON handle form for every arch object (inspect, pick3d, command replies) | gate | claude-sonnet-b |
| `p7-ui-pick-selects` | A 3D pick becomes the 2D selection | gate | bonsai-a |
| `p7-roof-eave-at-the-wall` | Roof underside meets the eave wall top with an overhang (walls no longer stretched into the roof) | working | mai-flash |
| `p7-doc-views` | Linked view objects (ELEVATION / SECTION) and ArchKind in the arch model | gate | darwin-ws3 |
| `p7-render-arch-in-viewports` | Arch linework shows through paper-space viewports and layout plots | gate | claude-sonnet-a |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 29 17:25 | `harden-p6f-cmd-camera-full-eye` | merged | 4,967 pass | tturbo-mac2 |
| Oct 29 17:05 | `harden-p6-arch3d-pick` | merged | 4,962 pass | tturbo-mac |
| Oct 29 16:49 | `harden-p6f-cli-orbit-target` | merged | 4,911 pass | tturbo-mac2 |
| Oct 29 16:46 | `p6f-cmd-camera-full-eye` | merged | 4,903 pass | darwin-ws3 |
| Oct 29 16:13 | `harden-p6-docs` | merged | 4,899 pass | tturbo-mac2 |
| Oct 29 16:06 | `p6f-r3d-glass-house` | merged | 4,899 pass | mai-flash |
| Oct 29 15:54 | `p6f-cli-orbit-target` | merged | 4,896 pass | bonsai-b |
| Oct 29 15:46 | `harden-p6-r3d-shadow` | merged | 4,893 pass | tturbo-mac2 |
| Oct 29 15:31 | `p6f-r3d-shadow-reach` | merged | 4,886 pass | claude-sonnet-a |
| Oct 29 14:48 | `p6-docs` | merged | 4,883 pass | darwin-ws3 |
| Oct 29 14:45 | `harden-p6-r3d-edges` | merged | 4,882 pass | tturbo-mac2 |
| Oct 29 14:38 | `p6-ui-pane3d` | merged | 4,874 pass | claude-sonnet-a |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | gate | 625 |
| bonsai-b | bonsai2-27b | agent | 579 |
| bonsai-ws2-a | bonsai2-27b | held | 700 |
| bonsai-ws2-b | bonsai2-27b | held | 650 |
| claude-sonnet-a | sonnet | gate | 39 |
| claude-sonnet-b | sonnet | gate | 33 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | gate | 468 |
| kimi-k31 | k3.1-pro-highspeed | idle | 75 |
| mai-flash | mai-code-1.1-flash | agent | 9 |
| tturbo-mac | tturbo-ultraheretic | agent | 421 |
| tturbo-mac2 | tturbo-ultraheretic | merging | 261 |
