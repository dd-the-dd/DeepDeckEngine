use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    BoundPermanentSetNode, CollectionBinding, ConditionalEffectNode, CounterDestinationReplacement,
    CounterReplacementSequenceNode, DrawCardsEffectParseAttempt, EffectConnector,
    EffectRecognitionContext, EffectSequenceNode, LibraryLookAction, LocalEffectNode,
    ObjectSetBinding, PlayerTargetBinding, TargetBinding, TargetCardZoneChangeParseAttempt,
    TemporaryCombatRestrictionParseAttempt, TemporaryPlayerPermissionParseAttempt,
    ZoneChangeTargetSupport, apply_for_each_multiplier, claims_put_counters_effect,
    for_each_count_starts, library_look_reminder_starts_at, recognize_add_mana_effect,
    recognize_amass_effect, recognize_conditional_damage_replacement,
    recognize_controller_library_search, recognize_counter_effect, recognize_damage_effect,
    recognize_destroy_permanent_collection_effect, recognize_destroy_referenced_set_effect,
    recognize_earthbend_action, recognize_effect_connector, recognize_empower_jace_effect,
    recognize_empower_jace_reminder, recognize_exile_graveyard_collection_effect,
    recognize_for_each_count, recognize_investigate_effect, recognize_library_look_reminder,
    recognize_mill_cards_effect, recognize_move_cards_effect,
    recognize_named_token_creation_reminder,
    recognize_optional_card_deployment_effect_with_context, recognize_optional_library_search,
    recognize_ordinary_sacrifice_effect, recognize_put_counters_effect,
    recognize_quantified_player_action, recognize_referenced_draw_effect, recognize_scry_effect,
    recognize_set_ability_loss_effect, recognize_source_zone_move_effect, recognize_surveil_effect,
    recognize_target_card_zone_change_effect, recognize_temporary_power_toughness,
    recognize_transform_source_effect, recognize_untap_permanent_effect,
    recognize_zone_change_effect_with_policy,
    recognize_zone_change_with_regeneration_amendment_with_support,
    signed_power_toughness_from_raw, source_or_anaphor_reference_starts_at,
    source_reference_starts_at, try_recognize_draw_cards_effect_with_context,
    try_recognize_exile_target_card_zone_change_effect,
    try_recognize_target_card_zone_change_effect_with_verb,
    try_recognize_temporary_combat_restriction, try_recognize_temporary_player_permission,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CounterReplacementSequenceState {
    Counter,
    SentenceBoundary,
    If,
    Antecedent,
    Predicate,
    Manner,
    Separator,
    DestinationAction,
    Pronoun,
    Instead,
    Of,
    ReplacedAction,
    ReplacedObject,
    Into,
    Possessor,
    Zone,
    Terminator,
    Complete,
}

/// Composes an atomic counter effect with a conditional destination
/// replacement. Each anaphor is validated by this parent machine while target
/// syntax remains owned by the shared counter/selector automata.
pub(in crate::oracle::canonical) fn recognize_counter_replacement_sequence(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CounterReplacementSequenceNode> {
    let mut state = CounterReplacementSequenceState::Counter;
    let mut counter = None;
    loop {
        state = match state {
            CounterReplacementSequenceState::Counter => {
                cursor.enter("sequence.counter-replacement.counter");
                let effect = recognize_counter_effect(cursor)?;
                let LocalEffectNode::CounterSpell(effect) = effect else {
                    return Err(cursor.fail("a counter-spell antecedent"));
                };
                counter = Some(effect);
                CounterReplacementSequenceState::SentenceBoundary
            }
            CounterReplacementSequenceState::SentenceBoundary => {
                cursor.enter("sequence.counter-replacement.sentence-boundary");
                cursor.consume(".")?;
                CounterReplacementSequenceState::If
            }
            CounterReplacementSequenceState::If => {
                cursor.enter("sequence.counter-replacement.if");
                cursor.consume("if")?;
                CounterReplacementSequenceState::Antecedent
            }
            CounterReplacementSequenceState::Antecedent => {
                cursor.enter("sequence.counter-replacement.antecedent");
                cursor.consume_phrase(&["that", "spell"])?;
                CounterReplacementSequenceState::Predicate
            }
            CounterReplacementSequenceState::Predicate => {
                cursor.enter("sequence.counter-replacement.predicate");
                cursor.consume_phrase(&["is", "countered"])?;
                CounterReplacementSequenceState::Manner
            }
            CounterReplacementSequenceState::Manner => {
                cursor.enter("sequence.counter-replacement.manner");
                cursor.consume_phrase(&["this", "way"])?;
                CounterReplacementSequenceState::Separator
            }
            CounterReplacementSequenceState::Separator => {
                cursor.enter("sequence.counter-replacement.separator");
                cursor.consume(",")?;
                CounterReplacementSequenceState::DestinationAction
            }
            CounterReplacementSequenceState::DestinationAction => {
                cursor.enter("sequence.counter-replacement.destination-action");
                cursor.consume(
                    crate::oracle::canonical::automata::verb_entry(VerbLemma::Exile)
                        .surface(VerbForm::Base),
                )?;
                CounterReplacementSequenceState::Pronoun
            }
            CounterReplacementSequenceState::Pronoun => {
                cursor.enter("sequence.counter-replacement.pronoun");
                cursor.consume("it")?;
                CounterReplacementSequenceState::Instead
            }
            CounterReplacementSequenceState::Instead => {
                cursor.enter("sequence.counter-replacement.instead");
                cursor.consume("instead")?;
                CounterReplacementSequenceState::Of
            }
            CounterReplacementSequenceState::Of => {
                cursor.enter("sequence.counter-replacement.of");
                cursor.consume("of")?;
                CounterReplacementSequenceState::ReplacedAction
            }
            CounterReplacementSequenceState::ReplacedAction => {
                cursor.enter("sequence.counter-replacement.replaced-action");
                cursor.consume("putting")?;
                CounterReplacementSequenceState::ReplacedObject
            }
            CounterReplacementSequenceState::ReplacedObject => {
                cursor.enter("sequence.counter-replacement.replaced-object");
                cursor.consume("it")?;
                CounterReplacementSequenceState::Into
            }
            CounterReplacementSequenceState::Into => {
                cursor.enter("sequence.counter-replacement.into");
                cursor.consume("into")?;
                CounterReplacementSequenceState::Possessor
            }
            CounterReplacementSequenceState::Possessor => {
                cursor.enter("sequence.counter-replacement.possessor");
                cursor.consume("its")?;
                let possessor = cursor.consume_type("possessivePlayerRoleWord")?;
                cursor.require(
                    possessor.raw.eq_ignore_ascii_case("owner's")
                        || possessor.raw.eq_ignore_ascii_case("owner’s"),
                    "the possessive owner reference",
                )?;
                CounterReplacementSequenceState::Zone
            }
            CounterReplacementSequenceState::Zone => {
                cursor.enter("sequence.counter-replacement.zone");
                cursor.consume("graveyard")?;
                CounterReplacementSequenceState::Terminator
            }
            CounterReplacementSequenceState::Terminator => {
                cursor.enter("sequence.counter-replacement.terminator");
                cursor.consume(".")?;
                CounterReplacementSequenceState::Complete
            }
            CounterReplacementSequenceState::Complete => {
                cursor.enter("sequence.counter-replacement.complete");
                cursor.finish()?;
                return Ok(CounterReplacementSequenceNode {
                    counter: cursor.require_value(counter, "a populated counter antecedent")?,
                    destination: CounterDestinationReplacement::Exile,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SequenceState {
    Effect,
    ConnectorOrEnd,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConditionalSequenceState {
    Antecedent,
    SentenceBoundary,
    Replacement,
    Complete,
}

/// Composes a complete effect sentence with a later conditional replacement.
/// Sentence punctuation is owned here rather than by either child effect.
pub(in crate::oracle::canonical) fn recognize_conditional_replacement_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<ConditionalEffectNode> {
    let mut state = ConditionalSequenceState::Antecedent;
    let mut antecedent = None;
    let mut conditional = None;
    loop {
        state = match state {
            ConditionalSequenceState::Antecedent => {
                cursor.enter("sequence.conditional.antecedent");
                let effect = cursor.attempt(|attempt| {
                    recognize_damage_effect(attempt, name_links, name_failure)
                })?;
                antecedent = match effect {
                    LocalEffectNode::DealDamage(effect) => Some(effect),
                    _ => return Err(cursor.fail("a damage effect eligible for replacement")),
                };
                ConditionalSequenceState::SentenceBoundary
            }
            ConditionalSequenceState::SentenceBoundary => {
                cursor.enter("sequence.conditional.sentence-boundary");
                cursor.consume(".")?;
                ConditionalSequenceState::Replacement
            }
            ConditionalSequenceState::Replacement => {
                cursor.enter("sequence.conditional.replacement");
                let antecedent = antecedent
                    .as_ref()
                    .ok_or_else(|| cursor.fail("a completed antecedent effect"))?;
                conditional = Some(recognize_conditional_damage_replacement(
                    cursor,
                    antecedent,
                    name_links,
                    name_failure,
                )?);
                ConditionalSequenceState::Complete
            }
            ConditionalSequenceState::Complete => {
                cursor.enter("sequence.conditional.complete");
                cursor.finish()?;
                return cursor.require_value(conditional, "a completed conditional replacement");
            }
        };
    }
}

#[derive(Default)]
struct SequenceMemory {
    effects: Vec<LocalEffectNode>,
    connectors: Vec<EffectConnector>,
    last_target: Option<TargetBinding>,
    permanent_subject_scope: super::PermanentSubjectScope,
    last_player_target: Option<PlayerTargetBinding>,
    current_player_subject: Option<super::PlayerEffectSubject>,
    /// The performer of the previous action and the sentence it opened: a
    /// subjectless verb of the same sentence continues it (`each player
    /// exiles ..., then sacrifices ...`).
    current_performer: Option<(super::PerformerNode, super::VerbAgreement, usize)>,
    last_object_set: Option<BoundPermanentSetNode>,
    next_search_binding: usize,
    /// Preposed duration of the sentence being read (CR 611.2a). It scopes
    /// every continuous-effect action up to the next sentence boundary.
    sentence_duration: Option<super::EffectDurationNode>,
    /// Index of the first action of the sentence being read; a postposed
    /// `where X is EXPR` binds X for the actions from here to the clause.
    sentence_start: usize,
    where_x: Vec<super::WhereXBindingNode>,
    /// Sentence conditionals in written order (CR 608.2c).
    conditionals: Vec<super::SentenceConditionalNode>,
    /// The conditional sentence (`If` or `Otherwise`) being read.
    open_branch: Option<ConditionalBranch>,
    /// A conditional whose `If` sentence just closed; only the very next
    /// sentence may open its `Otherwise` branch.
    else_candidate: Option<usize>,
    /// Anaphor state before each conditional's `If` sentence. Its `Otherwise`
    /// sentence reads the same antecedents, not the `If` branch's results.
    branch_snapshots: Vec<AnaphorSnapshot>,
    unless: Vec<super::UnlessFrameNode>,
    /// Remaining per-player actions of one coordinated-subject sentence, in
    /// reverse order. Each is pushed as its own action joined by `and`.
    distributed: Vec<LocalEffectNode>,
    /// Delayed action ranges (CR 603.7a) closed so far.
    delayed: Vec<super::DelayedFrameNode>,
    /// A preposed `At the beginning of ... ,` timing scoping the sentence
    /// being read, with the index of its first action.
    sentence_delay: Option<(usize, super::DelayedStepTimingNode)>,
    /// The last action read after a causative `have`. A verb coordinated with
    /// it (`have it get +2/+0 and gain haste`) keeps the bare agreement.
    causative_effect: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConditionalBranch {
    Then(usize),
    Else(usize),
}

#[derive(Clone)]
struct AnaphorSnapshot {
    last_target: Option<TargetBinding>,
    permanent_subject_scope: super::PermanentSubjectScope,
    last_player_target: Option<PlayerTargetBinding>,
    current_player_subject: Option<super::PlayerEffectSubject>,
    last_object_set: Option<BoundPermanentSetNode>,
}

impl SequenceMemory {
    fn anaphor_snapshot(&self) -> AnaphorSnapshot {
        AnaphorSnapshot {
            last_target: self.last_target,
            permanent_subject_scope: self.permanent_subject_scope,
            last_player_target: self.last_player_target,
            current_player_subject: self.current_player_subject.clone(),
            last_object_set: self.last_object_set.clone(),
        }
    }

    fn restore_anaphors(&mut self, snapshot: AnaphorSnapshot) {
        self.last_target = snapshot.last_target;
        self.permanent_subject_scope = snapshot.permanent_subject_scope;
        self.last_player_target = snapshot.last_player_target;
        self.current_player_subject = snapshot.current_player_subject;
        self.last_object_set = snapshot.last_object_set;
    }

    /// Closes the conditional sentence being read at the current action count.
    fn close_conditional_branch(&mut self) {
        match self.open_branch.take() {
            Some(ConditionalBranch::Then(index)) => {
                self.conditionals[index].then_end = self.effects.len();
                self.else_candidate = Some(index);
            }
            Some(ConditionalBranch::Else(index)) => {
                self.conditionals[index].else_end = Some(self.effects.len());
            }
            None => {}
        }
    }

    /// Closes a preposed delayed timing at the end of its sentence.
    fn close_sentence_delay(&mut self) {
        if let Some((first_effect, timing)) = self.sentence_delay.take() {
            self.delayed.push(super::DelayedFrameNode {
                first_effect,
                end_effect: self.effects.len(),
                timing,
            });
        }
    }

    fn into_sequence(mut self) -> EffectSequenceNode {
        self.close_conditional_branch();
        self.close_sentence_delay();
        EffectSequenceNode {
            effects: self.effects,
            connectors: self.connectors,
            where_x: self.where_x,
            conditionals: self.conditionals,
            unless: self.unless,
            delayed: self.delayed,
        }
    }

    /// The card a pronoun may denote: the latest action introducing an object
    /// is a card-collection action. Lowering proves its singular result.
    fn card_focus_available(&self) -> bool {
        for effect in self.effects.iter().rev() {
            match effect {
                LocalEffectNode::CardAction(_) => return true,
                LocalEffectNode::ControllerLibrarySearch(_)
                | LocalEffectNode::OptionalLibrarySearch(_)
                | LocalEffectNode::DrawCards(_)
                | LocalEffectNode::OptionalDrawCards(_)
                | LocalEffectNode::MillCards(_)
                | LocalEffectNode::Scry(_)
                | LocalEffectNode::Surveil(_)
                | LocalEffectNode::CreateTokens(_)
                | LocalEffectNode::CreateNamedTokens(_)
                | LocalEffectNode::CreateExplicitTokens(_)
                | LocalEffectNode::TargetCardZoneChange(_)
                | LocalEffectNode::ZoneChange(_)
                | LocalEffectNode::OptionalCardDeployment(_)
                | LocalEffectNode::MoveCards(_)
                | LocalEffectNode::ZoneBatch(_) => return false,
                // An optional block of card actions keeps the scope's
                // singular result (lowering proves it is not replaced).
                LocalEffectNode::OptionalSequence(body)
                    if body
                        .effects
                        .iter()
                        .all(|effect| matches!(effect, LocalEffectNode::CardAction(_))) => {}
                LocalEffectNode::OptionalSequence(_) => return false,
                LocalEffectNode::PlayerAction(action)
                    if matches!(
                        action.action,
                        super::SingularPlayerAction::Draw(_)
                            | super::SingularPlayerAction::Mill(_)
                            | super::SingularPlayerAction::Hand(_)
                    ) =>
                {
                    return false;
                }
                _ => {}
            }
        }
        false
    }

    /// The singular card declared by the latest card-target zone change.
    fn card_target_antecedent(&self) -> Option<TargetBinding> {
        self.last_target.filter(|binding| {
            matches!(
                self.effects.get(binding.effect_index),
                Some(LocalEffectNode::TargetCardZoneChange(_))
            )
        })
    }

    /// Singular permanent targets moved out of the battlefield by an action.
    fn departed_permanent_targets(&self) -> Vec<TargetBinding> {
        self.effects
            .iter()
            .enumerate()
            .filter_map(|(effect_index, effect)| match effect {
                LocalEffectNode::ZoneChange(_) => Some(TargetBinding {
                    effect_index,
                    slot_index: 0,
                }),
                _ => None,
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SequenceCommitment {
    #[default]
    Tentative,
    Committed,
}

#[derive(Default)]
struct SequenceAttemptFrame {
    commitment: SequenceCommitment,
}

impl SequenceAttemptFrame {
    fn clear(&mut self) {
        self.commitment = SequenceCommitment::Tentative;
    }

    fn commit(&mut self) {
        self.commitment = SequenceCommitment::Committed;
    }

    fn is_committed(&self) -> bool {
        self.commitment == SequenceCommitment::Committed
    }
}

pub(in crate::oracle::canonical) enum EffectSequenceParseAttempt {
    Parsed(EffectSequenceNode),
    RejectedCandidate(MachineFailure),
    FailedCommitted(MachineFailure),
}

/// Composes independently recognized effects into one ordered sequence.
pub(in crate::oracle::canonical) fn recognize_effect_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<EffectSequenceNode> {
    recognize_effect_sequence_internal(
        cursor,
        name_links,
        name_failure,
        &EffectRecognitionContext::default(),
        false,
        false,
    )
}

/// Recognizes an effect sequence whose terminating period is followed by
/// activated-ability metadata rather than the end of the token stream.
pub(in crate::oracle::canonical) fn recognize_effect_sequence_before_activation_reduction(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<EffectSequenceNode> {
    recognize_effect_sequence_internal(
        cursor,
        name_links,
        name_failure,
        &EffectRecognitionContext::default(),
        true,
        false,
    )
}

/// Composes independent effects using bindings supplied by the surrounding
/// ability envelope. The wrapper above intentionally keeps every existing
/// caller on an empty context and therefore preserves its grammar exactly.
pub(in crate::oracle::canonical) fn recognize_effect_sequence_with_context(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectSequenceNode> {
    recognize_effect_sequence_internal(cursor, name_links, name_failure, context, false, false)
}

/// Transactional migration boundary for parents that must distinguish a
/// complete supported sequence from an unsupported candidate and a malformed
/// sequence whose child or connector explicitly committed.
pub(in crate::oracle::canonical) fn try_recognize_effect_sequence_with_context(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> EffectSequenceParseAttempt {
    let checkpoint = cursor.checkpoint();
    let mut attempt_frame = SequenceAttemptFrame::default();
    match recognize_effect_sequence_attempt_internal(
        cursor,
        name_links,
        name_failure,
        context,
        false,
        false,
        &mut attempt_frame,
    ) {
        Ok(node) => EffectSequenceParseAttempt::Parsed(node),
        Err(failure) if attempt_frame.is_committed() => {
            EffectSequenceParseAttempt::FailedCommitted(failure)
        }
        Err(failure) => {
            cursor.rollback(checkpoint);
            EffectSequenceParseAttempt::RejectedCandidate(failure)
        }
    }
}

/// The activation parent owns any annotation or cost-reduction sentence after
/// the final effect period. Their tokens stay unconsumed for its typed children.
pub(in crate::oracle::canonical) fn try_recognize_effect_sequence_before_activation_annotation(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> EffectSequenceParseAttempt {
    let checkpoint = cursor.checkpoint();
    let mut frame = SequenceAttemptFrame::default();
    match recognize_effect_sequence_attempt_internal(
        cursor,
        name_links,
        name_failure,
        context,
        true,
        true,
        &mut frame,
    ) {
        Ok(node) => EffectSequenceParseAttempt::Parsed(node),
        Err(failure) if frame.is_committed() => {
            EffectSequenceParseAttempt::FailedCommitted(failure)
        }
        Err(failure) => {
            cursor.rollback(checkpoint);
            EffectSequenceParseAttempt::RejectedCandidate(failure)
        }
    }
}

fn recognize_effect_sequence_internal(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
    stop_before_activation_reduction: bool,
    stop_before_activation_annotation: bool,
) -> MachineResult<EffectSequenceNode> {
    let mut attempt_frame = SequenceAttemptFrame::default();
    recognize_effect_sequence_attempt_internal(
        cursor,
        name_links,
        name_failure,
        context,
        stop_before_activation_reduction,
        stop_before_activation_annotation,
        &mut attempt_frame,
    )
}

/// The sequence owns the binding environment of the actions it reads: it
/// installs one per action and restores the enclosing one when it ends.
fn recognize_effect_sequence_attempt_internal(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
    stop_before_activation_reduction: bool,
    stop_before_activation_annotation: bool,
    attempt_frame: &mut SequenceAttemptFrame,
) -> MachineResult<EffectSequenceNode> {
    let enclosing = cursor.replace_binding_environment(None);
    let result = recognize_effect_sequence_attempt_scoped(
        cursor,
        name_links,
        name_failure,
        context,
        stop_before_activation_reduction,
        stop_before_activation_annotation,
        attempt_frame,
    );
    cursor.replace_binding_environment(enclosing);
    result
}

fn recognize_effect_sequence_attempt_scoped(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
    stop_before_activation_reduction: bool,
    stop_before_activation_annotation: bool,
    attempt_frame: &mut SequenceAttemptFrame,
) -> MachineResult<EffectSequenceNode> {
    let mut state = SequenceState::Effect;
    let mut stopped_at_activation_restriction = false;
    let mut memory = SequenceMemory {
        permanent_subject_scope: context.permanent_subject_scope,
        ..SequenceMemory::default()
    };
    loop {
        state = match state {
            SequenceState::Effect => {
                cursor.enter("sequence.effect");
                let effect_index = memory.effects.len();
                let object_set_binding = ObjectSetBinding(effect_index);
                attempt_frame.clear();
                let move_ellipsis = matches!(
                    memory.effects.last(),
                    Some(LocalEffectNode::CardAction(
                        super::CardActionNode::Move { .. }
                    ))
                );
                let mut action_context = context.clone();
                action_context.permanent_subject_scope = memory.permanent_subject_scope;
                action_context.current_effect_index = Some(effect_index);
                action_context.verb_chain_subject = match (
                    memory.connectors.last(),
                    memory.effects.last(),
                    memory.permanent_subject_scope,
                ) {
                    (
                        Some(EffectConnector::And),
                        Some(
                            LocalEffectNode::ModifyStats(_)
                            | LocalEffectNode::TemporaryKeywordGrant(_)
                            | LocalEffectNode::BecomeTypes(_),
                        ),
                        super::PermanentSubjectScope::Local(Some(reference)),
                    ) => Some(reference),
                    _ => None,
                };
                if action_context.verb_chain_subject.is_some()
                    && memory.causative_effect == effect_index.checked_sub(1)
                {
                    action_context.subject_verb_agreement =
                        super::SubjectVerbAgreement::BareInfinitive;
                }
                action_context.permanent_subject_selector = match memory.permanent_subject_scope {
                    super::PermanentSubjectScope::Local(Some(
                        super::PermanentEffectReference::Target(binding),
                    )) => memory
                        .effects
                        .get(binding.effect_index)
                        .and_then(|effect| {
                            super::permanent_subject::target_slot_selector(
                                effect,
                                binding.slot_index,
                            )
                        })
                        .cloned(),
                    super::PermanentSubjectScope::Local(Some(
                        super::PermanentEffectReference::Observed(
                            super::TypedObservedEventObject::MovedOntoBattlefield,
                        ),
                    )) => memory
                        .effects
                        .iter()
                        .rev()
                        .find_map(super::permanent_subject::moved_onto_battlefield_selector)
                        .flatten(),
                    _ => None,
                };
                // `its controller` / `that NOUN's owner`: the singular object
                // of the previous action (a countered spell, or the permanent
                // subject scope), with the selector that declared it.
                let object_antecedent = match memory.effects.last() {
                    Some(LocalEffectNode::CounterSpell(counter))
                        if counter.target.slots.len() == 1
                            && counter.target.slots[0].cardinality.maximum == 1 =>
                    {
                        Some(super::ObjectSubjectAntecedent {
                            reference: super::PermanentEffectReference::Target(TargetBinding {
                                effect_index: effect_index - 1,
                                slot_index: 0,
                            }),
                            selector: Some(counter.target.slots[0].selector.clone()),
                        })
                    }
                    Some(LocalEffectNode::CounterSpell(_)) => None,
                    _ => super::permanent_subject::proven_permanent_antecedent(&action_context)
                        .map(|reference| super::ObjectSubjectAntecedent {
                            reference,
                            selector: match reference {
                                super::PermanentEffectReference::Target(binding) => memory
                                    .effects
                                    .get(binding.effect_index)
                                    .and_then(|effect| {
                                        super::permanent_subject::target_slot_selector(
                                            effect,
                                            binding.slot_index,
                                        )
                                    })
                                    .cloned(),
                                // The card a move put onto the battlefield: its
                                // declared card predicate names the permanent.
                                super::PermanentEffectReference::Observed(
                                    super::TypedObservedEventObject::MovedOntoBattlefield,
                                ) => memory
                                    .effects
                                    .iter()
                                    .rev()
                                    .find_map(
                                        super::permanent_subject::moved_onto_battlefield_selector,
                                    )
                                    .flatten(),
                                _ => None,
                            },
                        }),
                };
                // One environment for every reference child of this action:
                // the frame's event bindings and the lexical antecedents.
                cursor.replace_binding_environment(Some(std::sync::Arc::new(
                    super::BindingEnvironment {
                        frame: super::FrameBindings::from_context(&action_context),
                        local_object_scope: matches!(
                            memory.permanent_subject_scope,
                            super::PermanentSubjectScope::Local(_)
                        ),
                        object: object_antecedent.clone(),
                        player: memory
                            .last_player_target
                            .map(|binding| super::PlayerBinding(binding.effect_index)),
                        subject: None,
                        effect_index: Some(effect_index),
                    },
                )));
                let base_action_context = action_context;
                let mut preposed_duration = memory.sentence_duration.clone();
                let mut opened_preposed_duration = false;
                // `For each COUNTABLE, ACTION.`: the preposed form of the
                // trailing multiplier, scaling the one action of its sentence.
                let mut preposed_for_each: Option<PreposedForEach> = None;
                let mut effect_start = cursor.position();
                let mut distributed_rest = Vec::new();
                let mut causative: bool;
                let mut effect = loop {
                    if let Some(pending) = memory.distributed.pop() {
                        // A distributed per-player action is already read.
                        causative = false;
                        break pending;
                    }
                    let mut action_context = base_action_context.clone();
                    action_context.preposed_duration = preposed_duration.clone();
                    // `have SUBJECT VERB` (`you may have target player lose 3
                    // life`): the causative only names the performer; the
                    // ordinary subject-verb readers own the action with the
                    // bare verb form. Its failure stays transactional.
                    causative = super::causative_action_starts(cursor);
                    if causative {
                        action_context.subject_verb_agreement =
                            super::SubjectVerbAgreement::BareInfinitive;
                    }
                    let mut read_action = |attempt: &mut OracleTokenCursor<'_>,
                                           inherited: Option<&super::PlayerEffectSubject>,
                                           attempt_frame: &mut SequenceAttemptFrame|
                     -> MachineResult<LocalEffectNode> {
                        if causative {
                            attempt.enter("effect.causative.verb");
                            attempt.consume("have")?;
                        }
                        // `sacrifice A, B, and C`: one verb, independent
                        // headed objects, each its own sacrifice action. Any
                        // other `and a ...` clause stays with the other readers.
                        // `deals 1 damage to A and 2 damage to B`: the second
                        // predicate shares the previous action's damage source.
                        if memory.connectors.last() == Some(&EffectConnector::And)
                            && let Some(LocalEffectNode::DealDamage(previous)) =
                                memory.effects.last()
                        {
                            let previous_index = effect_index - 1;
                            // The previous action's single target is a bound
                            // target here; the shared predicate reads its
                            // clause subject from the source operand.
                            let source = match &previous.source {
                                super::PermanentOperandNode::Targets(targets)
                                    if targets.slots.len() == 1 =>
                                {
                                    super::PermanentOperandNode::BoundReference(
                                        super::PermanentEffectReference::Target(TargetBinding {
                                            effect_index: previous_index,
                                            slot_index: 0,
                                        }),
                                    )
                                }
                                source => source.clone(),
                            };
                            if let Ok(node) = attempt.attempt(|child| {
                                super::recognize_gapped_damage_predicate(child, source)
                            }) {
                                return Ok(LocalEffectNode::DealDamage(node));
                            }
                        }
                        if memory.connectors.last() == Some(&EffectConnector::And)
                            && super::elided_sacrifice_operand_starts(
                                attempt,
                                memory.effects.last(),
                            )
                            && let Ok(operand) =
                                attempt.attempt(super::recognize_elided_sacrifice_operand)
                        {
                            return Ok(LocalEffectNode::Sacrifice(operand));
                        }
                        // `[SUBJECT] gain(s)|lose(s) AMOUNT life`: the shared
                        // life clause owns its subject policy and agreement.
                        match super::try_recognize_life_effect_clause(
                            attempt,
                            super::LifeEffectBindings {
                                declared_target: Some(super::PlayerBinding(effect_index)),
                                player_antecedent: memory
                                    .last_player_target
                                    .map(|binding| super::PlayerBinding(binding.effect_index)),
                                inherited: inherited,
                                object_subject: object_antecedent.clone(),
                                object_antecedent: memory.last_target,
                            },
                            &action_context,
                        ) {
                            Ok(Some(effect)) => return Ok(effect),
                            Ok(None) => {}
                            Err(failure) => return Err(failure),
                        }
                        // The keyword action `venture into …` (CR 701.49) is
                        // an action head even when the lexical name resolver
                        // left its words unresolved; it is never a source name.
                        if super::venture_action_starts(attempt)
                            || super::defending_player_starts(attempt)
                            || !source_reference_starts_at(
                                attempt.position(),
                                attempt.peek(),
                                name_links,
                                name_failure,
                            )
                        {
                            match super::try_recognize_coordinated_player_actions(
                                attempt,
                                effect_index,
                                memory
                                    .last_player_target
                                    .map(|binding| super::PlayerBinding(binding.effect_index)),
                                &action_context,
                            ) {
                                Ok(Some(actions)) => {
                                    let mut actions = actions
                                        .into_iter()
                                        .map(LocalEffectNode::PlayerAction)
                                        .collect::<Vec<_>>();
                                    actions.reverse();
                                    let first = actions.pop().expect("two or more subjects");
                                    distributed_rest = actions;
                                    return Ok(first);
                                }
                                Ok(None) => {}
                                Err(failure) => {
                                    attempt_frame.commit();
                                    return Err(failure);
                                }
                            }
                            match super::try_recognize_player_action_with_object_antecedent(
                                attempt,
                                super::PlayerBinding(effect_index),
                                memory
                                    .last_player_target
                                    .map(|binding| super::PlayerBinding(binding.effect_index)),
                                inherited,
                                object_antecedent.clone(),
                                &action_context,
                            ) {
                                Ok(Some(action)) => {
                                    return Ok(LocalEffectNode::PlayerAction(action));
                                }
                                Ok(None) => {}
                                Err(failure) => {
                                    attempt_frame.commit();
                                    return Err(failure);
                                }
                            }
                        }
                        // `exile|sacrifice it` on the permanent a move put
                        // onto the battlefield or a declared permanent
                        // target: the bound permanent, never a card
                        // collection.
                        let permanent_antecedent =
                            object_antecedent.as_ref().is_some_and(|antecedent| {
                                matches!(
                                    antecedent.reference,
                                    super::PermanentEffectReference::Observed(
                                        super::TypedObservedEventObject::MovedOntoBattlefield,
                                    )
                                ) || (matches!(
                                    antecedent.reference,
                                    super::PermanentEffectReference::Target(_)
                                ) && antecedent.selector.is_some())
                            });
                        if permanent_antecedent
                            && let Some(node) = super::recognize_referenced_permanent_zone_action(
                                attempt,
                                name_links,
                                name_failure,
                                &action_context,
                            )
                        {
                            return node.map(LocalEffectNode::DestroyReference);
                        }
                        match super::try_recognize_target_card_action(attempt) {
                            super::CollectionOperandAttempt::Recognized(fragment) => {
                                return Ok(
                                    if let Some(compat) =
                                        super::legacy_target_card_move(&fragment.node)
                                    {
                                        LocalEffectNode::TargetCardZoneChange(compat)
                                    } else {
                                        LocalEffectNode::CardAction(fragment.node)
                                    },
                                );
                            }
                            super::CollectionOperandAttempt::FailedCommitted(failure) => {
                                attempt_frame.commit();
                                return Err(failure);
                            }
                            super::CollectionOperandAttempt::NotClaimed
                            | super::CollectionOperandAttempt::RejectedCandidate(_) => {}
                        }
                        if let Ok(Some(action)) = attempt.attempt(|child| {
                            super::recognize_card_action(
                                child,
                                move_ellipsis,
                                action_context.preposed_duration.as_ref(),
                            )
                        }) {
                            return Ok(LocalEffectNode::CardAction(action));
                        }
                        let legacy = attempt.attempt(|child| {
                            recognize_effect(
                                child,
                                name_links,
                                name_failure,
                                memory.last_target,
                                object_set_binding,
                                memory.last_object_set.as_ref(),
                                &action_context,
                                attempt_frame,
                            )
                        });
                        match legacy {
                            Ok(effect) => Ok(effect),
                            Err(failure)
                                if !memory.effects.is_empty()
                                    && attempt
                                        .peek()
                                        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
                                    && attempt
                                        .peek_ahead(1)
                                        .is_some_and(|raw| raw.eq_ignore_ascii_case("may")) =>
                            {
                                // CR 118.12: the payer decides before the
                                // permission is exercised; a `you may` body
                                // cannot represent that order, so it keeps
                                // rejecting `unless`.
                                let mut permission_context = action_context.clone();
                                permission_context.unless_frames =
                                    super::UnlessFrameSupport::Unsupported;
                                attempt
                                    .attempt(|body| {
                                        super::recognize_optional_sequence(
                                            body,
                                            name_links,
                                            name_failure,
                                            &permission_context,
                                        )
                                    })
                                    .map(LocalEffectNode::OptionalSequence)
                                    .or(Err(failure))
                            }
                            Err(failure) => Err(failure),
                        }
                    };
                    let parsed = cursor.attempt(|attempt| {
                        read_action(
                            attempt,
                            memory.current_player_subject.as_ref(),
                            attempt_frame,
                        )
                    });
                    // `you VERB ...`: the controller named as the explicit
                    // subject performs the same clause as the imperative
                    // (CR 109.5); every action reader takes it as the
                    // subject the verb continues, never a new declaration.
                    let parsed = match parsed {
                        Err(failure)
                            if !causative
                                && !attempt_frame.is_committed()
                                && super::controller_subject_starts(cursor) =>
                        {
                            let controller = super::PlayerEffectSubject::Controller;
                            cursor
                                .attempt(|attempt| {
                                    attempt.enter("effect.subject.controller");
                                    attempt.consume("you")?;
                                    let verb = attempt.clone();
                                    read_action(attempt, Some(&controller), attempt_frame).map_err(
                                        |retry| {
                                            if retry.state == "effect.start"
                                                && retry.expected == "a supported effect subject"
                                            {
                                                super::clause_verb_unsupported(
                                                    &verb,
                                                    "the controller (`you`)",
                                                )
                                            } else {
                                                retry
                                            }
                                        },
                                    )
                                })
                                .map_err(|retry| {
                                    // The original reader's failure stands
                                    // when the retry claims nothing more.
                                    if retry.token_index < failure.token_index {
                                        failure
                                    } else {
                                        retry
                                    }
                                })
                        }
                        parsed => parsed,
                    };
                    // `PLAYER VERBs ...`: another player performs the clause
                    // (CR 608.2c). The clause reads the subject and checks the
                    // verb's agreement; the ordinary readers read the predicate
                    // as that player's own instruction.
                    let parsed = match parsed {
                        Err(failure)
                            if !causative
                                && (!attempt_frame.is_committed()
                                    || matches!(
                                        failure.state,
                                        "effect.quantified-player.action"
                                            | "clause.verb.unsupported"
                                    )) =>
                        {
                            let furthest = |first: super::super::MachineFailure,
                                            second: super::super::MachineFailure| {
                                if second.token_index > first.token_index {
                                    second
                                } else {
                                    first
                                }
                            };
                            let performer_context = super::PerformerContext {
                                player_antecedent: memory
                                    .last_player_target
                                    .map(|binding| super::PlayerBinding(binding.effect_index)),
                                event_player: base_action_context.frame.event_player,
                                defending_player: base_action_context.frame.defending_player,
                                object_antecedent: object_antecedent.clone(),
                            };
                            let mut head = cursor.clone();
                            // A subjectless verb continues the performer of
                            // the previous action of its sentence.
                            let continued = memory
                                .current_performer
                                .clone()
                                .filter(|(_, _, sentence)| *sentence == memory.sentence_start)
                                .map(|(performer, agreement, _)| Ok((performer, agreement)));
                            let performer =
                                super::recognize_performer(&mut head, &performer_context).or_else(
                                    || {
                                        head = cursor.clone();
                                        continued
                                    },
                                );
                            match performer {
                                None => Err(failure),
                                Some(Err(subject)) => Err(furthest(failure, subject)),
                                Some(Ok((performer, agreement))) => {
                                    let description = super::performer_description(&performer);
                                    match super::person_shifted_tokens(
                                        &head,
                                        agreement,
                                        description,
                                    ) {
                                        Err(verb) => Err(furthest(failure, verb)),
                                        Ok(tokens) => {
                                            let mut shifted = head.with_tokens(&tokens);
                                            let controller = super::PlayerEffectSubject::Controller;
                                            // `PLAYER may ...`: the performer's
                                            // own optional sentence, read from
                                            // the subject's last word as `you`.
                                            let modal = head
                                                .peek()
                                                .is_some_and(|raw| raw.eq_ignore_ascii_case("may"));
                                            let read = if modal {
                                                let at = shifted.position() - 1;
                                                shifted.rollback_to_position(at);
                                                super::recognize_optional_sequence(
                                                    &mut shifted,
                                                    name_links,
                                                    name_failure,
                                                    &action_context,
                                                )
                                                .map(LocalEffectNode::OptionalSequence)
                                            } else {
                                                read_action(
                                                    &mut shifted,
                                                    Some(&controller),
                                                    attempt_frame,
                                                )
                                            };
                                            match read {
                                                Ok(effect) => {
                                                    let end = shifted.position();
                                                    cursor.advance_to(end).map(|()| {
                                                        LocalEffectNode::Performed(
                                                            super::PerformedEffectNode {
                                                                performer,
                                                                effect: Box::new(effect),
                                                            },
                                                        )
                                                    })
                                                }
                                                Err(action) => Err(furthest(failure, action)),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        parsed => parsed,
                    };
                    match parsed {
                        Ok(effect) => break effect,
                        Err(failure) => {
                            // `If CONDITION, EFFECTS.` / `Otherwise, EFFECTS.`:
                            // structural introducers of a whole sentence, read
                            // only after the ordinary readers decline at its
                            // first action, never after a committed failure,
                            // and only for parents that lower the ranges.
                            if !attempt_frame.is_committed()
                                && preposed_duration.is_none()
                                && context.sentence_conditionals
                                && memory.open_branch.is_none()
                                && memory.effects.len() == memory.sentence_start
                            {
                                if cursor
                                    .peek()
                                    .is_some_and(|raw| raw.eq_ignore_ascii_case("otherwise"))
                                {
                                    cursor.enter("sequence.conditional.otherwise");
                                    let index = memory
                                    .else_candidate
                                    .filter(|index| {
                                        memory.conditionals[*index].then_end
                                            == memory.effects.len()
                                    })
                                    .ok_or_else(|| {
                                        cursor.fail(
                                            "an `Otherwise` sentence directly after a conditional sentence",
                                        )
                                    })?;
                                    cursor.consume("otherwise")?;
                                    cursor.consume(",")?;
                                    let snapshot = memory.branch_snapshots[index].clone();
                                    memory.restore_anaphors(snapshot);
                                    memory.open_branch = Some(ConditionalBranch::Else(index));
                                    memory.else_candidate = None;
                                    attempt_frame.clear();
                                    effect_start = cursor.position();
                                    continue;
                                }
                                let follows_clause_comma = cursor.position() > 0
                                    && cursor
                                        .token_slice(cursor.position() - 1, cursor.position())
                                        .first()
                                        .is_some_and(|token| token.raw == ",");
                                if super::sentence_condition_starts(cursor) && !follows_clause_comma
                                {
                                    let departed = memory.departed_permanent_targets();
                                    let condition_context = super::SentenceConditionContext {
                                        name_links,
                                        name_failure,
                                        effects: &base_action_context,
                                        card_target: memory.card_target_antecedent(),
                                        card_focus: memory.card_focus_available(),
                                        departed_targets: &departed,
                                    };
                                    let mut trial = cursor.clone();
                                    let condition = super::recognize_sentence_condition(
                                        &mut trial,
                                        &condition_context,
                                    )?;
                                    *cursor = trial;
                                    let snapshot = memory.anaphor_snapshot();
                                    memory.branch_snapshots.push(snapshot);
                                    memory.conditionals.push(super::SentenceConditionalNode {
                                        condition,
                                        then_first: memory.effects.len(),
                                        then_end: memory.effects.len(),
                                        else_end: None,
                                    });
                                    memory.open_branch = Some(ConditionalBranch::Then(
                                        memory.conditionals.len() - 1,
                                    ));
                                    memory.else_candidate = None;
                                    attempt_frame.clear();
                                    effect_start = cursor.position();
                                    continue;
                                }
                            }
                            // `At the beginning of the next STEP, EFFECTS.`
                            // (CR 603.7a): a structural introducer scoping
                            // the actions of its sentence, read only after
                            // the ordinary readers decline at its first action.
                            if !attempt_frame.is_committed()
                                && preposed_duration.is_none()
                                && memory.sentence_delay.is_none()
                                && memory.effects.len() == memory.sentence_start
                                && super::delayed_step_timing_starts(cursor)
                            {
                                let mut trial = cursor.clone();
                                let timing = super::recognize_delayed_step_timing(&mut trial)?;
                                trial.enter("sequence.delayed.preposed-separator");
                                trial.consume(",")?;
                                *cursor = trial;
                                memory.sentence_delay = Some((memory.effects.len(), timing));
                                attempt_frame.clear();
                                effect_start = cursor.position();
                                continue;
                            }
                            // `For each COUNTABLE, ACTION.` (the trailing
                            // multiplier written first): a structural
                            // introducer of the sentence's one action.
                            if !attempt_frame.is_committed()
                                && preposed_for_each.is_none()
                                && preposed_duration.is_none()
                                && memory.effects.len() == memory.sentence_start
                                && for_each_count_starts(cursor)
                            {
                                // `For each opponent, ...`: a population of
                                // players is the distributive frame, not a
                                // countable; its explicit failure names why.
                                if let Some(width) = player_population_width(cursor) {
                                    attempt_frame.commit();
                                    return Err(player_iteration_failure(cursor, width));
                                }
                                let mut trial = cursor.clone();
                                let count = recognize_preposed_for_each(&mut trial)?;
                                trial.enter("sequence.for-each.preposed-separator");
                                trial.consume(",")?;
                                *cursor = trial;
                                preposed_for_each = Some(count);
                                attempt_frame.clear();
                                effect_start = cursor.position();
                                continue;
                            }
                            // `Until DURATION, EFFECTS.`: the introducer is read
                            // only after the ordinary effect readers decline, and
                            // never after a committed failure (diagnostics of
                            // those readers stay authoritative).
                            if attempt_frame.is_committed()
                                || preposed_duration.is_some()
                                || !cursor
                                    .peek()
                                    .is_some_and(|raw| raw.eq_ignore_ascii_case("until"))
                            {
                                return Err(failure);
                            }
                            let mut trial = cursor.clone();
                            let Ok(duration) = super::recognize_play_window_duration(&mut trial)
                            else {
                                return Err(failure);
                            };
                            if trial.consume(",").is_err() {
                                return Err(failure);
                            }
                            *cursor = trial;
                            attempt_frame.clear();
                            preposed_duration = Some(duration);
                            opened_preposed_duration = true;
                            effect_start = cursor.position();
                        }
                    }
                };
                memory.distributed.extend(distributed_rest);
                attempt_frame.clear();
                if let Some(preposed) = preposed_for_each.take() {
                    attempt_frame.commit();
                    cursor.enter("sequence.for-each.preposed-action");
                    let count = match preposed {
                        PreposedForEach::Count(count) => count,
                        PreposedForEach::ActionResult(reference) => {
                            // CR 608.2c: an earlier action of this program
                            // affected the objects; the scaled action follows.
                            let mut written = memory.effects.clone();
                            written.push(effect.clone());
                            let (effect_index, filter) = cursor.require_value(
                                super::resolve_action_result_antecedent(&written, &reference),
                                "an earlier action of this program whose affected objects the phrase restates",
                            )?;
                            super::NumericExpression::ActionResultCount {
                                effect_index,
                                filter,
                            }
                        }
                    };
                    cursor.require(
                        matches!(cursor.peek(), None | Some(".") | Some("(")),
                        "one action ending the sentence of a preposed `for each`",
                    )?;
                    cursor.require(
                        apply_for_each_multiplier(&mut effect, count),
                        "an action with a literal quantity that a preposed `for each` can multiply",
                    )?;
                }
                if let Some(duration) = &preposed_duration {
                    let carries_duration = match &effect {
                        LocalEffectNode::ModifyStats(node) => &node.duration == duration,
                        LocalEffectNode::TemporaryKeywordGrant(node) => &node.duration == duration,
                        LocalEffectNode::BecomeTypes(node) => {
                            node.duration.as_ref() == Some(duration)
                        }
                        LocalEffectNode::CardAction(super::CardActionNode::Permit(node)) => {
                            node.duration.as_ref() == Some(duration)
                        }
                        LocalEffectNode::ControlChange(super::ControlChangeEffectNode {
                            change:
                                super::ControlChange::Gain {
                                    duration: Some(own),
                                },
                            ..
                        }) => own == duration,
                        _ => false,
                    };
                    let own_duration = cursor
                        .token_slice(effect_start, cursor.position())
                        .iter()
                        .any(|token| token.raw.eq_ignore_ascii_case("until"));
                    if !carries_duration || own_duration {
                        attempt_frame.commit();
                        cursor.enter("sequence.preposed-duration.scope");
                        return Err(cursor.fail(
                            "only duration-bearing continuous effects, each without its own duration, after a preposed duration",
                        ));
                    }
                    if opened_preposed_duration {
                        memory.sentence_duration = Some(duration.clone());
                    }
                }
                if matches!(
                    &effect,
                    LocalEffectNode::CardAction(_) | LocalEffectNode::OptionalSequence(_)
                ) {
                    memory.last_target = None;
                }
                match &mut effect {
                    LocalEffectNode::ControllerLibrarySearch(search)
                    | LocalEffectNode::OptionalLibrarySearch(search) => {
                        search.binding = CollectionBinding(memory.next_search_binding);
                        memory.next_search_binding += 1;
                        // A searched collection is a new object antecedent, but it is
                        // not a target slot. Until collection/object anaphora is
                        // modeled explicitly, never let `it` fall back to a target
                        // chosen by an earlier, unrelated effect.
                        memory.last_target = None;
                    }
                    _ => {}
                }
                if let LocalEffectNode::ZoneChange(zone_change) = &effect {
                    // A collection is not a singular object antecedent. Clear
                    // an earlier target instead of letting it leak into `it`.
                    memory.last_target = (zone_change.target.slots.len() == 1
                        && zone_change.target.slots[0].cardinality.maximum == 1)
                        .then_some(TargetBinding {
                            effect_index,
                            slot_index: 0,
                        });
                }
                if matches!(
                    &effect,
                    LocalEffectNode::CardAction(
                        super::CardActionNode::Move {
                            cards: super::CardOperandNode::Targets(_),
                            ..
                        } | super::CardActionNode::Reveal(super::CardOperandNode::Targets(_))
                    )
                ) {
                    // A new card declaration is not the earlier permanent
                    // antecedent. Card anaphora needs its own typed binding.
                    memory.last_target = None;
                }
                if matches!(effect, LocalEffectNode::TargetCardZoneChange(_)) {
                    memory.last_target = Some(TargetBinding {
                        effect_index,
                        slot_index: 0,
                    });
                }
                if let LocalEffectNode::Attach(attach) = &effect {
                    // The destination is the singular object a following
                    // `that creature` / `it` denotes; the attached object is not.
                    memory.last_target = match &attach.recipient {
                        super::PermanentOperandNode::Targets(targets) => (targets.slots.len() == 1
                            && targets.slots[0].cardinality.maximum == 1)
                            .then_some(TargetBinding {
                                effect_index,
                                slot_index: 0,
                            }),
                        _ => None,
                    };
                }
                if let Some(subject) = match &effect {
                    LocalEffectNode::ModifyStats(modifier) => Some(&modifier.targets),
                    LocalEffectNode::BecomeTypes(change) => Some(&change.objects),
                    _ => None,
                } {
                    if let super::PermanentOperandNode::Targets(targets) = subject {
                        memory.last_target = (targets.slots.len() == 1
                            && targets.slots[0].cardinality.maximum == 1)
                            .then_some(TargetBinding {
                                effect_index,
                                slot_index: 0,
                            });
                    }
                }
                if let LocalEffectNode::Choose(choice) = &effect {
                    // The chosen target is the antecedent of a later `it`,
                    // `that creature` or `that player`; a plural choice is not.
                    let singular = choice.declared_targets().is_some_and(|targets| {
                        targets.slots.len() == 1 && targets.slots[0].cardinality.maximum == 1
                    });
                    match &choice.domain {
                        super::ChoiceDomainNode::PermanentTargets(_) => {
                            memory.last_target = singular.then_some(TargetBinding {
                                effect_index,
                                slot_index: 0,
                            });
                        }
                        super::ChoiceDomainNode::PlayerTarget(_) => {
                            memory.last_player_target = Some(PlayerTargetBinding { effect_index });
                        }
                        // A characteristic choice introduces no object or
                        // player antecedent; earlier antecedents remain.
                        super::ChoiceDomainNode::CreatureType => {}
                    }
                }
                if let LocalEffectNode::TargetPlayerLife(life) = &effect
                    && life.declares_target
                {
                    memory.last_player_target = Some(life.player);
                }
                memory.current_performer = match &effect {
                    LocalEffectNode::Performed(node) => Some((
                        node.performer.clone(),
                        performed_agreement(&node.performer, cursor, effect_start),
                        memory.sentence_start,
                    )),
                    // The ordinary readers of a player subject: its later
                    // verbs of the sentence continue the same performer.
                    LocalEffectNode::QuantifiedPlayerAction(node) => Some((
                        super::PerformerNode::Population {
                            players: node.players,
                            excluding_controller: node.excluding_controller,
                        },
                        super::VerbAgreement::ThirdPersonSingular,
                        memory.sentence_start,
                    )),
                    LocalEffectNode::PlayerAction(node)
                        if matches!(
                            node.player,
                            super::PlayerEffectSubject::EventPlayer
                                | super::PlayerEffectSubject::Bound(_)
                                | super::PlayerEffectSubject::ObjectRelation { .. }
                        ) =>
                    {
                        Some((
                            super::PerformerNode::Player(node.player.clone()),
                            performed_agreement_of_player(cursor, effect_start),
                            memory.sentence_start,
                        ))
                    }
                    _ => None,
                };
                memory.current_player_subject = match &effect {
                    LocalEffectNode::PlayerAction(action) => {
                        if let super::PlayerEffectSubject::DeclaredTarget { binding, .. } =
                            &action.player
                        {
                            memory.last_player_target = Some(PlayerTargetBinding {
                                effect_index: binding.0,
                            });
                        }
                        Some(super::continuing_player_subject(&action.player))
                    }
                    LocalEffectNode::TargetPlayerLife(life) => {
                        Some(super::PlayerEffectSubject::Bound(super::PlayerBinding(
                            life.player.effect_index,
                        )))
                    }
                    // Other actions and nested programs do not transport an
                    // implicit singular player subject into a later clause.
                    _ => None,
                };
                if let LocalEffectNode::LoseAbilities(loss) = &effect {
                    memory.last_object_set = Some(loss.objects.clone());
                }
                memory.permanent_subject_scope =
                    super::permanent_subject::permanent_subject_after_effect(
                        &effect,
                        effect_index,
                        memory.permanent_subject_scope,
                    );
                if let LocalEffectNode::PutCounters(node) = &effect {
                    if let super::PermanentOperandNode::Targets(targets) = &node.permanent {
                        memory.last_target = (targets.slots.len() == 1
                            && targets.slots[0].cardinality.maximum == 1)
                            .then_some(TargetBinding {
                                effect_index,
                                slot_index: 0,
                            });
                    }
                }
                memory.causative_effect = causative.then_some(effect_index);
                memory.effects.push(effect);
                if !matches!(memory.open_branch, Some(ConditionalBranch::Else(_))) {
                    memory.else_candidate = None;
                }
                SequenceState::ConnectorOrEnd
            }
            SequenceState::ConnectorOrEnd => {
                cursor.enter("sequence.connector-or-end");
                if !memory.distributed.is_empty() {
                    // The coordinated subjects' remaining actions belong to
                    // the same sentence: one `and` joins each distributed action.
                    memory.connectors.push(EffectConnector::And);
                    state = SequenceState::Effect;
                    continue;
                }
                // An `unless` clause closes its guarded actions: only the
                // sentence end or a postposed `where X` may follow it.
                if memory
                    .unless
                    .last()
                    .is_some_and(|frame| frame.end_effect == memory.effects.len())
                    && !matches!(cursor.peek(), None | Some("."))
                    && !super::where_x_definition_starts(cursor)
                {
                    cursor.enter("sequence.unless.sentence-end");
                    return Err(cursor.fail("a sentence end after the `unless` clause"));
                }
                // A quoted ability that ends with its own period has already
                // closed this sentence inside the quotation marks.
                let sentence_closed_by_quote = super::quoted_sentence_just_closed(cursor)
                    && !matches!(
                        cursor.peek().map(str::to_ascii_lowercase).as_deref(),
                        Some("and" | "then" | ",")
                    );
                let sentence_head = if sentence_closed_by_quote {
                    Some(".".to_string())
                } else {
                    cursor.peek().map(str::to_ascii_lowercase)
                };
                match sentence_head.as_deref() {
                    Some(",") if super::where_x_definition_starts(cursor) => {
                        // CR 107.3: the sentence parent owns the scope of X;
                        // the child only reads the postposed definition.
                        let expression = super::recognize_where_x_definition(cursor)?;
                        cursor.enter("sequence.where-x.scope");
                        cursor.require(
                            memory.sentence_start < memory.effects.len()
                                && !memory
                                    .where_x
                                    .iter()
                                    .any(|binding| binding.first_effect == memory.sentence_start),
                            "one definition of X for the actions of this sentence",
                        )?;
                        memory.where_x.push(super::WhereXBindingNode {
                            first_effect: memory.sentence_start,
                            end_effect: memory.effects.len(),
                            expression,
                        });
                        SequenceState::ConnectorOrEnd
                    }
                    Some("and" | "then" | ",") => {
                        if matches!(
                            memory.effects.last(),
                            Some(
                                LocalEffectNode::OptionalDrawCards(_)
                                    | LocalEffectNode::OptionalLibrarySearch(_)
                                    | LocalEffectNode::OptionalCardDeployment(_)
                            )
                        ) {
                            attempt_frame.commit();
                            cursor.enter("sequence.optional.permission-group");
                            return Err(cursor.fail(
                                "a represented optional action group, or a period ending the current permission",
                            ));
                        }
                        let semantic_connector_head = cursor.peek().is_some_and(|raw| {
                            raw.eq_ignore_ascii_case("and") || raw.eq_ignore_ascii_case("then")
                        }) || (cursor.peek() == Some(",")
                            && cursor.peek_ahead(1).is_some_and(|raw| {
                                raw.eq_ignore_ascii_case("and") || raw.eq_ignore_ascii_case("then")
                            }));
                        let connector = match recognize_effect_connector(cursor) {
                            Ok(connector) => connector,
                            Err(failure) => {
                                if semantic_connector_head {
                                    attempt_frame.commit();
                                }
                                return Err(failure);
                            }
                        };
                        if cursor.peek().is_none() || cursor.peek() == Some(".") {
                            attempt_frame.commit();
                            cursor.enter("sequence.connector.effect");
                            return Err(cursor.fail("an effect after the connector"));
                        }
                        attempt_frame.clear();
                        memory.connectors.push(connector);
                        SequenceState::Effect
                    }
                    Some("instead") if memory.open_branch.is_some() => {
                        cursor.enter("sequence.conditional.instead");
                        return Err(cursor.fail(
                            "a conditional sentence that does not replace an earlier instruction with `instead`",
                        ));
                    }
                    Some(".") => {
                        memory.current_player_subject = None;
                        memory.sentence_duration = None;
                        memory.close_sentence_delay();
                        if !sentence_closed_by_quote {
                            cursor.consume(".")?;
                        }
                        memory.close_conditional_branch();
                        if super::copy_new_targets_amendment_starts(cursor) {
                            // CR 707.10c: the permission amends the copy
                            // instruction of the sentence just closed.
                            attempt_frame.commit();
                            let antecedent = match memory.effects.last_mut() {
                                Some(LocalEffectNode::CopyStackObject(copy)) => Some(copy),
                                _ => None,
                            };
                            super::recognize_copy_new_targets_amendment(cursor, antecedent)?;
                            cursor.enter("sequence.copy-new-targets.terminator");
                            cursor.consume(".")?;
                        }
                        if library_look_reminder_starts_at(cursor)
                            && let Some(reminded) =
                                memory.effects.iter().rev().find_map(|effect| match effect {
                                    LocalEffectNode::Scry(scry) => {
                                        Some((LibraryLookAction::Scry, &scry.count))
                                    }
                                    LocalEffectNode::Surveil(surveil) => {
                                        Some((LibraryLookAction::Surveil, &surveil.count))
                                    }
                                    _ => None,
                                })
                        {
                            let (action, count) = reminded;
                            recognize_library_look_reminder(cursor, action, count)?;
                            cursor.enter("sequence.library-look-reminder.complete");
                            cursor.finish()?;
                            return Ok(memory.into_sequence());
                        }
                        if cursor.peek().is_some_and(|raw| raw == "(")
                            && let Some(LocalEffectNode::CreateNamedTokens(creation)) =
                                memory.effects.last()
                        {
                            recognize_named_token_creation_reminder(cursor, &creation.creation)?;
                            cursor.enter("sequence.named-token.complete");
                            cursor.finish()?;
                            return Ok(memory.into_sequence());
                        }
                        if cursor.peek() == Some("(")
                            && !cursor
                                .peek_ahead(1)
                                .is_some_and(|raw| raw.eq_ignore_ascii_case("activate"))
                            && let Some(LocalEffectNode::EmpowerJace(empower)) =
                                memory.effects.last()
                        {
                            // CR 701.71a: the reminder restates the keyword
                            // action; it is validated, then the sentence
                            // boundary logic below continues as usual.
                            recognize_empower_jace_reminder(cursor, &empower.count)?;
                        }
                        if cursor.peek() == Some("(")
                            && cursor
                                .peek_ahead(1)
                                .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
                            && cursor
                                .peek_ahead(2)
                                .is_some_and(|raw| raw.eq_ignore_ascii_case("next"))
                            && let Some(LocalEffectNode::PermanentKeywordAction(regenerate)) =
                                memory.effects.last()
                            && regenerate.action == super::PermanentKeywordAction::Regenerate
                        {
                            // CR 701.15a: the reminder restates the keyword
                            // action and is validated against its object.
                            super::recognize_regenerate_reminder(cursor, &regenerate.targets)?;
                        }
                        // Only an `(Activate ...)` parenthetical is an
                        // activation annotation; any other one is reminder text.
                        let activation_annotation_follows = stop_before_activation_annotation
                            && cursor.peek() == Some("(")
                            && cursor
                                .peek_ahead(1)
                                .is_some_and(|raw| raw.eq_ignore_ascii_case("activate"));
                        // `manifest dread` restates its own look in its
                        // reminder (CR 701.62a); a library-look reminder is
                        // otherwise owned by the scry/surveil validation above.
                        let reminder_owned_by_action = matches!(
                            memory.effects.last(),
                            Some(LocalEffectNode::ControllerKeywordAction(
                                super::ControllerKeywordActionNode {
                                    action: super::ControllerKeywordAction::ManifestDread,
                                    ..
                                }
                            ))
                        );
                        if cursor.peek() == Some("(")
                            && !activation_annotation_follows
                            && (reminder_owned_by_action
                                || !library_look_reminder_starts_at(cursor))
                        {
                            // CR 207.2: reminder text has no rules meaning.
                            // Typed reminders above keep their validation;
                            // any other balanced parenthetical after a
                            // complete sentence is recorded and skipped. An
                            // activation parent keeps only its `(Activate ...)`
                            // timing annotation, which it validates itself.
                            cursor.enter("sequence.reminder-text");
                            skip_balanced_parenthetical(cursor)?;
                        }
                        let at_activation_reduction = stop_before_activation_reduction
                            && cursor
                                .peek()
                                .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
                            && cursor
                                .peek_ahead(1)
                                .is_some_and(|raw| raw.eq_ignore_ascii_case("ability"))
                            && cursor
                                .peek_ahead(2)
                                .is_some_and(|raw| raw.eq_ignore_ascii_case("costs"));
                        if at_activation_reduction
                            || (stop_before_activation_annotation
                                && cursor.peek() == Some("(")
                                && cursor
                                    .peek_ahead(1)
                                    .is_some_and(|raw| raw.eq_ignore_ascii_case("activate")))
                        {
                            SequenceState::Complete
                        } else if (context.ability_restriction_owner
                            == super::AbilityRestrictionOwner::Activated
                            && super::activation_restriction_starts(cursor))
                            || (context.ability_restriction_owner
                                == super::AbilityRestrictionOwner::Triggered
                                && super::trigger_limit_starts(cursor))
                        {
                            // `Activate only ...` restricts the enclosing
                            // ability (CR 602.5); it is never an effect. The
                            // activated parent owns it and the final finish.
                            stopped_at_activation_restriction = true;
                            SequenceState::Complete
                        } else if cursor.peek().is_some() {
                            cursor.enter("sequence.sentence-boundary");
                            let explicit_then = cursor
                                .peek()
                                .is_some_and(|raw| raw.eq_ignore_ascii_case("then"));
                            if explicit_then {
                                cursor.consume("then")?;
                                if cursor.peek().is_none() || cursor.peek() == Some(".") {
                                    attempt_frame.commit();
                                    cursor.enter("sequence.connector.effect");
                                    return Err(cursor.fail("an effect after `then`"));
                                }
                            }
                            attempt_frame.clear();
                            memory.connectors.push(EffectConnector::Then);
                            memory.sentence_start = memory.effects.len();
                            SequenceState::Effect
                        } else {
                            SequenceState::Complete
                        }
                    }
                    Some("unless")
                        if context.unless_frames == super::UnlessFrameSupport::SharedLowering =>
                    {
                        // CR 118.12: the sentence parent owns the frame; the
                        // clause child reads the payer and cost or condition.
                        let first_effect = memory
                            .connectors
                            .iter()
                            .rposition(|connector| *connector == EffectConnector::Then)
                            .map_or(0, |index| index + 1);
                        let end_effect = memory.effects.len();
                        let target = guarded_singular_target(&memory, first_effect);
                        let player = memory
                            .last_player_target
                            .filter(|binding| binding.effect_index >= first_effect);
                        let quantified_subject = end_effect == first_effect + 1
                            && matches!(
                                memory.effects.get(first_effect),
                                Some(LocalEffectNode::QuantifiedPlayerAction(_))
                            );
                        let clause = super::recognize_unless_clause(
                            cursor,
                            target,
                            player,
                            quantified_subject,
                        )?;
                        memory.unless.push(super::UnlessFrameNode {
                            first_effect,
                            end_effect,
                            clause,
                        });
                        SequenceState::ConnectorOrEnd
                    }
                    Some("for") if for_each_count_starts(cursor) => {
                        // The introducer is structural: the count child owns
                        // the countable, this frame owns which action it scales.
                        attempt_frame.commit();
                        let restates_result = {
                            let mut countable = cursor.clone();
                            countable.advance_to(countable.position() + 2).is_ok()
                                && super::action_result_reference_starts(&countable)
                        };
                        let count = if restates_result {
                            // CR 608.2c: `OBJECTS VERBed this way` counts what an
                            // earlier action of this program affected; this frame
                            // owns the earlier actions, so it resolves which.
                            cursor.enter("sequence.for-each.action-result");
                            cursor.consume_phrase(&["for", "each"])?;
                            let reference = super::recognize_action_result_reference(cursor)?;
                            cursor.enter("sequence.for-each.action-result.antecedent");
                            let (effect_index, filter) = cursor.require_value(
                                super::resolve_action_result_antecedent(
                                    &memory.effects,
                                    &reference,
                                ),
                                "an earlier action of this program whose affected objects the phrase restates",
                            )?;
                            super::NumericExpression::ActionResultCount {
                                effect_index,
                                filter,
                            }
                        } else {
                            recognize_for_each_count(cursor)?
                        };
                        cursor.enter("sequence.for-each.action");
                        let scaled = memory
                            .effects
                            .last_mut()
                            .is_some_and(|effect| apply_for_each_multiplier(effect, count));
                        cursor.require(
                            scaled,
                            "an action with a literal quantity that a trailing `for each` can multiply",
                        )?;
                        SequenceState::ConnectorOrEnd
                    }
                    Some("at") if super::delayed_step_timing_starts(cursor) => {
                        // CR 603.7a: a trailing timing delays the action it
                        // follows; the sentence must end with it.
                        attempt_frame.commit();
                        let timing = super::recognize_delayed_step_timing(cursor)?;
                        cursor.enter("sequence.delayed.trailing-scope");
                        cursor.require(
                            memory.sentence_delay.is_none()
                                && matches!(cursor.peek(), None | Some(".")),
                            "one delayed timing ending its sentence",
                        )?;
                        let end_effect = memory.effects.len();
                        memory.delayed.push(super::DelayedFrameNode {
                            first_effect: end_effect - 1,
                            end_effect,
                            timing,
                        });
                        SequenceState::ConnectorOrEnd
                    }
                    Some("until")
                        if super::zone_change_until_starts(cursor)
                            && matches!(
                                memory.effects.last(),
                                Some(LocalEffectNode::ZoneChange(zone_change))
                                    if zone_change.action == super::ZoneChangeAction::Exile
                                        && zone_change.until.is_none()
                            ) =>
                    {
                        // CR 610.3: `until EVENT` belongs to the exile it
                        // follows; the child reads only the ending event.
                        attempt_frame.commit();
                        let until =
                            super::recognize_zone_change_until(cursor, name_links, name_failure)?;
                        if let Some(LocalEffectNode::ZoneChange(zone_change)) =
                            memory.effects.last_mut()
                        {
                            zone_change.until = Some(until);
                        }
                        SequenceState::ConnectorOrEnd
                    }
                    _ => return Err(cursor.fail("`and`, `then`, or sentence terminator")),
                }
            }
            SequenceState::Complete => {
                cursor.enter("sequence.complete");
                if !stop_before_activation_reduction
                    && !stop_before_activation_annotation
                    && !stopped_at_activation_restriction
                {
                    cursor.finish()?;
                }
                cursor.require(
                    memory.connectors.len() + 1 == memory.effects.len(),
                    "one connector between each pair of effects",
                )?;
                return Ok(memory.into_sequence());
            }
        };
    }
}

/// The singular object target the last guarded action declared: the
/// antecedent of `its` in an `unless` payer. Never an earlier sentence's.
fn guarded_singular_target(memory: &SequenceMemory, first_effect: usize) -> Option<TargetBinding> {
    let effect_index = memory.effects.len().checked_sub(1)?;
    if let Some(LocalEffectNode::CounterSpell(counter)) = memory.effects.last() {
        return (counter.target.slots.len() == 1
            && counter.target.slots[0].cardinality.maximum == 1)
            .then_some(TargetBinding {
                effect_index,
                slot_index: 0,
            });
    }
    let scoped = match memory.permanent_subject_scope {
        super::PermanentSubjectScope::Local(Some(super::PermanentEffectReference::Target(
            binding,
        ))) => Some(binding),
        _ => None,
    };
    memory
        .last_target
        .or(scoped)
        .filter(|binding| binding.effect_index >= first_effect)
}

/// Consumes one parenthetical, including nested parentheses and quoted text.
fn skip_balanced_parenthetical(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.consume("(")?;
    let mut depth = 1usize;
    while depth > 0 {
        match cursor.peek() {
            None => return Err(cursor.fail("a closing parenthesis for the reminder text")),
            Some("(") => depth += 1,
            Some(")") => depth -= 1,
            Some(_) => {}
        }
        cursor.advance_to(cursor.position() + 1)?;
    }
    Ok(())
}

fn recognize_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    antecedent: Option<TargetBinding>,
    object_set_binding: ObjectSetBinding,
    object_set_antecedent: Option<&BoundPermanentSetNode>,
    context: &EffectRecognitionContext,
    attempt_frame: &mut SequenceAttemptFrame,
) -> MachineResult<LocalEffectNode> {
    cursor.enter("effect.start");
    match super::try_recognize_retarget_action(cursor) {
        super::RetargetActionAttempt::Recognized(node) => {
            return Ok(LocalEffectNode::Retarget(node));
        }
        super::RetargetActionAttempt::FailedCommitted(failure) => {
            attempt_frame.commit();
            return Err(failure);
        }
        super::RetargetActionAttempt::NotClaimed => {}
    }
    if super::control_change_starts(cursor) {
        // `[you] gain control of` / `exchange control of` owns its verb frame;
        // `gain N life` never reaches this reader.
        attempt_frame.commit();
        return super::recognize_control_change_effect(cursor, name_links, name_failure, context)
            .map(LocalEffectNode::ControlChange);
    }
    if super::copy_stack_object_starts(cursor) {
        attempt_frame.commit();
        return super::recognize_copy_stack_object_effect(cursor)
            .map(LocalEffectNode::CopyStackObject);
    }

    if crate::oracle::canonical::automata::lemma_at(
        cursor,
        VerbLemma::Discard,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    ) || (cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| is_verb_form(&raw, VerbLemma::Discard, VerbForm::Base)))
    {
        return super::recognize_ordinary_player_discard(cursor).map(LocalEffectNode::DiscardCards);
    }

    // Keyword actions (CR 701) are ordinary verbs: the verb owns the claim and
    // its operand comes from the shared permanent operand or numeric grammar.
    if cursor.peek().is_some_and(|raw| {
        super::PermanentKeywordAction::from_imperative_object_verb(raw).is_some()
    }) {
        return super::recognize_imperative_permanent_keyword_action(
            cursor,
            name_links,
            name_failure,
            context,
        )
        .map(LocalEffectNode::PermanentKeywordAction);
    }
    if cursor
        .peek()
        .is_some_and(|raw| super::PermanentKeywordAction::from_self_quantity_verb(raw).is_some())
    {
        return super::recognize_self_quantity_keyword_action(cursor, antecedent, context)
            .map(LocalEffectNode::PermanentKeywordAction);
    }
    if cursor
        .peek()
        .is_some_and(|raw| super::ControllerKeywordAction::from_imperative_verb(raw).is_some())
    {
        return super::recognize_controller_keyword_action(cursor, antecedent)
            .map(LocalEffectNode::ControllerKeywordAction);
    }
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("support"))
    {
        return super::recognize_support_action(cursor, context).map(LocalEffectNode::PutCounters);
    }

    match try_recognize_temporary_player_permission(cursor) {
        TemporaryPlayerPermissionParseAttempt::Parsed(node) => {
            return Ok(LocalEffectNode::TemporaryPlayerPermission(node));
        }
        TemporaryPlayerPermissionParseAttempt::FailedCommitted(failure) => {
            attempt_frame.commit();
            return Err(failure);
        }
        TemporaryPlayerPermissionParseAttempt::NotClaimed
        | TemporaryPlayerPermissionParseAttempt::RejectedCandidate(_) => {}
    }
    let targeted_head = cursor.peek().is_some_and(|token| {
        token.eq_ignore_ascii_case("target")
            || token.eq_ignore_ascii_case("another")
            || token.eq_ignore_ascii_case("up")
    }) || cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "quantity");
    if targeted_head {
        match try_recognize_temporary_combat_restriction(cursor) {
            TemporaryCombatRestrictionParseAttempt::Parsed(restriction) => {
                return Ok(LocalEffectNode::TemporaryKeywordGrant(
                    super::ordinary_keyword_subject(restriction),
                ));
            }
            TemporaryCombatRestrictionParseAttempt::RejectedCandidate(_) => {}
            TemporaryCombatRestrictionParseAttempt::FailedCommitted(failure) => {
                attempt_frame.commit();
                return Err(failure);
            }
        }
    }
    match super::try_recognize_permanent_action(cursor, name_links, name_failure, context) {
        super::PermanentActionAttempt::Parsed(node) => return Ok(node),
        super::PermanentActionAttempt::FailedCommitted(failure) => {
            attempt_frame.commit();
            return Err(failure);
        }
        super::PermanentActionAttempt::NotClaimed
        | super::PermanentActionAttempt::RejectedCandidate(_) => {}
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("choose"))
    {
        // `choose DOMAIN` owns its verb: every domain either binds or fails
        // at its own typed state (CR 115.1, 608.2d). The failure stays
        // tentative so modal parents keep their own claim on mode choices.
        return super::recognize_choose_instruction(cursor).map(LocalEffectNode::Choose);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("attach"))
    {
        let mut trial = cursor.clone();
        let node = super::recognize_attach_effect(&mut trial, name_links, name_failure, context)?;
        *cursor = trial;
        return Ok(LocalEffectNode::Attach(node));
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("prevent"))
    {
        return super::recognize_prevent_damage_effect(cursor, name_links, name_failure, context)
            .map(LocalEffectNode::PreventDamage);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("regenerate"))
    {
        return super::recognize_regenerate_effect(cursor, name_links, name_failure, context)
            .map(LocalEffectNode::PermanentKeywordAction);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("goad"))
    {
        return super::recognize_goad_effect(cursor, name_links, name_failure, context)
            .map(LocalEffectNode::PermanentKeywordAction);
    }
    if crate::oracle::canonical::automata::lemma_at(
        cursor,
        VerbLemma::Tap,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    ) {
        cursor.enter("effect.tap.action");
        let mut trial = cursor.clone();
        trial.consume(
            crate::oracle::canonical::automata::verb_entry(VerbLemma::Tap).surface(VerbForm::Base),
        )?;
        cursor.enter("effect.tap.object");
        let targets = super::recognize_contextual_permanent_operand(
            &mut trial,
            name_links,
            name_failure,
            context,
        )?;
        *cursor = trial;
        return Ok(LocalEffectNode::TapPermanents(
            super::TapPermanentsEffectNode {
                targets,
                untap: false,
            },
        ));
    }
    if crate::oracle::canonical::automata::lemma_at(
        cursor,
        VerbLemma::Untap,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    ) {
        if !cursor
            .peek_ahead(1)
            .is_some_and(|word| word.eq_ignore_ascii_case("it"))
        {
            let mut historical = cursor.clone();
            match super::recognize_untap_permanents_effect(&mut historical) {
                Ok(fragment) => {
                    *cursor = historical;
                    cursor.require(
                        fragment.end_position == cursor.position(),
                        "the selected untap child boundary",
                    )?;
                    return Ok(LocalEffectNode::SelectedPermanents(fragment.node));
                }
                Err(failure) => {
                    // `untap OBJECT` shares the tap verb frame and its
                    // permanent operand (`target creature`, `up to two ...`).
                    let mut trial = cursor.clone();
                    trial.consume(
                        crate::oracle::canonical::automata::verb_entry(VerbLemma::Untap)
                            .surface(VerbForm::Base),
                    )?;
                    let Ok(targets) = super::recognize_contextual_permanent_operand(
                        &mut trial,
                        name_links,
                        name_failure,
                        context,
                    ) else {
                        *cursor = historical;
                        return Err(failure);
                    };
                    *cursor = trial;
                    return Ok(LocalEffectNode::TapPermanents(
                        super::TapPermanentsEffectNode {
                            targets,
                            untap: true,
                        },
                    ));
                }
            }
        }
        let reference =
            super::permanent_subject::proven_permanent_antecedent(context).map(|reference| {
                if matches!(reference, super::PermanentEffectReference::Source(_))
                    && context.observed_source_relationship
                        == super::ObservedSourceRelationship::SameEnteringObject
                {
                    super::PermanentEffectReference::Observed(
                        super::TypedObservedEventObject::EnteringPermanent,
                    )
                } else {
                    reference
                }
            });
        let fragment = recognize_untap_permanent_effect(cursor, reference)?;
        cursor.require(
            fragment.end_position == cursor.position(),
            "the untap child cursor boundary",
        )?;
        return Ok(LocalEffectNode::UntapTarget(fragment.node));
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("add"))
        || (cursor
            .peek()
            .is_some_and(|token| token.eq_ignore_ascii_case("for"))
            && cursor
                .peek_ahead(1)
                .is_some_and(|token| token.eq_ignore_ascii_case("each"))
            && cursor
                .peek_ahead(2)
                .is_some_and(|token| token.eq_ignore_ascii_case("color")))
    {
        return recognize_add_mana_effect(cursor).map(LocalEffectNode::AddMana);
    }
    if crate::oracle::canonical::automata::lemma_at(
        cursor,
        VerbLemma::Sacrifice,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    ) {
        return recognize_ordinary_sacrifice_effect(
            cursor,
            name_links,
            name_failure,
            context.source.as_ref(),
        )
        .map(LocalEffectNode::Sacrifice);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("target"))
    {
        return recognize_mill_cards_effect(cursor).map(LocalEffectNode::MillCards);
    }
    if super::quantified_player_action_starts(cursor) {
        attempt_frame.commit();
        return recognize_quantified_player_action(cursor, context)
            .map(LocalEffectNode::QuantifiedPlayerAction);
    }
    // `each player|opponent|other player ...` is a player-set subject, never
    // an object quantifier. When the player-set reader does not take its
    // verb, the failure names the verb and the subject kind (S5, S2).
    if let Some(verb_offset) = super::player_set_subject_length(cursor) {
        let mut verb = cursor.clone();
        verb.advance_to(verb.position() + verb_offset)?;
        if verb
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("may"))
        {
            verb.advance_to(verb.position() + 1)?;
        }
        return Err(super::clause_verb_unsupported(
            &verb,
            "a player set (`each player`)",
        ));
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("each"))
    {
        return recognize_set_ability_loss_effect(cursor, object_set_binding)
            .map(LocalEffectNode::LoseAbilities);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("draw"))
        || (cursor
            .peek()
            .is_some_and(|token| token.eq_ignore_ascii_case("you"))
            && cursor
                .peek_ahead(1)
                .is_some_and(|token| token.eq_ignore_ascii_case("draw")))
    {
        return match try_recognize_draw_cards_effect_with_context(cursor, context) {
            DrawCardsEffectParseAttempt::Parsed(effect) => Ok(LocalEffectNode::DrawCards(effect)),
            DrawCardsEffectParseAttempt::RejectedCandidate(failure) => Err(failure),
            DrawCardsEffectParseAttempt::FailedCommitted(failure) => {
                attempt_frame.commit();
                Err(failure)
            }
        };
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("amass"))
    {
        return recognize_amass_effect(cursor, antecedent).map(LocalEffectNode::Amass);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("empower"))
    {
        return recognize_empower_jace_effect(cursor, antecedent).map(LocalEffectNode::EmpowerJace);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("investigate"))
    {
        return recognize_investigate_effect(cursor).map(LocalEffectNode::Investigate);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("scry"))
    {
        return recognize_scry_effect(cursor).map(LocalEffectNode::Scry);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("surveil"))
    {
        return recognize_surveil_effect(cursor, antecedent).map(LocalEffectNode::Surveil);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("earthbend"))
    {
        return recognize_earthbend_action(cursor).map(LocalEffectNode::Earthbend);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("search"))
    {
        return recognize_controller_library_search(cursor)
            .map(LocalEffectNode::ControllerLibrarySearch);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("transform"))
    {
        return recognize_transform_source_effect(cursor, name_links, name_failure)
            .map(LocalEffectNode::TransformPermanent);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("double"))
        && !cursor
            .peek_ahead(1)
            .is_some_and(|token| token.eq_ignore_ascii_case("strike"))
    {
        return super::recognize_double_counters_effect(cursor, name_links, name_failure, context)
            .map(LocalEffectNode::PutCounters);
    }
    if super::claims_move_counters_effect(cursor) {
        return super::recognize_move_counters_effect(cursor, name_links, name_failure, context)
            .map(LocalEffectNode::MoveCounters);
    }
    if super::claims_remove_counters_effect(cursor) {
        return super::recognize_remove_counters_effect(cursor, name_links, name_failure, context)
            .map(LocalEffectNode::PutCounters);
    }
    if crate::oracle::canonical::automata::lemma_at(
        cursor,
        VerbLemma::Put,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    ) {
        if targeted_card_zone_move_starts(cursor) {
            return recognize_target_card_zone_change_effect(cursor)
                .map(LocalEffectNode::TargetCardZoneChange);
        }
        if claims_put_counters_effect(cursor) {
            return recognize_put_counters_effect(cursor, name_links, name_failure, context)
                .map(LocalEffectNode::PutCounters);
        }
        // The object-to-zone frame of `put` with a declared target permanent:
        // the shared targeted zone change, to its owner's library.
        if cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("target"))
            && let Ok(effect) = cursor.attempt(super::recognize_zone_change_effect)
        {
            return Ok(effect);
        }
        return recognize_move_cards_effect(cursor).map(LocalEffectNode::MoveCards);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("counter"))
    {
        return recognize_counter_effect(cursor);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("destroy"))
        && cursor.peek_ahead(1).is_some_and(|token| {
            token.eq_ignore_ascii_case("all") || token.eq_ignore_ascii_case("each")
        })
    {
        let mut collection = super::recognize_destroy_permanent_collection_effect_with_source(
            cursor,
            context.source_numeric_reads,
        )?;
        collection.cannot_regenerate = super::recognize_plural_regeneration_amendment(cursor)?;
        return Ok(LocalEffectNode::DestroyCollection(collection));
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("destroy"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|token| token.eq_ignore_ascii_case("those"))
    {
        return recognize_destroy_referenced_set_effect(cursor, object_set_antecedent)
            .map(LocalEffectNode::DestroyReferencedSet);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("destroy"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|token| token.eq_ignore_ascii_case("that"))
    {
        // `destroy that creature`: a bound reference, never a declaration.
        return super::recognize_destroy_reference_effect(
            cursor,
            name_links,
            name_failure,
            context,
        )
        .map(LocalEffectNode::DestroyReference);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("destroy"))
        && cursor.peek_ahead(1).is_some_and(|token| {
            !token.eq_ignore_ascii_case("all")
                && !token.eq_ignore_ascii_case("each")
                && !token.eq_ignore_ascii_case("those")
        })
    {
        return recognize_zone_change_with_regeneration_amendment_with_support(
            cursor,
            ZoneChangeTargetSupport::TargetCollection,
        );
    }
    match try_recognize_exile_target_card_zone_change_effect(cursor) {
        TargetCardZoneChangeParseAttempt::Parsed(node) => {
            return Ok(LocalEffectNode::TargetCardZoneChange(node));
        }
        TargetCardZoneChangeParseAttempt::FailedCommitted(failure) => {
            attempt_frame.commit();
            return Err(failure);
        }
        TargetCardZoneChangeParseAttempt::NotClaimed
        | TargetCardZoneChangeParseAttempt::RejectedCandidate(_) => {}
    }
    if crate::oracle::canonical::automata::lemma_at(
        cursor,
        VerbLemma::Exile,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    ) && cursor.peek_ahead(1).is_some_and(|token| {
        token.eq_ignore_ascii_case("all") || token.eq_ignore_ascii_case("each")
    }) {
        return recognize_exile_graveyard_collection_effect(cursor).map(LocalEffectNode::ZoneBatch);
    }
    if crate::oracle::canonical::automata::lemma_at(
        cursor,
        VerbLemma::Exile,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    ) && source_reference_starts_at(
        cursor.position() + 1,
        cursor.peek_ahead(1),
        name_links,
        name_failure,
    ) {
        return recognize_source_zone_move_effect(cursor, name_links, name_failure, context)
            .map(LocalEffectNode::MoveSource);
    }
    // A counted card-target head (`up to two target creature cards from`)
    // is claimed by the card-target operand itself, not by a lexical probe.
    if crate::oracle::canonical::automata::lemma_at(
        cursor,
        VerbLemma::Return,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    ) && !targeted_card_zone_move_starts(cursor)
    {
        match try_recognize_target_card_zone_change_effect_with_verb(cursor, "return") {
            TargetCardZoneChangeParseAttempt::Parsed(node) => {
                return Ok(LocalEffectNode::TargetCardZoneChange(node));
            }
            TargetCardZoneChangeParseAttempt::FailedCommitted(failure) => {
                attempt_frame.commit();
                return Err(failure);
            }
            TargetCardZoneChangeParseAttempt::NotClaimed
            | TargetCardZoneChangeParseAttempt::RejectedCandidate(_) => {}
        }
    }
    if crate::oracle::canonical::automata::lemma_at(
        cursor,
        VerbLemma::Return,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    ) && targeted_card_zone_move_starts(cursor)
    {
        return recognize_target_card_zone_change_effect(cursor)
            .map(LocalEffectNode::TargetCardZoneChange);
    }
    if crate::oracle::canonical::automata::lemma_at(
        cursor,
        VerbLemma::Return,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    ) && source_or_anaphor_reference_starts_at(
        cursor.position() + 1,
        cursor.peek_ahead(1),
        name_links,
        name_failure,
    ) {
        return recognize_source_zone_move_effect(cursor, name_links, name_failure, context)
            .map(LocalEffectNode::MoveSource);
    }
    if crate::oracle::canonical::automata::lemma_at(
        cursor,
        VerbLemma::Return,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    ) {
        // A resolution choice (`a land you control`) shares the selection
        // operand with `untap`; a target head stays with the targeted frame.
        let mut trial = cursor.clone();
        if let Ok(fragment) = super::recognize_return_selected_permanents_effect(&mut trial) {
            *cursor = trial;
            cursor.require(
                fragment.end_position == cursor.position(),
                "the selected return child boundary",
            )?;
            return Ok(LocalEffectNode::SelectedPermanents(fragment.node));
        }
    }
    if cursor.peek().is_some_and(|token| {
        is_verb_form(&token, VerbLemma::Exile, VerbForm::Base)
            || token.eq_ignore_ascii_case("destroy")
            || is_verb_form(&token, VerbLemma::Return, VerbForm::Base)
    }) {
        return recognize_zone_change_effect_with_policy(
            cursor,
            ZoneChangeTargetSupport::TargetCollection,
            context.zone_change_domains,
        );
    }
    if super::controller_reference_head_length(
        cursor.token_slice(cursor.position(), cursor.position() + 3),
    )
    .is_some_and(|length| {
        cursor
            .peek_ahead(length)
            .is_some_and(|raw| is_verb_form(&raw, VerbLemma::Create, VerbForm::ThirdPersonSingular))
    }) || (cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("you"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|token| is_verb_form(&token, VerbLemma::Create, VerbForm::Base)))
        || crate::oracle::canonical::automata::lemma_at(
            cursor,
            VerbLemma::Create,
            crate::oracle::canonical::automata::VerbAgreement::Base,
        )
    {
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
        {
            // A registered possessed-object controller followed by creates is
            // an owned action head; binding/nominal failures remain explicit.
            attempt_frame.commit();
            // Only this ordinary token action receives the caller's explicit
            // actor-reference capability. Life/damage/duration readers keep
            // their historical unqualified wrapper.
            return super::recognize_create_creature_tokens_effect_with_reference_support(
                cursor,
                antecedent,
                context.target_reference_support,
            )
            .map(LocalEffectNode::CreateTokens);
        }
        return match super::try_recognize_token_creation_action(cursor, antecedent) {
            super::TokenCreationAttempt::Recognized(effect) => Ok(effect),
            super::TokenCreationAttempt::FailedCommitted(failure) => Err(failure),
            super::TokenCreationAttempt::NotClaimed => {
                Err(cursor.fail("a complete token creation action"))
            }
        };
    }
    if source_or_anaphor_reference_starts_at(
        cursor.position(),
        cursor.peek(),
        name_links,
        name_failure,
    ) {
        return super::effects::recognize_damage_effect_with_agreement(
            cursor,
            name_links,
            name_failure,
            super::permanent_subject::proven_permanent_antecedent(context),
            context.subject_verb_agreement,
        );
    }
    let optional_action_offset = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("may"))
    {
        Some(2)
    } else if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("its") || raw.eq_ignore_ascii_case("that"))
        && cursor
            .peek_ahead(2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("may"))
    {
        Some(3)
    } else {
        None
    };
    if optional_action_offset.is_some_and(|offset| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("draw"))
    }) {
        attempt_frame.commit();
        return recognize_referenced_draw_effect(cursor, antecedent);
    }
    if optional_action_offset.is_some_and(|offset| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("search"))
    }) {
        attempt_frame.commit();
        return recognize_optional_library_search(cursor, antecedent)
            .map(LocalEffectNode::OptionalLibrarySearch);
    }
    if cursor.peek().is_some_and(|token| {
        token.eq_ignore_ascii_case("its") || token.eq_ignore_ascii_case("that")
    }) && cursor
        .peek_ahead(2)
        .is_some_and(|token| token.eq_ignore_ascii_case("may"))
    {
        return recognize_optional_library_search(cursor, antecedent)
            .map(LocalEffectNode::OptionalLibrarySearch);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("you"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|token| token.eq_ignore_ascii_case("may"))
        && cursor
            .peek_ahead(2)
            .is_some_and(|token| is_verb_form(&token, VerbLemma::Put, VerbForm::Base))
    {
        return recognize_optional_card_deployment_effect_with_context(cursor, context)
            .map(LocalEffectNode::OptionalCardDeployment);
    }
    Err(cursor.fail("a supported effect subject"))
}

/// The agreement of a performer's continued verb: plural after `they`, else
/// third-person singular (read from the subject as written).
fn performed_agreement(
    performer: &super::PerformerNode,
    cursor: &OracleTokenCursor<'_>,
    effect_start: usize,
) -> super::VerbAgreement {
    let they = cursor
        .token_slice(effect_start, effect_start + 1)
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("they"));
    match performer {
        super::PerformerNode::Player(_) if they => super::VerbAgreement::Base,
        _ => super::VerbAgreement::ThirdPersonSingular,
    }
}

/// The agreement a player subject read by an ordinary reader gave its verb.
fn performed_agreement_of_player(
    cursor: &OracleTokenCursor<'_>,
    effect_start: usize,
) -> super::VerbAgreement {
    if cursor
        .token_slice(effect_start, effect_start + 1)
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("they"))
    {
        super::VerbAgreement::Base
    } else {
        super::VerbAgreement::ThirdPersonSingular
    }
}

/// Token width of `for each opponent|player|other player ,` at the cursor.
fn player_population_width(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let is = |offset: usize, word: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
    };
    if !(is(0, "for") && is(1, "each")) {
        return None;
    }
    let width = if is(2, "opponent") || is(2, "player") {
        3
    } else if is(2, "other") && is(3, "player") {
        4
    } else {
        return None;
    };
    is(width, ",").then_some(width + 1)
}

/// CR 608.2c: `For each PLAYER, BODY` repeats the body once per player of
/// the population. A body that declares targets would declare them per
/// player while the spell or ability is put on the stack (CR 601.2c), which
/// the engine's single declaration does not represent; any other body has no
/// player-iteration lowering yet. Both stay explicit.
fn player_iteration_failure(cursor: &OracleTokenCursor<'_>, width: usize) -> MachineFailure {
    let mut body = cursor.clone();
    let _ = body.advance_to(body.position() + width);
    let mut targets = false;
    let mut offset = 0;
    while let Some(raw) = body.peek_ahead(offset) {
        if raw == "." {
            break;
        }
        targets |= raw.eq_ignore_ascii_case("target");
        offset += 1;
    }
    if targets {
        body.enter("sequence.for-each-player.per-player-targets");
        body.fail("targets declared once per player of the population (CR 601.2c), which the engine does not declare")
    } else {
        body.enter("sequence.for-each-player.body");
        body.fail("a player-iteration body with a lowering")
    }
}

/// The count a preposed `for each` names, resolved once its action is read.
enum PreposedForEach {
    Count(super::NumericExpression),
    /// `OBJECTS VERBed this way`: resolved against the program's actions.
    ActionResult(super::ActionResultReferenceNode),
}

fn recognize_preposed_for_each(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PreposedForEach> {
    let restates_result = {
        let mut countable = cursor.clone();
        countable.advance_to(countable.position() + 2).is_ok()
            && super::action_result_reference_starts(&countable)
    };
    if restates_result {
        cursor.enter("sequence.for-each.action-result");
        cursor.consume_phrase(&["for", "each"])?;
        return super::recognize_action_result_reference(cursor).map(PreposedForEach::ActionResult);
    }
    recognize_for_each_count(cursor).map(PreposedForEach::Count)
}

fn targeted_card_zone_move_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek().is_some_and(|raw| {
        is_verb_form(&raw, VerbLemma::Return, VerbForm::Base)
            || is_verb_form(&raw, VerbLemma::Put, VerbForm::Base)
    }) && cursor
        .peek_ahead(1)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("target"))
        && (2..16)
            .take_while(|offset| cursor.peek_ahead(*offset).is_some_and(|raw| raw != "."))
            .any(|offset| {
                cursor.peek_ahead(offset).is_some_and(|raw| {
                    raw.eq_ignore_ascii_case("card") || raw.eq_ignore_ascii_case("cards")
                }) && cursor
                    .peek_ahead(offset + 1)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("from"))
            })
}

#[cfg(test)]
#[path = "sequence_context_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "sequence_optional_actions_tests.rs"]
mod optional_action_tests;

#[cfg(test)]
#[path = "mana_effect_sequence_tests.rs"]
mod mana_effect_tests;

#[cfg(test)]
#[path = "zone_change_life_sequence_tests.rs"]
mod zone_change_life_tests;

#[cfg(test)]
#[path = "surveil_reminder_sequence_tests.rs"]
mod surveil_reminder_sequence_tests;

#[cfg(test)]
#[path = "surveil_sequence_tests.rs"]
mod surveil_sequence_tests;

#[cfg(test)]
#[path = "controller_subject_sequence_tests.rs"]
mod controller_subject_sequence_tests;

#[cfg(test)]
#[path = "sequence_attempt_tests.rs"]
mod sequence_attempt_tests;
