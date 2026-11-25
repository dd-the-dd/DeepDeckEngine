use super::super::super::super::{MachineResult, OracleTokenCursor};
use super::super::super::{
    PermanentEffectReference, TargetCardinality, TargetListNode, TargetSelector,
    installed_binding_environment, recognize_defending_player, recognize_object_related_player,
    recognize_target_list, recognize_that_player,
};
pub(in crate::oracle::canonical) use super::super::super::{
    defending_player_starts, object_related_player_head_length,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerBinding(pub usize);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerEffectSubject {
    Controller,
    DeclaredTarget {
        target: TargetListNode,
        binding: PlayerBinding,
    },
    Bound(PlayerBinding),
    /// `that player` naming the player of the triggering event, such as the
    /// player dealt combat damage (CR 603.2). The event, not the effect
    /// program, binds this player; the runtime reads it as the triggering player.
    EventPlayer,
    /// `its controller` / `that creature's owner`: the player related to an
    /// object the parent already bound (CR 108.3 owner, 109.4 controller).
    /// The runtime reads the object's current or last known information.
    ObjectRelation {
        relation: ObjectPlayerRelation,
        object: PermanentEffectReference,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObjectPlayerRelation {
    Controller,
    Owner,
}

/// The singular object a parent proved as the antecedent of `its` / `that
/// NOUN's`, with the selector that declared it when it is a target.
#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) struct ObjectSubjectAntecedent {
    pub reference: PermanentEffectReference,
    pub selector: Option<TargetSelector>,
}

/// The written form of a bound player anaphor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerAnaphor {
    /// `that player`.
    ThatPlayer,
    /// `the player` (not `the player who|with|whose ...`, a new description).
    ThePlayer,
    /// `that opponent`: the same antecedent, which is an opponent.
    ThatOpponent,
}

/// The bound player anaphor at the cursor, if any. Lexical lookahead only;
/// the shared resolver proves its antecedent.
pub(in crate::oracle::canonical) fn player_anaphor_at(
    cursor: &OracleTokenCursor<'_>,
) -> Option<PlayerAnaphor> {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    if word(0, "that") && word(1, "player") {
        Some(PlayerAnaphor::ThatPlayer)
    } else if word(0, "that") && word(1, "opponent") {
        Some(PlayerAnaphor::ThatOpponent)
    } else if word(0, "the")
        && word(1, "player")
        && !["who", "with", "whose", "that", "to", "on", "of"]
            .iter()
            .any(|word_after| word(2, word_after))
    {
        Some(PlayerAnaphor::ThePlayer)
    } else {
        None
    }
}

/// The written token reclassified as `word` by the shared tokenizer.
fn reclassify(token: &mut crate::oracle::model::RecognizedEntity, word: &str) {
    if let Some(class) = crate::oracle::syntax::recognize_entities(word)
        .into_iter()
        .next()
    {
        token.category = class.category;
        token.token_type = class.token_type;
    }
    token.raw = word.to_string();
}

/// `defending player` in an attack trigger (CR 506.2), resolved by the shared
/// binding environment.
pub(in crate::oracle::canonical::automata) fn recognize_defending_player_subject(
    cursor: &mut OracleTokenCursor<'_>,
    available: bool,
) -> MachineResult<PlayerEffectSubject> {
    recognize_defending_player(cursor, available).map(|reference| {
        reference
            .into_effect_subject()
            .expect("`defending player` is the event's player")
    })
}

/// Reads an action's operands with its acting player installed: the
/// distributive `they` / `that player` without another antecedent names
/// the player of the action being read (`BindingEnvironment::acting_player`).
/// The parent calls it only when that player is not the ability's
/// controller and its lowering substitutes the player slot.
pub(in crate::oracle::canonical::automata) fn with_acting_player<T>(
    cursor: &mut OracleTokenCursor<'_>,
    read: impl FnOnce(&mut OracleTokenCursor<'_>) -> T,
) -> T {
    let mut environment = installed_binding_environment(cursor);
    environment.acting_player = true;
    let enclosing = cursor.replace_binding_environment(Some(std::sync::Arc::new(environment)));
    let result = read(cursor);
    cursor.replace_binding_environment(enclosing);
    result
}

/// The objects whose controller and owner the runtime reads, live or as
/// last known information (CR 608.2h): a declared target, the source, its
/// attached permanent, the event's permanent, the entering permanent, the
/// other combatant of a block, or a result that is still a permanent.
fn relation_readable(reference: PermanentEffectReference) -> bool {
    match reference {
        PermanentEffectReference::Observed(observed) => matches!(
            observed,
            super::super::super::TypedObservedEventObject::EventPermanent
                | super::super::super::TypedObservedEventObject::EnteringPermanent
                | super::super::super::TypedObservedEventObject::CombatCounterpart
        ),
        PermanentEffectReference::TargetSet(_) => false,
        PermanentEffectReference::ActionResult(result) => result.permanent,
        _ => true,
    }
}

/// The object of `its|that NOUN's controller|owner` (CR 108.3, 109.4) when
/// no lexical antecedent proves it, named by the shared resolvers of the
/// binding environment the program installed: `its` is the event's subject
/// (`whenever enchanted creature attacks, its controller ...`, the subject
/// a bare `it` names); `that NOUN's` is the object the nominal anaphor `that
/// NOUN` names, read for its last known information (`whenever this
/// creature blocks a creature, that creature's controller ...` names the
/// other combatant). A lexical antecedent that has the nominal keeps the
/// ordinary reading; `None` leaves the caller's diagnostic.
pub(in crate::oracle::canonical::automata) fn unbound_relation_object(
    cursor: &OracleTokenCursor<'_>,
    antecedent: Option<&ObjectSubjectAntecedent>,
    nominal: Option<super::super::super::TargetNominal>,
) -> Option<PermanentEffectReference> {
    let environment = installed_binding_environment(cursor);
    let Some(nominal) = nominal else {
        if antecedent.is_some() || environment.local_object_scope {
            return None;
        }
        return environment
            .frame
            .event_object
            .as_ref()
            .filter(|slot| slot.subject && !slot.departed)
            .map(|slot| slot.reference)
            .filter(|reference| relation_readable(*reference));
    };
    if antecedent.is_some_and(|antecedent| {
        antecedent.selector.as_ref().is_some_and(|selector| {
            super::super::super::selector_names_nominals(selector, &[nominal])
        })
    }) {
        return None;
    }
    super::super::super::resolve_nominal_antecedent(&environment, &[nominal], true)
        .ok()
        .filter(|reference| relation_readable(*reference))
}

/// `its controller|owner` / `that NOUN's controller|owner` (CR 108.3,
/// 109.4), resolved by the shared binding environment.
pub(in crate::oracle::canonical::automata) fn recognize_object_related_player_subject(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<ObjectSubjectAntecedent>,
) -> MachineResult<PlayerEffectSubject> {
    // The frame's event object is read from the environment the enclosing
    // program installed; the lexical antecedent is the caller's, else the
    // singular object that environment publishes.
    let environment = installed_binding_environment(cursor);
    let antecedent = antecedent.or(environment.object);
    let reference = recognize_object_related_player(
        cursor,
        antecedent.as_ref(),
        environment.frame.event_object.as_ref(),
        false,
    )?;
    reference
        .into_effect_subject()
        .ok_or_else(|| cursor.fail("a player relation with an effect-subject lowering"))
}

/// Recognizes a player acting as the subject of an effect. `that player`
/// resolves only to the exact binding supplied by the parent, or, without a
/// local antecedent, to the player of a triggering event that names one
/// (`event_player`); it never opens a new target declaration implicitly.
pub(in crate::oracle::canonical::automata) fn recognize_player_effect_subject(
    cursor: &mut OracleTokenCursor<'_>,
    declared_target_binding: PlayerBinding,
    antecedent: Option<PlayerBinding>,
    event_player: bool,
) -> MachineResult<PlayerEffectSubject> {
    cursor.enter("player-subject.start");
    let Some(first) = cursor.peek_token().cloned() else {
        return Err(cursor.fail("`you`, a player target, or bound `that player`"));
    };

    if first.raw.eq_ignore_ascii_case("you") {
        cursor.enter("player-subject.controller");
        cursor.consume("you")?;
        cursor.enter("player-subject.complete");
        return Ok(PlayerEffectSubject::Controller);
    }

    if first.raw.eq_ignore_ascii_case("target") {
        cursor.enter("player-subject.target");
        let target = recognize_target_list(cursor)?;
        cursor.enter("player-subject.target.validation");
        cursor.require(
            matches!(
                target.slots.as_slice(),
                [slot]
                    if matches!(&slot.selector, TargetSelector::Player(_))
                        && slot.cardinality == TargetCardinality::EXACTLY_ONE
            ),
            "exactly one target player or opponent",
        )?;
        cursor.enter("player-subject.complete");
        return Ok(PlayerEffectSubject::DeclaredTarget {
            target,
            binding: declared_target_binding,
        });
    }

    if let Some(written) = player_anaphor_at(cursor) {
        // The environment the enclosing program installed (its frame, the
        // event object whose controller `that player` names (CR 109.4), and
        // the player antecedents it publishes), with the bindings this
        // caller proved. A lexical object antecedent stays with the clause
        // layer's performer slot.
        let mut environment = installed_binding_environment(cursor);
        environment.object = None;
        // The subject names the acting player; it is never its own operand.
        environment.acting_player = false;
        environment.frame.event_player |= event_player;
        if antecedent.is_some() {
            environment.player = antecedent;
        }
        // `the player` / `that opponent` name the same antecedent as `that
        // player` (the opponent being one the antecedent already is): the
        // shared resolver reads them over a view of the same tokens whose
        // words are `that player` (equal length, indices and spans).
        let reference = match written {
            PlayerAnaphor::ThatPlayer => recognize_that_player(cursor, &environment)?,
            PlayerAnaphor::ThePlayer | PlayerAnaphor::ThatOpponent => {
                let mut tokens = cursor.token_slice(0, usize::MAX).to_vec();
                let at = cursor.position();
                reclassify(&mut tokens[at], "that");
                reclassify(&mut tokens[at + 1], "player");
                let mut view = cursor.with_tokens(&tokens);
                let reference = recognize_that_player(&mut view, &environment)?;
                cursor.advance_to(view.position())?;
                reference
            }
        };
        return reference
            .into_effect_subject()
            .ok_or_else(|| cursor.fail("a player relation with an effect-subject lowering"));
    }

    Err(cursor.fail_with_span(
        "`you`, a player target, or bound `that player`",
        first.index,
        first.span_start,
        first.span_end,
        first.raw,
    ))
}
