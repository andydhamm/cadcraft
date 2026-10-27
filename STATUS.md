# arch3d build status

Updated 2026-10-27 19:15 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 4 of 13: Automatic roofs — 77 of 79 segments merged · 8 of 11 acceptance checks**

4,202 workspace tests passing at the last merge · 340 segments merged in all · 0 of 12 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | building | 8 / 11 | 34.4 h so far · expected done Oct 27 21:18–Oct 28 00:32 UTC |
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

77 merged, 6 replaced, 1 working, 1 merging

| Segment | What | State | Seat |
|---|---|---|---|
| `p4h-roofplane-arg-forms` | roofplane: accept base as [x0,y0,x1,y1] and fascia/soffit as booleans | working | bonsai-ws2-b |
| `p4h-roof-eave-z-per-plane` | Roof meshes use each plane's own eave_z | merging | claude-sonnet-b |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 27 19:15 | `p4h-roof-invariant-tests` | merged | 4,202 pass | bonsai-a |
| Oct 27 19:14 | `p4h-probe-crease-jitter` | merged | 4,200 pass | darwin-ws3 |
| Oct 27 19:10 | `p4h-valley-convexity` | merged | 4,198 pass | claude-sonnet-a |
| Oct 27 19:04 | `harden-p4h-profile-hole-rects` | merged | 4,196 pass | tturbo-mac |
| Oct 27 18:33 | `harden-p4h-profile-prism-extrude` | merged | 4,182 pass | tturbo-mac |
| Oct 27 18:16 | `harden-p4x-cli-render-draws` | merged | 4,175 pass | tturbo-mac |
| Oct 27 18:02 | `harden-p2-arch-fit-and-sill` | merged | 4,168 pass | tturbo-mac2 |
| Oct 27 17:39 | `p4h-opening-hole-acceptance` | merged | 4,160 pass | darwin-ws3 |
| Oct 27 17:34 | `p4h-profile-prism-extrude` | merged | 4,156 pass | bonsai-ws2-b |
| Oct 27 17:29 | `harden-p2-engine-opening-params` | merged | 4,153 pass | tturbo-mac2 |
| Oct 27 17:24 | `harden-p4r-doc-validate` | merged | 4,148 pass | tturbo-mac |
| Oct 27 17:12 | `upstream-sync-0ff1b0353` | merged | 4,142 pass | bonsai-ws2-a |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | merging | 521 |
| bonsai-b | bonsai2-27b | agent | 460 |
| bonsai-ws2-a | bonsai2-27b | merging | 672 |
| bonsai-ws2-b | bonsai2-27b | agent | 646 |
| claude-sonnet-a | sonnet | idle | 16 |
| claude-sonnet-b | sonnet | merging | 15 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | idle | 322 |
| kimi-k31 | k3.1-pro-highspeed | idle | 62 |
| ornith-a | ornith15-397b-exl3-35 | gate | 26 |
| ornith-b | ornith15-397b-exl3-35 | agent | 16 |
| tturbo-mac | tturbo-ultraheretic | agent | 332 |
| tturbo-mac2 | tturbo-ultraheretic | agent | 149 |
