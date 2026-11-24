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
    /// `a|N|up to N|any number of QUERY card(s) from|in ZONE`: the acting
    /// player chooses this many matching cards on resolution (the shared
    /// selection quantity); `None` names every matching card.
    pub quantity: Option<CardSelectionQuantityNode>,
}

/// `a|an|N|up to N|any number of ... card(s) from|in your|target player's
/// ZONE`: a quantity, a card query and `from`/`in` before the owned zone,
/// within the current sentence.
fn chosen_zone_cards_start(cursor: &OracleTokenCursor<'_>) -> bool {
    let head = cursor.peek().unwrap_or_default();
    let quantified = head.eq_ignore_ascii_case("a")
        || head.eq_ignore_ascii_case("an")
        || head.parse::<i64>().is_ok()
        || super::super::parse_number_word(head).is_some()
        || (head.eq_ignore_ascii_case("up")
            && cursor
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("to")))
        || (head.eq_ignore_ascii_case("any")
            && cursor
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("number")));
    if !quantified {
        return false;
    }
    let sentence = cursor.token_slice(cursor.position(), usize::MAX);
    let sentence = &sentence[..sentence
        .iter()
        .position(|token| token.raw == "." || token.raw == ",")
        .unwrap_or(sentence.len())];
    sentence.windows(3).any(|triple| {
        (triple[0].raw.eq_ignore_ascii_case("card") || triple[0].raw.eq_ignore_ascii_case("cards"))
            && (triple[1].raw.eq_ignore_ascii_case("from")
                || triple[1].raw.eq_ignore_ascii_case("in"))
            && (triple[2].raw.eq_ignore_ascii_case("your")
                || triple[2].raw.eq_ignore_ascii_case("target"))
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
            // `a|N|up to N|any number of QUERY card(s)`: the shared object
            // noun phrase reads the count and the card query; this parent
            // type-checks both.
            let phrase = recognize_object_noun_phrase(
                &mut trial,
                ObjectNounPhraseExpectation::in_domains(&[ObjectKind::Card]),
            )?;
            let count = match phrase.determiner {
                Some(ObjectDeterminer::Chosen {
                    quantity,
                    another: false,
                }) if !phrase.exclude_source => match quantity {
                    ChosenObjectQuantity::Exactly(count) => {
                        CardSelectionQuantityNode::Exactly(count)
                    }
                    ChosenObjectQuantity::UpTo(count) => CardSelectionQuantityNode::UpTo(count),
                    ChosenObjectQuantity::AnyNumber => CardSelectionQuantityNode::AnyNumber,
                },
                _ => {
                    return Err(trial.fail("`a`, a count, `up to` or `any number` of chosen cards"));
                }
            };
            trial.require(
                phrase.query.relations().is_empty() && phrase.domain == Some(ObjectKind::Card),
                "a card-domain predicate before a separate zone owner",
            )?;
            let relation = trial
                .peek()
                .filter(|r| r.eq_ignore_ascii_case("in") || r.eq_ignore_ascii_case("from"))
                .map(str::to_owned)
                .ok_or_else(|| trial.fail("in or from before an owned card zone"))?;
            trial.consume(&relation)?;
            quantity = Some(count);
            Some(phrase.query.predicate().clone())
        } else if explicit_cards {
            let phrase = recognize_object_noun_phrase(
                &mut trial,
                ObjectNounPhraseExpectation::in_domains(&[ObjectKind::Card]),
            )?;
            trial.require(
                matches!(
                    phrase.determiner,
                    Some(ObjectDeterminer::Universal { each: false })
                ) && !phrase.exclude_source,
                "`all` cards of a zone without a source exclusion",
            )?;
            trial.require(
                phrase.query.relations().is_empty() && phrase.domain == Some(ObjectKind::Card),
                "a card-domain predicate before a separate zone owner",
            )?;
            let query = phrase.query;
            let relation = trial
                .peek()
                .filter(|r| r.eq_ignore_ascii_case("in") || r.eq_ignore_ascii_case("from"))
                .map(str::to_owned)
                .ok_or_else(|| trial.fail("in or from before an explicitly owned card zone"))?;
            trial.consume(&relation)?;
            Some(query.predicate().clone())
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
