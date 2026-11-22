//! One binding environment for references (`docs/oracle-binding-environment.md`).
//!
//! The ability frame publishes what its event names (the event player, the
//! defending player, the other combatant); the effect program adds the lexical
//! antecedents of the position being read (the singular object and the player
//! of the previous actions). Every reference child resolves `that player`,
//! `defending player`, `its controller`, `that NOUN's owner`, `it` and `that
//! NOUN` against this one environment instead of a parameter of its own.
//! Nothing is guessed: an absent slot is a committed diagnostic.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectRecognitionContext, ObjectPlayerRelation, ObjectSubjectAntecedent,
    PermanentEffectReference, PermanentSubjectScope, PlayerBinding, PlayerEffectSubject,
    TargetNominal, TypedObservedEventObject, selector_implies_nominal,
};
use crate::oracle::characteristic_vocabulary::{
    CharacteristicLexeme, lookup_object_selector_lexeme,
};

/// The references the ability frame binds from its typed event. Only the
/// trigger parent sets them; every other frame leaves them unbound.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct FrameBindings {
    /// The event records exactly one player (`that player`, CR 603.2).
    pub event_player: bool,
    /// The event is an attack that records the attacked player (CR 506.2).
    pub defending_player: bool,
    /// The event is a block whose every branch names the other combatant.
    pub combat_counterpart: bool,
}

impl FrameBindings {
    pub(in crate::oracle::canonical) fn from_context(context: &EffectRecognitionContext) -> Self {
        Self {
            event_player: context.event_player,
            defending_player: context.defending_player,
            combat_counterpart: context.combat_counterpart,
        }
    }
}

/// The frame bindings plus the lexical antecedents at one program position.
#[derive(Clone, Debug, Default)]
pub(in crate::oracle::canonical) struct BindingEnvironment {
    pub frame: FrameBindings,
    /// A previous action opened a lexical object scope (even an exhausted
    /// one); the frame's event objects are then no longer the default.
    pub local_object_scope: bool,
    /// The singular object `it` / `that NOUN` / `its controller` names.
    pub object: Option<ObjectSubjectAntecedent>,
    /// The player an earlier `target player` declared (`that player`).
    pub player: Option<PlayerBinding>,
}

impl BindingEnvironment {
    /// The environment of a reference read directly by a context parent: its
    /// frame bindings and its proven permanent antecedent.
    pub(in crate::oracle::canonical) fn for_permanent_context(
        context: &EffectRecognitionContext,
    ) -> Self {
        Self {
            frame: FrameBindings::from_context(context),
            local_object_scope: matches!(
                context.permanent_subject_scope,
                PermanentSubjectScope::Local(_)
            ),
            object: super::permanent_subject::proven_permanent_antecedent(context).map(
                |reference| ObjectSubjectAntecedent {
                    reference,
                    selector: match reference {
                        PermanentEffectReference::Target(_) => {
                            context.permanent_subject_selector.clone()
                        }
                        _ => None,
                    },
                },
            ),
            player: None,
        }
    }
}

/// A player named through the environment, never a new declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum BoundPlayerReference {
    /// `that player` after an earlier `target player` of the same program.
    Bound(PlayerBinding),
    /// `that player` / `defending player`: the player the event records.
    EventPlayer,
    /// `its controller` / `that NOUN's owner` (CR 108.3, 109.4).
    ObjectRelation {
        relation: ObjectPlayerRelation,
        object: PermanentEffectReference,
    },
}

impl From<BoundPlayerReference> for PlayerEffectSubject {
    fn from(reference: BoundPlayerReference) -> Self {
        match reference {
            BoundPlayerReference::Bound(binding) => Self::Bound(binding),
            BoundPlayerReference::EventPlayer => Self::EventPlayer,
            BoundPlayerReference::ObjectRelation { relation, object } => {
                Self::ObjectRelation { relation, object }
            }
        }
    }
}

fn player_relation(raw: &str) -> Option<ObjectPlayerRelation> {
    if raw.eq_ignore_ascii_case("controller") {
        Some(ObjectPlayerRelation::Controller)
    } else if raw.eq_ignore_ascii_case("owner") {
        Some(ObjectPlayerRelation::Owner)
    } else {
        None
    }
}

/// A registered card type or object domain nominal (`creature`, `land`,
/// `permanent`, `spell`), read from a bare or possessive surface.
fn registered_nominal(surface: &str) -> Option<TargetNominal> {
    match lookup_object_selector_lexeme(surface) {
        Some(CharacteristicLexeme::CardType(value)) => Some(TargetNominal::CardType(value)),
        Some(CharacteristicLexeme::ObjectDomain(value)) => Some(TargetNominal::Domain(value)),
        _ => None,
    }
}

fn possessive_base(raw: &str) -> Option<String> {
    let normalized = raw.to_ascii_lowercase();
    normalized
        .strip_suffix("'s")
        .or_else(|| normalized.strip_suffix("\u{2019}s"))
        .map(str::to_string)
}

pub(in crate::oracle::canonical) fn defending_player_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("defending"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("player"))
}

fn that_player_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("player"))
}

/// Bounded lexical head: `its controller|owner` or `that NOUN's
/// controller|owner`. Returns the token length; it proves no binding.
pub(in crate::oracle::canonical) fn object_related_player_head_length(
    cursor: &OracleTokenCursor<'_>,
) -> Option<usize> {
    let first = cursor.peek()?;
    if first.eq_ignore_ascii_case("its") {
        return player_relation(cursor.peek_ahead(1)?).map(|_| 2);
    }
    if first.eq_ignore_ascii_case("that")
        && cursor
            .token_slice(cursor.position() + 1, cursor.position() + 2)
            .first()
            .is_some_and(|token| token.token_type == "possessiveTypeWord")
    {
        return player_relation(cursor.peek_ahead(2)?).map(|_| 3);
    }
    None
}

/// Lexical lookahead for any player reference this resolver owns.
pub(in crate::oracle::canonical) fn bound_player_reference_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    defending_player_starts(cursor)
        || that_player_starts(cursor)
        || object_related_player_head_length(cursor).is_some()
}

/// Resolves `defending player`, `that player`, `its controller|owner` and
/// `that NOUN's controller|owner` against the environment.
pub(in crate::oracle::canonical) fn recognize_bound_player_reference(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<BoundPlayerReference> {
    if defending_player_starts(cursor) {
        return recognize_defending_player(cursor, environment.frame.defending_player);
    }
    if that_player_starts(cursor) {
        return recognize_that_player(cursor, environment);
    }
    recognize_object_related_player(cursor, environment.object.as_ref())
}

/// `defending player` in an attack trigger (CR 506.2): the attacked player
/// the event records. Elsewhere it has no binding and fails explicitly.
pub(in crate::oracle::canonical) fn recognize_defending_player(
    cursor: &mut OracleTokenCursor<'_>,
    available: bool,
) -> MachineResult<BoundPlayerReference> {
    cursor.enter("player-subject.defending-player");
    let first = cursor
        .peek_token()
        .cloned()
        .ok_or_else(|| cursor.fail("`defending player`"))?;
    cursor.consume_phrase(&["defending", "player"])?;
    if !available {
        return Err(cursor.fail_with_span(
            "an attack trigger event that records the defending player",
            first.index,
            first.span_start,
            first.span_end,
            first.raw,
        ));
    }
    cursor.enter("player-subject.complete");
    Ok(BoundPlayerReference::EventPlayer)
}

/// `that player`: the exact earlier player binding of the program, or,
/// without one, the player of a triggering event that names one. It never
/// opens a new target declaration.
pub(in crate::oracle::canonical) fn recognize_that_player(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<BoundPlayerReference> {
    let first = cursor
        .peek_token()
        .cloned()
        .ok_or_else(|| cursor.fail("`that player`"))?;
    cursor.enter("player-subject.bound.reference");
    cursor.consume_phrase(&["that", "player"])?;
    cursor.enter("player-subject.bound.resolve");
    if let Some(binding) = environment.player {
        cursor.enter("player-subject.complete");
        return Ok(BoundPlayerReference::Bound(binding));
    }
    if environment.frame.event_player {
        cursor.enter("player-subject.event-player");
        cursor.enter("player-subject.complete");
        return Ok(BoundPlayerReference::EventPlayer);
    }
    Err(cursor.fail_with_span(
        "a prior player binding for `that player`",
        first.index,
        first.span_start,
        first.span_end,
        first.raw,
    ))
}

/// `its controller|owner` / `that NOUN's controller|owner` (CR 108.3,
/// 109.4). The object is the environment's antecedent; a nominal must follow
/// from the selector that declared that target. Nothing is guessed.
pub(in crate::oracle::canonical) fn recognize_object_related_player(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<&ObjectSubjectAntecedent>,
) -> MachineResult<BoundPlayerReference> {
    cursor.enter("player-subject.object-relation");
    let first = cursor
        .peek_token()
        .cloned()
        .ok_or_else(|| cursor.fail("`its` or `that NOUN's`"))?;
    let nominal = if first.raw.eq_ignore_ascii_case("its") {
        cursor.consume("its")?;
        None
    } else {
        cursor.consume("that")?;
        cursor.enter("player-subject.object-relation.nominal");
        let possessive = cursor.consume_type("possessiveTypeWord")?;
        let surface = possessive_base(&possessive.raw)
            .ok_or_else(|| cursor.fail("a possessive registered object nominal"))?;
        Some(
            registered_nominal(&surface)
                .ok_or_else(|| cursor.fail("a registered card type or object domain nominal"))?,
        )
    };
    cursor.enter("player-subject.object-relation.relation");
    let relation = cursor
        .peek()
        .and_then(player_relation)
        .ok_or_else(|| cursor.fail("`controller` or `owner`"))?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("player-subject.object-relation.antecedent");
    let antecedent = antecedent.ok_or_else(|| {
        cursor.fail_with_span(
            "a preceding singular object binding for the possessive",
            first.index,
            first.span_start,
            first.span_end,
            first.raw.clone(),
        )
    })?;
    // Event-observed objects have no runtime player-relation contract yet.
    cursor.require(
        !matches!(antecedent.reference, PermanentEffectReference::Observed(_)),
        "a declared target, the source, or its attached permanent as the possessive antecedent",
    )?;
    if let Some(nominal) = nominal {
        cursor.require(
            antecedent.selector.as_ref().is_some_and(|selector| {
                selector_implies_nominal(selector, &nominal.as_predicate())
            }),
            "a nominal implied by the declared selector of the preceding target",
        )?;
    }
    cursor.enter("player-subject.complete");
    Ok(BoundPlayerReference::ObjectRelation {
        relation,
        object: antecedent.reference,
    })
}

/// Lexical lookahead for `it` / `that NOUN` (a registered nominal that is
/// not possessive). It proves no binding.
pub(in crate::oracle::canonical) fn bound_object_reference_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(super::reference::is_source_anaphor)
        || cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
            && cursor
                .token_slice(cursor.position() + 1, cursor.position() + 2)
                .first()
                .is_some_and(|token| {
                    token.token_type != "possessiveTypeWord"
                        && registered_nominal(&token.raw).is_some()
                })
}

/// Resolves `it` / `he` / `she` or `that NOUN` against the environment.
pub(in crate::oracle::canonical) fn recognize_bound_object_reference(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<PermanentEffectReference> {
    if cursor
        .peek()
        .is_some_and(super::reference::is_source_anaphor)
    {
        let reference = environment
            .object
            .as_ref()
            .map(|object| object.reference)
            .ok_or_else(|| {
                cursor.fail(
                "a source binding or preceding singular permanent subject in this lexical scope",
            )
            })?;
        let pronoun = cursor.peek().expect("checked object anaphor").to_owned();
        cursor.consume(&pronoun)?;
        return Ok(reference);
    }
    recognize_nominal_object_reference(cursor, environment)
}

/// `that creature` / `that permanent`: a nominal anaphor. Without a lexical
/// antecedent a block event's other combatant is the creature named (every
/// blocker and blocked object is a creature); otherwise the nominal must
/// follow from the selector that declared the antecedent target.
pub(in crate::oracle::canonical) fn recognize_nominal_object_reference(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<PermanentEffectReference> {
    cursor.enter("permanent.subject.nominal-anaphor");
    cursor.consume("that")?;
    cursor.enter("permanent.subject.nominal-anaphor.nominal");
    let noun = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a registered card type or permanent nominal"))?
        .to_owned();
    let nominal = registered_nominal(&noun)
        .ok_or_else(|| cursor.fail("a registered card type or permanent nominal"))?;
    cursor.enter("permanent.subject.nominal-anaphor.antecedent");
    if environment.frame.combat_counterpart
        && !environment.local_object_scope
        && nominal.as_predicate()
            == super::ObjectPredicate::CardType(super::ObjectCardType::Creature)
    {
        cursor.consume(&noun)?;
        return Ok(PermanentEffectReference::Observed(
            TypedObservedEventObject::CombatCounterpart,
        ));
    }
    let antecedent = environment.object.as_ref().ok_or_else(|| {
        cursor.fail("a preceding singular permanent subject in this lexical scope")
    })?;
    let PermanentEffectReference::Target(_) = antecedent.reference else {
        return Err(cursor.fail("a declared target as the nominal anaphor antecedent"));
    };
    let selector = antecedent
        .selector
        .as_ref()
        .ok_or_else(|| cursor.fail("the declared selector of the preceding permanent target"))?;
    cursor.require(
        selector_implies_nominal(selector, &nominal.as_predicate()),
        "a nominal implied by the preceding target's declared selector",
    )?;
    cursor.consume(&noun)?;
    Ok(antecedent.reference)
}

/// The environment the parent installed. A parent that installs none binds
/// nothing, so every reference then fails explicitly.
pub(in crate::oracle::canonical) fn installed_binding_environment(
    cursor: &OracleTokenCursor<'_>,
) -> BindingEnvironment {
    cursor.binding_environment().cloned().unwrap_or_default()
}

#[cfg(test)]
#[path = "binding_environment_tests.rs"]
mod tests;
