# Target identity after a zone-change round trip

## Current limitation

The engine revalidates a targeted card against its original declaration selector
when the spell or ability resolves. This correctly rejects a graveyard target
that is now in a hand, library, exile, or another unsupported zone.

`TargetRef::Card` currently stores only the card instance ID. Zone moves preserve
that ID, so a card that leaves a zone and returns to the same eligible zone before
resolution can still compare equal to the original target. Under Magic's zone
change rules, it is a new object and the old target must remain illegal.

## Missing primitive

Nonbattlefield object references need a zone-change generation (or another
stable object-version identifier) captured with the target. Resolution must
compare both the instance ID and that generation. This belongs in the shared
target/reference layer, not in any card-specific effect.

## Scope of the current proof

The Noxious Revival runtime fixture proves the common departure case: another
spell moves the targeted card from a graveyard to its owner's hand, and Noxious
Revival then fizzles rather than finding the card in its new zone. It does not
claim coverage for a leave-and-return round trip.
