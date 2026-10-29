# arch3d build status

Updated 2026-10-29 21:00 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 7 of 13: Generated elevations and sections — 4 of 12 segments merged · 1 of 9 acceptance checks**

5,001 workspace tests passing at the last merge · 471 segments merged in all · 0 of 11 seats working

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
| 7 | Generated elevations and sections | building | 1 / 9 | 3.8 h so far · expected done Oct 30 09:15–Oct 31 12:52 UTC |
| 8 | Recognition from plain DXF | planned | — | — |
| 9 | Terrain and site | planned | — | — |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 7 segments

4 merged, 3 queued, 2 working, 2 gate, 1 escalate

| Segment | What | State | Seat |
|---|---|---|---|
| `p7-roof-eave-at-the-wall` | Roof underside meets the eave wall top with an overhang (walls no longer stretched into the roof) | escalate |  |
| `p7-hlr-kinds` | HLR lines and section rings carry their object's kind | working | claude-sonnet-a |
| `p7-render-views` | Draw linked views in the plan: linework, filled poche, overall dimensions; template layers | gate | claude-sonnet-b |
| `p7-io-views` | DXF round trip for views (CADCRAFT_ARCH v7) and save-time view proxies | working | mai-flash |
| `p7-engine-viewsheet` | viewsheet: linked views on a layout at their scale (ARCH D at 1:50) | gate | darwin-ws3 |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 29 19:22 | `harden-p7-render-arch-in-viewports` | merged | 5,001 pass | tturbo-mac2 |
| Oct 29 19:18 | `harden-p7-ui-pick-selects` | merged | 4,996 pass | tturbo-mac |
| Oct 29 18:52 | `harden-p6f-r3d-glass-house` | merged | 4,991 pass | tturbo-mac |
| Oct 29 18:43 | `p7-ui-pick-selects` | merged | 4,982 pass | bonsai-a |
| Oct 29 18:32 | `p7-render-arch-in-viewports` | merged | 4,980 pass | claude-sonnet-a |
| Oct 29 18:21 | `p7-handles-one-form` | merged | 4,976 pass | claude-sonnet-b |
| Oct 29 18:09 | `harden-p6-cmd-camera` | merged | 4,972 pass | tturbo-mac2 |
| Oct 29 17:25 | `harden-p6f-cmd-camera-full-eye` | merged | 4,967 pass | tturbo-mac2 |
| Oct 29 17:05 | `harden-p6-arch3d-pick` | merged | 4,962 pass | tturbo-mac |
| Oct 29 16:49 | `harden-p6f-cli-orbit-target` | merged | 4,911 pass | tturbo-mac2 |
| Oct 29 16:46 | `p6f-cmd-camera-full-eye` | merged | 4,903 pass | darwin-ws3 |
| Oct 29 16:13 | `harden-p6-docs` | merged | 4,899 pass | tturbo-mac2 |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 638 |
| bonsai-b | bonsai2-27b | gate | 599 |
| bonsai-ws2-a | bonsai2-27b | held | 700 |
| bonsai-ws2-b | bonsai2-27b | held | 650 |
| claude-sonnet-a | sonnet | agent | 40 |
| claude-sonnet-b | sonnet | gate | 34 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | gate | 484 |
| kimi-k31 | k3.1-pro-highspeed | down | 81 |
| mai-flash | mai-code-1.1-flash | agent | 12 |
| tturbo-mac | tturbo-ultraheretic | gate | 430 |
| tturbo-mac2 | tturbo-ultraheretic | idle | 264 |
