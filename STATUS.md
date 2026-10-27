# arch3d build status

Updated 2026-10-27 02:30 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 4 of 13: Automatic roofs — 23 of 30 segments merged · 5 of 11 acceptance checks**

3,773 workspace tests passing at the last merge · 238 segments merged in all · 0 of 10 seats working · PAUSED

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | building | 5 / 11 | 17.7 h so far · expected done Oct 27 12:28–Oct 27 21:04 UTC |
| 5 | Stairs, railings and decks | planned | — | — |
| 6 | The 3D view, properly | planned | — | — |
| 7 | Generated elevations and sections | planned | — | — |
| 8 | Recognition from plain DXF | planned | — | — |
| 9 | Terrain and site | planned | — | — |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 4 segments

23 merged, 5 replaced, 5 queued, 2 working

| Segment | What | State | Seat |
|---|---|---|---|
| `p4x-wall-mesh-winding` | Wall, floor and ceiling meshes wind outward | working | claude-sonnet-b |
| `p4x-cli-render-draws` | cadcraft-cli render draws the scene, not only the clear colour | working | bonsai-a |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 27 02:17 | `p4g-roof-top-polyline` | merged | 3,773 pass | bonsai-ws2-a |
| Oct 27 02:10 | `p4x-glb-chunk-padding` | merged | 3,770 pass | claude-sonnet-a |
| Oct 27 01:45 | `harden-p3r-cli-projected-exit3` | merged | 3,768 pass | tturbo-mac |
| Oct 27 01:18 | `harden-p3r-pipeline-depth` | merged | 3,764 pass | tturbo-mac |
| Oct 27 01:15 | `harden-p3-area-rules` | merged | 3,758 pass | tturbo-mac2 |
| Oct 27 01:04 | `harden-p3r-png-codec` | merged | 3,750 pass | tturbo-mac |
| Oct 27 00:33 | `harden-p3-room-label` | merged | 3,745 pass | tturbo-mac2 |
| Oct 26 23:33 | `harden-p3-i18n-uk-roomdetect` | merged | 3,740 pass | tturbo-mac |
| Oct 26 23:19 | `harden-p3-cli-render-projected-v2` | merged | 3,737 pass | tturbo-mac2 |
| Oct 26 23:13 | `harden-p4g-arch-roof-property-tests` | merged | 3,731 pass | tturbo-mac |
| Oct 26 22:59 | `harden-p3-i18n-uk-rooms` | merged | 3,726 pass | tturbo-mac |
| Oct 26 22:39 | `p4g-arch-roof-property-tests` | merged | 3,721 pass | bonsai-b |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 470 |
| bonsai-b | bonsai2-27b | gate | 420 |
| bonsai-ws2-a | bonsai2-27b | idle | 600 |
| bonsai-ws2-b | bonsai2-27b | gate | 573 |
| claude-sonnet-a | sonnet | idle | 2 |
| claude-sonnet-b | sonnet | agent | 1 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | idle | 248 |
| kimi-k31 | k3.1-pro-highspeed | idle | 55 |
| tturbo-mac | tturbo-ultraheretic | agent | 275 |
| tturbo-mac2 | tturbo-ultraheretic | gate | 82 |
