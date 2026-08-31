# Oracle parser structure

The Oracle parser translates card text into the engine's canonical rule IR.
Its module boundaries follow parsing responsibilities rather than sets or card
names:

- `model.rs` owns the public request, result, diagnostic, and audit contracts.
- `syntax/` owns normalization, ability boundaries, tokenization,
  classification, and Class progression.
- `pipeline.rs` orchestrates parsing and audit output.
- `audit.rs` builds simplification iterations and audit stages.
- `canonical/dispatch.rs` owns parser precedence by ability family.
- `canonical/context.rs`, `ir.rs`, `numeric.rs`, `operations.rs`, and
  `values.rs` own reusable semantic primitives and construction context.
- `canonical/costs.rs`, `criteria.rs`, and `conditions.rs` own reusable grammar.
- `canonical/effects/` owns effect parsing and composition.
- `canonical/abilities/` owns activated, triggered, replacement, static, spell,
  keyword, and modal ability families.

## Canonical parser migration rule

`canonical/mod.rs` is only the ordered composition root. Parser code belongs in
the grammar, effect, or ability-family file that owns its responsibility.
Complete-text comparisons and card-shaped parser families still exist in those
files as migration debt, not as extension points.

New canonical modules must recognize reusable grammatical fragments and compose
small semantic primitives. They must not compare `text` with a complete Oracle
ability, dispatch with `match text`, or replace the comparison with a long regex
anchored over the whole ability. The architecture test measures those forms
across all canonical production files, freezes their current ceilings, and
requires them to decrease as rules are generalized.

Further decomposition should continue to follow grammar and semantics:

- costs and activation restrictions;
- selectors and permanent/card criteria;
- conditions and object bindings;
- atomic effects such as draw, damage, move, create, and modify;
- ability composition for activated, triggered, replacement, static, spell,
  keyword, and modal abilities.

Exact Oracle text belongs in tests and fixtures only.

## Operations and execution contexts

Costs and resolving effects share canonical operations. For example, moving a
card from a graveyard to exile is represented by the same `move` operation in
either context. The containing `costs` or `effects` sequence determines how the
engine executes it:

- a cost operation is chosen during declaration, must be fully payable, and is
  committed while activating the ability;
- a resolving operation uses resolution semantics and does as much as it can.

Object choices used to pay a cost are represented by `chooseObjects` and
`chosenObjects`; they are deliberately distinct from `chooseTargets` and do not
become targets of the resulting stack object.
