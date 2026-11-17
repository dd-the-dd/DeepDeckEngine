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
                    .map(str::to_owned)
                    .ok_or_else(|| trial.fail("owner's or owners'"))?;
                trial.consume(&owner)?;
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
        let tapped = if zone == CardZoneName::Battlefield
            && trial
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("tapped"))
        {
            trial.consume("tapped")?;
            true
        } else {
            false
        };
        // `under its owner's control` / `under their owners' control` / `under
        // your control`: the controller of the entering permanents (CR
        // 110.2a). Without it, the instruction's actor controls them.
        let owner = if zone == CardZoneName::Battlefield
            && trial
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("under"))
        {
            trial.enter("collection.destination.controller");
            trial.consume("under")?;
            let owner = match trial.peek().map(str::to_ascii_lowercase).as_deref() {
                Some("your") => {
                    trial.consume("your")?;
                    CardMoveOwnerNode::Player(LocalPlayerReference::Controller)
                }
                Some(pronoun @ ("its" | "their")) => {
                    let pronoun = pronoun.to_owned();
                    trial.consume(&pronoun)?;
                    let possessive = trial
                        .peek()
                        .filter(|raw| {
                            ["owner's", "owner\u{2019}s", "owners'", "owners\u{2019}"]
                                .iter()
                                .any(|form| raw.eq_ignore_ascii_case(form))
                        })
                        .map(str::to_owned)
                        .ok_or_else(|| trial.fail("owner's or owners'"))?;
                    trial.consume(&possessive)?;
                    CardMoveOwnerNode::EachCardOwner
                }
                _ => {
                    return Err(trial.fail("`your`, `its owner's` or `their owners'` control"));
                }
            };
            trial.consume("control")?;
            owner
        } else {
            owner
        };
        let entering_counters = if zone == CardZoneName::Battlefield
            && trial
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("with"))
        {
            Some(recognize_entering_counters(&mut trial)?)
        } else {
            None
        };
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

#[cfg(test)]
#[path = "card_move_destination_tests.rs"]
mod tests;
