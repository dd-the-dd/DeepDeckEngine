//! A singular grammatical player subject followed by one independent action.
//! Sequence owns subject lifetime; target declarations remain separate bindings.
use super::super::{MachineResult, OracleTokenCursor};
use super::recognize_counter_name;
use super::source_sacrifice::atomic_action::ActingSubject;
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
    CardCollectionReferenceNode, CollectionOperandAttempt, EffectRecognitionContext,
    LifeActionNode, NumericExpression, PlayerBinding, PlayerEffectSubject,
    PlayerSacrificeActionNode, SubjectVerbAgreement, VerbAgreement, recognize_draw_action_fragment,
    recognize_numeric_expression, recognize_player_sacrifice_action_with_form,
    recognize_supplied_life_action, supplied_life_action_starts,
    try_recognize_card_collection_reference,
};
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form, lookup_verb};
use crate::oracle::counter_name::CounterName;

/// Lexicon lemmas this child dispatches on (other verbs keep their readers).
const MIGRATED_PLAYER_LEMMAS: &[VerbLemma] = &[
    VerbLemma::Mill,
    VerbLemma::Discard,
    VerbLemma::Sacrifice,
    VerbLemma::Exile,
    VerbLemma::Reveal,
    VerbLemma::Get,
];

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
    /// `reveals|discards their hand`: the shared whole-hand verb child.
    Hand(PlayerHandVerb),
    /// `discards|exiles that card|it|them`: the player acts on cards an
    /// earlier action of the same program bound, never on a new choice.
    ReferencedCards {
        verb: ReferencedCardVerb,
        cards: CardCollectionReferenceNode,
    },
    /// `gets N KIND counter(s)` | `gets {E}{E}` (CR 122.1): counters placed
    /// on the acting player.
    Counters(PlayerCountersNode),
}

/// Counters a player gets: the canonical counter name and the amount.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerCountersNode {
    pub counter: CounterName,
    pub count: NumericExpression,
}

/// True when the `get` at the cursor takes player counters: a counter
/// symbol, or a counted counter noun before any clause boundary that is not
/// placed `on` or taken `from` an object.
pub(in crate::oracle::canonical) fn player_counters_follow(cursor: &OracleTokenCursor<'_>) -> bool {
    if cursor
        .peek()
        .is_none_or(|raw| lookup_verb(raw, &[VerbLemma::Get]).is_none())
    {
        return false;
    }
    if cursor
        .token_slice(cursor.position() + 1, cursor.position() + 2)
        .first()
        .is_some_and(|token| token.token_type == "counterSymbol")
    {
        return true;
    }
    for offset in 1..6 {
        let Some(raw) = cursor.peek_ahead(offset) else {
            return false;
        };
        if matches!(raw, "." | "," | ";" | ":" | "(") {
            return false;
        }
        if raw.eq_ignore_ascii_case("counter") || raw.eq_ignore_ascii_case("counters") {
            return !cursor.peek_ahead(offset + 1).is_some_and(|next| {
                next.eq_ignore_ascii_case("on") || next.eq_ignore_ascii_case("from")
            });
        }
    }
    false
}

/// `get(s) {E}{E} [(two energy counters)]` | `get(s) N KIND counter(s)`.
pub(in crate::oracle::canonical) fn recognize_player_counters_fragment(
    cursor: &mut OracleTokenCursor<'_>,
    form: VerbAgreement,
) -> MachineResult<PlayerCountersNode> {
    cursor.enter("player-action.counters.verb");
    cursor.consume(super::verb_entry(VerbLemma::Get).surface(form.form()))?;
    cursor.enter("player-action.counters.symbol");
    let mut symbols = 0_i64;
    while cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "counterSymbol")
    {
        let symbol = cursor.consume_type("counterSymbol")?;
        cursor.require(
            symbol.raw.eq_ignore_ascii_case("{E}"),
            "the registered energy-counter symbol `{E}`",
        )?;
        symbols += 1;
    }
    if symbols > 0 {
        let counter = CounterName::from_parts(["energy"]).expect("a non-empty counter name");
        // CR 207.2: the reminder restates the symbols; it must agree.
        if cursor.peek() == Some("(") {
            cursor.enter("player-action.counters.reminder");
            cursor.consume("(")?;
            let count = recognize_numeric_expression(cursor, None)?;
            cursor.require(
                count == NumericExpression::Atom(super::NumericAtom::Integer(symbols)),
                "a reminder that counts the energy symbols",
            )?;
            let name = cursor.consume_type("counterTypeWord")?;
            cursor.require(
                name.raw.eq_ignore_ascii_case("energy"),
                "the energy counter the symbols name",
            )?;
            recognize_counter_noun(cursor, &count)?;
            cursor.consume(")")?;
        }
        return Ok(PlayerCountersNode {
            counter,
            count: NumericExpression::Atom(super::NumericAtom::Integer(symbols)),
        });
    }
    cursor.enter("player-action.counters.count");
    let count = recognize_numeric_expression(cursor, None)?;
    cursor.enter("player-action.counters.name");
    let counter = recognize_counter_name(cursor)?;
    recognize_counter_noun(cursor, &count)?;
    Ok(PlayerCountersNode { counter, count })
}

/// `counter` after one, `counters` otherwise.
fn recognize_counter_noun(
    cursor: &mut OracleTokenCursor<'_>,
    count: &NumericExpression,
) -> MachineResult<()> {
    cursor.enter("player-action.counters.noun");
    let singular = *count == NumericExpression::Atom(super::NumericAtom::Integer(1));
    cursor.consume(if singular { "counter" } else { "counters" })?;
    Ok(())
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
    form: VerbAgreement,
) -> Option<PlayerHandVerb> {
    let action = if crate::oracle::canonical::automata::lemma_at(cursor, VerbLemma::Reveal, form) {
        PlayerHandVerb::Reveal
    } else if crate::oracle::canonical::automata::lemma_at(cursor, VerbLemma::Discard, form) {
        PlayerHandVerb::Discard
    } else {
        return None;
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
    form: VerbAgreement,
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
    // After a causative `have`, the explicit subject takes a bare verb and
    // keeps every other argument of the same action (`have target player
    // lose 3 life`, `have that player discard a card`).
    let bare = context.subject_verb_agreement == SubjectVerbAgreement::BareInfinitive;
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
    // `they` (CR 109.5: singular they) names the same player `that player`
    // would: the program's player binding, else the event's player. It takes
    // the plural verb form; without such a player it is not claimed here.
    let explicit_they = !bare
        && head.eq_ignore_ascii_case("they")
        && (antecedent.is_some() || context.frame.event_player);
    let plural = bare || explicit_they;
    let object_relation = object_related_player_head_length(cursor).is_some();
    let defending = defending_player_starts(cursor);
    let player = if explicit_target || explicit_bound || explicit_you {
        // Binding validation is tentative until this child owns the action.
        // Another parent may supply a different typed player relationship.
        recognize_player_effect_subject(cursor, binding, antecedent, context.frame.event_player)
    } else if explicit_they {
        cursor.enter("player-subject.pronoun");
        cursor.consume("they")?;
        Ok(antecedent.map_or(PlayerEffectSubject::EventPlayer, PlayerEffectSubject::Bound))
    } else if defending {
        match recognize_defending_player_subject(cursor, context.frame.defending_player) {
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
    } else if bare {
        // A causative names its performer; it never inherits one.
        return Ok(None);
    } else if let Some(player) = inherited {
        Ok(player.clone())
    } else if is_verb_form(&head, VerbLemma::Mill, VerbForm::Base)
        || (is_verb_form(&head, VerbLemma::Get, VerbForm::Base) && player_counters_follow(cursor))
        || head.eq_ignore_ascii_case("venture")
        || head.eq_ignore_ascii_case("proliferate")
    {
        // Imperative mood: an instruction without a subject is performed by
        // the ability's controller (`Mill two cards.`). Draw keeps its
        // established imperative reader and exact historical IR; life is the
        // shared life clause's (`life_clause.rs`).
        Ok(PlayerEffectSubject::Controller)
    } else {
        return Ok(None);
    };
    let verb = cursor.peek().unwrap_or("").to_ascii_lowercase();
    let is_draw = matches!(verb.as_str(), "draw" | "draws");
    // Lexicon lemmas at the head, in any agreement (the reader checks it).
    let lemma = cursor
        .peek()
        .and_then(|raw| {
            crate::oracle::canonical::automata::lookup_verb(raw, MIGRATED_PLAYER_LEMMAS)
        })
        .map(|(entry, _)| entry.lemma);
    let is_mill = lemma == Some(VerbLemma::Mill);
    // `reveals|discards their hand`: the possessive must name the acting
    // player's own hand; other objects stay with their own readers.
    let hand_form = if plural {
        VerbAgreement::Base
    } else {
        VerbAgreement::ThirdPersonSingular
    };
    let is_reveal_hand = player_hand_verb_at(cursor, hand_form).is_some();
    let is_venture = matches!(verb.as_str(), "venture" | "ventures")
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("into"));
    // Imperative `discard`/`sacrifice` of the controller keeps its established
    // readers; these third-person actions belong to a non-controller subject.
    let agreement = if plural {
        VerbAgreement::Base
    } else {
        VerbAgreement::ThirdPersonSingular
    };
    let is_third_person_other =
        crate::oracle::canonical::automata::lemma_at(cursor, VerbLemma::Discard, agreement)
            || crate::oracle::canonical::automata::lemma_at(
                cursor,
                VerbLemma::Sacrifice,
                agreement,
            );
    // `exiles that card|it`: only a bound card reference is claimed; a
    // counted exile (`exiles a card from their hand`) has no reader yet.
    let is_referenced_exile =
        crate::oracle::canonical::automata::lemma_at(cursor, VerbLemma::Exile, agreement)
            && referenced_card_operand_follows(cursor);
    let is_proliferate = verb == "proliferate" || (!plural && verb == "proliferates");
    let is_counters = lemma == Some(VerbLemma::Get) && player_counters_follow(cursor);
    let claimed = is_counters
        || is_draw
        || is_mill
        || is_venture
        || is_proliferate
        || is_third_person_other
        || is_reveal_hand
        || is_referenced_exile;
    // The shared sequence can still inherit a declared player for the next verb.
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
        _ if plural => VerbAgreement::Base,
        PlayerEffectSubject::Controller => VerbAgreement::Base,
        PlayerEffectSubject::DeclaredTarget { .. }
        | PlayerEffectSubject::Bound(_)
        | PlayerEffectSubject::EventPlayer
        | PlayerEffectSubject::ObjectRelation { .. } => VerbAgreement::ThirdPersonSingular,
    };
    // `ActingSubject` is the shared conjugation of the selecting verbs: the
    // bare surface keeps the third-person chooser (`of their choice`).
    let other_form = if plural {
        ActingSubject::THIRD_PLURAL
    } else {
        ActingSubject::THIRD_SINGULAR
    };
    let action = if is_reveal_hand {
        SingularPlayerAction::Hand(recognize_player_hand_action(cursor, form)?)
    } else if is_referenced_exile {
        cursor.enter("player-action.referenced-cards.exile");
        cursor.consume(
            crate::oracle::canonical::automata::verb_entry(VerbLemma::Exile)
                .surface(agreement.form()),
        )?;
        let cards = recognize_referenced_card_operand(cursor)?;
        SingularPlayerAction::ReferencedCards {
            verb: ReferencedCardVerb::Exile,
            cards,
        }
    } else if lemma == Some(VerbLemma::Discard) && referenced_card_operand_follows(cursor) {
        cursor.enter("player-action.referenced-cards.discard");
        cursor.consume(
            super::verb_entry(super::VerbLemma::Discard).surface(other_form.agreement.form()),
        )?;
        let cards = recognize_referenced_card_operand(cursor)?;
        SingularPlayerAction::ReferencedCards {
            verb: ReferencedCardVerb::Discard,
            cards,
        }
    } else if is_draw {
        SingularPlayerAction::Draw(recognize_draw_action_fragment(cursor, form, context)?)
    } else if is_mill {
        SingularPlayerAction::Mill(recognize_mill_action_fragment(cursor, form)?)
    } else if is_proliferate {
        cursor.enter("player-action.proliferate.verb");
        cursor.consume(match form {
            VerbAgreement::Base => "proliferate",
            VerbAgreement::ThirdPersonSingular => "proliferates",
        })?;
        SingularPlayerAction::Proliferate
    } else if is_venture {
        SingularPlayerAction::Venture(recognize_venture_action_fragment(cursor, form)?)
    } else if is_counters {
        SingularPlayerAction::Counters(recognize_player_counters_fragment(cursor, form)?)
    } else if lemma == Some(VerbLemma::Discard) {
        SingularPlayerAction::Discard(recognize_player_discard_action_fragment(
            cursor,
            player.clone(),
            other_form,
        )?)
    } else {
        SingularPlayerAction::Sacrifice(recognize_player_sacrifice_action_with_form(
            cursor, other_form,
        )?)
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
            context.frame.event_player,
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
                VerbAgreement::Base,
                context,
            )?),
            _ if crate::oracle::canonical::automata::lemma_at(
                cursor,
                VerbLemma::Mill,
                VerbAgreement::Base,
            ) =>
            {
                SingularPlayerAction::Mill(recognize_mill_action_fragment(
                    cursor,
                    VerbAgreement::Base,
                )?)
            }
            _ => match recognize_supplied_life_action(cursor, VerbAgreement::Base, None)? {
                Some(life) => SingularPlayerAction::Life(life),
                None => {
                    return Err(cursor.fail(
                        "a supported coordinated player action (`draw`, `mill`, `gain`, or `lose`)",
                    ));
                }
            },
        });
        let list_checkpoint = cursor.checkpoint();
        let and = cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"));
        if and {
            cursor.consume("and")?;
            if cursor.peek().is_some_and(|raw| {
                raw.eq_ignore_ascii_case("draw")
                    || is_verb_form(&raw, VerbLemma::Mill, VerbForm::Base)
            }) || supplied_life_action_starts(cursor, VerbAgreement::Base)
            {
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
    form: VerbAgreement,
) -> MachineResult<NumericExpression> {
    cursor.enter("player-action.mill.verb");
    cursor.consume(
        crate::oracle::canonical::automata::verb_entry(VerbLemma::Mill).surface(form.form()),
    )?;
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
    form: VerbAgreement,
) -> MachineResult<VentureActionNode> {
    cursor.enter("player-action.venture.verb");
    cursor.consume(match form {
        VerbAgreement::Base => "venture",
        VerbAgreement::ThirdPersonSingular => "ventures",
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
