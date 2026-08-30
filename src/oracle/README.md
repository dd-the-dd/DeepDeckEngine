# Oracle parser structure

The Oracle parser translates card text into the engine's canonical rule IR.
Its module boundaries follow parsing responsibilities rather than sets or card
names:

- `model.rs` owns the public request, result, diagnostic, and audit contracts.
- `syntax/` owns document normalization and ability boundaries.
- `pipeline.rs` orchestrates parsing and audit output.
- `canonical/dispatch.rs` owns parser precedence by ability family.
- `canonical/ir.rs` and `canonical/numeric.rs` own reusable semantic primitives.

## Canonical parser migration rule

`canonical/mod.rs` is the legacy monolith. It still contains complete-text
comparisons and card-shaped parser families. Those constructs are migration
debt, not extension points.

New canonical modules must recognize reusable grammatical fragments and compose
small semantic primitives. They must not compare `text` with a complete Oracle
ability, dispatch with `match text`, or replace the comparison with a long regex
anchored over the whole ability. The architecture test freezes the current
legacy ceilings and requires them to decrease as rules are generalized.

Future extraction should be organized by grammar and semantics, for example:

- costs and activation restrictions;
- selectors and permanent/card criteria;
- conditions and object bindings;
- atomic effects such as draw, damage, move, create, and modify;
- ability composition for activated, triggered, replacement, static, spell,
  keyword, and modal abilities.

Exact Oracle text belongs in tests and fixtures only.
