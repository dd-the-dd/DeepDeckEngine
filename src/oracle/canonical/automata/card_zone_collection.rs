//! Pure zone collection operands, independent of their consuming action.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardZoneOwnerNode {
    Player(LocalPlayerReference),
    TargetPlayer(TargetListNode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardZoneCollectionNode {
    pub zone: CardZoneName,
    pub owner: CardZoneOwnerNode,
    pub predicate: Option<ObjectPredicate>,
}

pub(in crate::oracle::canonical) fn try_recognize_card_zone_collection(
    cursor: &mut OracleTokenCursor<'_>,
) -> CollectionOperandAttempt<CardZoneCollectionNode> {
    let mut trial = cursor.clone();
    let explicit_cards = trial.peek().is_some_and(|v| v.eq_ignore_ascii_case("all"));
    let direct_owner = trial
        .peek()
        .is_some_and(|v| v.eq_ignore_ascii_case("your") || v.eq_ignore_ascii_case("target"));
    if !explicit_cards && !direct_owner {
        return CollectionOperandAttempt::NotClaimed;
    }
    let mut committed = false;
    let result = (|| -> MachineResult<CardZoneCollectionNode> {
        trial.enter("collection.zone.predicate");
        let predicate = if explicit_cards {
            trial.consume("all")?;
            let query = recognize_object_query_before_separator(&mut trial)?;
            trial.require(
                query.controller.is_none()
                    && query.predicate.has_domain(ObjectKind::Card)
                    && query.predicate.supports_domain(ObjectKind::Card),
                "a card-domain predicate before a separate zone owner",
            )?;
            let relation = trial
                .peek()
                .filter(|r| r.eq_ignore_ascii_case("in") || r.eq_ignore_ascii_case("from"))
                .map(str::to_owned)
                .ok_or_else(|| trial.fail("in or from before an explicitly owned card zone"))?;
            trial.consume(&relation)?;
            Some(query.predicate)
        } else {
            None
        };
        trial.enter("collection.zone.owner");
        let owner = if trial.peek().is_some_and(|v| v.eq_ignore_ascii_case("your")) {
            trial.consume("your")?;
            CardZoneOwnerNode::Player(LocalPlayerReference::Controller)
        } else {
            CardZoneOwnerNode::TargetPlayer(recognize_possessive_target_player(&mut trial)?)
        };
        committed = true;
        trial.enter("collection.zone.name");
        let zone = recognize_card_zone_atom(&mut trial)?;
        Ok(CardZoneCollectionNode {
            zone,
            owner,
            predicate,
        })
    })();
    match result {
        Ok(node) => {
            trial.enter("collection.zone.complete");
            let end_position = trial.position();
            *cursor = trial;
            CollectionOperandAttempt::Recognized(RecognizedEffectFragment { node, end_position })
        }
        Err(f) if committed => CollectionOperandAttempt::FailedCommitted(f),
        Err(f) => CollectionOperandAttempt::RejectedCandidate(f),
    }
}

#[cfg(test)]
#[path = "card_zone_collection_tests.rs"]
mod tests;
