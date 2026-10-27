# arch3d build status

Updated 2026-10-27 11:30 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 4 of 13: Automatic roofs — 63 of 71 segments merged · 5 of 11 acceptance checks**

4,012 workspace tests passing at the last merge · 303 segments merged in all · 0 of 12 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | building | 5 / 11 | 26.7 h so far · expected done Oct 27 14:59–Oct 27 18:15 UTC |
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

63 merged, 5 replaced, 4 working, 4 queued

| Segment | What | State | Seat |
|---|---|---|---|
| `p4f-geom-roof-invariant-tests` | geom: roof-skeleton invariant test target | working | ornith-a |
| `p4f-valley-classifier` | arch: classify hips/valleys/ridges from the surfaces | working | kimi-k31 |
| `p4f-roofplane-create` | engine: roofplane accepts a closed ring and numeric fascia/soffit | working | bonsai-b |
| `p4f-gable-clip-probe` | arch3d: gable walls really reach the ridge and are probeable | working | bonsai-ws2-b |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 27 10:57 | `harden-p4r-wall-cmd-bands-joins` | merged | 4,012 pass | tturbo-mac |
| Oct 27 10:50 | `harden-p4r-glb-gltf-conformance` | merged | 4,008 pass | tturbo-mac2 |
| Oct 27 10:44 | `harden-p4r-join-tidy` | merged | 4,003 pass | tturbo-mac |
| Oct 27 10:36 | `harden-p4r-camera-validate` | merged | 3,995 pass | tturbo-mac2 |
| Oct 27 10:24 | `harden-p4r-render3d-lints` | merged | 3,992 pass | tturbo-mac2 |
| Oct 27 10:02 | `harden-p4r-wall-cmd-validate` | merged | 3,987 pass | tturbo-mac2 |
| Oct 27 09:44 | `harden-p4r-property-walls-generator` | merged | 3,982 pass | tturbo-mac2 |
| Oct 27 09:36 | `harden-p4r-bench-rebuild-honest` | merged | 3,978 pass | tturbo-mac2 |
| Oct 27 09:34 | `harden-p4r-arch-layers-on-first-use` | merged | 3,974 pass | tturbo-mac |
| Oct 27 09:26 | `harden-p4r-graph-linear-build` | merged | 3,970 pass | tturbo-mac2 |
| Oct 27 09:17 | `p4r-join-tidy` | merged | 3,965 pass | bonsai-a |
| Oct 27 09:13 | `p4r-wall-cmd-bands-joins` | merged | 3,964 pass | claude-sonnet-b |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 506 |
| bonsai-b | bonsai2-27b | agent | 448 |
| bonsai-ws2-a | bonsai2-27b | agent | 651 |
| bonsai-ws2-b | bonsai2-27b | agent | 608 |
| claude-sonnet-a | sonnet | idle | 12 |
| claude-sonnet-b | sonnet | idle | 12 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | agent | 296 |
| kimi-k31 | k3.1-pro-highspeed | agent | 58 |
| ornith-a | ornith15-397b-exl3-35 | agent | 22 |
| ornith-b | ornith15-397b-exl3-35 | agent | 15 |
| tturbo-mac | tturbo-ultraheretic | merging | 304 |
| tturbo-mac2 | tturbo-ultraheretic | agent | 114 |
