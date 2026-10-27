# arch3d build status

Updated 2026-10-27 17:15 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 4 of 13: Automatic roofs — 72 of 74 segments merged · 6 of 11 acceptance checks**

4,142 workspace tests passing at the last merge · 329 segments merged in all · 0 of 12 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | building | 6 / 11 | 32.4 h so far · expected done Oct 27 19:44–Oct 27 22:38 UTC |
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

72 merged, 6 replaced, 1 working, 1 queued

| Segment | What | State | Seat |
|---|---|---|---|
| `p4h-profile-prism-extrude` | arch3d: extrude wall profiles through push_prism_with_holes | working | claude-sonnet-a |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 27 17:12 | `upstream-sync-0ff1b0353` | merged | 4,142 pass | bonsai-ws2-a |
| Oct 27 17:08 | `p4h-profile-hole-rects` | merged | 4,142 pass | claude-sonnet-a |
| Oct 27 16:58 | `harden-p2-open-style-defaults` | merged | 4,137 pass | tturbo-mac2 |
| Oct 27 15:26 | `harden-p4r-join-x-crossing` | merged | 4,131 pass | tturbo-mac |
| Oct 27 14:56 | `harden-p4r-join-raw-ring-fallback` | merged | 4,126 pass | tturbo-mac |
| Oct 27 14:34 | `harden-p2-openings-envelope` | merged | 4,122 pass | tturbo-mac2 |
| Oct 27 14:26 | `harden-p4r-join-t-junction` | merged | 4,116 pass | tturbo-mac |
| Oct 27 14:08 | `harden-p2-sample-honours-openings` | merged | 4,108 pass | tturbo-mac2 |
| Oct 27 14:02 | `harden-p4r-join-mitre-limit` | merged | 4,104 pass | tturbo-mac |
| Oct 27 13:48 | `harden-p4r-join-own-kind` | merged | 4,099 pass | tturbo-mac |
| Oct 27 13:34 | `harden-p4f-roof-rebuild-on-wall-edit` | merged | 4,095 pass | tturbo-mac |
| Oct 27 13:18 | `harden-p4f-roof-mesh-solid` | merged | 4,088 pass | tturbo-mac |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 517 |
| bonsai-b | bonsai2-27b | agent | 456 |
| bonsai-ws2-a | bonsai2-27b | idle | 669 |
| bonsai-ws2-b | bonsai2-27b | agent | 635 |
| claude-sonnet-a | sonnet | agent | 13 |
| claude-sonnet-b | sonnet | idle | 14 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | idle | 317 |
| kimi-k31 | k3.1-pro-highspeed | idle | 62 |
| ornith-a | ornith15-397b-exl3-35 | held | 25 |
| ornith-b | ornith15-397b-exl3-35 | held | 16 |
| tturbo-mac | tturbo-ultraheretic | agent | 325 |
| tturbo-mac2 | tturbo-ultraheretic | agent | 140 |
