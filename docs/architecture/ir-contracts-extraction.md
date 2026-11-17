# Lightweight IR executability contracts

Status: approved implementation plan. Extraction starts after the active
Secret Invasion runtime changes have landed in `src/engine/mod.rs`.

## Objective

Make Oracle-to-engine contract tests compile the executable-IR validator
without compiling the complete game runtime. There must remain one validator,
one set of accepted IR shapes, and compatibility with the existing
`engine::rule_is_executable` entry point.

The target dependency graph is:

```text
deepdeck-ir-contracts --> serde_json, regex
          ^                         ^
          |                         |
deepdeck-engine-runtime      deepdeck-oracle-engine-contracts
          ^
          |
      mtg-engine
```

`deepdeck-ir-contracts` must not depend on the Oracle parser, card model, game
state, or runtime.

## Public boundary

The lightweight crate owns:

- JSON scalar and schema helpers;
- card, permanent, stack, and target filters;
- numeric, object, player, and condition expressions;
- costs, mana, keywords, replacement effects, and declarations;
- primitive effect validation;
- static, triggered, activated, and spell rule validation;
- the recursive executable-rule dispatcher.

Its deliberately narrow public surface is:

```rust
pub trait NamedTokenCatalog {
    fn contains_named_token(&self, name: &str) -> bool;
}

pub struct ExecutabilityValidator<C> { /* private fields */ }

impl<C: NamedTokenCatalog> ExecutabilityValidator<C> {
    pub fn rule_is_executable(&self, rule: &serde_json::Value) -> bool;
    pub fn effect_is_executable(&self, effect: &serde_json::Value) -> bool;
}
```

Additional validator facades may be exposed for runtime call sites that need
to validate a condition, expression, filter, cost, keyword, or event. Leaf
validators remain private.

Both runtime and contract crates preserve their current compatibility API:

```rust
engine::rule_is_executable(&rule)
```

These wrappers instantiate the shared validator with their card catalog.

## Cycles that must be removed

### Named tokens

The validator currently reaches through `named_token_rules` into model
compilation. The lightweight crate instead asks `NamedTokenCatalog` whether a
named token exists. Runtime and contract adapters use their existing catalog.
The recursive validator never calls the card compiler.

Tests must cover known, unknown, and catalog-error token names.

### Embedded prepare-spell definitions

The current validator deserializes `CardDefinition`, which belongs to the
runtime model. The lightweight crate validates the embedded JSON directly:

- required string fields: `id`, `name`, `typeLine`;
- `manaCost` is a string when present;
- `power` and `toughness` are strings or null when present;
- optional flags are booleans;
- `rules` is an array and every rule is recursively executable;
- the embedded card is an Instant or Sorcery.

Malformed-field parity tests are required before removing the legacy path.

## Extraction sequence

1. Add `deepdeck-ir-contracts` with schema and mana modules. Move, do not copy,
   the pure mana code and re-export it for runtime callers.
2. Move filters, references, numeric expressions, conditions, costs, and
   keywords behind thin runtime wrappers.
3. Move effect validation, including the existing damage, life, and token
   validator leaves. Keep effect execution in the runtime.
4. Introduce `NamedTokenCatalog`, move declaration and rule recursion, and
   validate embedded prepare-spell JSON without runtime model types.
5. Switch `deepdeck-oracle-engine-contracts` from
   `deepdeck-engine-runtime` to `deepdeck-ir-contracts`.
6. Remove the legacy validator only after corpus parity and a production
   symbol search prove that no duplicate implementation remains.

Each step must leave its focused crate checks green. No step may temporarily
accept an unknown kind merely to preserve compilation.

## Required proof

Before the contract crate is switched, a test-only legacy facade compares the
old and new validators over:

- all canonical rules emitted by the migration corpus;
- every dedicated card-coverage fixture;
- nested effects, granted abilities, split faces, and rules markers;
- known and unknown named tokens;
- valid and malformed embedded prepare-spell definitions.

The required result is zero divergence.

After the switch:

```powershell
cargo tree -p deepdeck-oracle-engine-contracts
```

must contain neither `deepdeck-engine-runtime` nor `rand`.

The following measurements are recorded before and after with isolated target
directories:

- cold contract build;
- warm contract build after a validator-only edit;
- warm card contract test;
- warm set contract audit.

The architectural success criterion is dependency removal and validator
parity. Timing improvements are reported as measured values, not estimates.

## Conflict policy

The extraction begins only after Secret Invasion has finished modifying the
validator and runtime sections of `src/engine/mod.rs`. During extraction, one
agent owns a validator family at a time. Card migrations may continue in
parser-only files and dedicated tests, but changes to validator-owned sections
must be coordinated before editing.
