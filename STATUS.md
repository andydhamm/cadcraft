# arch3d build status

Updated 2026-10-31 19:45 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 10 of 13: Lighting, light fixtures and final renders — 1 of 21 segments merged · 0 of 11 acceptance checks**

6,023 workspace tests passing at the last merge · 583 segments merged in all · 0 of 13 seats working

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
| 8 | Recognition from plain DXF | done | 10 / 10 | 16.6 h |
| 9 | Terrain and site | done | 12 / 12 | 19.4 h |
| 10 | Lighting, light fixtures and final renders | building | 0 / 11 | 3.2 h so far · expected done Nov 01 02:51–Nov 02 16:35 UTC |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 10 segments

13 queued, 6 merging, 1 merged, 1 gate

| Segment | What | State | Seat |
|---|---|---|---|
| `p10-arch-light-placement` | Fixture validation, colour temperature, light point, aim and room grid | merging | darwin-ws3 |
| `p10-arch-light-symbol` | Plan symbols of the fixture shapes | merging | bonsai-ws2-a |
| `p10-r3d-light-pack` | Which lights are drawn and shadowed, and their uniform bytes | merging | bonsai-ws2-b |
| `p10-r3d-shadow-atlas` | Fixture shadow maps: spot maps and point-light cubes in one depth array, cached per light | merging | claude-sonnet-c |
| `p10-cmd-render` | RENDER 3D command over the app's render hook | merging | bonsai-a |
| `p10-cmd-sunstudy` | SUNSTUDY: stills through the day from the current camera | gate | bonsai-b |
| `p10-io-glb-lights` | GLB: KHR_lights_punctual lights and their nodes | merging | mai-flash |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 31 19:32 | `harden-p9-arch-site` | refused | 6,032 pass, 3 fail | tturbo-mac2 |
| Oct 31 18:49 | `p10-r3d-shadow-atlas` | refused | 6,033 pass, 3 fail | claude-sonnet-c |
| Oct 31 18:25 | `harden-p9-cmd-wall-retaining` | refused | 6,033 pass, 3 fail | tturbo-mac |
| Oct 31 17:29 | `harden-p9-arch-terrain-pad` | merged | 6,023 pass | tturbo-mac2 |
| Oct 31 17:10 | `harden-p9-cli-render-sun-from-site` | merged | 6,019 pass | tturbo-mac |
| Oct 31 16:46 | `harden-p9f-mesh-pad-slope` | merged | 6,014 pass | tturbo-mac |
| Oct 31 16:33 | `upstream-sync-b6331c860` | merged | 6,008 pass | bonsai-a |
| Oct 31 16:15 | `upstream-sync-b6331c860` | refused | 6,008 pass | darwin-ws3 |
| Oct 31 16:01 | `harden-p9-cmd-plant` | merged | 5,954 pass | tturbo-mac2 |
| Oct 31 15:49 | `harden-p9-geom-cdt` | merged | 5,948 pass | tturbo-mac |
| Oct 31 14:48 | `p9f-mesh-pad-slope` | refused | 5,943 pass | kimi-k31 |
| Oct 31 14:33 | `harden-p9-cmd-site` | merged | 5,940 pass | tturbo-mac2 |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | merging | 734 |
| bonsai-b | bonsai2-27b | gate | 684 |
| bonsai-ws2-a | bonsai2-27b | merging | 740 |
| bonsai-ws2-b | bonsai2-27b | merging | 700 |
| claude-sonnet-a | sonnet | idle | 58 |
| claude-sonnet-b | sonnet | idle | 55 |
| claude-sonnet-c | sonnet | merging | 10 |
| claude-sonnet-d | sonnet | idle | 2 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | merging | 589 |
| kimi-k31 | k3.1-pro-highspeed | idle | 106 |
| mai-flash | mai-code-1.1-flash | merging | 30 |
| tturbo-mac | tturbo-ultraheretic | gate | 510 |
| tturbo-mac2 | tturbo-ultraheretic | gate | 330 |
