//! Pure zone collection operands, independent of their consuming action.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardZoneOwnerNode {
    Player(LocalPlayerReference),
    TargetPlayer(TargetListNode),
    /// The same zone of every player of a population, read on resolution:
    /// `all graveyards`, `a graveyard` (any one of them), `each opponent's
    /// graveyard`, `an opponent's graveyard`. Public zones only.
    Players(PlayerSetReferenceNode),
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
    /// `from your hand or graveyard`, `from your graveyard and/or library`:
    /// the other zones of the same owner, read as one union with `zone`.
    pub additional_zones: Vec<CardZoneName>,
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
    quantified && card_noun_then_owned_zone(cursor)
}

/// `each QUERY card in|from OWNER ZONE`: the distributive universal of a
/// named zone's matching cards (the same collection as `all ... cards`).
fn each_zone_card_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("each"))
        && card_noun_then_owned_zone(cursor)
}

/// A card noun, then `from|in OWNER` before the sentence ends.
fn card_noun_then_owned_zone(cursor: &OracleTokenCursor<'_>) -> bool {
    // The card noun, then `from|in OWNER` before the sentence ends. A
    // comma of a coordinated type list (`an artifact, creature, or land
    // card`) and a postposed qualifier (`with mana value 3 or less`) stay
    // inside the noun phrase; the owner word must name an owned zone.
    let sentence = cursor.token_slice(cursor.position(), usize::MAX);
    let sentence = &sentence[..sentence
        .iter()
        .position(|token| token.raw == ".")
        .unwrap_or(sentence.len())];
    let Some(noun) = sentence.iter().position(|token| {
        token.raw.eq_ignore_ascii_case("card") || token.raw.eq_ignore_ascii_case("cards")
    }) else {
        return false;
    };
    sentence[noun + 1..].windows(2).any(|pair| {
        (pair[0].raw.eq_ignore_ascii_case("from") || pair[0].raw.eq_ignore_ascii_case("in"))
            && zone_owner_head(&pair[1].raw)
    })
}

/// A word that begins an owned-zone phrase after `from|in`.
fn zone_owner_head(raw: &str) -> bool {
    ["your", "target", "a", "an", "all", "each"]
        .iter()
        .any(|word| raw.eq_ignore_ascii_case(word))
}

/// `a graveyard`, `an opponent's graveyard`, `all graveyards`, `each
/// opponent's graveyard`, `all [your] opponents' graveyards`: the population
/// whose zones the collection reads. `None` without consuming when the
/// owner is not a population. The zone word itself is read by the caller.
fn recognize_zone_owner_population(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<PlayerSetReferenceNode>> {
    let opponents = |raw: &str| {
        [
            "opponent's",
            "opponent\u{2019}s",
            "opponents'",
            "opponents\u{2019}",
        ]
        .iter()
        .any(|form| raw.eq_ignore_ascii_case(form))
    };
    let head = cursor.peek().map(str::to_ascii_lowercase);
    let population = match head.as_deref() {
        Some("a")
            if cursor
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("graveyard")) =>
        {
            cursor.consume("a")?;
            PlayerSetReferenceNode::EachPlayer
        }
        Some("an" | "each") if cursor.peek_ahead(1).is_some_and(opponents) => {
            let head = cursor.peek().unwrap_or("an").to_owned();
            cursor.consume(&head)?;
            let role = cursor.peek().unwrap_or("opponent's").to_owned();
            cursor.consume(&role)?;
            PlayerSetReferenceNode::OpponentsOfAbilityController
        }
        Some("all") => {
            cursor.consume("all")?;
            if cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("your"))
                && cursor.peek_ahead(1).is_some_and(opponents)
            {
                cursor.consume("your")?;
            }
            if cursor.peek().is_some_and(opponents) {
                let role = cursor.peek().unwrap_or("opponents'").to_owned();
                cursor.consume(&role)?;
                PlayerSetReferenceNode::OpponentsOfAbilityController
            } else {
                PlayerSetReferenceNode::EachPlayer
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(population))
}

pub(in crate::oracle::canonical) fn try_recognize_card_zone_collection(
    cursor: &mut OracleTokenCursor<'_>,
) -> CollectionOperandAttempt<CardZoneCollectionNode> {
    let mut trial = cursor.clone();
    let explicit_cards = trial.peek().is_some_and(|v| v.eq_ignore_ascii_case("all"))
        || each_zone_card_starts(&trial);
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
                matches!(phrase.determiner, Some(ObjectDeterminer::Universal { .. }))
                    && !phrase.exclude_source,
                "`all` or `each` cards of a zone without a source exclusion",
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
        } else if let Some(population) = recognize_zone_owner_population(&mut trial)? {
            CardZoneOwnerNode::Players(population)
        } else {
            CardZoneOwnerNode::TargetPlayer(recognize_possessive_target_player(&mut trial)?)
        };
        committed = true;
        trial.enter("collection.zone.name");
        let zone = recognize_card_zone_atom(&mut trial)?;
        // `hand or graveyard`, `hand and/or graveyard`: a coordinated zone
        // of the same owner. A connector followed by anything else is the
        // parent's (`... from your graveyard and you gain 1 life`).
        let mut additional_zones = Vec::new();
        while trial.peek().is_some_and(|raw| {
            ["or", "and", "and/or"]
                .iter()
                .any(|word| raw.eq_ignore_ascii_case(word))
        }) && trial.peek_ahead(1).is_some_and(|raw| {
            ["hand", "graveyard", "library", "exile"]
                .iter()
                .any(|word| raw.eq_ignore_ascii_case(word))
        }) {
            trial.enter("collection.zone.coordinated");
            let connector = trial.peek().unwrap_or("or").to_owned();
            trial.consume(&connector)?;
            let other = recognize_card_zone_atom(&mut trial)?;
            if other != zone && !additional_zones.contains(&other) {
                additional_zones.push(other);
            }
        }
        // `... in your graveyard that were put there from the battlefield
        // this turn`: the shared relative slot qualifies the zone's cards.
        let predicate = if super::card_zone_relative_clause_starts(&trial) {
            let relative = super::recognize_relative_clause(&mut trial)?;
            Some(ObjectPredicate::Intersection(vec![
                predicate.unwrap_or(ObjectPredicate::ObjectKind(ObjectKind::Card)),
                relative,
            ]))
        } else {
            predicate
        };
        // `a creature card in your hand without blitz`: the shared
        // postnominal list of the cards continues after their zone.
        let predicate = match predicate {
            Some(head) => {
                trial.enter("collection.zone.postnominal");
                let plural = !matches!(
                    quantity,
                    Some(CardSelectionQuantityNode::Exactly(NumericExpression::Atom(
                        NumericAtom::Integer(1)
                    )))
                );
                Some(continue_unrelated_predicate(
                    &mut trial,
                    ObjectQueryContext::ORDINARY,
                    head,
                    plural,
                )?)
            }
            None => None,
        };
        if matches!(owner, CardZoneOwnerNode::Players(_)) {
            // Another player's hidden zone is not an open collection to
            // read or choose from (CR 400.2); only public zones admit a
            // population of owners.
            trial.enter("collection.zone.population-zone");
            trial.require(
                matches!(zone, CardZoneName::Graveyard | CardZoneName::Exile),
                "a public graveyard or exile zone for a population of owners",
            )?;
        }
        Ok(CardZoneCollectionNode {
            zone,
            owner,
            predicate,
            quantity,
            additional_zones,
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

/// Where the cards of a card noun phrase are, stated after the noun (CR
/// 400.1). The location is a query domain, independent of the consumer that
/// chooses, moves, casts or counts the cards.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardLocationNode {
    /// `exiled with it|this NOUN|SOURCE NAME`: the cards the ability source
    /// exiled with its linked abilities (CR 607.2a), all in exile.
    LinkedExile,
}

/// `exiled with ...` after a card noun: the lookahead of the linked-exile
/// location. Pure lookahead; [`recognize_card_location`] reads it.
pub(in crate::oracle::canonical) fn card_location_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("exiled"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("with"))
}

/// Reads one card location tail. `exiled with` names the source by `it`,
/// by `this` and a registered object noun (`this artifact`, `this
/// Vehicle`), or by a name link to the current card or face (`exiled with
/// Chaos Shrine's Black Crystal`). Another object after `exiled with` (`a
/// card exiled with another creature`) is no linked exile of the source and
/// fails here.
pub(in crate::oracle::canonical) fn recognize_card_location(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CardLocationNode> {
    cursor.enter("collection.location.linked-exile");
    cursor.consume_phrase(&["exiled", "with"])?;
    cursor.enter("collection.location.linked-exile.source");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("it"))
    {
        cursor.consume("it")?;
        return Ok(CardLocationNode::LinkedExile);
    }
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
    {
        cursor.consume("this")?;
        let noun = cursor
            .peek()
            .ok_or_else(|| cursor.fail("a source noun after `exiled with this`"))?
            .to_string();
        cursor.require(
            crate::oracle::characteristic_vocabulary::lookup_object_selector_lexeme(&noun)
                .is_some()
                || crate::oracle::characteristic_vocabulary::lookup_noncreature_subtype(&noun)
                    .is_some(),
            "a registered source noun after `exiled with this`",
        )?;
        cursor.consume(&noun)?;
        return Ok(CardLocationNode::LinkedExile);
    }
    let position = cursor.position();
    let source_end = cursor
        .source_names()
        .iter()
        .find(|link| {
            link.token_start == position
                && matches!(
                    link.kind,
                    crate::oracle::syntax::NameLinkKind::CurrentCard
                        | crate::oracle::syntax::NameLinkKind::CurrentFace
                )
        })
        .map(|link| link.token_end.max(link.token_start + 1));
    let end = cursor.require_value(
        source_end,
        "`it`, `this` and a source noun, or the name of this card after `exiled with`",
    )?;
    cursor.advance_to(end)?;
    Ok(CardLocationNode::LinkedExile)
}

#[cfg(test)]
#[path = "card_zone_collection_tests.rs"]
mod tests;
