# Local card catalog

DeepDeckEngine does not redistribute Scryfall bulk data or card images.

Generate or obtain the minimized catalog through your own permitted data
pipeline, place it at `data/cards-minimized.json`, or point the engine to it:

```powershell
$env:MTG_CARD_CATALOG_PATH = 'D:\data\cards-minimized.json'
cargo run --locked --bin mtg-engine-server
```

The catalog file is ignored by Git. Respect the upstream data and image usage
policies that apply to your copy.

## Migration priority queue

`../docs/played-card-migration-priority-2026-11-09.json` is a dated,
reproducible snapshot used to migrate highly played cards before continuing in
set order.
It contains the top 50 cards from each of Legacy, Commander, and Standard,
deduplicated by Scryfall `oracle_id` and fused with equal-format Reciprocal
Rank Fusion. Consumers should process `queue` by `global_rank`; each row keeps
its original format rank and source score for auditability.

This snapshot is intentionally dated because metagames and format legalities
change. Refresh the sources and Scryfall legalities before treating it as a
current ranking after 2026-11-09.

`../docs/played-card-migration-priority-expanded-2026-11-09.json` preserves
those first 145 rows exactly and appends the public EDHREC Top Cards positions
51 through 500. Moxou exposes only the top 50 Legacy and Standard cards without
an account, so deeper ranks for those formats are deliberately not inferred.

Generate the exhaustive local queue with:

```powershell
.\scripts\build-card-migration-queue.ps1
```

Verify that the seed remains unchanged, popularity stays at the front, ranks
are continuous, and every exhaustive Oracle identifier is unique with:

```powershell
.\scripts\check-card-migration-queue.ps1
```

The script downloads the official Scryfall Oracle and Default Cards bulk
files into `.tmp/card-migration-cache/source/` and writes the complete queue to
`.tmp/card-migration-cache/played-card-migration-exhaustive-2026-11-09.json`.
Bulk files and the exhaustive derived catalog remain outside Git. The queue is
deduplicated by `oracle_id` and ordered in explicit tiers:

1. the preserved cross-format top-50 seed;
2. EDHREC's published top 500;
3. Scryfall EDHREC ranks up to the configured cutoff;
4. the Marvel Super Heroes set family;
5. older sets, newest to oldest;
6. released post-cutoff reconciliation cards;
7. future cards, then tokens/emblems, then art-series and memorabilia objects.

Cards pulled forward by popularity occur only once and are skipped when their
set is reached. The final tiers keep every Scryfall Oracle ID visible without
letting unreleased or non-game objects displace playable cards.
