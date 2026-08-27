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
