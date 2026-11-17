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
            } else {
                trial.consume("your")?;
                CardMoveOwnerNode::Player(LocalPlayerReference::Controller)
            }
        } else {
            CardMoveOwnerNode::Player(LocalPlayerReference::Controller)
        };
        trial.enter("collection.destination.zone");
        let zone = recognize_card_zone_atom(&mut trial)?;
        trial.require(if surface { zone == CardZoneName::Battlefield || (zone == CardZoneName::Library && position.is_some()) } else { zone != CardZoneName::Battlefield && position.is_none() }, "on/onto the battlefield or a positioned library; into/to an explicitly owned card zone")?;
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
