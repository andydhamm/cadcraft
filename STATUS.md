# arch3d build status

Updated 2026-10-27 12:00 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 4 of 13: Automatic roofs — 66 of 71 segments merged · 6 of 11 acceptance checks**

4,030 workspace tests passing at the last merge · 308 segments merged in all · 0 of 12 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | building | 6 / 11 | 27.2 h so far · expected done Oct 27 14:37–Oct 27 17:18 UTC |
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

66 merged, 5 replaced, 3 working, 2 queued

| Segment | What | State | Seat |
|---|---|---|---|
| `p4f-geom-roof-invariant-tests` | geom: roof-skeleton invariant test target | working | ornith-a |
| `p4f-roof-rebuild-on-wall-edit` | engine: a ROOFAUTO roof rebuilds when its walls change | working | bonsai-b |
| `p4f-gable-clip-probe` | arch3d: gable walls really reach the ridge and are probeable | working | kimi-k31 |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 27 11:48 | `harden-p4r-mesh-side-normals` | merged | 4,030 pass | tturbo-mac |
| Oct 27 11:44 | `p4f-arch-roof-property-test-names` | merged | 4,026 pass | claude-sonnet-b |
| Oct 27 11:42 | `p4f-roofplane-create` | merged | 4,022 pass | bonsai-b |
| Oct 27 11:37 | `p4f-valley-classifier` | merged | 4,019 pass | kimi-k31 |
| Oct 27 11:32 | `harden-p4r-join-box-prefilter` | merged | 4,017 pass | tturbo-mac |
| Oct 27 10:57 | `harden-p4r-wall-cmd-bands-joins` | merged | 4,012 pass | tturbo-mac |
| Oct 27 10:50 | `harden-p4r-glb-gltf-conformance` | merged | 4,008 pass | tturbo-mac2 |
| Oct 27 10:44 | `harden-p4r-join-tidy` | merged | 4,003 pass | tturbo-mac |
| Oct 27 10:36 | `harden-p4r-camera-validate` | merged | 3,995 pass | tturbo-mac2 |
| Oct 27 10:24 | `harden-p4r-render3d-lints` | merged | 3,992 pass | tturbo-mac2 |
| Oct 27 10:02 | `harden-p4r-wall-cmd-validate` | merged | 3,987 pass | tturbo-mac2 |
| Oct 27 09:44 | `harden-p4r-property-walls-generator` | merged | 3,982 pass | tturbo-mac2 |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 510 |
| bonsai-b | bonsai2-27b | agent | 451 |
| bonsai-ws2-a | bonsai2-27b | agent | 655 |
| bonsai-ws2-b | bonsai2-27b | idle | 614 |
| claude-sonnet-a | sonnet | idle | 12 |
| claude-sonnet-b | sonnet | idle | 13 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | idle | 298 |
| kimi-k31 | k3.1-pro-highspeed | agent | 59 |
| ornith-a | ornith15-397b-exl3-35 | agent | 23 |
| ornith-b | ornith15-397b-exl3-35 | agent | 15 |
| tturbo-mac | tturbo-ultraheretic | agent | 306 |
| tturbo-mac2 | tturbo-ultraheretic | agent | 116 |
