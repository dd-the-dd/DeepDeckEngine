# Casting total-cost composition — 2026-11-12

## Outcome

The engine now computes casting mana in the rules order shared by ordinary, alternative, and free-base casts:

1. choose the printed or alternative base;
2. add mana-cost increases and additional mana costs;
3. resolve `X`;
4. apply generic cost reductions across the complete mana-component vector;
5. apply the maximum applicable mana floor;
6. apply Delve as a payment substitution after the total cost is determined.

This removes the previous base-only reduction behavior. The discriminating case `{1}` base + `{1}` tax - `{2}` reduction is now free, and a colored `{R}` base + `{1}` tax - `{1}` reduction remains `{R}`.

Delve is intentionally not modeled as a cost reduction. Under a three-mana floor, a `{1}` Delve spell first becomes `{3}`; exiling two cards then leaves `{1}` payable. A Delve spell with no `delveCards` decision receives no payment substitution.

## Implementation boundary

- `src/engine/casting_cost.rs`
  - pure component-wide generic removal;
  - `X` resolution, cost reductions, and mana floor finalization;
  - post-floor Delve substitution;
  - common base-component entry point.
- `src/engine/mod.rs`
  - ordinary/rules-based alternative casts;
  - casts without the printed mana cost;
  - permission and conditional alternatives;
  - commander tax and target-dependent casting reductions;
  - prototype, morph, blitz, dash, and warp explicit bases.
- `src/engine/casting_cost_tests.rs`
  - seven new focused unit tests.

No non-mana cost representation or payment path was changed. Convoke and improvise still operate after finalized casting mana. Existing `Vec<String>` component identity is preserved, including empty components after a complete generic reduction.

## Runtime proofs

| Proof | Expected behavior | Result |
|---|---|---|
| `casting-tax-reduction-order.json` | `{1} + {1} - 2 = 0` | Green after being a confirmed red known gap |
| `casting-tax-reduction-colored.json` | `{R} + {1} - 1 = {R}` | Green |
| `casting-delve-minimum-floor.json` | floor to `{3}`, then Delve two to `{1}` | Green |
| full runtime gate | existing and new runtime fixtures | 369/369 scenarios, 2,299 assertions |

The initial 368-scenario gate excluded the then-`known-gaps` copy of `casting-tax-reduction-order.json`; root ran that fixture directly, confirmed it green and moved it into the ordinary gate. The final replay includes all 369 scenarios.

Runtime build and gate timing:

- compilation/link: **45.857 s**;
- fixture execution: **2.105 s**;
- report: `.tmp/casting-total-cost-runtime-build.json`.

Final replay: **1.439 s**, report `.tmp/casting-total-cost-runtime-final.json`.
Refreshing the fingerprint after the test-only import correction required a
**0.250 s Cargo cache check**, with no production recompilation. The contract
runner was subsequently relinked against the updated engine in **2.74 s**.

## Unit proofs and development loop

The focused partition finished with **12/12 tests green**:

- compilation/link: **38.552 s**;
- wrapper execution: **1.538 s**;
- Rust harness execution: **0.00 s**;
- report: `.tmp/casting-total-cost-unit-final-timing.json`.

The first focused attempt is retained rather than hidden: it failed before test execution because the child test module did not import the two private helper functions. That attempt spent **21.616 s** compiling and **0 s** executing tests; report `.tmp/casting-total-cost-unit-timing.json`. Root added the two test-only imports, with no production behavior change, before the successful rerun.

## Source fingerprints at acceptance

| File | SHA-256 |
|---|---|
| `src/engine/casting_cost.rs` | `73CE7AF84C3E149F743BB037DC64911A5E434839539E8E22EDEF37286CD62EFA` |
| `src/engine/casting_cost_tests.rs` | `32D2954C07AFBB9788E7FFBF5022134272CF5ABA08F9E9221365C398CF2BA49D` |
| `src/engine/mod.rs` | `CA3F1D21705DD33790BFE0547B132DC46116FE27249C4F76AFC6C206370343A3` |
| `casting-tax-reduction-order.json` | `C4E2170796E8CE9F0B38268E0F80D50F9EAB6BA2BDF9CB12242ECFEF8E2B2BC3` |
| `casting-tax-reduction-colored.json` | `229B64B0D32D1CE4736E0D9B112805F51BF66341EECA0F12D6F89492B4C9DCEA` |
| `casting-delve-minimum-floor.json` | `6E0CA9F85EF7001BC49FBEFB3E24036347D43C90FD5ED2A93CE44AE9B1CF1332` |

## Deliberate limits

- This slice composes generic mana increases and generic reductions. It does not introduce colored-cost reduction semantics such as Morophon.
- Hybrid symbols with a generic option, such as `{2/W}`, retain the engine's previous parsing/payment behavior; this change does not broaden mana-symbol semantics.
- The spell-tax Oracle parser is a separate token/FSM change. Runtime support here proves the primitive interaction but does not by itself certify every Oracle surface.
- Special casting modes now share total-cost finalization. This slice does not add new target-dependent reduction grammar or new non-mana alternative-cost grammar.
- Payment-source legality, Delve card validation, convoke, improvise, and non-mana costs continue through their existing dedicated validators.

