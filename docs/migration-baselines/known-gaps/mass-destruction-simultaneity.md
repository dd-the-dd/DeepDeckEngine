# Known gap: simultaneous mass destruction

Status: bounded collection planner accepted on 2026-11-12. The two original
diagnostics now pass (14 assertions, 0.044 s), together with 354 normal runtime
fixtures (2,201 assertions, 1.391 s). Exact Oracle/contract/runtime equality was
independently verified for Supreme Verdict and Ruinous Ultimatum. Global parser
recognition and individual runtime certification remain separate metrics.

The original investigation below records the pre-fix behavior. The accepted
planner lives in `src/engine/destruction_batch.rs`: it freezes protections,
replacement outcomes and death/destruction trigger sources before mutations,
then publishes death/destruction triggers after the collection exits. Competing
replacement choices fail before mutation. APNAP, general continuous-effect
dependencies and all left-battlefield/graveyard-entry trigger snapshots remain
outside this proof. The Umbra limitations described below are still open.

The destroyed-versus-died distinction is backed by the official Bane of Progress
ruling archived in `.tmp/bane-of-progress-rulings-2026-11-12.json` (Wizards,
2013-10-17): a permanent destroyed while Rest in Peace replaces its graveyard
destination still counts as destroyed, but it did not die.

## Reusable primitive path

Both cards can use the existing primitive `destroyPermanent` over an
`eachPermanent` collection. Supreme Verdict selects creatures and composes the
separate stack-scoped `cantBeCountered` static modifier. Ruinous Ultimatum
selects nonland permanents controlled by `opponentsOf(controllerOf(self))`.
No card-named or combined mass-destruction operation is needed.

The parser/runtime contract should therefore remain a composition of:

- `destroyPermanent(eachPermanent(...))`;
- generic `ObjectPredicate` filters and player references;
- `cantBeCountered(self)` for Supreme Verdict only.

## Original runtime defect (before the planner)

`destroyPermanent` resolves the collection once, but then destroys its members
sequentially (`src/engine/mod.rs`, execution branch near line 41425). Every call
to `destroy_permanent_from_stack` reevaluates protections and replacement effects
against the battlefield left by the previous call (near lines 56247-56449 and
56486-56544).

This makes the result depend on battlefield iteration order:

1. a lord removed first stops granting indestructible before the next member is
   processed;
2. a graveyard-replacement source removed first no longer replaces its own move
   or those of the remaining members, because `put_card_into_graveyard` scans the
   live battlefield after removal (near lines 10676-10750);
3. death-trigger sources and `opponentDestroyedControlledPermanent` sources can
   disappear before later members publish their events;
4. the runtime needs to distinguish effects that prevent or replace destruction
   from effects that replace only the resulting death or zone change.
   Regeneration, shield counters, Umbra Armor and indestructible mean the
   permanent was not destroyed. Rest-in-Peace-, Kalitas-, Pillar- and
   Unearth-style replacements still leave the permanent destroyed, but it did
   not die.

The state-based-action death snapshot in
`src/engine/death_trigger_batch.rs` solves only simultaneous SBA deaths. The
collection instruction does not currently reuse it.

## Reproductions

The original red diagnostics have been promoted into the normal regression gate:

- `crates/deepdeck-runtime-fixture-runner/fixtures/mass-destruction-indestructible-lord.json`
- `crates/deepdeck-runtime-fixture-runner/fixtures/mass-destruction-graveyard-replacement.json`

Observed against the pre-fix runtime runner on 2026-11-12:

### Indestructible source removed first

Expected: the lord is destroyed, the creature it protected survives, and exactly
one `permanentDestroyed` event is recorded.

Actual: both creatures are put into the graveyard and two
`permanentDestroyed` events are recorded. The diagnostic took 0.544 seconds.

### Graveyard replacement source removed first

Expected: the replacement source and the later creature are both exiled, the
creature does not enter the graveyard, two `graveyardDestinationReplaced` and
two `permanentDestroyed` events are recorded, and no `permanentDied` event is
recorded.

Actual: neither permanent is exiled, the creature enters the graveyard, and no
replacement event is recorded. The diagnostic took 0.085 seconds. The existing
single-object Rest-in-Peace replacement fixture remains green while the source
stays on the battlefield, which isolates the defect to batch ordering.

## Minimal generic design

Introduce a narrow destruction-batch planner rather than cloning the engine or
adding a mass-destruction operation.

1. Resolve the candidate IDs and exclusions exactly once, preserving their
   stable order.
2. Before moving any candidate, capture only the relevant immutable inputs:
   candidate LKI, effective indestructible state, applicable regeneration/shield/
   Umbra Armor state, graveyard and death replacement sources, death-trigger
   sources, and destruction-trigger sources.
3. Produce a typed planned outcome per candidate, for example `Prevented`,
   `Regenerated`, `ReplacedWithExile`, or `DestroyedTo(destination)`.
4. Apply those outcomes without reevaluating battlefield-dependent inputs after
   the first removal. Resolve mutable resources such as shields and Armor by
   stable instance ID, never by stored vector index.
5. Publish events and triggers from the typed outcomes and captured LKI. Only an
   actual destruction contributes to `destroyed_ids` or
   `permanentDestroyed`. Indestructible and replacements that prevent the
   destruction itself must not count as destroyed. Replacements of the ensuing
   death or zone change still count as destroyed while publishing no death
   event.

The existing `DeathTriggerSourceSnapshot` should be reused or generalized for
the death-trigger portion. A parallel narrow snapshot is required for
`opponentDestroyedControlledPermanent`; rescanning the live battlefield after
each removal is not sufficient.

## Required acceptance matrix

Promotion of the two cards requires all of the following generic proofs:

- the two red diagnostics above become green without changing their expected
  results;
- destruction order does not change the result;
- a surviving death-trigger source sees every applicable death in the batch;
- a death-trigger source destroyed in the same batch also sees all applicable
  deaths, without double-triggering itself;
- indestructible, regeneration, shield counters, Umbra Armor, and
  `cannotRegenerate` retain their current single-object behavior;
- indestructible and replacements of the destruction itself emit neither death
  nor destruction events and do not enter `destroyed_ids`;
- replacements of the ensuing death or zone change still emit
  `permanentDestroyed` and enter `destroyed_ids`, but emit no death event;
- actual deaths publish one death/destruction event each with the correct LKI;
- Supreme Verdict still composes the independent `cantBeCountered` modifier;
- Ruinous Ultimatum still excludes lands and affects only opponents' permanents.

## Explicit remaining boundaries

This bounded correction must not be advertised as full simultaneous-event
support. Multiple competing replacement effects still require a player-choice
model and ordering policy. Umbra Armor that is itself selected in the same batch
needs a deduplicated plan. APNAP ordering of simultaneous triggers and arbitrary
continuous-effect dependencies may require later shared infrastructure.

The bounded implementation defers death-trigger, destruction-trigger, and
death-replacement `onReplaced` ability construction until every planned move has
been applied, so their targets are chosen from the final battlefield. Generic
`permanentLeftBattlefield` and `cardsEnteredGraveyard` trigger sources are not
yet snapshotted by this module; those event families remain an explicit later
simultaneity boundary.

Umbra Armor remains additionally bounded: when the Armor itself is selected in
the destruction collection the planner rejects the ambiguous batch. When the
Armor is outside the collection, the existing Armor path destroys it through
`move_permanent_to_graveyard` without separately checking whether that Aura is
indestructible. That pre-existing limitation is not promoted as covered here.

Implementation effort is therefore concentrated in one narrow planner module,
one delegation from the collection form of `destroyPermanent`, and dedicated
runtime/unit tests. The parser and primitive IR need no composite operation.
