use super::super::super::super::{MachineResult, OracleTokenCursor};
use super::super::super::{NumericExpression, recognize_numeric_expression};
use super::counted_card::recognize_card_count_noun;
use super::player_subject::{PlayerBinding, PlayerEffectSubject, recognize_player_effect_subject};
use super::selection_method::{CollectionSelectionMethod, recognize_postfix_selection_method};
use super::{ActingSubject, recognize_discard_action};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerDiscardEffectNode {
    pub player: PlayerEffectSubject,
    pub quantity: DiscardQuantity,
    pub selection: CollectionSelectionMethod,
}

/// The quantity slot of a discard: a number of cards, or the whole holder
/// (`your hand`, `their hand`, `all the cards in your hand`, CR 701.9a). The
/// holder is the acting player's own hand; another player's hand has no
/// discard reading and fails explicitly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DiscardQuantity {
    Count(NumericExpression),
    /// `up to N cards`: zero to N cards, chosen by the discarding player.
    UpTo(NumericExpression),
    /// `any number of cards`: zero or more cards, chosen by the discarding
    /// player (CR 701.9a).
    AnyNumber,
    WholeHand,
}

impl DiscardQuantity {
    /// The numeric amount, when the slot states one (an exact count or an
    /// `up to` bound).
    pub(in crate::oracle::canonical) fn count(&self) -> Option<&NumericExpression> {
        match self {
            Self::Count(count) | Self::UpTo(count) => Some(count),
            Self::AnyNumber | Self::WholeHand => None,
        }
    }

    /// The discarding player chooses how many cards (an `up to` bound or
    /// `any number of`).
    pub(in crate::oracle::canonical) fn is_chosen_amount(&self) -> bool {
        matches!(self, Self::UpTo(_) | Self::AnyNumber)
    }
}

/// Resolve a singular subject, then delegate the same action tail that
/// quantified player parents use. Neither reader consumes sequence connectors.
pub(in crate::oracle::canonical::automata) fn recognize_player_discard_effect(
    cursor: &mut OracleTokenCursor<'_>,
    declared_target_binding: PlayerBinding,
    antecedent: Option<PlayerBinding>,
) -> MachineResult<PlayerDiscardEffectNode> {
    cursor.enter("effect.player-discard.subject");
    let player =
        recognize_player_effect_subject(cursor, declared_target_binding, antecedent, false)?;
    let form = match player {
        PlayerEffectSubject::Controller => ActingSubject::CONTROLLER,
        _ => ActingSubject::THIRD_SINGULAR,
    };
    recognize_player_discard_action_fragment(cursor, player, form)
}

pub(in crate::oracle::canonical::automata) fn recognize_player_discard_action_fragment(
    cursor: &mut OracleTokenCursor<'_>,
    player: PlayerEffectSubject,
    form: ActingSubject,
) -> MachineResult<PlayerDiscardEffectNode> {
    cursor.enter("effect.player-discard.action");
    let action = recognize_discard_action(cursor, form, |cursor| {
        if let Some(quantity) = recognize_whole_hand_quantity(cursor, form)? {
            return Ok(quantity);
        }
        if let Some(quantity) = recognize_chosen_discard_amount(cursor)? {
            return Ok(quantity);
        }
        cursor.enter("effect.player-discard.count");
        let count = recognize_numeric_expression(cursor, None)?;
        recognize_card_count_noun(cursor, &count)?;
        Ok(DiscardQuantity::Count(count))
    })?;
    cursor.enter("effect.player-discard.selection-method");
    let selection = recognize_postfix_selection_method(cursor)?;
    cursor.enter("effect.player-discard.whole-hand-selection");
    cursor.require(
        action.operand != DiscardQuantity::WholeHand
            || selection == CollectionSelectionMethod::Choice,
        "no selection method after a whole-hand discard",
    )?;
    cursor.enter("effect.player-discard.chosen-amount-selection");
    cursor.require(
        !action.operand.is_chosen_amount() || selection == CollectionSelectionMethod::Choice,
        "no random selection for an amount the discarding player chooses",
    )?;
    cursor.enter("effect.player-discard.complete");
    Ok(PlayerDiscardEffectNode {
        player,
        quantity: action.operand,
        selection,
    })
}

/// `any number of cards` / `up to N cards`: the shared resolution-choice
/// determiners of the object noun phrase (`ChosenObjectQuantity`), read over
/// the card noun. Without either determiner this reader makes no claim.
fn recognize_chosen_discard_amount(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<DiscardQuantity>> {
    let word = |cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    if word(cursor, 0, "any") && word(cursor, 1, "number") {
        cursor.enter("effect.player-discard.any-number");
        cursor.consume_phrase(&["any", "number", "of"])?;
        cursor.enter("effect.player-discard.any-number.noun");
        cursor.consume("cards")?;
        return Ok(Some(DiscardQuantity::AnyNumber));
    }
    if word(cursor, 0, "up") && word(cursor, 1, "to") {
        cursor.enter("effect.player-discard.up-to");
        cursor.consume_phrase(&["up", "to"])?;
        let maximum = recognize_numeric_expression(cursor, None)?;
        recognize_card_count_noun(cursor, &maximum)?;
        return Ok(Some(DiscardQuantity::UpTo(maximum)));
    }
    Ok(None)
}

/// `[all [the] cards in] POSSESSIVE hand`, where the possessive names the
/// acting player (`your` for the controller, `their` for another subject).
/// Without `all` and without the possessive this reader makes no claim.
fn recognize_whole_hand_quantity(
    cursor: &mut OracleTokenCursor<'_>,
    form: ActingSubject,
) -> MachineResult<Option<DiscardQuantity>> {
    let word = |cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    let possessive = if form.third_person { "their" } else { "your" };
    let collective = word(cursor, 0, "all");
    let holder_named = ["your", "their"]
        .iter()
        .any(|holder| word(cursor, 0, holder))
        && word(cursor, 1, "hand");
    if !collective && !holder_named {
        return Ok(None);
    }
    cursor.enter("effect.player-discard.whole-hand");
    if collective {
        cursor.consume("all")?;
        if word(cursor, 0, "the") {
            cursor.consume("the")?;
        }
        cursor.consume_phrase(&["cards", "in"])?;
    }
    cursor.enter("effect.player-discard.whole-hand.holder");
    cursor.require(
        word(cursor, 0, possessive) && word(cursor, 1, "hand"),
        if form.third_person {
            "`their hand`: the acting player's own hand"
        } else {
            "`your hand`: the acting player's own hand"
        },
    )?;
    cursor.consume_phrase(&[possessive, "hand"])?;
    Ok(Some(DiscardQuantity::WholeHand))
}

#[cfg(test)]
#[path = "player_discard_tests.rs"]
mod tests;
