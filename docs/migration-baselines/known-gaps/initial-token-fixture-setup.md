# Initial token setup in the runtime fixture runner

Status: fixed and verified in the runtime fixture runner. This correction does
not add a card to the coverage count.

The existing regression fixture
`crates/deepdeck-runtime-fixture-runner/fixtures/leyline-of-the-void-token.json`
declares a token on the initial battlefield. The runner currently constructs
initial objects through `GameEngine::new` and then relocates deck objects with
`configure_deterministic_position`. Deck construction correctly excludes token
definitions from the library. Relocation therefore cannot find the initial
token and fails before the spell, destruction, replacement, or assertions run:

```text
deterministic position is missing player-1 card opponent-spirit-token
```

The deterministic setup boundary now materializes a real token as an initial
battlefield object with its correct definition and a unique setup identity. It
preserves the declared battlefield order and applies controller/state/counter
overrides afterward. Construction publishes no entry event, replacement, or
token-created trigger. The deck token filter remains intact; no ordinary card
is converted into a token. Tokens in unsupported initial zones fail explicitly.

Before the correction, this was the sole failure among the 337 stable
first-level runtime fixtures replayed against snapshot 87D9 on 2026-11-12;
the other 336 passed. See
`.tmp/targeted-zone-move-runtime-regression-snapshot.jsonl` and its timing report
(1.231 s).

After relinking the runtime runner on 2026-11-12 at 17:33 UTC, the unchanged
Leyline fixture passes. The complete stable runtime replay passes 342/342
scenarios and 2,118 assertions in 1.170 s. Evidence:
`.tmp/permanent-died-runtime-regression-snapshot.jsonl` and
`.tmp/permanent-died-runtime-regression-snapshot-timing.json`.
