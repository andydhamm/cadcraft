# arch3d build status

Updated 2026-10-28 00:00 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 4 of 13: Automatic roofs — 81 of 84 segments merged · 10 of 11 acceptance checks**

4,248 workspace tests passing at the last merge · 351 segments merged in all · 0 of 12 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | building | 10 / 11 | 39.2 h so far · expected done Oct 28 01:55–Oct 28 06:42 UTC |
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

81 merged, 7 replaced, 2 working, 1 queued

| Segment | What | State | Seat |
|---|---|---|---|
| `p4m-footprint-connectivity-walk` | build the roof footprint ring by edge connectivity, pruning spurs | working | bonsai-a |
| `upstream-sync-3929d94d8` | merge upstream upstream/main up to 3929d94d8 (23 commits) | working | claude-sonnet-a |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 27 23:36 | `harden-p2-arch-sample-openings` | merged | 4,248 pass | tturbo-mac2 |
| Oct 27 23:20 | `harden-p4j-build3d-reports-roofs` | merged | 4,241 pass | tturbo-mac2 |
| Oct 27 22:36 | `harden-p4k-roof-plane-vertical-thickness` | merged | 4,234 pass | tturbo-mac2 |
| Oct 27 22:18 | `harden-p4h-roof-invariant-tests` | merged | 4,229 pass | tturbo-mac2 |
| Oct 27 21:59 | `p4k-roof-plane-vertical-thickness` | merged | 4,226 pass | bonsai-ws2-b |
| Oct 27 21:49 | `p4j-build3d-reports-roofs` | merged | 4,224 pass | bonsai-ws2-a |
| Oct 27 21:40 | `harden-p4h-roof-eave-z-per-plane` | merged | 4,222 pass | tturbo-mac2 |
| Oct 27 21:17 | `harden-p4h-probe-crease-jitter` | merged | 4,217 pass | tturbo-mac2 |
| Oct 27 20:45 | `harden-p4h-roofplane-arg-forms` | merged | 4,210 pass | tturbo-mac2 |
| Oct 27 19:24 | `p4h-roofplane-arg-forms` | merged | 4,205 pass | bonsai-ws2-b |
| Oct 27 19:20 | `p4h-roof-eave-z-per-plane` | merged | 4,203 pass | claude-sonnet-b |
| Oct 27 19:15 | `p4h-roof-invariant-tests` | merged | 4,202 pass | bonsai-a |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | agent | 522 |
| bonsai-b | bonsai2-27b | agent | 472 |
| bonsai-ws2-a | bonsai2-27b | held | 700 |
| bonsai-ws2-b | bonsai2-27b | held | 650 |
| claude-sonnet-a | sonnet | agent | 17 |
| claude-sonnet-b | sonnet | idle | 15 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | agent | 335 |
| kimi-k31 | k3.1-pro-highspeed | idle | 65 |
| ornith-a | ornith15-397b-exl3-35 | held | 29 |
| ornith-b | ornith15-397b-exl3-35 | held | 17 |
| tturbo-mac | tturbo-ultraheretic | agent | 342 |
| tturbo-mac2 | tturbo-ultraheretic | gate | 170 |
