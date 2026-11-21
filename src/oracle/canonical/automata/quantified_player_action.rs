//! One quantified player subject shared by a coordinated list of player
//! actions (`each player sacrifices a creature, discards a card, and loses 4
//! life`). The list ends before any connector not followed by another player
//! action verb, so `and you gain 2 life` stays with the enclosing sequence.
use super::super::{MachineResult, OracleTokenCursor};
use super::source_sacrifice::atomic_action::{
    DiscardVerbForm,
    player_discard::{PlayerDiscardEffectNode, recognize_player_discard_action_fragment},
    player_subject::{PlayerBinding, PlayerEffectSubject},
};
use super::{
    EffectRecognitionContext, LifeActionNode, LifeVerbForm, NumericExpression, PlayerHandVerb,
    PlayerSacrificeActionNode, PlayerSetReferenceNode, player_hand_verb_at,
    recognize_draw_action_fragment, recognize_life_action, recognize_mill_action_fragment,
    recognize_player_hand_action, recognize_player_sacrifice_action,
    recognize_player_set_reference,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum QuantifiedPlayerAction {
    Life(LifeActionNode),
    Discard(PlayerDiscardEffectNode),
    Sacrifice(PlayerSacrificeActionNode),
    Draw(NumericExpression),
    Mill(NumericExpression),
    /// `discards|reveals their hand`: the same whole-hand child as singular
    /// subjects; no single hand becomes an observed collection here.
    Hand(PlayerHandVerb),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct QuantifiedPlayerActionNode {
    pub players: PlayerSetReferenceNode,
    /// `each other player`: the population minus the ability controller.
    pub excluding_controller: bool,
    /// Ordered actions of the one shared subject. Each action is completed by
    /// every player of the population before the next begins (CR 608.2c).
    pub actions: Vec<QuantifiedPlayerAction>,
}

fn is_quantified_action_verb(raw: &str) -> bool {
    matches!(
        raw.to_ascii_lowercase().as_str(),
        "gains" | "loses" | "discards" | "sacrifices" | "draws" | "mills"
    )
}

/// A verb that continues the shared subject's list: an ordinary action verb
/// or a whole-hand verb (`reveals their hand`).
fn quantified_action_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek().is_some_and(is_quantified_action_verb)
        || player_hand_verb_at(cursor, LifeVerbForm::ThirdPersonSingular).is_some()
}

/// True when the cursor starts `each opponent|player|other player` followed
/// by a verb this parent supports. Opponents keep their historical claim.
pub(in crate::oracle::canonical) fn quantified_player_action_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let is = |offset: usize, word: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
    };
    if !is(0, "each") {
        return false;
    }
    if is(1, "opponent") {
        return true;
    }
    let verb_offset = if is(1, "player") {
        2
    } else if is(1, "other") && is(2, "player") {
        3
    } else {
        return false;
    };
    let mut verb = cursor.clone();
    verb.advance_to(verb.position() + verb_offset).is_ok() && quantified_action_starts(&verb)
}

pub(in crate::oracle::canonical) fn recognize_quantified_player_action(
    cursor: &mut OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> MachineResult<QuantifiedPlayerActionNode> {
    cursor.enter("effect.quantified-player.quantifier");
    cursor.require(
        cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("each")),
        "`each` before the quantified player action",
    )?;
    let excluding_controller = cursor
        .peek_ahead(1)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("other"));
    let players = if excluding_controller {
        cursor.enter("player-set.other");
        cursor.consume_phrase(&["each", "other", "player"])?;
        PlayerSetReferenceNode::EachPlayer
    } else {
        recognize_player_set_reference(cursor)?
    };
    cursor.enter("effect.quantified-player.subject");
    let mut actions = vec![recognize_quantified_action(cursor, context)?];
    loop {
        let checkpoint = cursor.checkpoint();
        cursor.enter("effect.quantified-player.list-separator");
        let comma = cursor.peek().is_some_and(|raw| raw == ",");
        if comma {
            cursor.consume(",")?;
        }
        // `and` or `then` (CR 608.2c: the order is the written order either
        // way; each action still completes for every player before the next).
        let and = cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and") || raw.eq_ignore_ascii_case("then"));
        if and {
            let connector = cursor.peek().expect("checked connector").to_string();
            cursor.consume(&connector)?;
        }
        // A separator belongs to this list only before another action verb;
        // any other continuation (`you`, a new clause) is the enclosing
        // sequence's, so the subject's scope ends here.
        if (!comma && !and) || !quantified_action_starts(cursor) {
            cursor.rollback(checkpoint);
            break;
        }
        actions.push(recognize_quantified_action(cursor, context)?);
    }
    cursor.enter("effect.quantified-player.complete");
    Ok(QuantifiedPlayerActionNode {
        players,
        excluding_controller,
        actions,
    })
}

fn recognize_quantified_action(
    cursor: &mut OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> MachineResult<QuantifiedPlayerAction> {
    cursor.enter("effect.quantified-player.action");
    if player_hand_verb_at(cursor, LifeVerbForm::ThirdPersonSingular).is_some() {
        return recognize_player_hand_action(cursor, LifeVerbForm::ThirdPersonSingular)
            .map(QuantifiedPlayerAction::Hand);
    }
    let verb = cursor.peek().map(str::to_ascii_lowercase);
    Ok(match verb.as_deref() {
        Some("gains" | "loses") => QuantifiedPlayerAction::Life(recognize_life_action(
            cursor,
            LifeVerbForm::ThirdPersonSingular,
            None,
        )?),
        Some("discards") => {
            QuantifiedPlayerAction::Discard(recognize_player_discard_action_fragment(
                cursor,
                PlayerEffectSubject::Bound(PlayerBinding(0)),
                DiscardVerbForm::ThirdPersonSingular,
            )?)
        }
        Some("sacrifices") => {
            QuantifiedPlayerAction::Sacrifice(recognize_player_sacrifice_action(cursor)?)
        }
        Some("draws") => QuantifiedPlayerAction::Draw(recognize_draw_action_fragment(
            cursor,
            LifeVerbForm::ThirdPersonSingular,
            context,
        )?),
        Some("mills") => QuantifiedPlayerAction::Mill(recognize_mill_action_fragment(
            cursor,
            LifeVerbForm::ThirdPersonSingular,
        )?),
        _ => {
            return Err(cursor.fail(
                "a supported quantified player action (`gains`, `loses`, `discards`, `sacrifices`, `draws`, or `mills`)",
            ));
        }
    })
}

#[cfg(test)]
#[path = "quantified_player_action_tests.rs"]
mod tests;
