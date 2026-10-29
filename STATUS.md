# arch3d build status

Updated 2026-10-29 00:30 UTC. This page and the card in the README are regenerated every 15 minutes from the swarm's own dashboard.

**Phase 5 of 13: Stairs, railings and decks — 21 of 24 segments merged · 7 of 9 acceptance checks**

4,680 workspace tests passing at the last merge · 420 segments merged in all · 0 of 13 seats working

## Phases

| # | Phase | Status | Acceptance checks | Time |
|---|---|---|---|---|
| 0 | Survey | done | — | 0.2 h |
| 1 | Walls and joins | done | 9 / 9 | 31.0 h |
| 2 | Openings | done | 9 / 9 | 15.8 h |
| 3 | Rooms, floors, ceilings and levels | done | 9 / 9 | 56.2 h |
| 4 | Automatic roofs | done | 11 / 11 | 52.1 h |
| 5 | Stairs, railings and decks | building | 7 / 9 | 11.5 h so far · expected done Oct 29 04:59–Oct 29 09:58 UTC |
| 6 | The 3D view, properly | planned | — | — |
| 7 | Generated elevations and sections | planned | — | — |
| 8 | Recognition from plain DXF | planned | — | — |
| 9 | Terrain and site | planned | — | — |
| 10 | Lighting, light fixtures and final renders | planned | — | — |
| 11 | Ray-traced Finalize Render | planned | — | — |
| 12 | Exports and interoperability | planned | — | — |
| 13 | Polish | planned | — | — |

## Phase 5 segments

21 merged, 2 merging, 1 gate

| Segment | What | State | Seat |
|---|---|---|---|
| `p5f-stair-cmd-optional-levels` | stair: levels optional, and the well holes the upper room | gate | claude-sonnet-a |
| `p5f-inspect-holes-are-rings` | arch.inspect reports room holes as rings, not a count | merging | mai-flash |
| `p5f-dxf-deck-height-roundtrip` | Deck rooms and room holes survive the DXF arch record | merging | bonsai-a |

## Recent merges

| When (UTC) | Segment | Result | Workspace tests | Seat |
|---|---|---|---|---|
| Oct 29 00:30 | `harden-p1-tests-bench` | merged | 4,680 pass | tturbo-mac2 |
| Oct 29 00:24 | `p5f-stair-stepped-profile` | merged | 4,677 pass | claude-sonnet-b |
| Oct 28 23:59 | `p5f-build-merge-well-holes` | merged | 4,675 pass | claude-sonnet-a |
| Oct 28 23:18 | `harden-p5-docs` | merged | 4,673 pass | tturbo-mac2 |
| Oct 28 22:51 | `harden-p1-cli` | merged | 4,673 pass | tturbo-mac2 |
| Oct 28 21:51 | `p5-docs` | merged | 4,667 pass | darwin-ws3 |
| Oct 28 21:49 | `upstream-sync-336243e0a` | merged | 4,667 pass | claude-sonnet-b |
| Oct 28 21:47 | `harden-p5-golden-stairs` | merged | 4,667 pass | tturbo-mac2 |
| Oct 28 21:46 | `harden-p5-cmd-stair` | merged | 4,657 pass | tturbo-mac |
| Oct 28 21:40 | `p5-inspect-stairs` | merged | 4,652 pass | bonsai-b |
| Oct 28 21:29 | `p5-i18n-uk` | merged | 4,648 pass | darwin-ws3 |
| Oct 28 21:07 | `upstream-sync-9de2b8bbd` | merged | 4,646 pass | claude-sonnet-a |

## Seats active in the last hour

| Seat | Model | State | Rounds |
|---|---|---|---|
| bonsai-a | bonsai2-27b | merging | 592 |
| bonsai-b | bonsai2-27b | gate | 542 |
| bonsai-ws2-a | bonsai2-27b | held | 700 |
| bonsai-ws2-b | bonsai2-27b | held | 650 |
| claude-sonnet-a | sonnet | gate | 26 |
| claude-sonnet-b | sonnet | idle | 23 |
| darwin-ws3 | darwin-r3-exl3-3.05bpw-sc | gate | 419 |
| kimi-k31 | k3.1-pro-highspeed | idle | 71 |
| mai-flash | mai-code-1.1-flash | merging | 3 |
| ornith-a | ornith15-397b-exl3-35 | held | 29 |
| ornith-b | ornith15-397b-exl3-35 | held | 17 |
| tturbo-mac | tturbo-ultraheretic | merging | 383 |
| tturbo-mac2 | tturbo-ultraheretic | agent | 234 |
