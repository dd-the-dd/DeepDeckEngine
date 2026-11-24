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
    AttachedObjectRole, CardZoneName, EffectRecognitionContext, GrammaticalNumber,
    NumericExpression, ObjectCardType, ObjectKind, ObjectPlayerRelation, ObjectPredicate,
    ObjectSubjectAntecedent, PermanentEffectReference, PermanentSubjectScope, PlayerBinding,
    PlayerEffectSubject, SelectorNumericProperty, TargetNominal, TargetSelector,
    TypedObservedEventObject, selector_implies_nominal,
};
use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, CharacteristicLexeme, ObjectCharacteristic, lookup_card_type,
    lookup_object_characteristic, lookup_object_domain, lookup_object_selector_lexeme,
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
    /// The controller relation the event wrote for the object (`a creature
    /// an opponent controls`): the player `that player` names (CR 108.4).
    pub controller: Option<super::ObjectControllerScope>,
}

impl EventObjectSlot {
    fn implies(&self, nominal: TargetNominal) -> bool {
        predicate_names_nominal(&self.predicate, nominal)
    }

    fn names(&self, nominals: &[TargetNominal]) -> bool {
        predicate_names_nominals(&self.predicate, nominals)
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
    /// The object set the immediately preceding action of the program
    /// declared or produced (`those creatures`, `they`, `that token`). The
    /// program parent installs it from its earlier actions
    /// (`permanent_subject::prior_object_set`); it is never guessed.
    pub objects: Option<ObjectSetAntecedent>,
}

/// An object set an earlier action of the same program declared or produced
/// (CR 608.2c): every object of one plural target slot, or the tokens one
/// creation action created. Its number is the one the producer stated, so a
/// singular anaphor never names several objects and a plural one never
/// names a single declared object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ObjectSetAntecedent {
    /// What the set denotes when a later reference names it.
    pub denotes: ObjectSetDenotation,
    /// The declared selector, or the card types the created tokens have, a
    /// nominal anaphor must follow from. `None` when the producer states no
    /// type (a token copy of an object).
    pub selector: Option<TargetSelector>,
    /// `Singular` only when the producer stated exactly one object (`create a
    /// token`); a declared slot of several targets is `Plural`.
    pub number: GrammaticalNumber,
    /// The set is created tokens, so `token`/`tokens` names it.
    pub tokens: bool,
}

/// What an object set denotes when a later reference names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObjectSetDenotation {
    /// A declared target set (`TargetSet(slot)`) or the created tokens
    /// (`Observed(CreatedTokens)`), recorded by the runtime.
    Bound(PermanentEffectReference),
    /// The members of a battlefield collection an action just affected,
    /// read again by the same query: the preceding action changes none of
    /// the characteristics the query tests (`prior_object_set` proves it),
    /// so no object joins or leaves the set between the two actions
    /// (CR 608.2c; nothing else happens during the resolution).
    Collection {
        query: super::ObjectQueryNode,
        exclude_source: bool,
    },
}

impl ObjectSetDenotation {
    fn bound(&self) -> Option<PermanentEffectReference> {
        match self {
            Self::Bound(reference) => Some(*reference),
            Self::Collection { .. } => None,
        }
    }

    /// The permanent operand a reference to the set reads.
    fn operand(&self) -> super::PermanentOperandNode {
        match self {
            Self::Bound(reference) => super::PermanentOperandNode::BoundReference(*reference),
            Self::Collection {
                query,
                exclude_source,
            } => super::PermanentOperandNode::Collection {
                objects: query.clone(),
                exclude_source: *exclude_source,
            },
        }
    }
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
            objects: None,
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
    // `Destroy target creature an opponent controls. That player loses 3
    // life.` / `Whenever a creature an opponent controls dies, that player
    // ...`: the other player the object's controller relation named.
    if let Some(object) = controlled_object_player(environment) {
        cursor.enter("player-subject.object-controller");
        cursor.enter("player-subject.complete");
        return Ok(BoundPlayerReference::ObjectRelation {
            relation: ObjectPlayerRelation::Controller,
            object,
        });
    }
    Err(cursor.fail_with_span(
        "a prior player binding for `that player`",
        first.index,
        first.span_start,
        first.span_end,
        first.raw,
    ))
}

/// A controller relation that names a player other than the ability's
/// controller (`an opponent controls`, `you don't control`).
fn names_other_player(scope: super::ObjectControllerScope) -> bool {
    matches!(
        scope,
        super::ObjectControllerScope::NotAbilityController
            | super::ObjectControllerScope::OpponentOfAbilityController
    )
}

/// The singular object whose controller relation named another player:
/// the program's antecedent declared `target NOUN an opponent controls`,
/// else the event's object written `a NOUN an opponent controls`. Its
/// controller, live or as last known information, is `that player`.
fn controlled_object_player(environment: &BindingEnvironment) -> Option<PermanentEffectReference> {
    let event = environment
        .frame
        .event_object
        .as_ref()
        .filter(|slot| {
            slot.controller.is_some_and(names_other_player) && relation_supported(slot.reference)
        })
        .map(|slot| slot.reference);
    let Some(antecedent) = environment.object.as_ref() else {
        return event;
    };
    other_player_controlled_antecedent(antecedent).or_else(|| {
        // The lexical antecedent is the event's own object (`it` after `a
        // land an opponent controls enters`).
        event.filter(|reference| *reference == antecedent.reference)
    })
}

/// The one reader of a singular object antecedent declared among the
/// permanents another player controls (`target creature an opponent
/// controls`, `... you don't control`, CR 109.4): its controller is the
/// player `that player` names. Shared by the binding environment and the
/// clause layer's performer slot.
pub(in crate::oracle::canonical) fn other_player_controlled_antecedent(
    antecedent: &ObjectSubjectAntecedent,
) -> Option<PermanentEffectReference> {
    match antecedent.selector.as_ref() {
        Some(TargetSelector::ControlledObject { controller, .. })
            if names_other_player(*controller)
                && singular_reference(antecedent.reference)
                && relation_supported(antecedent.reference) =>
        {
            Some(antecedent.reference)
        }
        _ => None,
    }
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
        // A set of declared targets has no single controller or owner.
        PermanentEffectReference::TargetSet(_) => false,
        _ => true,
    }
}

fn lexical_antecedent_implies(
    antecedent: &ObjectSubjectAntecedent,
    nominal: TargetNominal,
) -> bool {
    antecedent_names(antecedent, &[nominal])
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
        let reference =
            resolve_pronoun_antecedent(environment).map_err(|expected| cursor.fail(expected))?;
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

/// The registered nominals of a disjunctive nominal phrase starting `offset`
/// tokens after the cursor (`creature`, `creature or planeswalker`), with
/// its token width. Lexical lookahead only; it proves no binding.
fn nominal_disjunction_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(Vec<TargetNominal>, usize)> {
    let nominal_at = |index: usize| {
        cursor
            .token_slice(cursor.position() + index, cursor.position() + index + 1)
            .first()
            .filter(|token| token.token_type != "possessiveTypeWord")
            .and_then(|token| registered_nominal(&token.raw))
    };
    let mut nominals = vec![nominal_at(offset)?];
    let mut width = 1;
    // `that creature or planeswalker`: each further nominal follows `or`.
    while cursor
        .peek_ahead(offset + width)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
        && let Some(next) = nominal_at(offset + width + 1)
    {
        nominals.push(next);
        width += 2;
    }
    Some((nominals, width))
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
    let (nominals, width) = nominal_disjunction_at(cursor, 0)
        .ok_or_else(|| cursor.fail("a registered card type or permanent nominal"))?;
    cursor.enter("permanent.subject.nominal-anaphor.antecedent");
    let reference = resolve_nominal_antecedent(environment, &nominals, departed_readable)
        .map_err(|expected| cursor.fail(expected))?;
    cursor.advance_to(cursor.position() + width)?;
    Ok(reference)
}

/// A registered nominal names a predicate when the predicate guarantees it,
/// or, for `creature`, when every branch heads with a creature subtype
/// (`a Knight you control`, `another Ally`): a creature type is carried only
/// by creatures and Kindred permanents (CR 205.3m), and the anaphor names the
/// same object either way.
pub(in crate::oracle::canonical) fn predicate_names_nominal(
    predicate: &ObjectPredicate,
    nominal: TargetNominal,
) -> bool {
    selector_implies_nominal(
        &TargetSelector::Object(predicate.clone()),
        &nominal.as_predicate(),
    ) || (nominal == TargetNominal::CardType(ObjectCardType::Creature)
        && predicate.has_creature_subtype_head()
        && predicate.supports_domain(ObjectKind::Permanent))
}

/// A disjunctive nominal (`creature or planeswalker`) names a predicate when
/// one of its nominals does, or when every branch of a disjunctive predicate
/// is named by one of them (`target creature or planeswalker`).
fn predicate_names_nominals(predicate: &ObjectPredicate, nominals: &[TargetNominal]) -> bool {
    nominals
        .iter()
        .any(|nominal| predicate_names_nominal(predicate, *nominal))
        || (nominals.len() > 1
            && matches!(
                predicate,
                ObjectPredicate::Union(branches)
                    if !branches.is_empty()
                        && branches
                            .iter()
                            .all(|branch| predicate_names_nominals(branch, nominals))
            ))
}

/// The selector form of [`predicate_names_nominals`]: each branch of a
/// selector union must be named.
pub(in crate::oracle::canonical) fn selector_names_nominals(
    selector: &TargetSelector,
    nominals: &[TargetNominal],
) -> bool {
    match selector {
        TargetSelector::Object(predicate) | TargetSelector::ControlledObject { predicate, .. } => {
            predicate_names_nominals(predicate, nominals)
        }
        TargetSelector::Union(branches) => {
            !branches.is_empty()
                && branches
                    .iter()
                    .all(|branch| selector_names_nominals(branch, nominals))
        }
        TargetSelector::Any | TargetSelector::Player(_) | TargetSelector::StackItem(_) => false,
    }
}

fn antecedent_names(antecedent: &ObjectSubjectAntecedent, nominals: &[TargetNominal]) -> bool {
    antecedent
        .selector
        .as_ref()
        .is_some_and(|selector| selector_names_nominals(selector, nominals))
}

/// A reference that denotes one object. A declared set and the tokens of a
/// creation action are object sets, never a singular antecedent.
fn singular_reference(reference: PermanentEffectReference) -> bool {
    !matches!(
        reference,
        PermanentEffectReference::TargetSet(_)
            | PermanentEffectReference::Observed(TypedObservedEventObject::CreatedTokens)
    )
}

const PLURAL_SET_FOR_SINGULAR: &str =
    "a singular antecedent; the preceding action declared or produced several objects";

/// The one resolver of a singular object pronoun (`it`, `he`, `she`): the
/// proven lexical antecedent of the position being read. A preceding plural
/// set is a number mismatch, not an antecedent.
pub(in crate::oracle::canonical) fn resolve_pronoun_antecedent(
    environment: &BindingEnvironment,
) -> Result<PermanentEffectReference, &'static str> {
    if let Some(object) = environment.object.as_ref() {
        return Ok(object.reference);
    }
    if environment
        .objects
        .as_ref()
        .is_some_and(|set| set.number == GrammaticalNumber::Plural)
    {
        return Err(PLURAL_SET_FOR_SINGULAR);
    }
    Err("a source binding or preceding singular permanent subject in this lexical scope")
}

/// The one resolver of a singular nominal anaphor (`that creature`, `the
/// permanent`, `that creature or planeswalker`, CR 608.2c). In order: a
/// block event's other combatant; a lexical target whose declared selector
/// names the nominal; the event's object when the event wrote that nominal
/// for it; the program's singular antecedent (a declared target, the card a
/// move put onto the battlefield, an equipped creature); the one token the
/// preceding action created. `departed_readable` admits an event object that
/// left the battlefield, for a read of its last known information only
/// (CR 400.7, 608.2h). Anything else is a committed diagnostic.
pub(in crate::oracle::canonical) fn resolve_nominal_antecedent(
    environment: &BindingEnvironment,
    nominals: &[TargetNominal],
    departed_readable: bool,
) -> Result<PermanentEffectReference, &'static str> {
    let creature = TargetNominal::CardType(ObjectCardType::Creature);
    if environment.frame.combat_counterpart
        && !environment.local_object_scope
        && nominals == [creature]
    {
        return Ok(PermanentEffectReference::Observed(
            TypedObservedEventObject::CombatCounterpart,
        ));
    }
    // A lexical target with this nominal wins; otherwise the event's object
    // when the event wrote this nominal for it.
    let lexical = environment.object.as_ref().filter(|antecedent| {
        matches!(antecedent.reference, PermanentEffectReference::Target(_))
            && antecedent_names(antecedent, nominals)
    });
    if lexical.is_none()
        && let Some(slot) = environment
            .frame
            .event_object
            .as_ref()
            .filter(|slot| slot.names(nominals))
    {
        // An operand reader acts on the permanent (tap, destroy, deal damage
        // to). A departed event object (`a creature dies`) is a new object in
        // another zone (CR 400.7); only a read of its last known information
        // (a condition, a property, its controller) may name it.
        if slot.departed && !departed_readable {
            return Err(
                "a permanent for the nominal anaphor; the event's object left the battlefield",
            );
        }
        return Ok(slot.reference);
    }
    if let Some(antecedent) = environment.object.as_ref() {
        match antecedent.reference {
            // A declared target, or the card a move put onto the battlefield
            // (its declared card predicate is the selector the nominal must
            // follow from).
            PermanentEffectReference::Target(_)
            | PermanentEffectReference::Observed(TypedObservedEventObject::MovedOntoBattlefield) => {
                let selector = antecedent
                    .selector
                    .as_ref()
                    .ok_or("the declared selector of the preceding permanent target")?;
                return if selector_names_nominals(selector, nominals) {
                    Ok(antecedent.reference)
                } else {
                    Err("a nominal implied by the preceding target's declared selector")
                };
            }
            // An Equipment equips only a creature (CR 301.5).
            PermanentEffectReference::Attached(AttachedObjectRole::Equipped)
                if nominals == [creature] =>
            {
                return Ok(antecedent.reference);
            }
            _ => {}
        }
    }
    // `Create a 1/1 Otter creature token. ... that creature`: the one token
    // the preceding action created, with the types it was created with.
    if let Some(set) = environment.objects.as_ref() {
        match (set.number, set.denotes.bound()) {
            (GrammaticalNumber::Singular, Some(reference))
                if set
                    .selector
                    .as_ref()
                    .is_some_and(|selector| selector_names_nominals(selector, nominals)) =>
            {
                return Ok(reference);
            }
            (GrammaticalNumber::Plural, _) if environment.object.is_none() => {
                return Err(PLURAL_SET_FOR_SINGULAR);
            }
            _ => {}
        }
    }
    Err(match environment.object {
        None => "a preceding singular permanent subject in this lexical scope",
        Some(_) => "a declared target as the nominal anaphor antecedent",
    })
}

/// The one resolver of a possessive `its` in a numeric or condition read
/// (`its power`, `its mana value`): the subject of the clause being read
/// (`it deals damage equal to its power`), else the program's singular
/// antecedent, else the event's grammatical subject, whose last known
/// information still answers the read (CR 608.2h). A set of objects is a
/// number mismatch; nothing is guessed.
pub(in crate::oracle::canonical) fn resolve_possessive_antecedent(
    environment: &BindingEnvironment,
) -> Result<PermanentEffectReference, &'static str> {
    if let Some(subject) = environment.subject {
        return singular_reference(subject)
            .then_some(subject)
            .ok_or(PLURAL_SET_FOR_SINGULAR);
    }
    if let Some(object) = environment.object.as_ref() {
        return singular_reference(object.reference)
            .then_some(object.reference)
            .ok_or(PLURAL_SET_FOR_SINGULAR);
    }
    if let Some(slot) = environment
        .frame
        .event_object
        .as_ref()
        .filter(|slot| slot.subject && relation_supported(slot.reference))
    {
        return Ok(slot.reference);
    }
    if environment
        .objects
        .as_ref()
        .is_some_and(|set| set.number == GrammaticalNumber::Plural)
    {
        return Err(PLURAL_SET_FOR_SINGULAR);
    }
    Err("an event subject or a preceding target that `its` names")
}

/// The object a condition reads (`If it's a creature`, `if its mana value is
/// 2 or less`, `If that creature was a Villain`): the possessive antecedent
/// for a pronoun (empty `nominals`), else the nominal antecedent read for its
/// last known information (CR 608.2h). The sentence-condition grammar calls
/// this resolver instead of guessing a target or an event object.
pub(in crate::oracle::canonical) fn resolve_condition_antecedent(
    environment: &BindingEnvironment,
    nominals: &[TargetNominal],
) -> Result<PermanentEffectReference, &'static str> {
    if nominals.is_empty() {
        resolve_possessive_antecedent(environment)
    } else {
        resolve_nominal_antecedent(environment, nominals, true)
    }
}

/// The one object every atom of a condition is about, when they share it
/// (`As long as this creature is enchanted or equipped`): the antecedent a
/// following `it` names in the same sentence. `None` when an atom is about
/// another object, a player or no object.
pub(in crate::oracle::canonical) fn condition_object_antecedent(
    condition: &super::ConditionNode,
) -> Option<PermanentEffectReference> {
    match condition {
        super::BooleanConditionNode::Atom(super::ConditionAtom::ObjectSatisfies(node)) => {
            match node.object {
                super::ConditionObjectReference::Permanent(reference)
                    if singular_reference(reference) =>
                {
                    Some(reference)
                }
                _ => None,
            }
        }
        super::BooleanConditionNode::Atom(_) => None,
        super::BooleanConditionNode::Not(inner) => condition_object_antecedent(inner),
        super::BooleanConditionNode::And(operands) | super::BooleanConditionNode::Or(operands) => {
            let (first, rest) = operands.split_first()?;
            let reference = condition_object_antecedent(first)?;
            rest.iter()
                .all(|operand| condition_object_antecedent(operand) == Some(reference))
                .then_some(reference)
        }
    }
}

/// `it` as the subject after a leading condition about one object (`As long
/// as this creature is equipped, it gets +1/+1`; `As long as enchanted
/// creature is red, it has ...`): the source or the permanent it is attached
/// to that every atom of the condition reads. `Ok(None)` when no pronoun
/// subject starts; a pronoun without such a condition antecedent is a
/// committed diagnostic, never the source by default.
pub(in crate::oracle::canonical) fn recognize_condition_pronoun_subject(
    cursor: &mut OracleTokenCursor<'_>,
    condition: Option<&super::ConditionNode>,
) -> MachineResult<Option<PermanentEffectReference>> {
    let Some(condition) = condition else {
        return Ok(None);
    };
    if !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("it"))
    {
        return Ok(None);
    }
    cursor.enter("static.condition-antecedent.pronoun");
    let reference = condition_object_antecedent(condition)
        .filter(|reference| {
            matches!(
                reference,
                PermanentEffectReference::Source(_) | PermanentEffectReference::Attached(_)
            )
        })
        .ok_or_else(|| {
            cursor.fail("a leading condition about the source or its attached permanent for `it`")
        })?;
    cursor.consume("it")?;
    Ok(Some(reference))
}

/// The nominal and number of an object nominal surface, or `None` when it
/// is not a registered nominal (`creatures` is plural, `creature` singular).
fn nominal_number(raw: &str) -> Option<(TargetNominal, GrammaticalNumber)> {
    let nominal = registered_nominal(raw)?;
    let singular = lookup_card_type(raw, AcceptedForms::SingularOnly).is_some()
        || lookup_object_domain(raw, AcceptedForms::SingularOnly).is_some();
    Some((
        nominal,
        if singular {
            GrammaticalNumber::Singular
        } else {
            GrammaticalNumber::Plural
        },
    ))
}

/// `token` / `tokens`: the created-token nominal and its number.
fn token_nominal_number(raw: &str) -> Option<GrammaticalNumber> {
    if lookup_object_characteristic(raw, AcceptedForms::SingularOnly)
        == Some(ObjectCharacteristic::Token)
    {
        Some(GrammaticalNumber::Singular)
    } else if lookup_object_characteristic(raw, AcceptedForms::SingularOrPlural)
        == Some(ObjectCharacteristic::Token)
    {
        Some(GrammaticalNumber::Plural)
    } else {
        None
    }
}

/// The head of an object-set reference at the cursor: its token width, the
/// number it states, the nominal it names (`None` for a pronoun or the token
/// noun) and whether it is the created-token noun.
struct ObjectSetHead {
    width: usize,
    number: GrammaticalNumber,
    nominal: Option<TargetNominal>,
    tokens: bool,
}

fn object_set_head(cursor: &OracleTokenCursor<'_>) -> Option<ObjectSetHead> {
    let word = |offset: usize| {
        cursor
            .peek_ahead(offset)
            .map(|raw| raw.to_ascii_lowercase())
    };
    let plural_pronoun = |width: usize| ObjectSetHead {
        width,
        number: GrammaticalNumber::Plural,
        nominal: None,
        tokens: false,
    };
    // `each of them` / `each of those NOUNs`: each member of the same set
    // (CR 608.2c); an action on a set already acts on each member.
    let first = word(0)?;
    let (prefix, determiner) = match (first.as_str(), word(1).as_deref(), word(2).as_deref()) {
        ("each", Some("of"), Some("them")) => return Some(plural_pronoun(3)),
        ("each", Some("of"), Some("those")) => (2, "those"),
        ("they", _, _) => return Some(plural_pronoun(1)),
        ("those", _, _) => (0, "those"),
        ("that", _, _) => (0, "that"),
        ("the", _, _) => (0, "the"),
        _ => return None,
    };
    let noun = cursor
        .token_slice(
            cursor.position() + prefix + 1,
            cursor.position() + prefix + 2,
        )
        .first()
        .filter(|token| token.token_type != "possessiveTypeWord")?
        .raw
        .clone();
    if let Some(number) = token_nominal_number(&noun) {
        // `that token` / `the token` / `those tokens` / `the tokens`.
        let agrees = match determiner {
            "that" => number == GrammaticalNumber::Singular,
            "those" => number == GrammaticalNumber::Plural,
            _ => true,
        };
        return agrees.then_some(ObjectSetHead {
            width: prefix + 2,
            number,
            nominal: None,
            tokens: true,
        });
    }
    // `those creatures`: only the plural demonstrative opens a nominal set
    // reference; `that creature` / `the creature` stay singular anaphors.
    let (nominal, number) = nominal_number(&noun)?;
    (determiner == "those" && number == GrammaticalNumber::Plural).then_some(ObjectSetHead {
        width: prefix + 2,
        number,
        nominal: Some(nominal),
        tokens: false,
    })
}

/// True when `failure` is an unresolved antecedent of a demonstrative
/// object reference (`that creature`, `those creatures`, `that token`): the
/// reference was read and no proven antecedent has its nominal or number.
pub(in crate::oracle::canonical) fn demonstrative_binding_failure(
    failure: &super::super::MachineFailure,
) -> bool {
    matches!(
        failure.state,
        "permanent.subject.nominal-anaphor.antecedent"
            | "permanent.subject.object-set-anaphor.antecedent"
            | "permanent.subject.object-set-anaphor.nominal"
    )
}

/// Lexical lookahead for an object-set reference (`they`, `those
/// creatures`, `each of them`, `that token`, `the tokens`). It proves no
/// binding.
pub(in crate::oracle::canonical) fn object_set_reference_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    object_set_head(cursor).is_some()
}

/// Resolves an object-set reference against the set the preceding action
/// declared or produced (CR 608.2c). The stated number must be the
/// producer's: `those creatures` never names one declared target, `that
/// token` never names several created tokens. The nominal must follow from
/// the declared selector, or name the created tokens.
pub(in crate::oracle::canonical) fn recognize_object_set_reference(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<super::PermanentOperandNode> {
    cursor.enter("permanent.subject.object-set-anaphor");
    let head = object_set_head(cursor)
        .ok_or_else(|| cursor.fail("`they`, `those NOUNs`, `each of them` or `that token`"))?;
    cursor.enter("permanent.subject.object-set-anaphor.antecedent");
    let set = environment
        .objects
        .as_ref()
        .ok_or_else(|| cursor.fail("an object set the preceding action declared or produced"))?;
    cursor.require(
        set.number == head.number,
        match head.number {
            GrammaticalNumber::Singular => PLURAL_SET_FOR_SINGULAR,
            _ => "a plural antecedent; the preceding action declared or produced one object",
        },
    )?;
    cursor.enter("permanent.subject.object-set-anaphor.nominal");
    cursor.require(
        !head.tokens || set.tokens,
        "tokens the preceding action created for `token`",
    )?;
    if let Some(nominal) = head.nominal {
        cursor.require(
            set.selector
                .as_ref()
                .is_some_and(|selector| selector_names_nominals(selector, &[nominal])),
            "a nominal implied by the declared selector of the preceding object set",
        )?;
    }
    cursor.advance_to(cursor.position() + head.width)?;
    Ok(set.denotes.operand())
}

/// Lexical lookahead for `that NOUN's power|toughness|mana value` and the
/// source's own `this NOUN's power|toughness`.
pub(in crate::oracle::canonical) fn bound_object_property_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let possessive_property = |offset: usize| {
        cursor
            .token_slice(cursor.position() + offset, cursor.position() + offset + 1)
            .first()
            .is_some_and(|token| token.token_type == "possessiveTypeWord")
            && cursor.peek_ahead(offset + 1).is_some_and(|raw| {
                raw.eq_ignore_ascii_case("power") || raw.eq_ignore_ascii_case("toughness")
            })
    };
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
    {
        return possessive_property(1);
    }
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

/// The numeric read of one property of a singular bound object (CR 208,
/// 202.3). A declared target keeps the declared-slot property, validated
/// against its selector when lowered (`required_nominal`); the ability source
/// reads its own power and toughness; any other singular object the binding
/// environment names is read live while it is the same object, else as last
/// known information (CR 400.7, 608.2h).
pub(in crate::oracle::canonical) fn bound_object_numeric_property(
    reference: PermanentEffectReference,
    property: SelectorNumericProperty,
    required_nominal: ObjectPredicate,
) -> Result<NumericExpression, &'static str> {
    if !singular_reference(reference) {
        return Err(PLURAL_SET_FOR_SINGULAR);
    }
    Ok(match (reference, property) {
        (PermanentEffectReference::Target(binding), _) => {
            NumericExpression::QualifiedTargetProperty {
                object: binding,
                required_nominal,
                property,
            }
        }
        (PermanentEffectReference::Source(source), SelectorNumericProperty::Power) => {
            NumericExpression::ObjectProperty {
                object: source,
                property: super::NumericObjectProperty::Power,
            }
        }
        (PermanentEffectReference::Source(source), SelectorNumericProperty::Toughness) => {
            NumericExpression::ObjectProperty {
                object: source,
                property: super::NumericObjectProperty::Toughness,
            }
        }
        (PermanentEffectReference::Source(_), SelectorNumericProperty::ManaValue) => {
            return Err("a mana-value read the runtime records for this object");
        }
        (object, property) => NumericExpression::BoundObjectProperty { object, property },
    })
}

/// The genitive `that NOUN's power|toughness|mana value` (CR 208, 202.3):
/// the property of the object the environment names with that nominal,
/// resolved by the shared nominal resolver for its last known information.
/// `that spell's mana value` in a cast trigger is the cast record's value.
pub(in crate::oracle::canonical) fn recognize_bound_object_property(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.genitive.reference");
    let source = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("this"));
    cursor.consume(if source { "this" } else { "that" })?;
    cursor.enter("numeric.genitive.nominal");
    let possessive = cursor.consume_type("possessiveTypeWord")?;
    let nominal = possessive_nominal(&possessive.raw)
        .ok_or_else(|| cursor.fail("a registered card type or object domain nominal"))?;
    if source {
        // `this creature's power`: the ability source's own power or
        // toughness, live or as last known information (CR 608.2h).
        cursor.enter("numeric.genitive.source-property");
        let property = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
            Some("power") => super::NumericObjectProperty::Power,
            Some("toughness") => super::NumericObjectProperty::Toughness,
            _ => return Err(cursor.fail("`power` or `toughness` of the ability source")),
        };
        let raw = cursor.peek().unwrap_or_default().to_owned();
        cursor.consume(&raw)?;
        return Ok(NumericExpression::ObjectProperty {
            object: super::LocalObjectReference::SourceSelf,
            property,
        });
    }
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
    // `that spell's mana value`: the spell the cast record binds, unless a
    // lexical spell target of the program has that nominal.
    if environment.frame.event_spell
        && nominal == TargetNominal::Domain(ObjectKind::Spell)
        && property == SelectorNumericProperty::ManaValue
        && !environment.object.as_ref().is_some_and(|antecedent| {
            matches!(antecedent.reference, PermanentEffectReference::Target(_))
                && antecedent_names(antecedent, &[nominal])
        })
    {
        return Ok(NumericExpression::EventValue(
            super::EventNumericBinding::CastSpellManaValue,
        ));
    }
    let reference = resolve_condition_antecedent(environment, &[nominal])
        .map_err(|_| cursor.fail("a preceding target or an event object with this nominal"))?;
    // An event's observed object is read only where the runtime records it.
    if let PermanentEffectReference::Observed(observed) = reference {
        cursor.require(
            matches!(
                observed,
                TypedObservedEventObject::EventPermanent
                    | TypedObservedEventObject::EnteringPermanent
                    | TypedObservedEventObject::MovedOntoBattlefield
                    | TypedObservedEventObject::CombatCounterpart
            ),
            "a preceding target or an event object with this nominal",
        )?;
    }
    bound_object_numeric_property(reference, property, nominal.as_predicate())
        .map_err(|expected| cursor.fail(expected))
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

/// Reads with `antecedent` as the singular object antecedent when one is
/// proven, else with the installed environment unchanged. A postposed
/// `where X is its power` reads the object of the clause it closes.
pub(in crate::oracle::canonical) fn with_object_antecedent<T>(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<ObjectSubjectAntecedent>,
    read: impl FnOnce(&mut OracleTokenCursor<'_>) -> T,
) -> T {
    let Some(antecedent) = antecedent else {
        return read(cursor);
    };
    let mut environment = installed_binding_environment(cursor);
    environment.object = Some(antecedent);
    with_installed_environment(cursor, environment, read)
}

/// The clause subject a damage source operand denotes: the ability source,
/// a bound singular object, or the single target this action declares.
pub(in crate::oracle::canonical) fn damage_source_subject(
    cursor: &OracleTokenCursor<'_>,
    source: &super::PermanentOperandNode,
) -> Option<PermanentEffectReference> {
    match source {
        super::PermanentOperandNode::Reference(reference) => {
            Some(PermanentEffectReference::Source(*reference))
        }
        super::PermanentOperandNode::BoundReference(reference)
            if singular_reference(*reference) =>
        {
            Some(*reference)
        }
        super::PermanentOperandNode::Targets(targets)
            if targets.slots.len() == 1 && targets.slots[0].cardinality.maximum == 1 =>
        {
            cursor
                .binding_environment()
                .and_then(|environment| environment.effect_index)
                .map(|effect_index| {
                    PermanentEffectReference::Target(super::TargetBinding {
                        effect_index,
                        slot_index: 0,
                    })
                })
        }
        _ => None,
    }
}

/// Whether the cursor reads an action of an effect program, where an
/// earlier `choose` instruction of the same program may produce the value a
/// `the chosen ...` reference reads (CR 608.2d). The program lowering and
/// `contextual_binding` prove that producer. Outside a program (a static
/// ability, a trigger event pattern) such a reference has no resolution
/// producer, so its readers do not claim it.
pub(in crate::oracle::canonical) fn resolution_choice_in_scope(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .binding_environment()
        .is_some_and(|environment| environment.effect_index.is_some())
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
