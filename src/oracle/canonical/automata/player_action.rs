//! A singular grammatical player subject followed by one independent action.
//! Sequence owns subject lifetime; target declarations remain separate bindings.
use super::super::{MachineResult, OracleTokenCursor};
use super::source_sacrifice::atomic_action::DiscardVerbForm;
use super::source_sacrifice::atomic_action::counted_card::recognize_card_count_noun;
use super::source_sacrifice::atomic_action::player_discard::{
    PlayerDiscardEffectNode, recognize_player_discard_action_fragment,
};
use super::source_sacrifice::atomic_action::player_subject::recognize_player_effect_subject;
use super::{
    EffectRecognitionContext, LifeActionNode, LifeVerbForm, NumericExpression, PlayerBinding,
    PlayerEffectSubject, PlayerSacrificeActionNode, recognize_draw_action_fragment,
    recognize_life_action, recognize_numeric_expression, recognize_player_sacrifice_action,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SingularPlayerAction {
    Draw(NumericExpression),
    Life(LifeActionNode),
    /// `mill N cards` (CR 701.17): the player's verb; N is the shared numeric
    /// expression and the counted noun agrees with it.
    Mill(NumericExpression),
    /// `discards N cards [at random]`: the same action child that quantified
    /// subjects use. The node's own subject repeats the enclosing one.
    Discard(PlayerDiscardEffectNode),
    /// `sacrifices N QUERY [of their choice]` on the acting player's own
    /// permanents (CR 701.21a); the player chooses, then all are sacrificed.
    Sacrifice(PlayerSacrificeActionNode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerActionNode {
    pub player: PlayerEffectSubject,
    pub action: SingularPlayerAction,
}

/// A nonmatching action restores the cursor and leaves established readers in
/// charge. An action headed by a supported verb owns its complete child tail.
pub(in crate::oracle::canonical) fn try_recognize_player_action(
    cursor: &mut OracleTokenCursor<'_>,
    binding: PlayerBinding,
    antecedent: Option<PlayerBinding>,
    inherited: Option<&PlayerEffectSubject>,
    context: &EffectRecognitionContext,
) -> MachineResult<Option<PlayerActionNode>> {
    let checkpoint = cursor.checkpoint();
    let head = cursor.peek().unwrap_or("");
    let explicit_target = head.eq_ignore_ascii_case("target")
        && cursor.peek_ahead(1).is_some_and(|raw| {
            raw.eq_ignore_ascii_case("player") || raw.eq_ignore_ascii_case("opponent")
        });
    let explicit_bound = head.eq_ignore_ascii_case("that")
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("player"));
    let explicit_you = inherited.is_some() && head.eq_ignore_ascii_case("you");
    let player = if explicit_target || explicit_bound || explicit_you {
        // Binding validation is tentative until this child owns the action.
        // Another parent may supply a different typed player relationship.
        recognize_player_effect_subject(cursor, binding, antecedent)
    } else if let Some(player) = inherited {
        Ok(player.clone())
    } else if head.eq_ignore_ascii_case("mill") {
        // Imperative mood: an instruction without a subject is performed by
        // the ability's controller (`Mill two cards.`). Draw and life keep
        // their established imperative readers and exact historical IR.
        Ok(PlayerEffectSubject::Controller)
    } else {
        return Ok(None);
    };
    let verb = cursor.peek().unwrap_or("").to_ascii_lowercase();
    let is_draw = matches!(verb.as_str(), "draw" | "draws");
    let is_life = matches!(verb.as_str(), "gain" | "gains" | "lose" | "loses");
    let is_mill = matches!(verb.as_str(), "mill" | "mills");
    // Imperative `discard`/`sacrifice` of the controller keeps its established
    // readers; these third-person actions belong to a non-controller subject.
    let is_third_person_other = matches!(verb.as_str(), "discards" | "sacrifices");
    let claimed = is_draw || is_life || is_mill || is_third_person_other;
    // Exact old explicit life paths retain their nodes and diagnostics. The
    // shared sequence can still inherit their declared player for the next verb.
    if !claimed
        && !explicit_target
        && !explicit_bound
        && !explicit_you
        && matches!(inherited, Some(PlayerEffectSubject::Bound(_)))
        && cursor
            .peek_token()
            .is_some_and(|token| token.token_type == "actionWord")
    {
        cursor.enter("player-subject.action.scope");
        return Err(cursor.fail(
            "a represented action for the inherited player subject, an explicit actor, or a new sentence",
        ));
    }
    if !claimed
        || (is_life && (explicit_target || explicit_bound))
        // An unresolved subject (`that player` without antecedent) leaves
        // the other established parents in charge instead of committing.
        || (is_third_person_other
            && !matches!(&player, Ok(subject) if *subject != PlayerEffectSubject::Controller))
    {
        cursor.rollback(checkpoint);
        return Ok(None);
    }
    let player = player?;
    let form = match player {
        PlayerEffectSubject::Controller => LifeVerbForm::Base,
        PlayerEffectSubject::DeclaredTarget { .. } | PlayerEffectSubject::Bound(_) => {
            LifeVerbForm::ThirdPersonSingular
        }
    };
    let action = if is_draw {
        SingularPlayerAction::Draw(recognize_draw_action_fragment(cursor, form, context)?)
    } else if is_mill {
        SingularPlayerAction::Mill(recognize_mill_action_fragment(cursor, form)?)
    } else if verb == "discards" {
        SingularPlayerAction::Discard(recognize_player_discard_action_fragment(
            cursor,
            player.clone(),
            DiscardVerbForm::ThirdPersonSingular,
        )?)
    } else if verb == "sacrifices" {
        SingularPlayerAction::Sacrifice(recognize_player_sacrifice_action(cursor)?)
    } else {
        SingularPlayerAction::Life(recognize_life_action(cursor, form, None)?)
    };
    Ok(Some(PlayerActionNode { player, action }))
}

/// Subject-free `mill N cards` child; the caller resolves actor and agreement.
pub(in crate::oracle::canonical) fn recognize_mill_action_fragment(
    cursor: &mut OracleTokenCursor<'_>,
    form: LifeVerbForm,
) -> MachineResult<NumericExpression> {
    cursor.enter("player-action.mill.verb");
    cursor.consume(match form {
        LifeVerbForm::Base => "mill",
        LifeVerbForm::ThirdPersonSingular => "mills",
    })?;
    cursor.enter("player-action.mill.count");
    let count = recognize_numeric_expression(cursor, None)?;
    cursor.enter("player-action.mill.cards");
    recognize_card_count_noun(cursor, &count)?;
    Ok(count)
}

/// Declarations become bound uses in grammatical memory, never cloned targets.
pub(in crate::oracle::canonical) fn continuing_player_subject(
    player: &PlayerEffectSubject,
) -> PlayerEffectSubject {
    match player {
        PlayerEffectSubject::DeclaredTarget { binding, .. } => PlayerEffectSubject::Bound(*binding),
        other => other.clone(),
    }
}

#[cfg(test)]
#[path = "player_action_tests.rs"]
mod tests;
