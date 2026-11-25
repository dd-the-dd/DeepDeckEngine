//! `CARDS gain(s) KEYWORD [DURATION]` over cards outside the battlefield: a
//! keyword a card has in the zone it is in (CR 702.34a flashback). The
//! clause is the shared subject-verb frame: the cards are the shared card
//! operand (a declared card target, a named zone's cards, a bound card), the
//! verb is the lexicon's `gain` lemma, the keyword is a name from the shared
//! cost-keyword vocabulary (`KeywordAbilityName`), and the duration is the
//! shared effect duration or the sentence's preposed one.
//!
//! A cost keyword granted without its cost states the cost in a sentence of
//! its own (`The flashback cost is equal to its mana cost.`). That sentence
//! amends the grant of the sentence just closed through the sequence's
//! amendment frame (`recognize_granted_keyword_cost_amendment`); a grant
//! whose cost no sentence states is refused when it is lowered.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;

/// The cost a granted cost keyword states (CR 702.34a).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum GrantedKeywordCostNode {
    /// `equal to its|that card's mana cost`: each card's own mana cost
    /// (CR 202.1), read on the card that has the keyword.
    ManaCostOfCard,
}

/// A cost keyword granted to cards outside the battlefield. The cost is
/// `None` until the amendment sentence states it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct GrantedCardKeywordNode {
    pub name: KeywordAbilityName,
    pub cost: Option<GrantedKeywordCostNode>,
}

/// `CARDS gain(s) KEYWORD [DURATION]`: the affected cards are fixed when the
/// effect resolves (CR 611.2c).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardKeywordGrantNode {
    pub cards: CardOperandNode,
    pub keyword: GrantedCardKeywordNode,
    pub duration: EffectDurationNode,
}

/// The outcome of a card keyword grant attempt: the clause is claimed only
/// once the subject, the `gain` lemma and a cost-keyword name are read.
pub(in crate::oracle::canonical) enum CardKeywordGrantAttempt {
    NotClaimed,
    Recognized(CardKeywordGrantNode),
    FailedCommitted(super::super::MachineFailure),
}

/// The `gain` lemma at the cursor in either finite present agreement: a
/// singular (`target card gains`, `each card gains`) or plural (`cards
/// gain`) card subject. The lexicon reads the surface.
fn consume_gain(cursor: &mut OracleTokenCursor<'_>) -> bool {
    [VerbAgreement::ThirdPersonSingular, VerbAgreement::Base]
        .into_iter()
        .any(|agreement| {
            let mut trial = cursor.clone();
            if consume_lemma(&mut trial, VerbLemma::Gain, agreement).is_ok() {
                *cursor = trial;
                true
            } else {
                false
            }
        })
}

/// Reads `CARDS gain(s) KEYWORD [DURATION]` as one clause frame. The card
/// subject is read by the caller's shared card operand; the keyword cost
/// stays unstated for the amendment sentence. The closing period stays with
/// the sequence. `supplied_duration` is the sentence's preposed duration.
pub(in crate::oracle::canonical) fn try_recognize_card_keyword_grant(
    cursor: &mut OracleTokenCursor<'_>,
    supplied_duration: Option<&EffectDurationNode>,
    read_cards: impl FnOnce(&mut OracleTokenCursor<'_>) -> MachineResult<CardOperandNode>,
) -> CardKeywordGrantAttempt {
    let mut trial = cursor.clone();
    trial.enter("card-keyword-grant.cards");
    let Ok(cards) = read_cards(&mut trial) else {
        return CardKeywordGrantAttempt::NotClaimed;
    };
    trial.enter("card-keyword-grant.verb");
    if !consume_gain(&mut trial) {
        return CardKeywordGrantAttempt::NotClaimed;
    }
    trial.enter("card-keyword-grant.keyword");
    let Some(name) = trial.peek().and_then(KeywordAbilityName::from_oracle_word) else {
        return CardKeywordGrantAttempt::NotClaimed;
    };
    // Committed: the subject, the lemma and a cost-keyword name are read.
    match finish_card_keyword_grant(&mut trial, supplied_duration, cards, name) {
        Ok(node) => {
            *cursor = trial;
            CardKeywordGrantAttempt::Recognized(node)
        }
        Err(failure) => CardKeywordGrantAttempt::FailedCommitted(failure),
    }
}

fn finish_card_keyword_grant(
    cursor: &mut OracleTokenCursor<'_>,
    supplied_duration: Option<&EffectDurationNode>,
    cards: CardOperandNode,
    name: KeywordAbilityName,
) -> MachineResult<CardKeywordGrantNode> {
    let word = cursor
        .peek()
        .expect("a keyword name was peeked")
        .to_string();
    cursor.consume(&word)?;
    cursor.enter("card-keyword-grant.keyword.granted-runtime");
    // CR 702.34a: flashback is the cost keyword a card can be granted in
    // its zone with a runtime permission; another cost keyword (kicker,
    // cycling, awaken) granted to a card has no runtime yet.
    cursor.require(
        name == KeywordAbilityName::Flashback,
        "flashback, the cost keyword a card outside the battlefield can be granted",
    )?;
    cursor.enter("card-keyword-grant.cards.domain");
    let cards_supported = match &cards {
        CardOperandNode::Targets(targets) => targets.zone == CardZoneName::Graveyard,
        CardOperandNode::ZoneCollection(zone) => {
            zone.quantity.is_none()
                && zone.zone == CardZoneName::Graveyard
                && zone.additional_zones.is_empty()
        }
        CardOperandNode::Reference(_) => true,
        _ => false,
    };
    cursor.require(
        cards_supported,
        "declared graveyard card targets, every matching card of a graveyard, or a bound card",
    )?;
    cursor.enter("card-keyword-grant.duration");
    let duration = recognize_trailing_or_supplied_duration(cursor, supplied_duration)?;
    cursor.enter("card-keyword-grant.complete");
    Ok(CardKeywordGrantNode {
        cards,
        keyword: GrantedCardKeywordNode { name, cost: None },
        duration,
    })
}

fn word_is(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

/// True at `The KEYWORD cost is ...` opening a sentence: the subject of a
/// cost amendment, a cost-keyword name from the shared vocabulary.
pub(in crate::oracle::canonical) fn granted_keyword_cost_amendment_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_is(cursor, 0, "the")
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| KeywordAbilityName::from_oracle_word(raw).is_some())
        && word_is(cursor, 2, "cost")
}

/// The card keyword grant an amendment sentence amends: the last action of
/// the sentence just closed.
fn last_card_keyword_grant(
    effect: Option<&mut LocalEffectNode>,
) -> Option<&mut CardKeywordGrantNode> {
    match effect? {
        LocalEffectNode::CardAction(CardActionNode::GrantKeyword(grant)) => Some(grant),
        _ => None,
    }
}

/// `The KEYWORD cost is equal to its|that NOUN's mana cost.` as its own
/// sentence after a card keyword grant (CR 702.34a): the subject names the
/// keyword the preceding sentence granted without a cost; the possessive is
/// the anaphor of each granted card (a distributive singular, as in `each
/// card ... gains flashback`). The sentence terminator stays with the
/// enclosing sequence.
pub(in crate::oracle::canonical) fn recognize_granted_keyword_cost_amendment(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<&mut LocalEffectNode>,
) -> MachineResult<()> {
    cursor.enter("effect.granted-keyword-cost.antecedent");
    let grant = last_card_keyword_grant(antecedent).ok_or_else(|| {
        cursor.fail("a keyword grant in the preceding sentence to state the cost of")
    })?;
    cursor.require(
        grant.keyword.cost.is_none(),
        "a granted keyword whose cost no sentence has stated",
    )?;
    cursor.enter("effect.granted-keyword-cost.subject");
    cursor.consume("the")?;
    let named = cursor
        .peek()
        .and_then(KeywordAbilityName::from_oracle_word)
        .ok_or_else(|| cursor.fail("a cost keyword name"))?;
    cursor.require(
        named == grant.keyword.name,
        "the keyword the preceding sentence granted",
    )?;
    let word = cursor
        .peek()
        .expect("a keyword name was peeked")
        .to_string();
    cursor.consume(&word)?;
    cursor.consume("cost")?;
    cursor.enter("effect.granted-keyword-cost.copula");
    cursor.require(
        cursor.peek().and_then(copula_agreement) == Some(VerbAgreement::ThirdPersonSingular),
        "`is`, agreeing with the singular cost",
    )?;
    cursor.consume("is")?;
    cursor.enter("effect.granted-keyword-cost.value");
    cursor.consume_phrase(&["equal", "to"])?;
    cursor.enter("effect.granted-keyword-cost.possessor");
    if word_is(cursor, 0, "its") {
        cursor.consume("its")?;
    } else {
        cursor.consume("that")?;
        let possessive = cursor
            .peek()
            .filter(|raw| possessive_nominal(raw).is_some())
            .map(str::to_owned)
            .ok_or_else(|| cursor.fail("`its` or `that NOUN's`, the granted card"))?;
        cursor.consume(&possessive)?;
    }
    cursor.enter("effect.granted-keyword-cost.property");
    cursor.consume_phrase(&["mana", "cost"])?;
    grant.keyword.cost = Some(GrantedKeywordCostNode::ManaCostOfCard);
    Ok(())
}
