# Ability Map declarative expansion boundary

The Ability Map is a versioned description of Magic rules surfaces. It is not
an interpreter for Comprehensive Rules prose. `rulesText` and `tokenGrammar`
remain documentary inputs used for traceability, coverage reports, diagnostics,
and review; neither string is executable by the parser or engine.

## Four separate planes

1. **Documentation** owns the official name, aliases, CR references, normalized
   clauses, reminder-template identity, provenance, and version. Its output is
   the documentary index only.
2. **Tokenized recognition** maps a stable `automatonId` to compiled Rust code.
   The automaton borrows the one immutable token stream, returns typed bindings,
   and either consumes its complete range or reports a structured `UNKNOWN`.
   Catalog grammar atoms describe that contract but never dispatch or execute it.
3. **Typed lowering** maps the recognizer's typed result to an ordered sequence
   of registered primitive IDs. Inputs must be literals or named typed bindings;
   no free-form expression or rules-text fragment may become engine data.
4. **Primitive validation/runtime** checks every emitted operation against a
   compiled primitive signature registry before execution. The engine receives
   canonical IR only and has no dependency on rules prose, grammar templates,
   reminder text, ability names, or representative cards.

```text
official rules snapshot ──> documentary index
                                  │ review/diagnostics only
Oracle text ──> tokenize once ──> compiled local automaton
                                     │ typed result
                                     v
                              compiled lowering adapter
                                     │ primitive IR
                                     v
                         contract validator ──> engine
```

## Proposed catalog contract

An executable integration may add structured references beside the existing
documentary fields. The first closed pilot uses this shape:

```json
{
  "compiledIntegration": {
    "recognizerId": "keyword.mobilize.v1",
    "loweringAdapterId": "lower.keyword.mobilize.v1",
    "bindings": [
      { "name": "quantity", "type": "positiveInteger", "source": "recognizerOutput" },
      { "name": "mobilizeTokens", "type": "objectSet", "source": "loweringOutput" }
    ],
    "emittedPrimitives": [
      "createTokens",
      "installDelayedStepTrigger",
      "sacrificePermanents"
    ],
    "unknownPolicy": "structuredFailure"
  }
}
```

Every referenced ID and binding must resolve at build time to a closed Rust enum
and a known contract. The catalog cannot contain executable conditions,
selectors, Rust paths, JSON snippets, or interpolated rules prose.
`primitiveDecomposition` continues to document intended semantics until a
compiled lowering adapter and runtime proof exist; it does not itself emit IR.
The generated semantic registry may carry `compiledIntegration`, but the
lexical and documentary indexes omit it. The current Mobilize adapter still
constructs ordinary primitive IR in Rust, then verifies the actual nested
primitive sequence against the typed contract. It never dispatches a function
from a catalog string.

Reminder text is validated by its own local automaton and may populate the same
typed bindings as the primary surface. It never replaces Oracle text and never
creates behavior independently. A mismatch between keyword parameters and a
reminder is a committed parse failure.

## Failure and promotion rules

- Missing automaton ID, unknown grammar variant, unbound lowering input, unknown
  primitive, or signature mismatch produces explicit `UNKNOWN`; no legacy or
  prose interpretation is implicit.
- Lexical, parser, runtime, and proof support remain independent fields.
- `supportedNative` requires a compiled automaton, typed lowering, contract
  validation, runtime behavior, positive sibling coverage, and a negative
  near-miss. Documentary completeness alone never promotes support.
- Ability signals are emitted only when the CR defines a corresponding event.
  Ability words and documentary markers never gain semantics from their label.
- Composite abilities expand into reusable primitives. The engine must not add
  an ability-named handler merely because the catalog contains that ability.

## Incremental implementation order

1. Validate stable automaton/adapter ID syntax without changing runtime.
2. Generate a typed registry containing IDs and support metadata only.
3. Register one already-native vertical and reject unknown IDs in tests.
4. Add primitive-signature validation at the parser/runtime contract boundary.
5. Migrate catalog entries only after their existing parser and runtime proofs
   demonstrate exact semantic equivalence.

## Next reusable integration batch

| Ability | Current reusable path | Missing generic boundary before native promotion |
|---|---|---|
| Transform | `transformPermanent` plus `permanentTransformed` event already execute; Tamiyo supplies parser and runtime evidence | Complete local action/envelope coverage, double-faced eligibility diagnostics, and the once-per-stack/delayed-trigger transform guard from CR 701.27g |
| Amass | Token creation, object selection, subtype addition, counters, bindings, and `emitAbilitySignal` exist independently | A generic resolution-time object query/choice binding that can run after conditional token creation; current `amass` IR/runtime handler is composite debt and must not become the declarative adapter |
| Explore | Reveal, card predicates, `moveCards`, `putCounters`, choices, and conditional effects exist | Typed branching over the revealed-card binding and one shared continuation binding; current `explore` handler is composite debt |
| Overload | Alternative-cost declarations and cost-method bindings exist | A compiled alternate lowering branch that substitutes selector cardinality (`target` to `each`) without mutating or interpreting Oracle/rules text at runtime |

The c21 registry maps the highest-reuse generic action family:

| Official action | Existing atomic IR | Observable runtime event; named ability signal |
|---|---|---|
| Destroy | `destroyPermanent` | destruction/zone-change events; `never` |
| Exile | `exilePermanent` | permanent/card exile events; `never` |
| Discard | `discardCards` | one `cardDiscarded` event per card; `never` |
| Sacrifice | `sacrificePermanents` | sacrifice and leave-battlefield events; `never` |
| Tap and Untap | `tapPermanent`, `tapPermanents`, `untapPermanent`, `untapPermanents` | `permanentTapped` / `permanentUntapped`; `never` |

These actions do not need new runtime primitives. C21 records their official CR
clauses, lexical variants, local token grammars, bindings, primitive alternatives,
and intrinsic event hooks. Parser support remains `partial` across target,
chosen, each-player, cost, zone-context, and reflexive envelopes even though the
individual runtime operations are native. None has `compiledIntegration` until
one closed adapter validates its typed bindings and emitted IR.

Until those boundaries are implemented and proved, the registry must retain
partial/legacy/unsupported status and surface the unmatched form as structured
`UNKNOWN`. In particular, neither `amass`, `explore`, nor an Overload text
rewriter is an acceptable new ability-named primitive.

## Continuous Ability Map work queue

After each generalization batch, the Ability Map agent continues with the next
official rules surface instead of stopping at documentary coverage. Each batch
must deliver the following vertical slice:

1. Link the official ability/action identity to its Comprehensive Rules section,
   effective rules date, provenance, aliases, and rules text.
2. Describe its local token grammar and typed bindings without making the
   documentary grammar executable.
3. Reuse or add a compiled token-driven automaton that consumes the shared token
   stream and produces a structured failure for every unsupported form.
4. Lower the typed result into existing atomic IR primitives. If a reusable
   primitive is genuinely missing, add that primitive independently of the
   ability name and prove its generic contract first.
5. Connect intrinsic engine events and an ability signal only when the rules
   define an observable named event. A label alone never implies a signal.
6. Add positive, sibling, malformed near-miss, contract, and dynamic runtime
   proofs before setting `compiledIntegration` or `supportedNative`.
7. Regenerate and validate the lexicon, semantic registry, documentation index,
   and official inventory; report lexical, parser, runtime, and compiled support
   separately.

Every enriched entry also records two independent observability contracts. The
`namedActionPerformed` field explicitly says whether `whenever you <ability>`
has a rules-defined nominal signal; `never` is a meaningful answer and must not
be replaced by the ability label. Counter interactions remain name-open: an
entry may document preservation, creation, removal, or inspection of counters
through `intrinsicHooks`, but arbitrary counter names stay data. Only intrinsic
rules classes such as +1/+1, keyword, loyalty, and lore counters may select a
typed behavior; the map never grows a closed allowlist of proper counter names.

The queue favors reusable grammar and runtime boundaries encountered by the
priority-card migration, then closes remaining official actions, keyword
actions, keyword abilities, ability words, and markers. Documentary entries
stay useful while waiting, but they never count as parser or engine coverage.

The c22 Reveal slice applies this rule to CR 701.20 without borrowing the
in-flight Atraxa implementation. Reveal and its private `look at` variant are
mapped to existing `revealCards`, `revealHand`, and `lookAtTopCards` primitives,
with visibility and zone behavior retained as intrinsic hooks. Parser support
is intentionally `partial`: reveal-until loops, costs, arbitrary-zone sets, and
all viewer/duration variants still need one closed typed adapter before
`compiledIntegration` can be claimed.

The c23 Transform slice applies it to CR 701.27. A local token machine now
recognizes `ACTION(transform) SOURCE_REFERENCE` inside activated effect
sequences and lowers it to the existing atomic `transformPermanent` primitive.
The action publishes the intrinsic `permanentTransformed` event, not a nominal
`abilityPerformed(transform)` signal. Transformation preserves object identity
and therefore arbitrary named counters, while assigning no semantics based on
their names. Parser and runtime support remain deliberately partial until
targeted forms, double-faced/back-face eligibility, convert equivalence, and
the CR 701.27g stack/delayed-trigger guard are implemented and proved.

