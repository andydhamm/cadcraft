# arch3d build status

Updated 2026-10-27 22:00 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 4 of 13: Automatic roofs — 81 of 82 segments merged · 10 of 11 acceptance checks**

4,226 workspace tests passing at the last merge · 347 segments merged in all · 0 of 12 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | building | 10 / 11 | 37.2 h so far · expected done Oct 27 22:32–Oct 28 01:55 UTC |
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

81 merged, 6 replaced, 1 working

| Segment | What | State | Seat |
|---|---|---|---|
| `p4l-footprint-ring-walk` | order the roof footprint by edge connectivity, not by angle | working | darwin-ws3 |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 27 21:59 | `p4k-roof-plane-vertical-thickness` | merged | 4,226 pass | bonsai-ws2-b |
| Oct 27 21:49 | `p4j-build3d-reports-roofs` | merged | 4,224 pass | bonsai-ws2-a |
| Oct 27 21:40 | `harden-p4h-roof-eave-z-per-plane` | merged | 4,222 pass | tturbo-mac2 |
| Oct 27 21:17 | `harden-p4h-probe-crease-jitter` | merged | 4,217 pass | tturbo-mac2 |
| Oct 27 20:45 | `harden-p4h-roofplane-arg-forms` | merged | 4,210 pass | tturbo-mac2 |
| Oct 27 19:24 | `p4h-roofplane-arg-forms` | merged | 4,205 pass | bonsai-ws2-b |
| Oct 27 19:20 | `p4h-roof-eave-z-per-plane` | merged | 4,203 pass | claude-sonnet-b |
| Oct 27 19:15 | `p4h-roof-invariant-tests` | merged | 4,202 pass | bonsai-a |
| Oct 27 19:14 | `p4h-probe-crease-jitter` | merged | 4,200 pass | darwin-ws3 |
| Oct 27 19:10 | `p4h-valley-convexity` | merged | 4,198 pass | claude-sonnet-a |
| Oct 27 19:04 | `harden-p4h-profile-hole-rects` | merged | 4,196 pass | tturbo-mac |
| Oct 27 18:33 | `harden-p4h-profile-prism-extrude` | merged | 4,182 pass | tturbo-mac |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 521 |
| bonsai-b | bonsai2-27b | agent | 462 |
| bonsai-ws2-a | bonsai2-27b | agent | 676 |
| bonsai-ws2-b | bonsai2-27b | idle | 650 |
| claude-sonnet-a | sonnet | idle | 17 |
| claude-sonnet-b | sonnet | idle | 15 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | agent | 326 |
| kimi-k31 | k3.1-pro-highspeed | idle | 62 |
| ornith-a | ornith15-397b-exl3-35 | down | 29 |
| ornith-b | ornith15-397b-exl3-35 | down | 17 |
| tturbo-mac | tturbo-ultraheretic | agent | 339 |
| tturbo-mac2 | tturbo-ultraheretic | agent | 162 |
