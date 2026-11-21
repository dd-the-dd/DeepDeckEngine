# Known gap: combat damage to Battles

Status: unsupported runtime behavior. This document is not a migration claim.

## Required behavior

Comprehensive Rules 120.3h and 310.6 state that damage dealt to a Battle removes that many defense counters from it. Rule 122.1g then defines the zero-defense state-based action.

The local rules evidence is `.tmp/MagicCompRules-20261106.txt` (rules 120.3h, 122.1g, 310.6, and 310.7).

## Reproduction

The non-gating diagnostic fixture is:

`crates/deepdeck-runtime-fixture-runner/fixtures/known-gaps/battle-combat-defense.json`

It starts a 2/2 creature attacking a Battle with four defense counters. The attack is legal and is declared; the defending player's life is unchanged.

Run it directly against a current runtime runner:

```powershell
.\.tmp\targets\card-migration\debug\deepdeck-runtime-fixture-runner.exe `
  crates/deepdeck-runtime-fixture-runner/fixtures/known-gaps/battle-combat-defense.json
```

Observed on 2026-11-12:

- `attackerDeclared`: expected 1, actual 1;
- defending-player life: expected 20, actual 20;
- Battle defense counters: expected 2, actual 4.

The diagnostic therefore fails exactly the defense-counter assertion. It lives in `fixtures/known-gaps`, outside the normal green fixture glob.

## Scope and promotion rule

The gap is in combat-damage application to a Battle, not in defender selection or `attackTax`. It must not be hidden by changing the expected counter count, and Battle combat must not be marked migrated from attack-declaration evidence alone.

Promotion requires a generic damage path that:

1. applies combat damage to the attacked Battle;
2. removes the corresponding defense counters;
3. preserves replacement and prevention processing;
4. runs the zero-defense state-based action correctly;
5. turns this diagnostic green without a card-named branch.
