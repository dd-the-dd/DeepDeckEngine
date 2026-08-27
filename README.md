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

The WebSocket agent protocol is implemented in `src/agent_protocol.rs`. A
server can require an API key by setting `MTG_ENGINE_API_KEY`; clients then send
that key during registration. Never commit real keys or `.env` files.

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

`cargo check --all-targets` is intentionally part of CI because it type-checks
the very large private-access unit-test modules. CI executes the independently
linked integration-test binaries; contributors with enough virtual memory can
also run the complete monolithic unit-test binary with `cargo test --lib`.
The reviewed-fixture audit binaries remain visible as advisory CI steps while
their explicitly linked parser regressions are repaired; their fixtures are not
weakened to make a release pass.

See [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request.
