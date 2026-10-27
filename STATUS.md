# arch3d build status

Updated 2026-10-27 04:15 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 4 of 13: Automatic roofs — 28 of 31 segments merged · 5 of 11 acceptance checks**

3,795 workspace tests passing at the last merge · 246 segments merged in all · 0 of 12 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | building | 5 / 11 | 19.4 h so far · expected done Oct 27 10:18–Oct 27 16:06 UTC |
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

28 merged, 5 replaced, 1 merging, 1 working, 1 escalate

| Segment | What | State | Seat |
|---|---|---|---|
| `p4-docs-roof` | Design log and gaps for phase 4 | merging | claude-sonnet-a |
| `p4x-wall-mesh-winding` | Wall, floor and ceiling meshes wind outward | working | kimi-k31 |
| `p4x-cli-render-draws` | cadcraft-cli render draws the scene, not only the clear colour | escalate |  |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 27 04:09 | `p4g-golden-roofs` | merged | 3,795 pass | kimi-k31 |
| Oct 27 03:47 | `harden-p4g-gable-clip-test-names` | merged | 3,793 pass | tturbo-mac |
| Oct 27 03:38 | `upstream-sync-d864a08b0` | merged | 3,788 pass | claude-sonnet-b |
| Oct 27 03:33 | `p4g-gable-clip-test-names` | merged | 3,788 pass | bonsai-ws2-a |
| Oct 27 03:30 | `harden-p3r-mesh-shader` | merged | 3,788 pass | tturbo-mac2 |
| Oct 27 03:26 | `p4g-gable-clip-wire` | merged | 3,782 pass | bonsai-ws2-a |
| Oct 27 03:00 | `harden-p4x-glb-chunk-padding` | merged | 3,780 pass | tturbo-mac2 |
| Oct 27 02:52 | `p4g-profile-slab-extrude` | merged | 3,776 pass | claude-sonnet-a |
| Oct 27 02:17 | `p4g-roof-top-polyline` | merged | 3,773 pass | bonsai-ws2-a |
| Oct 27 02:10 | `p4x-glb-chunk-padding` | merged | 3,770 pass | claude-sonnet-a |
| Oct 27 01:45 | `harden-p3r-cli-projected-exit3` | merged | 3,768 pass | tturbo-mac |
| Oct 27 01:18 | `harden-p3r-pipeline-depth` | merged | 3,764 pass | tturbo-mac |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 477 |
| bonsai-b | bonsai2-27b | agent | 425 |
| bonsai-ws2-a | bonsai2-27b | agent | 612 |
| bonsai-ws2-b | bonsai2-27b | agent | 578 |
| claude-sonnet-a | sonnet | merging | 5 |
| claude-sonnet-b | sonnet | idle | 4 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | gate | 252 |
| kimi-k31 | k3.1-pro-highspeed | agent | 56 |
| ornith-a | ornith15-397b-exl3-35 | agent | 1 |
| ornith-b | ornith15-397b-exl3-35 | merging | 1 |
| tturbo-mac | tturbo-ultraheretic | agent | 280 |
| tturbo-mac2 | tturbo-ultraheretic | merging | 88 |
