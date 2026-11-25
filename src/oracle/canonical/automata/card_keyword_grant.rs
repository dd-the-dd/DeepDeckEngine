//! `CARDS gain(s) KEYWORD until end of turn` over cards outside the
//! battlefield: a keyword a card has in the zone it is in (CR 702.34a
//! flashback). The cards are the shared card operand (a declared card target,
//! a named zone's cards, a bound card); the keyword and its cost are typed
//! here, independently of the consumer that reads the cards.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;

/// The cost a granted flashback states (CR 702.34a).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum GrantedFlashbackCostNode {
    /// `The flashback cost is equal to its|that card's mana cost.`: each
    /// card's own mana cost (CR 202.1).
    ManaCostOfCard,
}

/// A keyword granted to cards outside the battlefield.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum GrantedCardKeywordNode {
    Flashback(GrantedFlashbackCostNode),
}

/// `CARDS gain(s) KEYWORD until end of turn`: the affected cards are fixed
/// when the effect resolves (CR 611.2c); the grant lasts until end of turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardKeywordGrantNode {
    pub cards: CardOperandNode,
    pub keyword: GrantedCardKeywordNode,
}

/// `gain|gains flashback` in the current sentence: the lookahead of a card
/// keyword grant. Pure lookahead; the operand and keyword are read by
/// [`recognize_card_keyword_grant`].
pub(in crate::oracle::canonical) fn card_keyword_grant_follows(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let sentence = cursor.token_slice(cursor.position(), usize::MAX);
    let sentence = &sentence[..sentence
        .iter()
        .position(|token| token.raw == ".")
        .unwrap_or(sentence.len())];
    sentence.windows(2).any(|pair| {
        (pair[0].raw.eq_ignore_ascii_case("gains") || pair[0].raw.eq_ignore_ascii_case("gain"))
            && pair[1].raw.eq_ignore_ascii_case("flashback")
    })
}

/// Reads `CARDS gain(s) flashback until end of turn. The flashback cost is
/// equal to its|that card's mana cost`. The cards are read by the caller's
/// shared card operand; the closing period stays with the sequence. A
/// flashback with another cost, another duration or another operand fails
/// at its own state.
pub(in crate::oracle::canonical) fn recognize_card_keyword_grant(
    cursor: &mut OracleTokenCursor<'_>,
    read_cards: impl FnOnce(&mut OracleTokenCursor<'_>) -> MachineResult<CardOperandNode>,
) -> MachineResult<CardKeywordGrantNode> {
    cursor.enter("card-keyword-grant.cards");
    let cards = read_cards(cursor)?;
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
    cursor.enter("card-keyword-grant.verb");
    let verb = cursor
        .peek()
        .filter(|raw| raw.eq_ignore_ascii_case("gains") || raw.eq_ignore_ascii_case("gain"))
        .map(str::to_owned)
        .ok_or_else(|| cursor.fail("`gain` or `gains`"))?;
    cursor.consume(&verb)?;
    cursor.enter("card-keyword-grant.keyword");
    cursor.consume("flashback")?;
    cursor.enter("card-keyword-grant.duration");
    cursor.consume_phrase(&["until", "end", "of", "turn"])?;
    cursor.enter("card-keyword-grant.flashback-cost");
    cursor.consume_phrase(&[".", "the", "flashback", "cost", "is", "equal", "to"])?;
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("its"))
    {
        cursor.consume("its")?;
    } else {
        cursor.consume("that")?;
        let possessive = cursor
            .peek()
            .filter(|raw| {
                raw.eq_ignore_ascii_case("card's") || raw.eq_ignore_ascii_case("card\u{2019}s")
            })
            .map(str::to_owned)
            .ok_or_else(|| cursor.fail("`its` or `that card's` mana cost"))?;
        cursor.consume(&possessive)?;
    }
    cursor.consume_phrase(&["mana", "cost"])?;
    cursor.enter("card-keyword-grant.complete");
    Ok(CardKeywordGrantNode {
        cards,
        keyword: GrantedCardKeywordNode::Flashback(GrantedFlashbackCostNode::ManaCostOfCard),
    })
}
