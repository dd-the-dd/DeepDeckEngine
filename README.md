# DeepDeckEngine

DeepDeckEngine is the public, authoritative Rust implementation of Deep Deck's
Magic: The Gathering Oracle parser, legal-action engine, state transitions,
local HTTP server, and agent-session protocol.

The hosted Deep Deck League website, accounts, rankings, matchmaking and
private model weights are deliberately not part of this repository. The Pixi
renderer and local visual client live in the independent
[`DeepDeckPixi`](https://github.com/dd-the-dd/DeepDeckPixi) repository.

## Quick start

Install the Rust toolchain, then run:

```powershell
cargo test --locked
cargo run --locked --bin mtg-engine-server
```

The API listens on `http://127.0.0.1:8787` by default. Set
`MTG_ENGINE_ADDR=0.0.0.0:8787` when it must accept connections outside the
current machine. `GET /health` is the readiness endpoint.

## Card catalog

Bulk card data and copyrighted card images are not committed. Endpoints that
need the minimized catalog resolve `data/cards-minimized.json` or the explicit
path in `MTG_CARD_CATALOG_PATH`. See [data/README.md](data/README.md).

## Agent protocol

### Commander qualification tables (0.1.3)

Commander remains a four-player, 40-life format. An organizer may explicitly
set `allowShortCommanderPod: true` when creating a local-deck or network game
to resolve a qualification tie at a two- or three-player Commander table.
The flag is off by default, is rejected for other formats, and does not switch
to Duel Commander or remove commander rules. It accepts only two to four seats;
ordinary Commander requests still require exactly four. This opt-in is not an
automatic tournament scheduler or a replacement for the organizer's result proof.

The WebSocket agent protocol is implemented in `src/agent_protocol.rs`. A
server can require an API key by setting `MTG_ENGINE_API_KEY`; clients then send
that key during registration. Never commit real keys or `.env` files.
The versioned `agent-manifest/v1` JSON Schema and compatibility fixtures live in
[`contracts/agent-manifest/v1`](contracts/agent-manifest/v1). Release archives and
containers include this contract bundle for SDK and integration consumers.

## Public contracts and releases

- Rust owns Oracle parsing, legal actions and game-state mutation.
- Breaking API or replay-schema changes require a versioned migration note.
- DeepDeckPixi releases declare the DeepDeckEngine versions they support.
- Consumers should pin a release tag or commit SHA, never a floating `main`.

## Development

```powershell
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo test --locked
pwsh ./scripts/check-public-tree.ps1
```

The workspace has three compilation boundaries for fast card migration:

```powershell
# Parser-only semantic fixture; does not compile the simulation runtime.
cargo test -p deepdeck-oracle --test card_coverage

# Parser-only Scryfall audit in official set order.
$env:AUDIT_CARD_BY_CARD='1'
$env:AUDIT_COLLECTOR='69'
cargo run -p deepdeck-oracle --features audit-cli --example audit_parser_set -- set:msh

# Runtime proof for a card, using only the relevant private-test partition.
cargo test -p deepdeck-engine-runtime --no-default-features `
  --features ci-unit-tests-2 parsed_mana_symbol_cast_trigger_filters

# Parser IR to runtime-schema contract regression.
cargo test -p deepdeck-oracle-engine-contracts --no-default-features `
  --features ci-unit-tests-3 reusable_marvel_leaves
```

`deepdeck-oracle` owns Oracle text to primitive IR. `deepdeck-engine-runtime`
owns deterministic simulation and rule executability. `mtg-engine` contains the
HTTP/session/application layer. The contract-only package keeps private parser
tests coupled to the public runtime validator without putting that cost in the
parser edit loop.

`cargo check --all-targets` is intentionally part of CI because it type-checks
every target. CI executes parser, runtime, parser-runtime contracts, and the
application tests as independently linked, memory-bounded targets on Linux and
Windows. Runtime and contract unit tests remain divided into four partitions.
`scripts/check-unit-test-partitions.ps1` guards the partition inventory and
fails when a new private-access test has not been assigned to a partition.
CI also executes the independently linked integration-test binaries.
The reviewed-fixture audit binaries remain visible as advisory CI steps while
their explicitly linked parser regressions are repaired; their fixtures are not
weakened to make a release pass.

See [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request.
