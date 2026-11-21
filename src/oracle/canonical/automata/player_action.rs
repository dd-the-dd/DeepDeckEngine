//! A singular grammatical player subject followed by one independent action.
//! Sequence owns subject lifetime; target declarations remain separate bindings.
use super::super::{MachineResult, OracleTokenCursor};
use super::source_sacrifice::atomic_action::DiscardVerbForm;
use super::source_sacrifice::atomic_action::counted_card::recognize_card_count_noun;
use super::source_sacrifice::atomic_action::player_discard::{
    PlayerDiscardEffectNode, recognize_player_discard_action_fragment,
};
use super::source_sacrifice::atomic_action::player_subject::{
    ObjectSubjectAntecedent, defending_player_starts, object_related_player_head_length,
    recognize_defending_player_subject, recognize_object_related_player_subject,
    recognize_player_effect_subject,
};
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
    /// `venture into the dungeon` / `venture into NAME` (CR 701.49): the
    /// named form is the restricted procedure of that Dungeon (CR 701.49c).
    Venture(VentureActionNode),
    /// `discards N cards [at random]`: the same action child that quantified
    /// subjects use. The node's own subject repeats the enclosing one.
    Discard(PlayerDiscardEffectNode),
    /// `sacrifices N QUERY [of their choice]` on the acting player's own
    /// permanents (CR 701.21a); the player chooses, then all are sacrificed.
    Sacrifice(PlayerSacrificeActionNode),
    /// `proliferate` (CR 701.34a): the acting player chooses any number of
    /// permanents and/or players that have a counter, then gives each one
    /// additional counter of each kind already there.
    Proliferate,
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
    try_recognize_player_action_with_object_antecedent(
        cursor, binding, antecedent, inherited, None, context,
    )
}

/// The same child with the singular object antecedent the sequence proved,
/// so `its controller` / `that NOUN's owner` can act as the player subject.
pub(in crate::oracle::canonical) fn try_recognize_player_action_with_object_antecedent(
    cursor: &mut OracleTokenCursor<'_>,
    binding: PlayerBinding,
    antecedent: Option<PlayerBinding>,
    inherited: Option<&PlayerEffectSubject>,
    object_antecedent: Option<ObjectSubjectAntecedent>,
    context: &EffectRecognitionContext,
) -> MachineResult<Option<PlayerActionNode>> {
    let checkpoint = cursor.checkpoint();
    let head = cursor.peek().unwrap_or("");
    let explicit_target = head.eq_ignore_ascii_case("target")
        && cursor.peek_ahead(1).is_some_and(|raw| {
            raw.eq_ignore_ascii_case("player") || raw.eq_ignore_ascii_case("opponent")
        });
    // The historical explicit life reader declares only `target player`;
    // every other declared player role uses this shared subject reader.
    let historical_life_target = explicit_target
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("player"));
    let explicit_bound = head.eq_ignore_ascii_case("that")
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("player"));
    let explicit_you = inherited.is_some() && head.eq_ignore_ascii_case("you");
    let object_relation = object_related_player_head_length(cursor).is_some();
    let defending = defending_player_starts(cursor);
    let player = if explicit_target || explicit_bound || explicit_you {
        // Binding validation is tentative until this child owns the action.
        // Another parent may supply a different typed player relationship.
        recognize_player_effect_subject(cursor, binding, antecedent, context.event_player)
    } else if defending {
        match recognize_defending_player_subject(cursor, context.defending_player) {
            Ok(player) => Ok(player),
            Err(_) => {
                cursor.rollback(checkpoint);
                return Ok(None);
            }
        }
    } else if object_relation {
        // Tentative like the explicit subjects above: without a proven object
        // antecedent the established readers keep their own diagnostics.
        match recognize_object_related_player_subject(cursor, object_antecedent) {
            Ok(player) => Ok(player),
            Err(_) => {
                cursor.rollback(checkpoint);
                return Ok(None);
            }
        }
    } else if let Some(player) = inherited {
        Ok(player.clone())
    } else if head.eq_ignore_ascii_case("mill")
        || head.eq_ignore_ascii_case("venture")
        || head.eq_ignore_ascii_case("proliferate")
    {
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
    let is_venture = matches!(verb.as_str(), "venture" | "ventures")
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("into"));
    // Imperative `discard`/`sacrifice` of the controller keeps its established
    // readers; these third-person actions belong to a non-controller subject.
    let is_third_person_other = matches!(verb.as_str(), "discards" | "sacrifices");
    let is_proliferate = matches!(verb.as_str(), "proliferate" | "proliferates");
    let claimed =
        is_draw || is_life || is_mill || is_venture || is_proliferate || is_third_person_other;
    // Exact old explicit life paths retain their nodes and diagnostics. The
    // shared sequence can still inherit their declared player for the next verb.
    if !claimed
        && !explicit_target
        && !explicit_bound
        && !explicit_you
        && !object_relation
        && !defending
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
        || (is_life
            && (historical_life_target
                // The historical reader binds `that player` only to a
                // declared target; an event player belongs to this reader.
                || (explicit_bound && (antecedent.is_some() || !context.event_player))))
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
        PlayerEffectSubject::DeclaredTarget { .. }
        | PlayerEffectSubject::Bound(_)
        | PlayerEffectSubject::EventPlayer
        | PlayerEffectSubject::ObjectRelation { .. } => LifeVerbForm::ThirdPersonSingular,
    };
    let action = if is_draw {
        SingularPlayerAction::Draw(recognize_draw_action_fragment(cursor, form, context)?)
    } else if is_mill {
        SingularPlayerAction::Mill(recognize_mill_action_fragment(cursor, form)?)
    } else if is_proliferate {
        cursor.enter("player-action.proliferate.verb");
        cursor.consume(match form {
            LifeVerbForm::Base => "proliferate",
            LifeVerbForm::ThirdPersonSingular => "proliferates",
        })?;
        SingularPlayerAction::Proliferate
    } else if is_venture {
        SingularPlayerAction::Venture(recognize_venture_action_fragment(cursor, form)?)
    } else if verb == "discards" {
        SingularPlayerAction::Discard(recognize_player_discard_action_fragment(
            cursor,
            player.clone(),
            DiscardVerbForm::ThirdPersonSingular,
        )?)
    } else if verb == "sacrifices" {
        SingularPlayerAction::Sacrifice(recognize_player_sacrifice_action(cursor)?)
    } else {
        // `its controller gains life equal to its power`: the possessive
        // subject's target is also the antecedent of the amount's `its`.
        let amount_antecedent = match &player {
            PlayerEffectSubject::ObjectRelation {
                object: super::PermanentEffectReference::Target(binding),
                ..
            } => Some(*binding),
            _ => None,
        };
        SingularPlayerAction::Life(recognize_life_action(cursor, form, amount_antecedent)?)
    };
    Ok(Some(PlayerActionNode { player, action }))
}

/// `SUBJECT and SUBJECT each ACTION [and ACTION]...` (CR 109.5): one sentence
/// whose coordinated player subjects each perform the same ordered actions.
/// The verbs take the plural base form after `each`. The result is the
/// distributed per-player list, action-major: every subject completes one
/// action before the next action begins (CR 608.2c). A declared target subject
/// keeps one declaration; its later uses are bound to it.
///
/// Returns `Ok(None)` (cursor restored) unless the coordinated subject frame
/// `... and ... each` is present; after `each` the frame owns its failures.
pub(in crate::oracle::canonical) fn try_recognize_coordinated_player_actions(
    cursor: &mut OracleTokenCursor<'_>,
    first_effect_index: usize,
    antecedent: Option<PlayerBinding>,
    context: &EffectRecognitionContext,
) -> MachineResult<Option<Vec<PlayerActionNode>>> {
    let checkpoint = cursor.checkpoint();
    let starts_subject = |cursor: &OracleTokenCursor<'_>| {
        cursor.peek().is_some_and(|raw| {
            raw.eq_ignore_ascii_case("you")
                || raw.eq_ignore_ascii_case("target")
                || raw.eq_ignore_ascii_case("that")
        })
    };
    if !starts_subject(cursor) {
        return Ok(None);
    }
    // Subjects are read tentatively: a target slot is bound to the effect
    // index its first distributed action will occupy, fixed below.
    let mut subjects = Vec::new();
    loop {
        let Ok(subject) = recognize_player_effect_subject(
            cursor,
            PlayerBinding(usize::MAX),
            antecedent,
            context.event_player,
        ) else {
            cursor.rollback(checkpoint);
            return Ok(None);
        };
        subjects.push(subject);
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
            && subjects.len() == 1
        {
            cursor.consume("and")?;
            if !starts_subject(cursor) {
                cursor.rollback(checkpoint);
                return Ok(None);
            }
            continue;
        }
        break;
    }
    if subjects.len() < 2
        || !cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("each"))
    {
        cursor.rollback(checkpoint);
        return Ok(None);
    }
    cursor.enter("player-subject.coordinated.each");
    cursor.consume("each")?;
    cursor.require(
        subjects.iter().enumerate().all(|(index, subject)| {
            !subjects[..index].contains(subject)
                && !matches!(subject, PlayerEffectSubject::EventPlayer)
        }),
        "distinct coordinated player subjects",
    )?;
    let mut actions = Vec::new();
    loop {
        cursor.enter("player-subject.coordinated.action");
        let verb = cursor.peek().unwrap_or("").to_ascii_lowercase();
        actions.push(match verb.as_str() {
            "draw" => SingularPlayerAction::Draw(recognize_draw_action_fragment(
                cursor,
                LifeVerbForm::Base,
                context,
            )?),
            "mill" => SingularPlayerAction::Mill(recognize_mill_action_fragment(
                cursor,
                LifeVerbForm::Base,
            )?),
            "gain" | "lose" => {
                SingularPlayerAction::Life(recognize_life_action(cursor, LifeVerbForm::Base, None)?)
            }
            _ => {
                return Err(cursor.fail(
                    "a supported coordinated player action (`draw`, `mill`, `gain`, or `lose`)",
                ));
            }
        });
        let list_checkpoint = cursor.checkpoint();
        let and = cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"));
        if and {
            cursor.consume("and")?;
            if cursor.peek().is_some_and(|raw| {
                matches!(
                    raw.to_ascii_lowercase().as_str(),
                    "draw" | "mill" | "gain" | "lose"
                )
            }) {
                continue;
            }
        }
        cursor.rollback(list_checkpoint);
        break;
    }
    // Distribute action-major; the first use of a declared target declares
    // it at its own effect index, every later use is bound to that index.
    let mut distributed = Vec::new();
    let mut declared: Vec<Option<PlayerBinding>> = vec![None; subjects.len()];
    for action in &actions {
        for (index, subject) in subjects.iter().enumerate() {
            let effect_index = first_effect_index + distributed.len();
            let player = match (subject, declared[index]) {
                (PlayerEffectSubject::DeclaredTarget { target, .. }, None) => {
                    let binding = PlayerBinding(effect_index);
                    declared[index] = Some(binding);
                    PlayerEffectSubject::DeclaredTarget {
                        target: target.clone(),
                        binding,
                    }
                }
                (PlayerEffectSubject::DeclaredTarget { .. }, Some(binding)) => {
                    PlayerEffectSubject::Bound(binding)
                }
                (other, _) => other.clone(),
            };
            distributed.push(PlayerActionNode {
                player,
                action: action.clone(),
            });
        }
    }
    Ok(Some(distributed))
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
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("cards"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("equal"))
    {
        // `mills cards equal to EXPR`: the shared `equal to` quantity.
        cursor.enter("player-action.mill.equal-amount");
        cursor.consume_phrase(&["cards", "equal", "to"])?;
        return super::recognize_equal_to_amount_expression(cursor, None);
    }
    cursor.enter("player-action.mill.count");
    let count = recognize_numeric_expression(cursor, None)?;
    cursor.enter("player-action.mill.cards");
    recognize_card_count_noun(cursor, &count)?;
    Ok(count)
}

/// `venture into` opens the imperative keyword action (CR 701.49).
pub(in crate::oracle::canonical) fn venture_action_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("venture"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("into"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct VentureActionNode {
    /// `None` for `the dungeon` (any Dungeon without an entry restriction).
    pub dungeon: Option<String>,
}

/// Subject-free `venture into the dungeon|NAME` child (CR 701.49).
pub(in crate::oracle::canonical) fn recognize_venture_action_fragment(
    cursor: &mut OracleTokenCursor<'_>,
    form: LifeVerbForm,
) -> MachineResult<VentureActionNode> {
    cursor.enter("player-action.venture.verb");
    cursor.consume(match form {
        LifeVerbForm::Base => "venture",
        LifeVerbForm::ThirdPersonSingular => "ventures",
    })?;
    cursor.consume("into")?;
    cursor.enter("player-action.venture.dungeon");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("dungeon"))
    {
        cursor.consume_phrase(&["the", "dungeon"])?;
        return Ok(VentureActionNode { dungeon: None });
    }
    let mut words = Vec::new();
    while let Some(raw) = cursor.peek() {
        let name_word = raw.starts_with(|character: char| character.is_ascii_uppercase())
            && raw.chars().all(|character| {
                character.is_alphanumeric() || character == '\'' || character == '\u{2019}'
            });
        if !name_word {
            break;
        }
        words.push(raw.to_string());
        cursor.advance_to(cursor.position() + 1)?;
    }
    cursor.require(!words.is_empty(), "`the dungeon` or a Dungeon name")?;
    Ok(VentureActionNode {
        dungeon: Some(words.join(" ")),
    })
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
