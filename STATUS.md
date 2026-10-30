# arch3d build status

Updated 2026-10-30 16:30 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 8 of 13: Recognition from plain DXF — 25 of 27 segments merged · 7 of 10 acceptance checks**

5,327 workspace tests passing at the last merge · 523 segments merged in all · 0 of 11 seats working

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
| 8 | Recognition from plain DXF | building | 7 / 10 | 13.4 h so far · expected done Oct 30 17:25–Oct 31 00:34 UTC |
| 9 | Terrain and site | planned | — | — |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 8 segments

25 merged, 2 merging

| Segment | What | State | Seat |
|---|---|---|---|
| `p8f-walls-stop-at-crossing-faces` | A wall run never bridges a gap that another wall's faces run across | merging | mai-flash |
| `p8f-orbit-fit-bounding-sphere` | Orbit::fit frames the scene's bounding sphere, so the 3D pane shows the house at a useful size | merging | claude-sonnet-b |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 30 16:22 | `harden-p8-detect-openings` | merged | 5,327 pass | tturbo-mac2 |
| Oct 30 16:01 | `p8f-rooms-arrangement-once` | merged | 5,323 pass | mai-flash |
| Oct 30 15:20 | `harden-p8-cli-convert-detects` | merged | 5,320 pass | tturbo-mac |
| Oct 30 14:48 | `harden-p8-guide-archdetect` | merged | 5,317 pass | tturbo-mac2 |
| Oct 30 14:39 | `p8-detect-tune-to-targets` | merged | 5,314 pass | kimi-k31 |
| Oct 30 14:13 | `harden-p8-cmd-archdetect` | merged | 5,311 pass | tturbo-mac2 |
| Oct 30 13:36 | `p8-guide-archdetect` | merged | 5,303 pass | darwin-ws3 |
| Oct 30 13:30 | `harden-p8-header-inside-the-wall` | merged | 5,302 pass | tturbo-mac |
| Oct 30 13:19 | `p8-ui-review-screenshots` | merged | 5,294 pass | claude-sonnet-a |
| Oct 30 12:58 | `p8-cli-convert-detects` | merged | 5,293 pass | darwin-ws3 |
| Oct 30 12:37 | `p8-ui-review-panel` | merged | 5,290 pass | claude-sonnet-a |
| Oct 30 12:11 | `p8-cmd-archdetect` | merged | 5,287 pass | claude-sonnet-a |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | merging | 695 |
| bonsai-b | bonsai2-27b | merging | 651 |
| bonsai-ws2-a | bonsai2-27b | held | 700 |
| bonsai-ws2-b | bonsai2-27b | held | 650 |
| claude-sonnet-a | sonnet | idle | 53 |
| claude-sonnet-b | sonnet | merging | 46 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | idle | 550 |
| kimi-k31 | k3.1-pro-highspeed | idle | 95 |
| mai-flash | mai-code-1.1-flash | merging | 23 |
| tturbo-mac | tturbo-ultraheretic | merging | 467 |
| tturbo-mac2 | tturbo-ultraheretic | agent | 287 |
