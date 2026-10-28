# arch3d build status

Updated 2026-10-28 15:15 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 5 of 13: Stairs, railings and decks — 4 of 17 segments merged · 1 of 9 acceptance checks**

4,483 workspace tests passing at the last merge · 387 segments merged in all · 0 of 13 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | done | 11 / 11 | 52.1 h |
| 5 | Stairs, railings and decks | building | 1 / 9 | 2.3 h so far · expected done Oct 29 01:01–Oct 29 12:28 UTC |
| 6 | The 3D view, properly | planned | — | — |
| 7 | Generated elevations and sections | planned | — | — |
| 8 | Recognition from plain DXF | planned | — | — |
| 9 | Terrain and site | planned | — | — |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 5 segments

9 queued, 4 merged, 4 merging

| Segment | What | State | Seat |
|---|---|---|---|
| `p5-arch-stair-geom` | arch::stair — flight outline and well polygon | merging | bonsai-a |
| `p5-arch3d-stairs` | arch3d::stairs — stepped solid mesh | merging | mai-flash |
| `p5-arch3d-railing-deck` | arch3d::railing + deck slab | merging | darwin-ws3 |
| `p5-dxf-v5` | DXF: CADCRAFT_ARCH v5 carries stairs and railings | merging | bonsai-b |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 28 15:11 | `p5-render-plan` | merged | 4,483 pass | claude-sonnet-a |
| Oct 28 15:01 | `harden-p1-arch-layers` | refused | 4,493 pass | tturbo-mac2 |
| Oct 28 14:45 | `harden-p2a-holed-prism` | merged | 4,481 pass | tturbo-mac |
| Oct 28 14:31 | `p5-arch-stair-calc` | merged | 4,475 pass | bonsai-a |
| Oct 28 14:21 | `p5-doc-stair-model` | merged | 4,468 pass | darwin-ws3 |
| Oct 28 14:05 | `p5-arch-railing-rules` | merged | 4,463 pass | claude-sonnet-a |
| Oct 28 13:52 | `harden-p3r-types-arch3d` | merged | 4,459 pass | tturbo-mac |
| Oct 28 13:20 | `harden-p4s-footprint-rejects-interior-crossing-wall` | merged | 4,453 pass | tturbo-mac |
| Oct 28 12:51 | `harden-p3-arch3d-floor-slab-functions` | merged | 4,446 pass | tturbo-mac |
| Oct 28 12:43 | `p4s-footprint-rejects-interior-crossing-wall` | merged | 4,437 pass | darwin-ws3 |
| Oct 28 12:18 | `harden-p4-arch3d-wall-profile` | merged | 4,433 pass | tturbo-mac |
| Oct 28 12:00 | `harden-p1-glb-spec-layout` | merged | 4,429 pass | tturbo-mac2 |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | merging | 568 |
| bonsai-b | bonsai2-27b | merging | 520 |
| bonsai-ws2-a | bonsai2-27b | held | 700 |
| bonsai-ws2-b | bonsai2-27b | held | 650 |
| claude-sonnet-a | sonnet | idle | 21 |
| claude-sonnet-b | sonnet | idle | 21 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | merging | 391 |
| kimi-k31 | k3.1-pro-highspeed | idle | 70 |
| mai-flash | mai-code-1.1-flash | merging | 1 |
| ornith-a | ornith15-397b-exl3-35 | held | 29 |
| ornith-b | ornith15-397b-exl3-35 | held | 17 |
| tturbo-mac | tturbo-ultraheretic | merging | 366 |
| tturbo-mac2 | tturbo-ultraheretic | agent | 212 |
