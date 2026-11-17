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
    recognize_life_effect_with_player_target, recognize_mill_cards_effect,
    recognize_move_cards_effect, recognize_named_token_creation_reminder,
    recognize_optional_card_deployment_effect_with_context, recognize_optional_library_search,
    recognize_ordinary_sacrifice_effect, recognize_put_counters_effect,
    recognize_quantified_player_action, recognize_referenced_draw_effect, recognize_scry_effect,
    recognize_set_ability_loss_effect, recognize_source_zone_move_effect, recognize_surveil_effect,
    recognize_target_card_zone_change_effect, recognize_temporary_animation_effect,
    recognize_temporary_power_toughness, recognize_transform_source_effect,
    recognize_untap_permanent_effect, recognize_zone_change_effect_with_policy,
    recognize_zone_change_with_regeneration_amendment_with_support,
    signed_power_toughness_from_raw, source_or_anaphor_reference_starts_at,
    source_reference_starts_at, try_recognize_draw_cards_effect_with_context,
    try_recognize_exile_target_card_zone_change_effect, try_recognize_temporary_combat_restriction,
    try_recognize_temporary_player_permission,
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
                cursor.consume("exile")?;
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

    fn into_sequence(mut self) -> EffectSequenceNode {
        self.close_conditional_branch();
        EffectSequenceNode {
            effects: self.effects,
            connectors: self.connectors,
            where_x: self.where_x,
            conditionals: self.conditionals,
            unless: self.unless,
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
                        super::SingularPlayerAction::Draw(_) | super::SingularPlayerAction::Mill(_)
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

fn recognize_effect_sequence_attempt_internal(
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
                    _ => None,
                };
                let base_action_context = action_context;
                let mut preposed_duration = memory.sentence_duration.clone();
                let mut opened_preposed_duration = false;
                let mut effect_start = cursor.position();
                let mut effect = loop {
                    let mut action_context = base_action_context.clone();
                    action_context.preposed_duration = preposed_duration.clone();
                    let parsed = cursor.attempt(|attempt| {
                        // `sacrifice A, B, and C`: one verb, independent
                        // headed objects, each its own sacrifice action. Any
                        // other `and a ...` clause stays with the other readers.
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
                        // The keyword action `venture into …` (CR 701.49) is
                        // an action head even when the lexical name resolver
                        // left its words unresolved; it is never a source name.
                        if super::venture_action_starts(attempt)
                            || !source_reference_starts_at(
                                attempt.position(),
                                attempt.peek(),
                                name_links,
                                name_failure,
                            )
                        {
                            match super::try_recognize_player_action(
                                attempt,
                                super::PlayerBinding(effect_index),
                                memory
                                    .last_player_target
                                    .map(|binding| super::PlayerBinding(binding.effect_index)),
                                memory.current_player_subject.as_ref(),
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
                                Some(PlayerTargetBinding { effect_index }),
                                memory.last_player_target,
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
                    });
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
                attempt_frame.clear();
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
                memory.effects.push(effect);
                if !matches!(memory.open_branch, Some(ConditionalBranch::Else(_))) {
                    memory.else_candidate = None;
                }
                SequenceState::ConnectorOrEnd
            }
            SequenceState::ConnectorOrEnd => {
                cursor.enter("sequence.connector-or-end");
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
                        if !sentence_closed_by_quote {
                            cursor.consume(".")?;
                        }
                        memory.close_conditional_branch();
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
                        if cursor.peek() == Some("(")
                            && !stop_before_activation_annotation
                            && !library_look_reminder_starts_at(cursor)
                        {
                            // CR 207.2: reminder text has no rules meaning.
                            // Typed reminders above keep their validation;
                            // any other balanced parenthetical after a
                            // complete sentence is recorded and skipped.
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
                            || (stop_before_activation_annotation && cursor.peek() == Some("("))
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
                        let count = recognize_for_each_count(cursor)?;
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
    current_player_target: Option<PlayerTargetBinding>,
    player_antecedent: Option<PlayerTargetBinding>,
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

    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("discard"))
        || (cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
            && cursor
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("discard")))
    {
        return super::recognize_ordinary_player_discard(cursor).map(LocalEffectNode::DiscardCards);
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
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("tap"))
    {
        cursor.enter("effect.tap.action");
        let mut trial = cursor.clone();
        trial.consume("tap")?;
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
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("untap"))
    {
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
                    trial.consume("untap")?;
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
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("sacrifice"))
    {
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
        if cursor
            .peek_ahead(1)
            .is_some_and(|token| token.eq_ignore_ascii_case("player"))
            && cursor.peek_ahead(2).is_some_and(|token| {
                token.eq_ignore_ascii_case("gains") || token.eq_ignore_ascii_case("loses")
            })
        {
            return recognize_life_effect_with_player_target(
                cursor,
                antecedent,
                current_player_target,
                player_antecedent,
            );
        }
        return recognize_mill_cards_effect(cursor).map(LocalEffectNode::MillCards);
    }
    let source_animation_shape = (0..32).any(|offset| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|token| token.eq_ignore_ascii_case("becomes"))
    }) && (0..32).any(|offset| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|token| token.eq_ignore_ascii_case("creature"))
    }) && (0..32).any(|offset| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|token| token.eq_ignore_ascii_case("still"))
    });
    if source_animation_shape
        && (cursor
            .peek()
            .is_some_and(|token| token.eq_ignore_ascii_case("until"))
            || source_reference_starts_at(
                cursor.position(),
                cursor.peek(),
                name_links,
                name_failure,
            ))
    {
        return recognize_temporary_animation_effect(cursor, name_links, name_failure)
            .map(LocalEffectNode::AnimateSource);
    }
    if super::quantified_player_action_starts(cursor) {
        attempt_frame.commit();
        return recognize_quantified_player_action(cursor, context)
            .map(LocalEffectNode::QuantifiedPlayerAction);
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
        .is_some_and(|token| token.eq_ignore_ascii_case("put"))
    {
        if targeted_card_zone_move_starts(cursor) {
            return recognize_target_card_zone_change_effect(cursor)
                .map(LocalEffectNode::TargetCardZoneChange);
        }
        if claims_put_counters_effect(cursor) {
            return recognize_put_counters_effect(cursor, name_links, name_failure, context)
                .map(LocalEffectNode::PutCounters);
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
        return super::recognize_destroy_permanent_collection_effect_with_source(
            cursor,
            context.source_numeric_reads,
        )
        .map(LocalEffectNode::DestroyCollection);
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
        && cursor.peek_ahead(1).is_some_and(|token| {
            !token.eq_ignore_ascii_case("all")
                && !token.eq_ignore_ascii_case("each")
                && !token.eq_ignore_ascii_case("those")
        })
    {
        return recognize_zone_change_with_regeneration_amendment_with_support(
            cursor,
            ZoneChangeTargetSupport::DestroyCollection,
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
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("exile"))
        && cursor.peek_ahead(1).is_some_and(|token| {
            token.eq_ignore_ascii_case("all") || token.eq_ignore_ascii_case("each")
        })
    {
        return recognize_exile_graveyard_collection_effect(cursor).map(LocalEffectNode::ZoneBatch);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("exile"))
        && source_reference_starts_at(
            cursor.position() + 1,
            cursor.peek_ahead(1),
            name_links,
            name_failure,
        )
    {
        return recognize_source_zone_move_effect(cursor, name_links, name_failure, context)
            .map(LocalEffectNode::MoveSource);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("return"))
        && targeted_card_zone_move_starts(cursor)
    {
        return recognize_target_card_zone_change_effect(cursor)
            .map(LocalEffectNode::TargetCardZoneChange);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("return"))
        && source_or_anaphor_reference_starts_at(
            cursor.position() + 1,
            cursor.peek_ahead(1),
            name_links,
            name_failure,
        )
    {
        return recognize_source_zone_move_effect(cursor, name_links, name_failure, context)
            .map(LocalEffectNode::MoveSource);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("return"))
    {
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
        token.eq_ignore_ascii_case("exile")
            || token.eq_ignore_ascii_case("destroy")
            || token.eq_ignore_ascii_case("return")
    }) {
        return recognize_zone_change_effect_with_policy(
            cursor,
            ZoneChangeTargetSupport::DestroyCollection,
            context.zone_change_domains,
        );
    }
    if super::controller_reference_head_length(
        cursor.token_slice(cursor.position(), cursor.position() + 3),
    )
    .is_some_and(|length| {
        cursor
            .peek_ahead(length)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("creates"))
    }) || (cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("you"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|token| token.eq_ignore_ascii_case("create")))
        || cursor
            .peek()
            .is_some_and(|token| token.eq_ignore_ascii_case("create"))
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
        return super::effects::recognize_damage_effect_with_source_antecedent(
            cursor,
            name_links,
            name_failure,
            super::permanent_subject::proven_permanent_antecedent(context),
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
            .is_some_and(|token| token.eq_ignore_ascii_case("put"))
    {
        return recognize_optional_card_deployment_effect_with_context(cursor, context)
            .map(LocalEffectNode::OptionalCardDeployment);
    }
    if cursor.peek().is_some_and(|token| {
        token.eq_ignore_ascii_case("gain") || token.eq_ignore_ascii_case("lose")
    }) {
        return recognize_life_effect_with_player_target(
            cursor,
            antecedent,
            current_player_target,
            player_antecedent,
        );
    }
    if cursor.peek().is_some_and(|token| {
        token.eq_ignore_ascii_case("you")
            || token.eq_ignore_ascii_case("its")
            || token.eq_ignore_ascii_case("that")
    }) {
        return recognize_life_effect_with_player_target(
            cursor,
            antecedent,
            current_player_target,
            player_antecedent,
        );
    }
    Err(cursor.fail("a supported effect subject"))
}

fn targeted_card_zone_move_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("return") || raw.eq_ignore_ascii_case("put"))
        && cursor
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
