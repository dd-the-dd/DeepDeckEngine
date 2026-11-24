use super::super::{MachineResult, OracleTokenCursor};
use super::card_collection_operand::CollectionOperandAttempt;
use super::collection::{
    LibraryPosition, recognize_collection_order, recognize_library_position_kind,
};
use super::zone_list::{CardZoneName, recognize_card_zone_atom};
use super::{CollectionOrder, LocalPlayerReference, RecognizedEffectFragment};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardMoveOwnerNode {
    Player(LocalPlayerReference),
    EachCardOwner,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardMoveDestinationNode {
    pub zone: CardZoneName,
    pub owner: CardMoveOwnerNode,
    pub position: Option<LibraryPosition>,
    pub order: Option<CollectionOrder>,
    pub tapped: bool,
    /// `with N COUNTER counters on it|them`: the cards enter the battlefield
    /// with these counters (CR 122.6), not receive them afterwards.
    pub entering_counters: Option<EnteringCountersNode>,
    /// `face down`: the cards are exiled face down (CR 406.3) or enter the
    /// battlefield face down (CR 708.2). Never together with `transformed`.
    pub face_down: bool,
    /// `transformed`: the cards enter the battlefield with their back face
    /// up (CR 712.14a). Battlefield only.
    pub transformed: bool,
    /// `shuffle X into a library`: each library that received a card is
    /// shuffled after the move (CR 701.24b). Library only.
    pub shuffle: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EnteringCountersNode {
    pub counter: crate::oracle::counter_name::CounterName,
    pub count: super::NumericExpression,
}

/// `with N COUNTER counter(s) on it|them` after a battlefield destination.
fn recognize_entering_counters(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EnteringCountersNode> {
    cursor.enter("collection.destination.entering-counters");
    cursor.consume("with")?;
    let count = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
    {
        let article = cursor.peek().unwrap_or("a").to_owned();
        cursor.consume(&article)?;
        // `with an additional COUNTER counter on it`: the same placement as
        // the object enters (CR 122.6); it has no counters it would
        // otherwise enter with from this move.
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("additional"))
        {
            cursor.consume("additional")?;
        }
        super::NumericExpression::from(1)
    } else {
        super::recognize_numeric_expression(cursor, None)?
    };
    cursor.enter("collection.destination.entering-counter-kind");
    let counter = super::recognize_counter_name(cursor)?;
    cursor.enter("collection.destination.entering-counter-noun");
    let singular = count == super::NumericExpression::from(1);
    cursor.consume(if singular { "counter" } else { "counters" })?;
    cursor.enter("collection.destination.entering-counter-holder");
    cursor.consume("on")?;
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("them"))
    {
        cursor.consume("them")?;
    } else {
        cursor.consume("it")?;
    }
    Ok(EnteringCountersNode { counter, count })
}

/// The entry modifiers one destination admits, read in any written order
/// (`tapped and transformed under your control with a +1/+1 counter on
/// it`). Each modifier is read once; a repeated or inapplicable one fails.
pub(in crate::oracle::canonical) struct EntryModifiers {
    pub tapped: bool,
    pub face_down: bool,
    pub transformed: bool,
    /// The controller of the entering permanents; the destination owner
    /// until an explicit `under X's control` replaces it (CR 110.2a).
    pub controller: CardMoveOwnerNode,
    pub entering_counters: Option<EnteringCountersNode>,
    explicit_controller: bool,
}

impl EntryModifiers {
    pub(in crate::oracle::canonical) fn new(controller: CardMoveOwnerNode) -> Self {
        Self {
            tapped: false,
            face_down: false,
            transformed: false,
            controller,
            entering_counters: None,
            explicit_controller: false,
        }
    }
}

/// A modifier word that may follow `and` inside the modifier list. Any other
/// `and` belongs to the parent's connector (`tapped and attacking` is not an
/// entry modifier this reader owns).
fn entry_modifier_starts(cursor: &OracleTokenCursor<'_>, offset: usize, battlefield: bool) -> bool {
    match cursor
        .peek_ahead(offset)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("tapped" | "transformed") => battlefield,
        Some("face") => cursor
            .peek_ahead(offset + 1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("down") || raw.eq_ignore_ascii_case("up")),
        _ => false,
    }
}

/// `[tapped] [transformed] [face down|face up] [under X's control] [with N
/// KIND counters on it|them]` after a battlefield destination, and `[face
/// down]` after exile. The list is a child of the destination for any moved
/// operand; the consuming lowerer proves each modifier for its object.
pub(in crate::oracle::canonical) fn recognize_entry_modifiers(
    cursor: &mut OracleTokenCursor<'_>,
    zone: CardZoneName,
    modifiers: &mut EntryModifiers,
) -> MachineResult<()> {
    let battlefield = zone == CardZoneName::Battlefield;
    loop {
        // `tapped and transformed`: the conjunction joins two modifiers.
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
            && entry_modifier_starts(cursor, 1, battlefield)
        {
            cursor.consume("and")?;
        }
        match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
            Some("tapped") if battlefield => {
                cursor.enter("collection.destination.entry.tapped");
                cursor.require(!modifiers.tapped, "one `tapped` entry modifier")?;
                cursor.consume("tapped")?;
                modifiers.tapped = true;
            }
            Some("transformed") if battlefield => {
                cursor.enter("collection.destination.entry.transformed");
                cursor.require(
                    !modifiers.transformed && !modifiers.face_down,
                    "one `transformed` entry modifier, never with a face-down entry",
                )?;
                cursor.consume("transformed")?;
                modifiers.transformed = true;
            }
            Some("face")
                if cursor
                    .peek_ahead(1)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("down")) =>
            {
                cursor.enter("collection.destination.entry.face-down");
                cursor.require(
                    !modifiers.face_down && !modifiers.transformed,
                    "one `face down` entry modifier, never with a transformed entry",
                )?;
                cursor.consume_phrase(&["face", "down"])?;
                modifiers.face_down = true;
            }
            // A card is put onto the battlefield face up unless an
            // instruction says otherwise (CR 708.1): `face up` restates it.
            Some("face")
                if battlefield
                    && cursor
                        .peek_ahead(1)
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("up")) =>
            {
                cursor.enter("collection.destination.entry.face-up");
                cursor.require(!modifiers.face_down, "one face orientation")?;
                cursor.consume_phrase(&["face", "up"])?;
            }
            Some("under") if battlefield => {
                cursor.enter("collection.destination.controller");
                cursor.require(
                    !modifiers.explicit_controller,
                    "one controller of the entering permanents",
                )?;
                cursor.consume("under")?;
                modifiers.controller = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                    Some("your") => {
                        cursor.consume("your")?;
                        CardMoveOwnerNode::Player(LocalPlayerReference::Controller)
                    }
                    Some(pronoun @ ("its" | "their")) => {
                        let pronoun = pronoun.to_owned();
                        cursor.consume(&pronoun)?;
                        let possessive = cursor
                            .peek()
                            .filter(|raw| {
                                ["owner's", "owner\u{2019}s", "owners'", "owners\u{2019}"]
                                    .iter()
                                    .any(|form| raw.eq_ignore_ascii_case(form))
                            })
                            .map(str::to_owned)
                            .ok_or_else(|| cursor.fail("owner's or owners'"))?;
                        cursor.consume(&possessive)?;
                        CardMoveOwnerNode::EachCardOwner
                    }
                    _ => {
                        return Err(cursor.fail("`your`, `its owner's` or `their owners'` control"));
                    }
                };
                cursor.consume("control")?;
                modifiers.explicit_controller = true;
            }
            Some("with") if battlefield => {
                cursor.require(
                    modifiers.entering_counters.is_none(),
                    "one entering-counter placement",
                )?;
                modifiers.entering_counters = Some(recognize_entering_counters(cursor)?);
            }
            _ => return Ok(()),
        }
    }
}

pub(in crate::oracle::canonical) type CardMoveDestinationAttempt =
    CollectionOperandAttempt<CardMoveDestinationNode>;

/// Reads a destination phrase, never the move action or its object binding.
pub(in crate::oracle::canonical) fn try_recognize_card_move_destination(
    cursor: &mut OracleTokenCursor<'_>,
) -> CardMoveDestinationAttempt {
    let mut trial = cursor.clone();
    let relation = match trial.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("into") => "into",
        Some("to") => "to",
        Some("on") => "on",
        Some("onto") => "onto",
        _ => return CollectionOperandAttempt::NotClaimed,
    };
    let result = (|| -> MachineResult<CardMoveDestinationNode> {
        trial.enter("collection.destination.relation");
        trial.consume(relation)?;
        let surface = relation == "on" || relation == "onto";
        if trial
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
        {
            trial.consume("the")?;
        }
        let position = if surface
            && trial.peek().is_some_and(|raw| {
                raw.eq_ignore_ascii_case("top") || raw.eq_ignore_ascii_case("bottom")
            }) {
            let position = recognize_library_position_kind(&mut trial)?;
            trial.consume("of")?;
            Some(position)
        } else {
            None
        };
        trial.enter("collection.destination.owner");
        // Battlefield control comes from the instruction's actor (CR 110.2a).
        // Other zones require an explicit possessive; bare exile is not guessed.
        let battlefield = trial
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("battlefield"));
        let mut owned_zone_only = false;
        let owner = if !battlefield {
            if trial
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("its"))
            {
                trial.consume("its")?;
                let owner = trial
                    .peek()
                    .filter(|raw| {
                        raw.eq_ignore_ascii_case("owner's")
                            || raw.eq_ignore_ascii_case("owner\u{2019}s")
                    })
                    .map(str::to_owned)
                    .ok_or_else(|| trial.fail("owner's"))?;
                trial.consume(&owner)?;
                CardMoveOwnerNode::EachCardOwner
            } else if trial
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("their"))
            {
                // `their owner's hand` / `their owners' hands`: the plural
                // object pronoun keeps the per-object owner relation.
                trial.consume("their")?;
                let owner = trial
                    .peek()
                    .filter(|raw| {
                        ["owner's", "owner\u{2019}s", "owners'", "owners\u{2019}"]
                            .iter()
                            .any(|form| raw.eq_ignore_ascii_case(form))
                    })
                    .map(str::to_owned);
                match owner {
                    Some(owner) => trial.consume(&owner)?,
                    // `to their hand`, `on top of their library`: a player's
                    // possessive naming the cards' owner. A hand, library or
                    // graveyard is always its owner's (CR 400.3), checked
                    // with the zone below.
                    None => owned_zone_only = true,
                }
                CardMoveOwnerNode::EachCardOwner
            } else {
                trial.consume("your")?;
                CardMoveOwnerNode::Player(LocalPlayerReference::Controller)
            }
        } else {
            CardMoveOwnerNode::Player(LocalPlayerReference::Controller)
        };
        trial.enter("collection.destination.zone");
        let zone = recognize_card_zone_atom(&mut trial)?;
        // `return ... to the battlefield` names the same battlefield entry as
        // `put ... onto the battlefield`; `into the battlefield` is not Oracle.
        trial.require(if surface { zone == CardZoneName::Battlefield || (zone == CardZoneName::Library && position.is_some()) } else { (zone != CardZoneName::Battlefield || relation == "to") && position.is_none() }, "on/onto/to the battlefield or a positioned library; into/to an explicitly owned card zone")?;
        trial.require(
            position.is_none() || zone == CardZoneName::Library,
            "a top/bottom position only for a library",
        )?;
        trial.require(
            !owned_zone_only
                || matches!(
                    zone,
                    CardZoneName::Hand | CardZoneName::Library | CardZoneName::Graveyard
                ),
            "a hand, library or graveyard after a bare player possessive",
        )?;
        let mut modifiers = EntryModifiers::new(owner);
        if matches!(zone, CardZoneName::Battlefield | CardZoneName::Exile) {
            recognize_entry_modifiers(&mut trial, zone, &mut modifiers)?;
        }
        let EntryModifiers {
            tapped,
            face_down,
            transformed,
            controller: owner,
            entering_counters,
            ..
        } = modifiers;
        let order = if trial
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("in"))
        {
            trial.enter("collection.destination.order");
            trial.require(
                zone == CardZoneName::Library,
                "library ordering, not ordering a hand, graveyard, exile or battlefield",
            )?;
            Some(recognize_collection_order(&mut trial)?)
        } else {
            None
        };
        Ok(CardMoveDestinationNode {
            zone,
            owner,
            position,
            order,
            tapped,
            entering_counters,
            face_down,
            transformed,
            shuffle: false,
        })
    })();
    match result {
        Ok(node) => {
            trial.enter("collection.destination.complete");
            let end_position = trial.position();
            *cursor = trial;
            CollectionOperandAttempt::Recognized(RecognizedEffectFragment { node, end_position })
        }
        Err(f) => CollectionOperandAttempt::FailedCommitted(f),
    }
}

/// A destination written before the moved object (`return to the
/// battlefield all creature cards ...`, `put onto the battlefield ...`).
/// An object operand never begins with `to`/`onto`, so this claims only a
/// preposed destination; once claimed, a malformed destination fails.
pub(in crate::oracle::canonical) fn try_recognize_preposed_card_move_destination(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<CardMoveDestinationNode>> {
    if !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("to") || raw.eq_ignore_ascii_case("onto"))
    {
        return Ok(None);
    }
    cursor.enter("collection.destination.preposed");
    match try_recognize_card_move_destination(cursor) {
        CollectionOperandAttempt::Recognized(fragment) => Ok(Some(fragment.node)),
        CollectionOperandAttempt::NotClaimed => Ok(None),
        CollectionOperandAttempt::RejectedCandidate(failure)
        | CollectionOperandAttempt::FailedCommitted(failure) => Err(failure),
    }
}

#[cfg(test)]
#[path = "card_move_destination_tests.rs"]
mod tests;
