# Known gap: free alternative costs and X

Status: fixed for supported mana costs and runtime-proven; the five focused Rust tests also pass in the parser-independent partition-1 harness. This engine-quality correction does not add a card to the coverage count.

Focused proof: `cargo test --locked -p deepdeck-engine-runtime --no-default-features --features ci-unit-tests-1 --lib engine::casting_cost::tests -- --nocapture` (5/5). Measured compilation: 35.68 s; test execution rounded to 0.00 s. Artifacts: `.tmp/casting-cost-unit-tests-second.log` and `.tmp/casting-cost-unit-tests-second-timing.json`.

Magic Comprehensive Rules 107.3b requires `X = 0` when a spell with `{X}` is cast while paying neither its mana cost nor an alternative cost containing X. The local rules snapshot is `.tmp/MagicCompRules-20261106.txt` (rule 107.3b; Wizards snapshot dated 2026-11-06).

Before the correction, the engine created ordinary X declarations before expanding `useAlternativeCost`. When mana was available, declarations greater than zero remained selectable even though the selected base cost became empty.

The pre-fix diagnostic witness is `.tmp/diagnostic-free-alternative-cost-x-red.json`; its red baseline is archived in `.tmp/diagnostic-free-alternative-cost-x-red-result.json`.

- It gives the controller three generic mana, a controlled commander, and a `{X}{U}` spell with the generic zero-cost alternative.
- It requests `xValue = 3` with `useAlternativeCost = true`.
- The pre-fix runtime accepted the cast and failed the assertions requiring the illegal declaration to leave the spell in hand with no `alternativeCostPaid` event.
- The witness lives under `.tmp`, not in the green runtime fixture suite.

The correction constrains declarations using the selected `effectiveAlternativeCost`, rather than scanning every alternative on the card. It preserves nonzero X when the selected base, alternative, or actually paid additional mana cost contains X, and substitutes the declared value in every paid mana component. A known declaration key with no legal matching value now reports cast unavailability without hiding unknown-key or ambiguity errors.

Green runtime proofs:

- `crates/deepdeck-runtime-fixture-runner/fixtures/casting-x-alternative-costs.json`: free X0 available, free X3 unavailable, normal X3 available, alternative `{X}{B}` X3 available.
- `crates/deepdeck-runtime-fixture-runner/fixtures/casting-x-alternative-payment.json`: X3 resolves to and pays `{3}{B}`.
- `crates/deepdeck-runtime-fixture-runner/fixtures/casting-x-additional-payment.json`: a free base cost preserves and pays an additional `{X}` as `{3}`.
- `.tmp/runtime-x-final-proof.jsonl`: 6/6 fixtures and 32/32 assertions, including the exact commander-family cards and Baleful Strix.

Remaining boundary: X embedded in non-mana costs is outside the currently executable alternative-cost contract and was not generalized by this change.

Fierce Guardianship and Deadly Rollick are unaffected because neither mana cost contains X.
