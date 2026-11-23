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
    /// `a|N QUERY card(s) from ZONE`: the acting player chooses this many
    /// matching cards on resolution; `None` names every matching card.
    pub quantity: Option<NumericExpression>,
}

/// `a|an|N ... card(s) from your|target player's ZONE`: a quantity, a card
/// query and `from` before the owned zone, within the current sentence.
fn chosen_zone_cards_start(cursor: &OracleTokenCursor<'_>) -> bool {
    let head = cursor.peek().unwrap_or_default();
    let quantified = head.eq_ignore_ascii_case("a")
        || head.eq_ignore_ascii_case("an")
        || head.parse::<i64>().is_ok()
        || super::super::parse_number_word(head).is_some();
    if !quantified {
        return false;
    }
    let sentence = cursor.token_slice(cursor.position(), usize::MAX);
    let sentence = &sentence[..sentence
        .iter()
        .position(|token| token.raw == "." || token.raw == ",")
        .unwrap_or(sentence.len())];
    sentence.windows(2).any(|pair| {
        (pair[0].raw.eq_ignore_ascii_case("card") || pair[0].raw.eq_ignore_ascii_case("cards"))
            && pair[1].raw.eq_ignore_ascii_case("from")
    })
}

pub(in crate::oracle::canonical) fn try_recognize_card_zone_collection(
    cursor: &mut OracleTokenCursor<'_>,
) -> CollectionOperandAttempt<CardZoneCollectionNode> {
    let mut trial = cursor.clone();
    let explicit_cards = trial.peek().is_some_and(|v| v.eq_ignore_ascii_case("all"));
    let direct_owner = trial
        .peek()
        .is_some_and(|v| v.eq_ignore_ascii_case("your") || v.eq_ignore_ascii_case("target"));
    let chosen_cards = !explicit_cards && !direct_owner && chosen_zone_cards_start(&trial);
    if !explicit_cards && !direct_owner && !chosen_cards {
        return CollectionOperandAttempt::NotClaimed;
    }
    let mut committed = false;
    let result = (|| -> MachineResult<CardZoneCollectionNode> {
        trial.enter("collection.zone.predicate");
        let mut quantity = None;
        let predicate = if chosen_cards {
            trial.enter("collection.zone.quantity");
            let count = if trial
                .peek()
                .is_some_and(|v| v.eq_ignore_ascii_case("a") || v.eq_ignore_ascii_case("an"))
            {
                let article = trial.peek().unwrap_or("a").to_owned();
                trial.consume(&article)?;
                NumericExpression::from(1)
            } else {
                recognize_numeric_expression(&mut trial, None)?
            };
            let query = recognize_object_query(&mut trial)?;
            trial.require(
                query.controller.is_none()
                    && query.predicate.has_domain(ObjectKind::Card)
                    && query.predicate.supports_domain(ObjectKind::Card),
                "a card-domain predicate before a separate zone owner",
            )?;
            trial.consume("from")?;
            quantity = Some(count);
            Some(query.predicate)
        } else if explicit_cards {
            trial.consume("all")?;
            let query = recognize_object_query(&mut trial)?;
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
            quantity,
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
