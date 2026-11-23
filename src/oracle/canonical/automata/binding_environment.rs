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
    CardZoneName, EffectRecognitionContext, NumericExpression, ObjectPlayerRelation,
    ObjectPredicate, ObjectSubjectAntecedent, PermanentEffectReference, PermanentSubjectScope,
    PlayerBinding, PlayerEffectSubject, SelectorNumericProperty, TargetNominal, TargetSelector,
    TypedObservedEventObject, selector_implies_nominal,
};
use crate::oracle::characteristic_vocabulary::{
    CharacteristicLexeme, lookup_object_selector_lexeme,
};
use crate::oracle::model::RecognizedEntity;

/// The object an event record binds, with the predicate the event wrote for
/// it. A nominal anaphor (`that creature`) must follow from that predicate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EventObjectSlot {
    pub reference: PermanentEffectReference,
    pub predicate: ObjectPredicate,
    /// The object is the grammatical subject of the event (`a creature
    /// attacks`, `a creature dies`), so a bare `it` can name it. A damaged
    /// recipient is named only with its nominal (`that creature`).
    pub subject: bool,
    /// The event moved the object off the battlefield (`a creature dies`):
    /// its last known information still answers property and player reads,
    /// but it is no longer a permanent an action can operate on (CR 400.7,
    /// 700.4). The card it became is the frame's `event_card`.
    pub departed: bool,
}

impl EventObjectSlot {
    fn implies(&self, nominal: TargetNominal) -> bool {
        selector_implies_nominal(
            &TargetSelector::Object(self.predicate.clone()),
            &nominal.as_predicate(),
        )
    }
}

/// The references the ability frame binds from its typed event. Only the
/// trigger parent sets them; every other frame leaves them unbound.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct FrameBindings {
    /// The event records exactly one player (`that player`, CR 603.2).
    pub event_player: bool,
    /// The event is an attack that records the attacked player (CR 506.2).
    pub defending_player: bool,
    /// The event is a block whose every branch names the other combatant.
    pub combat_counterpart: bool,
    /// The object the event record binds (`that creature`, `that land's
    /// controller` when no lexical antecedent has that nominal).
    pub event_object: Option<EventObjectSlot>,
    /// The event is the cast of one spell (`that spell`).
    pub event_spell: bool,
    /// The event moved its object into this public zone, where the ability
    /// can still find the card it became (CR 400.7, 603.6c): `it` / `that
    /// card` in a card action when the program observed no other card.
    pub event_card: Option<CardZoneName>,
}

impl FrameBindings {
    pub(in crate::oracle::canonical) fn from_context(context: &EffectRecognitionContext) -> Self {
        context.frame.clone()
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
    /// The subject of the clause being read (the object that deals damage):
    /// a possessive `its` in its arguments names it. Installed by the clause
    /// parent around its arguments only.
    pub subject: Option<PermanentEffectReference>,
    /// The index of the action being read in the enclosing program; a
    /// target declared by this action binds to it.
    pub effect_index: Option<usize>,
    /// The ActionResult antecedent: the preceding instruction or resolution
    /// cost whose actual performance an outcome connector (`If you do`,
    /// `If they don't`, `When you do`) observes. Installed by the program
    /// parent around the connector only (CR 608.2c, 603.12).
    pub action_result: Option<ActionResultSlot>,
    /// A number the enclosing frame already read (`that number` in a later
    /// clause of the same frame); the arithmetic grammar reads it as a
    /// primary.
    pub number: Option<NumericExpression>,
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
            object: super::permanent_subject::proven_read_antecedent(context).map(|reference| {
                ObjectSubjectAntecedent {
                    reference,
                    selector: match reference {
                        PermanentEffectReference::Target(_)
                        | PermanentEffectReference::Observed(
                            TypedObservedEventObject::MovedOntoBattlefield,
                        ) => context.permanent_subject_selector.clone(),
                        _ => None,
                    },
                }
            }),
            player: None,
            subject: None,
            effect_index: context.current_effect_index,
            action_result: None,
            number: None,
        }
    }
}

/// Reads with `environment` installed on the cursor, then restores the
/// enclosing environment. Every slot installer of this module uses it.
fn with_installed_environment<T>(
    cursor: &mut OracleTokenCursor<'_>,
    environment: BindingEnvironment,
    read: impl FnOnce(&mut OracleTokenCursor<'_>) -> T,
) -> T {
    let enclosing = cursor.replace_binding_environment(Some(std::sync::Arc::new(environment)));
    let result = read(cursor);
    cursor.replace_binding_environment(enclosing);
    result
}

/// What an outcome connector observes about its producer (CR 608.2c,
/// 603.12): whether the preceding instruction or cost was actually
/// performed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PerformanceOutcome {
    /// `If you do, …`: the body resolves only if the producer was performed.
    Performed,
    /// `If you don't, …`: the body resolves only if it was not performed.
    Declined,
    /// `When you do, …`: performing it creates a reflexive triggered ability
    /// that chooses its own targets (CR 603.12).
    Reflexive,
}

/// Who performs the producer an outcome connector names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum OutcomePerformer {
    /// `you may …`: the ability controller (`If you do`).
    Controller,
    /// `that player may …`: another player the environment names (`If they
    /// do`, `If that player does`, `If the player does`).
    OtherPlayer,
    /// A mandatory instruction: its performer is the instruction's own
    /// subject, which the connector's subject does not restate.
    Instruction,
}

/// The ActionResult antecedent of an outcome connector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActionResultSlot {
    pub performer: OutcomePerformer,
}

/// The grammatical subject of an outcome connector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConnectorSubject {
    You,
    OtherPlayer,
}

impl ActionResultSlot {
    fn admits(self, subject: ConnectorSubject) -> bool {
        match self.performer {
            OutcomePerformer::Controller => subject == ConnectorSubject::You,
            OutcomePerformer::OtherPlayer => subject == ConnectorSubject::OtherPlayer,
            OutcomePerformer::Instruction => true,
        }
    }
}

/// Lexical reading of `if|when SUBJECT do(es)|don't|doesn't ,` at `offset`:
/// the outcome, its subject and the connector's token width. It proves no
/// binding; `recognize_outcome_connector` resolves the subject.
fn outcome_connector_parts(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(PerformanceOutcome, ConnectorSubject, usize)> {
    let word = |index: usize| {
        cursor
            .peek_ahead(offset + index)
            .map(str::to_ascii_lowercase)
    };
    // The connector's subject is the producer's performer: `you`, or another
    // player named by `they` / `that player` / `the player` (CR 608.2c).
    let (subject, subject_width, singular) = match (word(1)?.as_str(), word(2).as_deref()) {
        ("you", _) => (ConnectorSubject::You, 1, false),
        ("they", _) => (ConnectorSubject::OtherPlayer, 1, false),
        ("that" | "the", Some("player")) => (ConnectorSubject::OtherPlayer, 2, true),
        _ => return None,
    };
    let verb = word(1 + subject_width)?;
    let outcome = match (word(0)?.as_str(), verb.as_str(), singular) {
        ("if", "do", false) | ("if", "does", true) => PerformanceOutcome::Performed,
        ("if", "don't" | "don’t", false) | ("if", "doesn't" | "doesn’t", true) => {
            PerformanceOutcome::Declined
        }
        ("when", "do", false) | ("when", "does", true) => PerformanceOutcome::Reflexive,
        _ => return None,
    };
    let width = 2 + subject_width;
    (word(width)?.as_str() == ",").then_some((outcome, subject, width + 1))
}

/// Lexical lookahead: the outcome and token width of a connector at
/// `offset`. It commits nothing.
pub(in crate::oracle::canonical) fn outcome_connector_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(PerformanceOutcome, usize)> {
    outcome_connector_parts(cursor, offset).map(|(outcome, _, width)| (outcome, width))
}

/// True when the first sentence after the cursor is followed by an outcome
/// connector. Lexical lookahead only; it commits nothing.
pub(in crate::oracle::canonical) fn outcome_connector_follows_first_sentence(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let mut offset = 0;
    while let Some(raw) = cursor.peek_ahead(offset) {
        if raw == "." {
            return outcome_connector_at(cursor, offset + 1).is_some();
        }
        offset += 1;
    }
    false
}

/// Reads `If you do ,` / `If they don't ,` / `When that player does ,` at a
/// sentence start and resolves it against the environment's ActionResult
/// antecedent: the connector's subject must name the producer's performer.
pub(in crate::oracle::canonical) fn recognize_outcome_connector(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PerformanceOutcome> {
    cursor.enter("cost.outcome.connector");
    let (outcome, subject, width) = outcome_connector_parts(cursor, 0)
        .ok_or_else(|| cursor.fail("`If you do,`, `If you don't,` or `When you do,`"))?;
    cursor.enter("binding.action-result.performer");
    let slot = installed_binding_environment(cursor)
        .action_result
        .ok_or_else(|| cursor.fail("a preceding instruction or cost for the outcome connector"))?;
    cursor.require(
        slot.admits(subject),
        "an outcome connector whose subject names the performer of the preceding instruction",
    )?;
    cursor.advance_to(cursor.position() + width)?;
    Ok(outcome)
}

/// Reads an outcome connector with `slot` as the ActionResult antecedent.
pub(in crate::oracle::canonical) fn recognize_outcome_connector_after(
    cursor: &mut OracleTokenCursor<'_>,
    slot: ActionResultSlot,
) -> MachineResult<PerformanceOutcome> {
    let mut environment = installed_binding_environment(cursor);
    environment.action_result = Some(slot);
    with_installed_environment(cursor, environment, recognize_outcome_connector)
}

/// Lexical lookahead for `that number`.
pub(in crate::oracle::canonical) fn prior_number_reference_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("number"))
}

/// `that number`: the number the enclosing frame already read (a power the
/// toughness reuses). An absent binding fails; it never becomes a literal.
pub(in crate::oracle::canonical) fn recognize_prior_number_reference(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.prior-number-anaphor.demonstrative");
    cursor.consume_phrase(&["that", "number"])?;
    cursor.enter("numeric.prior-number-anaphor.resolve");
    installed_binding_environment(cursor)
        .number
        .ok_or_else(|| cursor.fail("a prior numeric-expression binding for `that number`"))
}

/// Reads with `number` as the environment's prior number.
pub(in crate::oracle::canonical) fn with_prior_number<T>(
    cursor: &mut OracleTokenCursor<'_>,
    number: NumericExpression,
    read: impl FnOnce(&mut OracleTokenCursor<'_>) -> T,
) -> T {
    let mut environment = installed_binding_environment(cursor);
    environment.number = Some(number);
    with_installed_environment(cursor, environment, read)
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
    /// `that spell's controller`: the controller of the spell the cast
    /// record binds (CR 110.2), while it is on the stack, else its caster.
    EventSpellController,
}

impl BoundPlayerReference {
    /// The effect-subject form of the reference; a spell's controller has no
    /// effect-subject lowering yet.
    pub(in crate::oracle::canonical) fn into_effect_subject(self) -> Option<PlayerEffectSubject> {
        match self {
            Self::Bound(binding) => Some(PlayerEffectSubject::Bound(binding)),
            Self::EventPlayer => Some(PlayerEffectSubject::EventPlayer),
            Self::ObjectRelation { relation, object } => {
                Some(PlayerEffectSubject::ObjectRelation { relation, object })
            }
            Self::EventSpellController => None,
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

/// The registered nominal of a possessive word (`creature's`, `spell’s`):
/// the one genitive-nominal reader every reference child shares.
pub(in crate::oracle::canonical) fn possessive_nominal(raw: &str) -> Option<TargetNominal> {
    possessive_base(raw).and_then(|surface| registered_nominal(&surface))
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

/// Bounded lexical head over tokens: `its controller|owner` or `that NOUN's
/// controller|owner`, with its token length and relation. It proves no
/// binding; the resolver below does.
pub(in crate::oracle::canonical) fn object_related_player_head(
    tokens: &[RecognizedEntity],
) -> Option<(usize, ObjectPlayerRelation)> {
    let raw = |index: usize| tokens.get(index).map(|token| token.raw.as_str());
    let first = raw(0)?;
    if first.eq_ignore_ascii_case("its") {
        return player_relation(raw(1)?).map(|relation| (2, relation));
    }
    if first.eq_ignore_ascii_case("that")
        && tokens
            .get(1)
            .is_some_and(|token| token.token_type == "possessiveTypeWord")
    {
        return player_relation(raw(2)?).map(|relation| (3, relation));
    }
    None
}

/// The token length of an object-related player head at the cursor.
pub(in crate::oracle::canonical) fn object_related_player_head_length(
    cursor: &OracleTokenCursor<'_>,
) -> Option<usize> {
    object_related_player_head(cursor.token_slice(cursor.position(), cursor.position() + 3))
        .map(|(length, _)| length)
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
    recognize_object_related_player(
        cursor,
        environment.object.as_ref(),
        environment.frame.event_object.as_ref(),
        environment.frame.event_spell,
    )
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
    event_object: Option<&EventObjectSlot>,
    event_spell: bool,
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
    // `that spell's controller`: the spell the cast record binds.
    if event_spell
        && nominal == Some(TargetNominal::Domain(super::ObjectKind::Spell))
        && relation == ObjectPlayerRelation::Controller
    {
        cursor.enter("player-subject.complete");
        return Ok(BoundPlayerReference::EventSpellController);
    }
    // `that NOUN's`: a lexical antecedent with that nominal, else the event's
    // object when the event wrote that nominal for it.
    if let Some(nominal) = nominal
        && !antecedent.is_some_and(|antecedent| lexical_antecedent_implies(antecedent, nominal))
        && let Some(slot) = event_object.filter(|slot| slot.implies(nominal))
        && relation_supported(slot.reference)
    {
        cursor.enter("player-subject.complete");
        return Ok(BoundPlayerReference::ObjectRelation {
            relation,
            object: slot.reference,
        });
    }
    // `its controller` of a departed event subject (`a creature dies`): the
    // object is no permanent antecedent any more, but its last known
    // controller and owner still answer the relation (CR 608.2h).
    if nominal.is_none()
        && antecedent.is_none()
        && let Some(slot) = event_object.filter(|slot| slot.subject && slot.departed)
        && relation_supported(slot.reference)
    {
        cursor.enter("player-subject.complete");
        return Ok(BoundPlayerReference::ObjectRelation {
            relation,
            object: slot.reference,
        });
    }
    let antecedent = antecedent.ok_or_else(|| {
        cursor.fail_with_span(
            "a preceding singular object binding for the possessive",
            first.index,
            first.span_start,
            first.span_end,
            first.raw.clone(),
        )
    })?;
    cursor.require(
        relation_supported(antecedent.reference),
        "a declared target, the source, its attached permanent or an event object as the possessive antecedent",
    )?;
    if let Some(nominal) = nominal {
        cursor.require(
            lexical_antecedent_implies(antecedent, nominal),
            "a nominal implied by the declared selector of the preceding target",
        )?;
    }
    cursor.enter("player-subject.complete");
    Ok(BoundPlayerReference::ObjectRelation {
        relation,
        object: antecedent.reference,
    })
}

/// The objects whose controller and owner the runtime reads, live or as
/// last known information: declared targets, the source, its attached
/// permanent, the record's permanent and the entering one.
fn relation_supported(reference: PermanentEffectReference) -> bool {
    match reference {
        PermanentEffectReference::Observed(observed) => matches!(
            observed,
            TypedObservedEventObject::EventPermanent | TypedObservedEventObject::EnteringPermanent
        ),
        _ => true,
    }
}

fn lexical_antecedent_implies(
    antecedent: &ObjectSubjectAntecedent,
    nominal: TargetNominal,
) -> bool {
    antecedent
        .selector
        .as_ref()
        .is_some_and(|selector| selector_implies_nominal(selector, &nominal.as_predicate()))
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
    recognize_determined_object_reference(cursor, environment, "that", false)
}

/// `that creature` read for its last known information (`If that creature
/// was a Villain`, CR 608.2h): unlike an operand, it may name an event
/// object that left the battlefield.
pub(in crate::oracle::canonical) fn recognize_nominal_object_read(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<PermanentEffectReference> {
    recognize_determined_object_reference(cursor, environment, "that", true)
}

/// `the creature` (a definite description, CR 702.32a style): it names the
/// same antecedent `that creature` would, validated against the same
/// declared selector; it never names a new object.
pub(in crate::oracle::canonical) fn recognize_definite_object_reference(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<PermanentEffectReference> {
    recognize_determined_object_reference(cursor, environment, "the", false)
}

fn recognize_determined_object_reference(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
    determiner: &str,
    departed_readable: bool,
) -> MachineResult<PermanentEffectReference> {
    cursor.enter("permanent.subject.nominal-anaphor");
    cursor.consume(determiner)?;
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
    // A lexical target with this nominal wins; otherwise the event's object
    // when the event wrote this nominal for it.
    let lexical = environment.object.as_ref().filter(|antecedent| {
        matches!(antecedent.reference, PermanentEffectReference::Target(_))
            && lexical_antecedent_implies(antecedent, nominal)
    });
    if lexical.is_none()
        && let Some(slot) = environment
            .frame
            .event_object
            .as_ref()
            .filter(|slot| slot.implies(nominal))
    {
        // An operand reader acts on the permanent (tap, destroy, deal damage
        // to). A departed event object (`a creature dies`) is a new object in
        // another zone (CR 400.7); only a read of its last known information
        // (a condition, a property, its controller) may name it.
        if slot.departed && !departed_readable {
            return Err(cursor.fail(
                "a permanent for the nominal anaphor; the event's object left the battlefield",
            ));
        }
        cursor.consume(&noun)?;
        return Ok(slot.reference);
    }
    let antecedent = environment.object.as_ref().ok_or_else(|| {
        cursor.fail("a preceding singular permanent subject in this lexical scope")
    })?;
    // A declared target, or the card a move put onto the battlefield (its
    // declared card predicate is the selector the nominal must follow from).
    if !matches!(
        antecedent.reference,
        PermanentEffectReference::Target(_)
            | PermanentEffectReference::Observed(TypedObservedEventObject::MovedOntoBattlefield)
    ) {
        return Err(cursor.fail("a declared target as the nominal anaphor antecedent"));
    }
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

/// Lexical lookahead for `that NOUN's power|toughness|mana value`.
pub(in crate::oracle::canonical) fn bound_object_property_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
        && cursor
            .token_slice(cursor.position() + 1, cursor.position() + 2)
            .first()
            .is_some_and(|token| token.token_type == "possessiveTypeWord")
        && cursor.peek_ahead(2).is_some_and(|raw| {
            raw.eq_ignore_ascii_case("power")
                || raw.eq_ignore_ascii_case("toughness")
                || raw.eq_ignore_ascii_case("mana")
        })
}

/// The genitive `that NOUN's power|toughness|mana value` (CR 208, 202.3):
/// the property of the object the environment names with that nominal. A
/// lexical target keeps the declared-slot property (validated against its
/// selector when lowered); otherwise the event's object when the event wrote
/// that nominal for it.
pub(in crate::oracle::canonical) fn recognize_bound_object_property(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.genitive.reference");
    cursor.consume("that")?;
    cursor.enter("numeric.genitive.nominal");
    let possessive = cursor.consume_type("possessiveTypeWord")?;
    let nominal = possessive_nominal(&possessive.raw)
        .ok_or_else(|| cursor.fail("a registered card type or object domain nominal"))?;
    cursor.enter("numeric.genitive.property");
    let property = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("power"))
    {
        cursor.consume("power")?;
        SelectorNumericProperty::Power
    } else if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("toughness"))
    {
        cursor.consume("toughness")?;
        SelectorNumericProperty::Toughness
    } else {
        cursor.consume_phrase(&["mana", "value"])?;
        SelectorNumericProperty::ManaValue
    };
    cursor.enter("numeric.genitive.antecedent");
    if let Some(antecedent) = environment.object.as_ref()
        && let PermanentEffectReference::Target(binding) = antecedent.reference
        && lexical_antecedent_implies(antecedent, nominal)
    {
        return Ok(NumericExpression::QualifiedTargetProperty {
            object: binding,
            required_nominal: nominal.as_predicate(),
            property,
        });
    }
    // `that spell's mana value`: the spell the cast record binds.
    if environment.frame.event_spell
        && nominal == TargetNominal::Domain(super::ObjectKind::Spell)
        && property == SelectorNumericProperty::ManaValue
    {
        return Ok(NumericExpression::EventValue(
            super::EventNumericBinding::CastSpellManaValue,
        ));
    }
    let slot = environment
        .frame
        .event_object
        .as_ref()
        .filter(|slot| slot.implies(nominal) && relation_supported(slot.reference))
        .ok_or_else(|| cursor.fail("a preceding target or an event object with this nominal"))?;
    Ok(NumericExpression::BoundObjectProperty {
        object: slot.reference,
        property,
    })
}

/// Reads a clause's arguments with `subject` as the clause subject of the
/// installed environment, then restores the enclosing environment.
pub(in crate::oracle::canonical) fn with_clause_subject<T>(
    cursor: &mut OracleTokenCursor<'_>,
    subject: Option<PermanentEffectReference>,
    read: impl FnOnce(&mut OracleTokenCursor<'_>) -> T,
) -> T {
    let mut environment = installed_binding_environment(cursor);
    environment.subject = subject;
    with_installed_environment(cursor, environment, read)
}

/// The clause subject a damage source operand denotes: the ability source,
/// a bound target, or the single target this action declares.
pub(in crate::oracle::canonical) fn damage_source_subject(
    cursor: &OracleTokenCursor<'_>,
    source: &super::PermanentOperandNode,
) -> Option<PermanentEffectReference> {
    match source {
        super::PermanentOperandNode::Reference(reference) => {
            Some(PermanentEffectReference::Source(*reference))
        }
        super::PermanentOperandNode::BoundReference(
            reference @ PermanentEffectReference::Target(_),
        ) => Some(*reference),
        super::PermanentOperandNode::Targets(targets) if targets.slots.len() == 1 => cursor
            .binding_environment()
            .and_then(|environment| environment.effect_index)
            .map(|effect_index| {
                PermanentEffectReference::Target(super::TargetBinding {
                    effect_index,
                    slot_index: 0,
                })
            }),
        _ => None,
    }
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
