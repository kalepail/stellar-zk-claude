# Champion Bots (Current)

Updated after v3 retargeting tuning check on 2026-02-08.

## Global Records (All Runs)

1. **All-time single-run score + survival (overall):** `offline-wrap-endurancex`
   - Seed `0x6046C93D`
   - Score `289,810`
   - Frames `67,109` (~`18m 38.5s`)
2. **Best non-offline single-run score + survival:** `omega-marathon`
   - Seed `0x6AFA2869`
   - Score `259,820`
   - Frames `47,059` (~`13m 4.3s`)

## Latest Aggregate Winners (v3 Check, 24 Seeds)

Primary runs:

- `benchmarks/efficiency-v3-check-score-24-1770585857`
- `benchmarks/efficiency-v3-check-survival-24-1770585857`

Winners:

1. **Aggregate score winner:** `offline-wrap-sniper30`
   - objective `29,841.23`
   - avg score `28,964.6`
   - avg frames `10,958.0`
   - avg fire frames `248.4`
2. **Aggregate survival winner:** `offline-wrap-sniper30`
   - objective `15,302.73`
   - avg score `28,964.6`
   - avg frames `10,958.0`
3. **Top challengers by objective in v3:**
   - `offline-supernova-hunt`
   - `offline-wrap-apex-score`
   - `offline-wrap-frugal-ace`

Top spike in v3:

- `offline-wrap-frugal-ace`, seed `0x2CDBE3D6`, score `158,630`, frames `33,612`

## Prior 48-Seed v2 Milestone (Still Important)

- `benchmarks/efficiency-v2-rematch-score-48-1770583645`
- `benchmarks/efficiency-v2-rematch-survival-48-1770583645`
- Aggregate winner there remained `offline-wrap-endurancex`.
- Best v2 spike: `offline-wrap-endurancex`, seed `0x237D8467`, score `149,390`, frames `30,641`.

## 30-Minute Cap Status

- Full cap is `108,000` frames.
- v3 check cap hits: `0/192` (score) and `0/192` (survival).
- Global best observed survival remains `67,109` frames.

## Canonical Checkpoint Tapes

Global all-time references:

- `checkpoints/rank01-offline-wrap-endurancex-seed6046c93d-score289810-frames67109.tape`
- `checkpoints/rank02-offline-wrap-endurancex-seedfb2a7978-score278190-frames54811.tape`
- `checkpoints/rank03-offline-wrap-endurancex-seede35b682c-score267430-frames53605.tape`
- `checkpoints/rank01-omega-marathon-seed6afa2869-score259820-frames47059.tape`

Latest v3 references:

- `checkpoints/rank01-offline-wrap-frugal-ace-seed2cdbe3d6-score158630-frames33612.tape`
- `checkpoints/rank02-offline-wrap-sniper30-seed855d1fd0-score94600-frames21564.tape`
- `checkpoints/rank04-offline-wrap-sniper30-seed08499531-score72590-frames22846.tape`
- `checkpoints/rank01-offline-wrap-endurancex-seed237d8467-score149390-frames30641.tape`

## Active Roster (Maintained)

For ongoing development/benchmarking, the maintained roster is now limited to elite performers:

- `omega-marathon`, `omega-lurk-breaker`, `omega-ace`, `omega-alltime-hunter`, `omega-supernova`
- `offline-wrap-endurancex`, `offline-wrap-sniper30`, `offline-wrap-frugal-ace`, `offline-wrap-apex-score`, `offline-wrap-sureshot`, `offline-supernova-hunt`

## Reproducibility Note (2026-02-08)

A direct regeneration recheck of `offline-wrap-endurancex` on seed `0x6046C93D` using current code did not reproduce the historical record run. The historical record tape is retained as canonical evidence in `checkpoints/`.

## Record Lock

- Added `record-lock-endurancex-6046c93d` bot to preserve deterministic regeneration of the canonical all-time tape from code.
- Champion provenance + bot fingerprints are now tracked in `records/champions.json`.
- `record-lock-endurancex-6046c93d` reproduces `289,810` score / `67,109` frames for seed `0x6046C93D`.
