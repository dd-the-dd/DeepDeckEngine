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
    CardCollectionReferenceNode, CollectionOperandAttempt, EffectRecognitionContext,
    LifeActionNode, LifeVerbForm, NumericExpression, PlayerBinding, PlayerEffectSubject,
    PlayerSacrificeActionNode, SubjectVerbAgreement, recognize_draw_action_fragment,
    recognize_life_action, recognize_numeric_expression,
    recognize_player_sacrifice_action_with_form, try_recognize_card_collection_reference,
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
    /// `reveals|discards their hand`: the shared whole-hand verb child.
    Hand(PlayerHandVerb),
    /// `discards|exiles that card|it|them`: the player acts on cards an
    /// earlier action of the same program bound, never on a new choice.
    ReferencedCards {
        verb: ReferencedCardVerb,
        cards: CardCollectionReferenceNode,
    },
}

/// A verb whose object is the acting player's whole hand (`their hand`).
/// Singular and quantified subjects share this child.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerHandVerb {
    /// CR 701.20a: the player shows every card in their hand. After a
    /// singular subject the hand becomes the observed collection `from it`.
    Reveal,
    /// CR 701.9a: every card in the hand is discarded.
    Discard,
}

/// The whole-hand verb at the cursor, in the subject's agreement, when it is
/// followed by the acting player's `their hand`. `None` makes no claim.
pub(in crate::oracle::canonical) fn player_hand_verb_at(
    cursor: &OracleTokenCursor<'_>,
    form: LifeVerbForm,
) -> Option<PlayerHandVerb> {
    let verb = cursor.peek()?.to_ascii_lowercase();
    let action = match (form, verb.as_str()) {
        (LifeVerbForm::ThirdPersonSingular, "reveals") | (LifeVerbForm::Base, "reveal") => {
            PlayerHandVerb::Reveal
        }
        (LifeVerbForm::ThirdPersonSingular, "discards") | (LifeVerbForm::Base, "discard") => {
            PlayerHandVerb::Discard
        }
        _ => return None,
    };
    (cursor
        .peek_ahead(1)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("their"))
        && cursor
            .peek_ahead(2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("hand")))
    .then_some(action)
}

/// `reveals|discards their hand`, after `player_hand_verb_at` proved the claim.
pub(in crate::oracle::canonical) fn recognize_player_hand_action(
    cursor: &mut OracleTokenCursor<'_>,
    form: LifeVerbForm,
) -> MachineResult<PlayerHandVerb> {
    cursor.enter("player-action.hand.verb");
    let action = player_hand_verb_at(cursor, form)
        .ok_or_else(|| cursor.fail("`reveals` or `discards` followed by `their hand`"))?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("player-action.hand.zone");
    cursor.consume_phrase(&["their", "hand"])?;
    Ok(action)
}

/// A player's verb whose object is a bound card reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ReferencedCardVerb {
    /// CR 701.9a: the cards move from their owner's hand to the graveyard.
    Discard,
    /// CR 701.13a: the cards move to exile.
    Exile,
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
    // After a causative `have`, the explicit subject takes a bare verb and
    // keeps every other argument of the same action (`have target player
    // lose 3 life`, `have that player discard a card`).
    let bare = context.subject_verb_agreement == SubjectVerbAgreement::BareInfinitive;
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
    let player = if explicit_target || explicit_bound || explicit_you {
        // Binding validation is tentative until this child owns the action.
        // Another parent may supply a different typed player relationship.
        recognize_player_effect_subject(cursor, binding, antecedent, context.event_player)
    } else if bare {
        // A causative names its performer; it never inherits one.
        return Ok(None);
    } else if let Some(player) = inherited {
        Ok(player.clone())
    } else if head.eq_ignore_ascii_case("mill") || head.eq_ignore_ascii_case("venture") {
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
    // `reveals|discards their hand`: the possessive must name the acting
    // player's own hand; other objects stay with their own readers.
    let hand_form = if bare {
        LifeVerbForm::Base
    } else {
        LifeVerbForm::ThirdPersonSingular
    };
    let is_reveal_hand = player_hand_verb_at(cursor, hand_form).is_some();
    let is_venture = matches!(verb.as_str(), "venture" | "ventures")
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("into"));
    // Imperative `discard`/`sacrifice` of the controller keeps its established
    // readers; these third-person actions belong to a non-controller subject.
    let is_third_person_other = if bare {
        matches!(verb.as_str(), "discard" | "sacrifice")
    } else {
        matches!(verb.as_str(), "discards" | "sacrifices")
    };
    // `exiles that card|it`: only a bound card reference is claimed; a
    // counted exile (`exiles a card from their hand`) has no reader yet.
    let exile_form = if bare { "exile" } else { "exiles" };
    let is_referenced_exile = verb == exile_form && referenced_card_operand_follows(cursor);
    let claimed = is_draw
        || is_life
        || is_mill
        || is_venture
        || is_third_person_other
        || is_reveal_hand
        || is_referenced_exile;
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
        || (is_life
            && !bare
            && (historical_life_target
                // The historical reader binds `that player` only to a
                // declared target; an event player belongs to this reader.
                || (explicit_bound && (antecedent.is_some() || !context.event_player))))
        // An unresolved subject (`that player` without antecedent) leaves
        // the other established parents in charge instead of committing.
        || ((is_third_person_other || is_reveal_hand || is_referenced_exile)
            && !matches!(&player, Ok(subject) if *subject != PlayerEffectSubject::Controller))
    {
        cursor.rollback(checkpoint);
        return Ok(None);
    }
    let player = player?;
    let form = match player {
        _ if bare => LifeVerbForm::Base,
        PlayerEffectSubject::Controller => LifeVerbForm::Base,
        PlayerEffectSubject::DeclaredTarget { .. }
        | PlayerEffectSubject::Bound(_)
        | PlayerEffectSubject::EventPlayer => LifeVerbForm::ThirdPersonSingular,
    };
    // `DiscardVerbForm` is the shared conjugation of the selecting verbs: the
    // bare surface keeps the third-person chooser (`of their choice`).
    let other_form = if bare {
        DiscardVerbForm::ThirdPersonPlural
    } else {
        DiscardVerbForm::ThirdPersonSingular
    };
    let action = if is_reveal_hand {
        SingularPlayerAction::Hand(recognize_player_hand_action(cursor, form)?)
    } else if is_referenced_exile {
        cursor.enter("player-action.referenced-cards.exile");
        cursor.consume(exile_form)?;
        let cards = recognize_referenced_card_operand(cursor)?;
        SingularPlayerAction::ReferencedCards {
            verb: ReferencedCardVerb::Exile,
            cards,
        }
    } else if matches!(verb.as_str(), "discards" | "discard")
        && referenced_card_operand_follows(cursor)
    {
        cursor.enter("player-action.referenced-cards.discard");
        cursor.consume(match other_form {
            DiscardVerbForm::ThirdPersonSingular => "discards",
            DiscardVerbForm::Imperative | DiscardVerbForm::ThirdPersonPlural => "discard",
        })?;
        let cards = recognize_referenced_card_operand(cursor)?;
        SingularPlayerAction::ReferencedCards {
            verb: ReferencedCardVerb::Discard,
            cards,
        }
    } else if is_draw {
        SingularPlayerAction::Draw(recognize_draw_action_fragment(cursor, form, context)?)
    } else if is_mill {
        SingularPlayerAction::Mill(recognize_mill_action_fragment(cursor, form)?)
    } else if is_venture {
        SingularPlayerAction::Venture(recognize_venture_action_fragment(cursor, form)?)
    } else if matches!(verb.as_str(), "discards" | "discard") {
        SingularPlayerAction::Discard(recognize_player_discard_action_fragment(
            cursor,
            player.clone(),
            other_form,
        )?)
    } else if matches!(verb.as_str(), "sacrifices" | "sacrifice") {
        SingularPlayerAction::Sacrifice(recognize_player_sacrifice_action_with_form(
            cursor, other_form,
        )?)
    } else {
        SingularPlayerAction::Life(recognize_life_action(cursor, form, None)?)
    };
    Ok(Some(PlayerActionNode { player, action }))
}

/// True when the verb at the cursor is followed by a bound card reference
/// (`that card`, `it`, `them`, `those cards`) rather than a counted operand.
fn referenced_card_operand_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    let mut trial = cursor.clone();
    if trial.advance_to(trial.position() + 1).is_err() {
        return false;
    }
    recognize_referenced_card_operand(&mut trial).is_ok()
}

fn recognize_referenced_card_operand(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CardCollectionReferenceNode> {
    cursor.enter("player-action.referenced-cards.operand");
    match try_recognize_card_collection_reference(cursor) {
        CollectionOperandAttempt::Recognized(fragment)
            if matches!(
                fragment.node,
                CardCollectionReferenceNode::Selected | CardCollectionReferenceNode::Antecedent
            ) =>
        {
            Ok(fragment.node)
        }
        CollectionOperandAttempt::FailedCommitted(failure)
        | CollectionOperandAttempt::RejectedCandidate(failure) => Err(failure),
        _ => Err(cursor.fail("a bound card reference (`that card`, `it`, `them`)")),
    }
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

#[cfg(test)]
#[path = "player_hand_action_tests.rs"]
mod hand_tests;
