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
    recognize_amass_effect, recognize_conditional_damage_replacement, recognize_counter_effect,
    recognize_damage_effect, recognize_destroy_referenced_set_effect, recognize_earthbend_action,
    recognize_effect_connector, recognize_empower_jace_effect, recognize_empower_jace_reminder,
    recognize_exile_graveyard_collection_effect, recognize_for_each_count,
    recognize_investigate_effect, recognize_library_look_reminder, recognize_move_cards_effect,
    recognize_optional_card_deployment_effect_with_context, recognize_optional_library_search,
    recognize_ordinary_sacrifice_effect, recognize_quantified_player_action,
    recognize_referenced_draw_effect, recognize_scry_effect, recognize_set_ability_loss_effect,
    recognize_source_zone_move_effect, recognize_surveil_effect,
    recognize_target_card_zone_change_effect, recognize_transform_source_effect,
    recognize_untap_permanent_effect, recognize_zone_change_effect_with_policy,
    recognize_zone_change_with_regeneration_amendment_with_support,
    source_or_anaphor_reference_starts_at, source_reference_starts_at,
    try_recognize_draw_cards_effect_with_context,
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
    /// The performer of the clause whose remaining actions `distributed`
    /// holds: each of them is that performer's action too.
    distributed_performer: Option<super::PerformerNode>,
    /// The previous action declared the object of a `the owner|controller
    /// of TARGET` subject: its performed verb follows without a connector.
    subject_continuation: bool,
    /// Delayed action ranges (CR 603.7a) closed so far.
    delayed: Vec<super::DelayedFrameNode>,
    /// A preposed created rule (`At the beginning of ... ,`, `When EVENT
    /// this turn,`, `If OBJECT would die this turn,`) scoping the sentence
    /// being read, with the index of its first action.
    sentence_delay: Option<(usize, super::EffectCreatedRuleNode)>,
    /// The last action read after a causative `have`. A verb coordinated with
    /// it (`have it get +2/+0 and gain haste`) keeps the bare agreement.
    causative_effect: Option<usize>,
    /// The imperative lemma of the previous action of the sentence: a gapped
    /// conjunct (`destroy A and all B`) is an argument frame of it.
    elided_lemma: Option<VerbLemma>,
    /// Where the connector after the previous action began; the gapped
    /// conjunct writes the elided verb in the conjunction's slot.
    connector_start: Option<super::super::machine::MachineCheckpoint>,
    /// The finite verb of the previous action of the sentence when an
    /// explicit player subject performed it (`target player sacrifices an
    /// artifact and a land`): a gapped conjunct elides this verb in the
    /// written form and continues the same performer.
    finite_gap: Option<FiniteGap>,
    /// The tokens of the previous action, the antecedent whose library a
    /// gapped conjunct's elided library position restates.
    last_effect_span: Option<(usize, usize)>,
    /// An earlier action was performed by the player the triggering event
    /// records (`defending player reveals ...`, `that player draws ...`):
    /// a later `that player` / `they` names the same event player (CR
    /// 603.2, 506.2), even in a frame whose event names no other player.
    event_player_named: bool,
}

/// The elided verb of a finite clause: its lemma, the written form its
/// subject gave it, and the sentence it belongs to.
#[derive(Clone, Copy)]
struct FiniteGap {
    lemma: VerbLemma,
    form: VerbForm,
    sentence: usize,
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
        let index = match self.open_branch.take() {
            Some(ConditionalBranch::Then(index)) => {
                self.conditionals[index].then_end = self.effects.len();
                self.else_candidate = Some(index);
                index
            }
            Some(ConditionalBranch::Else(index)) => {
                self.conditionals[index].else_end = Some(self.effects.len());
                index
            }
            None => return,
        };
        // A permanent a branch put onto the battlefield exists after the
        // conditional sentence only if that branch was taken: it is no
        // antecedent of a later sentence (`If it's a Bison card, you may put
        // it onto the battlefield. If it's a creature card, ...` still names
        // the looked-at card).
        if self.permanent_subject_scope
            == super::PermanentSubjectScope::Local(Some(super::PermanentEffectReference::Observed(
                super::TypedObservedEventObject::MovedOntoBattlefield,
            )))
            && let Some(snapshot) = self.branch_snapshots.get(index)
        {
            self.permanent_subject_scope = snapshot.permanent_subject_scope;
        }
    }

    /// Closes a preposed delayed timing at the end of its sentence.
    fn close_sentence_delay(&mut self) {
        if let Some((first_effect, rule)) = self.sentence_delay.take() {
            self.delayed.push(super::DelayedFrameNode {
                first_effect,
                end_effect: self.effects.len(),
                rule,
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
                // A search ending at its selection binds its found cards.
                LocalEffectNode::ControllerLibrarySearch(search)
                    if super::search_binds_found_cards(search) =>
                {
                    return true;
                }
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
                            | super::SingularPlayerAction::Scry(_)
                            | super::SingularPlayerAction::CreateTokens(_)
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
    // The source's name links, for children that name the source (`the
    // number of verse counters on NAME`).
    let enclosing_names = cursor.replace_source_names(Some(name_links.into()));
    let result = recognize_effect_sequence_attempt_scoped(
        cursor,
        name_links,
        name_failure,
        context,
        stop_before_activation_reduction,
        stop_before_activation_annotation,
        attempt_frame,
    );
    cursor.replace_source_names(enclosing_names);
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
                super::scope_created_rule_body(
                    memory.sentence_delay.as_ref().map(|(_, rule)| rule),
                    &mut action_context,
                );
                action_context.permanent_subject_scope = memory.permanent_subject_scope;
                action_context.current_effect_index = Some(effect_index);
                action_context.frame.event_player |= memory.event_player_named;
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
                // `Exile target card from a graveyard. Its owner creates ...`:
                // a declared card target is no permanent antecedent, but the
                // player relations of that one card are (CR 108.3); only the
                // player-subject readers receive it.
                let relation_antecedent =
                    object_antecedent
                        .clone()
                        .or_else(|| match memory.effects.last() {
                            Some(LocalEffectNode::TargetCardZoneChange(change))
                                if change.cardinality
                                    == super::ExpressedTargetCardinality::EXACTLY_ONE =>
                            {
                                Some(super::ObjectSubjectAntecedent {
                                    reference: super::PermanentEffectReference::Target(
                                        TargetBinding {
                                            effect_index: effect_index - 1,
                                            slot_index: 0,
                                        },
                                    ),
                                    selector: None,
                                })
                            }
                            _ => None,
                        });
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
                        objects: super::permanent_subject::prior_object_set(&memory.effects),
                        ..super::BindingEnvironment::default()
                    },
                )));
                super::publish_installed_program_antecedents(cursor, &memory.effects);
                let base_action_context = action_context;
                let mut preposed_duration = memory.sentence_duration.clone();
                let mut opened_preposed_duration = false;
                // `For each COUNTABLE, ACTION.`: the preposed form of the
                // trailing multiplier, scaling the one action of its sentence.
                let mut preposed_for_each: Option<PreposedForEach> = None;
                let mut effect_start = cursor.position();
                let mut distributed_rest = Vec::new();
                let mut from_distributed = false;
                // `the owner|controller of TARGET`: the relation whose player
                // performs the rest of the clause after this declaration.
                let mut declared_subject: Option<super::ObjectPlayerRelation> = None;
                let mut causative: bool;
                let mut from_ellipsis = false;
                let mut effect = loop {
                    if let Some(pending) = memory.distributed.pop() {
                        // A distributed per-player action is already read.
                        // The later actions of one performed clause keep
                        // its performer (`each player shuffles their hand
                        // and graveyard into their library`).
                        causative = false;
                        from_distributed = true;
                        break match memory.distributed_performer.clone() {
                            Some(performer) => {
                                LocalEffectNode::Performed(super::PerformedEffectNode {
                                    performer,
                                    effect: Box::new(pending),
                                })
                            }
                            None => pending,
                        };
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
                        // `This creature gets +1/+0 until end of turn and
                        // deals 1 damage to you`: a second predicate of the
                        // same written subject (CR 608.2c).
                        if memory.connectors.last() == Some(&EffectConnector::And)
                            && let Some(source) =
                                super::shared_damage_subject(memory.effects.last())
                            && super::shared_subject_damage_starts(attempt)
                        {
                            return super::recognize_damage_predicate(
                                attempt,
                                source,
                                super::SubjectVerbAgreement::Finite,
                            )
                            .map(LocalEffectNode::DealDamage);
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
                                object_subject: relation_antecedent.clone(),
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
                            || super::ring_temptation_starts(attempt)
                            || super::player_designation_follows(attempt).is_some()
                            || super::skip_turn_part_follows(attempt)
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
                            // `you and OBJECTS gain KEYWORDS DURATION` (CR
                            // 109.5, 611.2a): one coordinated subject whose
                            // members each get the same keywords for the same
                            // duration. The objects' clause is the ordinary
                            // keyword grant (its plural verb agrees with the
                            // coordinated subject); the controller's half is
                            // the player keyword grant of the same keywords.
                            if !causative
                                && attempt
                                    .peek()
                                    .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
                                && attempt
                                    .peek_ahead(1)
                                    .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                            {
                                let mut objects = attempt.clone();
                                objects.advance_to(objects.position() + 2)?;
                                let mut object_frame = SequenceAttemptFrame::default();
                                let grant = objects.attempt(|child| {
                                    recognize_effect(
                                        child,
                                        name_links,
                                        name_failure,
                                        memory.last_target,
                                        object_set_binding,
                                        memory.last_object_set.as_ref(),
                                        &action_context,
                                        &mut object_frame,
                                    )
                                });
                                if let Ok(LocalEffectNode::TemporaryKeywordGrant(grant)) = grant {
                                    attempt_frame.commit();
                                    objects.enter("effect.subject.controller-and-objects.keywords");
                                    let keywords = grant
                                        .keywords
                                        .iter()
                                        .map(|keyword| {
                                            super::PlayerGrantedKeyword::of_granted_keyword(
                                                *keyword,
                                            )
                                        })
                                        .collect::<Option<Vec<_>>>()
                                        .filter(|keywords| {
                                            !keywords.is_empty() && grant.abilities.is_empty()
                                        });
                                    let keywords = objects.require_value(
                                        keywords,
                                        "keywords both a player and a permanent can have (hexproof, shroud) for the controller and the objects",
                                    )?;
                                    objects.enter("effect.subject.controller-and-objects.objects");
                                    objects.require(
                                        !matches!(
                                            grant.objects,
                                            super::PermanentOperandNode::Targets(_)
                                        ),
                                        "objects named without a target declaration beside the controller",
                                    )?;
                                    objects.enter("effect.subject.controller-and-objects.duration");
                                    objects.require(
                                        matches!(
                                            grant.duration,
                                            super::EffectDurationNode::UntilEndOfCurrentTurn
                                                | super::EffectDurationNode::UntilNextTurn {
                                                    player: super::LocalPlayerReference::Controller,
                                                }
                                        ),
                                        "a player keyword duration: `until end of turn` or `until your next turn`",
                                    )?;
                                    let player_grant = super::PlayerKeywordGrantNode {
                                        keywords,
                                        duration: grant.duration.clone(),
                                    };
                                    *attempt = objects;
                                    distributed_rest =
                                        vec![LocalEffectNode::TemporaryKeywordGrant(grant)];
                                    return Ok(LocalEffectNode::PlayerAction(
                                        super::PlayerActionNode {
                                            player: super::PlayerEffectSubject::Controller,
                                            action: super::SingularPlayerAction::Status(
                                                super::PlayerStatusAction::GainKeywords(
                                                    player_grant,
                                                ),
                                            ),
                                        },
                                    ));
                                }
                            }
                            match super::try_recognize_player_action_with_object_antecedent(
                                attempt,
                                super::PlayerBinding(effect_index),
                                memory
                                    .last_player_target
                                    .map(|binding| super::PlayerBinding(binding.effect_index)),
                                inherited,
                                relation_antecedent.clone(),
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
                                ) && antecedent
                                    .selector
                                    .as_ref()
                                    .is_some_and(selector_names_permanents))
                                    || super::permanent_subject::singular_created_token_antecedent(
                                        antecedent.reference,
                                        &memory.effects,
                                    )
                            });
                        if permanent_antecedent || shuffled_attached_object_starts(attempt) {
                            match super::recognize_referenced_permanent_zone_action(
                                attempt,
                                name_links,
                                name_failure,
                                &action_context,
                            ) {
                                super::ReferencedPermanentActionAttempt::Parsed(node) => {
                                    return Ok(LocalEffectNode::ReferencedPermanentAction(node));
                                }
                                super::ReferencedPermanentActionAttempt::NotClaimed => {}
                            }
                        }
                        // `shuffle your hand and graveyard into your library`:
                        // the card moves of each zone, then the shuffle, as
                        // consecutive actions of the clause's actor.
                        match super::try_recognize_shuffle_zones_into_library(attempt) {
                            Ok(Some(actions)) => {
                                let mut actions = actions
                                    .into_iter()
                                    .map(LocalEffectNode::CardAction)
                                    .collect::<Vec<_>>();
                                actions.reverse();
                                let first = actions.pop().expect("a move before the shuffle");
                                distributed_rest = actions;
                                return Ok(first);
                            }
                            Ok(None) => {}
                            Err(failure) => {
                                attempt_frame.commit();
                                return Err(failure);
                            }
                        }
                        // `shuffle NAME|this OBJECT into its owner's library`.
                        match super::try_recognize_shuffle_source_into_library(
                            attempt,
                            name_links,
                            name_failure,
                        ) {
                            Ok(Some(movement)) => return Ok(LocalEffectNode::MoveSource(movement)),
                            Ok(None) => {}
                            Err(failure) => {
                                attempt_frame.commit();
                                return Err(failure);
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
                        // The card-action child is transactional: a cast/play
                        // frame that read its `from` (or a tail only it reads)
                        // owns the clause and publishes its own failure.
                        match super::try_recognize_card_action_outcome(
                            attempt,
                            move_ellipsis,
                            super::CardActionReadContext::of(
                                &action_context,
                                name_links,
                                name_failure,
                            ),
                        ) {
                            super::CollectionOperandAttempt::Recognized(fragment) => {
                                return Ok(LocalEffectNode::CardAction(fragment.node));
                            }
                            super::CollectionOperandAttempt::FailedCommitted(failure) => {
                                attempt_frame.commit();
                                return Err(failure);
                            }
                            super::CollectionOperandAttempt::NotClaimed
                            | super::CollectionOperandAttempt::RejectedCandidate(_) => {}
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
                                if !memory.effects.is_empty() && {
                                    let head = super::read_effect_head(attempt);
                                    head.subject == super::EffectSubjectKind::Controller
                                        && head.modality == super::ClauseModality::May
                                } =>
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
                    // A subjectless verb coordinated by `and` with a clause
                    // whose explicit performer opened the sentence shares
                    // that subject (`Any number of target players each lose
                    // 2 life and sacrifice a creature of their choice`): it is
                    // never the controller's imperative (CR 608.2c).
                    let continues_performer = !causative
                        && memory.connectors.last() == Some(&EffectConnector::And)
                        && memory.effects.len() > memory.sentence_start
                        // Only a plural performer's base form reads like an
                        // imperative; a singular performer's verb agrees
                        // with it and its ordinary readers keep it.
                        && memory
                            .current_performer
                            .as_ref()
                            .is_some_and(|(_, agreement, sentence)| {
                                *sentence == memory.sentence_start
                                    && *agreement == super::VerbAgreement::Base
                            })
                        && cursor
                            .peek()
                            .and_then(|raw| super::lookup_verb(raw, &super::ALL_LEMMAS))
                            .is_some_and(|(_, forms)| forms.contains(&VerbForm::Base));
                    let parsed = if continues_performer {
                        cursor.enter("sequence.performer.continued");
                        Err(cursor.fail("the coordinated clause of the same performer"))
                    } else {
                        cursor.attempt(|attempt| {
                            read_action(
                                attempt,
                                memory.current_player_subject.as_ref(),
                                attempt_frame,
                            )
                        })
                    };
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
                    // `the owner|controller of TARGET VERBs ...` (CR 108.3,
                    // 109.4): the subject declares its target permanent with
                    // the ordinary `choose target` instruction; the related
                    // player performs the rest of the clause as the next
                    // action of the same sentence.
                    let parsed = match parsed {
                        Err(failure)
                            if !causative
                                && !attempt_frame.is_committed()
                                && super::object_relation_declaration_starts(cursor) =>
                        {
                            let mut subject = cursor.clone();
                            match super::recognize_object_relation_declaration(&mut subject) {
                                Ok((relation, choice)) => {
                                    *cursor = subject;
                                    declared_subject = Some(relation);
                                    Ok(LocalEffectNode::Choose(choice))
                                }
                                Err(subject) if subject.token_index > failure.token_index => {
                                    Err(subject)
                                }
                                Err(_) => Err(failure),
                            }
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
                                object_antecedent: relation_antecedent.clone(),
                                // The object-related player who performed
                                // the previous action, else the player the
                                // previous action named through its object.
                                object_player_antecedent: match &memory.current_performer {
                                    Some((
                                        super::PerformerNode::Player(
                                            super::PlayerEffectSubject::ObjectRelation {
                                                relation,
                                                object,
                                            },
                                        ),
                                        _,
                                        _,
                                    )) => Some((*relation, *object)),
                                    _ => object_player_mention(
                                        &memory.effects,
                                        object_antecedent.as_ref(),
                                    ),
                                },
                            };
                            let mut head = cursor.clone();
                            // A subjectless verb continues the performer of
                            // the previous action of its sentence.
                            let continued = memory
                                .current_performer
                                .clone()
                                .filter(|(_, _, sentence)| *sentence == memory.sentence_start)
                                .map(|(performer, agreement, _)| Ok((performer, agreement)));
                            // The performer slot may declare a target player
                            // or choose an opponent under this action's index.
                            let performer = super::recognize_declaring_performer(
                                &mut head,
                                &performer_context,
                                Some(super::PlayerBinding(effect_index)),
                            )
                            .or_else(|| {
                                head = cursor.clone();
                                continued
                            });
                            match performer {
                                None => Err(failure),
                                Some(Err(subject)) => Err(furthest(failure, subject)),
                                // `PLAYER may VERB ...` (CR 608.2d): the
                                // performer's own permission. The shared
                                // permission reader reads the sentence in the
                                // performer's person; the performer, put on the
                                // optional wrapper, owns the decision. A bound
                                // player's permission (`that player may`, `its
                                // controller may`) reaches this reader only
                                // when the program-level permission readers,
                                // tried first, did not read it.
                                Some(Ok((performer, _)))
                                    if super::performer_modality(&head)
                                        == super::ClauseModality::May =>
                                {
                                    let description = super::performer_description(&performer);
                                    // The permission needs its written subject:
                                    // a continued verb (`and may ...`) has none.
                                    let mut subject = cursor.clone();
                                    subject.enter("clause.performer.may.subject");
                                    match subject
                                        .require(
                                            head.position() > cursor.position(),
                                            "an explicit performer subject before `may`",
                                        )
                                        .and_then(|()| {
                                            super::person_shifted_permission_tokens(
                                                &head,
                                                description,
                                            )
                                        }) {
                                        Err(verb) => Err(furthest(failure, verb)),
                                        Ok((tokens, subject_end)) => {
                                            let _ = subject.advance_to(subject_end);
                                            let mut shifted = subject.with_tokens(&tokens);
                                            let mut permission_context = action_context.clone();
                                            // CR 118.12: the payer decides before
                                            // the permission; a permission body
                                            // keeps rejecting `unless`.
                                            permission_context.unless_frames =
                                                super::UnlessFrameSupport::Unsupported;
                                            // The permission owns its sentence;
                                            // the body keeps the original token
                                            // positions for its diagnostics.
                                            let body = shifted.attempt(|body| {
                                                body.enter("clause.performer.may.permission");
                                                body.consume_phrase(&["you", "may"])?;
                                                let end = super::permission_sentence_end(body)?;
                                                body.bounded(end + 1, |body| {
                                                    recognize_effect_sequence_with_context(
                                                        body,
                                                        name_links,
                                                        name_failure,
                                                        &permission_context,
                                                    )
                                                })
                                                .map(|sequence| (sequence, end))
                                            });
                                            match body {
                                                Ok((body, end)) => {
                                                    match super::controller_reference_failure(
                                                        &head,
                                                        head.position(),
                                                        end,
                                                    ) {
                                                        Some(ambiguous) => Err(ambiguous),
                                                        None => cursor.advance_to(end).map(|()| {
                                                            LocalEffectNode::Performed(
                                                                super::PerformedEffectNode {
                                                                    performer,
                                                                    effect: Box::new(
                                                                        LocalEffectNode::OptionalSequence(
                                                                            body,
                                                                        ),
                                                                    ),
                                                                },
                                                            )
                                                        }),
                                                    }
                                                }
                                                Err(body) => Err(furthest(failure, body)),
                                            }
                                        }
                                    }
                                }
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
                                            let read = read_action(
                                                &mut shifted,
                                                Some(&controller),
                                                attempt_frame,
                                            );
                                            match read {
                                                Ok(effect) => {
                                                    let end = shifted.position();
                                                    match super::controller_reference_failure(
                                                        &head,
                                                        head.position(),
                                                        end,
                                                    ) {
                                                        Some(ambiguous) => Err(ambiguous),
                                                        None => cursor.advance_to(end).map(|()| {
                                                            LocalEffectNode::Performed(
                                                                super::PerformedEffectNode {
                                                                    performer,
                                                                    effect: Box::new(effect),
                                                                },
                                                            )
                                                        }),
                                                    }
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
                    // `destroy A and all B`, `put N counters on A and M
                    // counters on B` (gapped coordination): when no reader
                    // takes the conjunct, it is read as an argument frame of
                    // the previous imperative lemma, whose verb is written
                    // back in the conjunction's slot. The conjunct must be a
                    // complete clause of that lemma; otherwise the ordinary
                    // failure stands. A separate attempt frame keeps the
                    // ordinary readers' commitment intact.
                    let parsed = match parsed {
                        Err(failure)
                            if !causative
                                && !attempt_frame.is_committed()
                                && matches!(
                                    memory.connectors.last(),
                                    Some(EffectConnector::And | EffectConnector::Or)
                                )
                                && memory.effects.len() > memory.sentence_start
                                && cursor.position() > 0
                                // The slot is the written coordinator (`and`,
                                // `or` or a coordinating comma) the connector read.
                                && cursor
                                    .token_slice(cursor.position() - 1, cursor.position())
                                    .first()
                                    .is_some_and(|token| {
                                        token.raw == ","
                                            || token.raw.eq_ignore_ascii_case("and")
                                            || token.raw.eq_ignore_ascii_case("or")
                                    }) =>
                        {
                            let finite_gap = memory
                                .finite_gap
                                .filter(|gap| gap.sentence == memory.sentence_start);
                            match (memory.elided_lemma, memory.connector_start, finite_gap) {
                                // `PLAYER VERBs A and B` (finite gapping): the
                                // elided verb in its written form continues the
                                // same performer, read by the same subjectless
                                // continuation readers as a written verb.
                                (None, Some(connector_start), Some(gap)) => {
                                    let slot = cursor.position() - 1;
                                    let mut at_slot = cursor.clone();
                                    at_slot.rollback(connector_start);
                                    if at_slot.position() > slot
                                        || at_slot.advance_to(slot).is_err()
                                    {
                                        Err(failure)
                                    } else {
                                        let tokens = super::elided_verb_tokens_in_form(
                                            &at_slot, slot, gap.lemma, gap.form,
                                        );
                                        let mut gapped = at_slot.with_tokens(&tokens);
                                        gapped.enter("sequence.gapped-conjunct.finite");
                                        let mut gapped_frame = SequenceAttemptFrame::default();
                                        let continued = memory.current_performer.clone().filter(
                                            |(_, _, sentence)| *sentence == memory.sentence_start,
                                        );
                                        let mut ambiguity = None;
                                        let read = match continued {
                                            // The performer path of a written
                                            // subjectless verb: its predicate in
                                            // the performer's person.
                                            Some((performer, agreement, _)) => {
                                                let description =
                                                    super::performer_description(&performer);
                                                super::person_shifted_tokens(
                                                    &gapped,
                                                    agreement,
                                                    description,
                                                )
                                                .and_then(|shifted_tokens| {
                                                    let mut shifted =
                                                        gapped.with_tokens(&shifted_tokens);
                                                    let controller =
                                                        super::PlayerEffectSubject::Controller;
                                                    read_action(
                                                        &mut shifted,
                                                        Some(&controller),
                                                        &mut gapped_frame,
                                                    )
                                                    .map(|effect| {
                                                        let end = shifted.position();
                                                        // The gapped conjunct is the
                                                        // performer's clause too: its
                                                        // written `you` (`or your
                                                        // graveyard`) is ambiguous
                                                        // exactly as in a written verb.
                                                        ambiguity =
                                                            super::controller_reference_failure(
                                                                &gapped, slot, end,
                                                            );
                                                        (
                                                            LocalEffectNode::Performed(
                                                                super::PerformedEffectNode {
                                                                    performer,
                                                                    effect: Box::new(effect),
                                                                },
                                                            ),
                                                            end,
                                                        )
                                                    })
                                                })
                                            }
                                            // An ordinary player-subject reader's
                                            // implicit subject continues instead.
                                            None => read_action(
                                                &mut gapped,
                                                memory.current_player_subject.as_ref(),
                                                &mut gapped_frame,
                                            )
                                            .map(|effect| (effect, gapped.position())),
                                        };
                                        match (read, ambiguity) {
                                            (Ok((_, end)), Some(ambiguous))
                                                if end > cursor.position() =>
                                            {
                                                Err(ambiguous)
                                            }
                                            (Ok((effect, end)), None)
                                                if end > cursor.position() =>
                                            {
                                                from_ellipsis = true;
                                                cursor.advance_to(end).map(|()| effect)
                                            }
                                            _ => Err(failure),
                                        }
                                    }
                                }
                                (Some(lemma), Some(connector_start), _) => {
                                    let slot = cursor.position() - 1;
                                    let mut at_slot = cursor.clone();
                                    at_slot.rollback(connector_start);
                                    if at_slot.position() > slot
                                        || at_slot.advance_to(slot).is_err()
                                    {
                                        Err(failure)
                                    } else {
                                        let tokens =
                                            super::elided_verb_tokens(&at_slot, slot, lemma);
                                        let mut gapped = at_slot.with_tokens(&tokens);
                                        gapped.enter("sequence.gapped-conjunct");
                                        let mut gapped_frame = SequenceAttemptFrame::default();
                                        let read =
                                            read_action(&mut gapped, None, &mut gapped_frame)
                                                .map(|effect| (effect, gapped.position()));
                                        // `... and the rest on the bottom`: the
                                        // conjunct's library is its antecedent's;
                                        // the same reading with that library
                                        // restated, positions mapped back.
                                        let read = match (read, memory.last_effect_span) {
                                            (Ok(read), _) => Ok(read),
                                            (Err(error), Some((start, end))) => {
                                                match super::elided_library_tokens(
                                                    &tokens,
                                                    slot + 1,
                                                    cursor.token_slice(start, end),
                                                ) {
                                                    Some((view, inserted_at, inserted)) => {
                                                        let mut restated =
                                                            at_slot.with_tokens(&view);
                                                        restated.enter(
                                                            "sequence.gapped-conjunct.library",
                                                        );
                                                        let mut restated_frame =
                                                            SequenceAttemptFrame::default();
                                                        match read_action(
                                                            &mut restated,
                                                            None,
                                                            &mut restated_frame,
                                                        ) {
                                                            Ok(effect)
                                                                if restated.position()
                                                                    >= inserted_at + inserted =>
                                                            {
                                                                Ok((
                                                                    effect,
                                                                    restated.position() - inserted,
                                                                ))
                                                            }
                                                            _ => Err(error),
                                                        }
                                                    }
                                                    None => Err(error),
                                                }
                                            }
                                            (Err(error), None) => Err(error),
                                        };
                                        match read {
                                            Ok((effect, end)) if end > cursor.position() => {
                                                from_ellipsis = true;
                                                cursor.advance_to(end).map(|()| effect)
                                            }
                                            _ => Err(failure),
                                        }
                                    }
                                }
                                _ => Err(failure),
                            }
                        }
                        parsed => parsed,
                    };
                    match parsed {
                        Ok(effect) => break effect,
                        Err(failure) => {
                            // `When EVENT this turn, EFFECTS.` / `If OBJECT
                            // would die this turn, ACTION instead.`: a rule the
                            // resolving effect creates over the actions of its
                            // sentence (CR 603.7, 614.1a), read only after the
                            // ordinary readers decline at its first action.
                            if !attempt_frame.is_committed()
                                && preposed_duration.is_none()
                                && memory.sentence_delay.is_none()
                                && memory.open_branch.is_none()
                                && memory.effects.len() == memory.sentence_start
                                && super::effect_created_rule_starts(cursor)
                            {
                                let mut trial = cursor.clone();
                                let rule = super::recognize_effect_created_rule(
                                    &mut trial,
                                    name_links,
                                    name_failure,
                                )?;
                                trial.enter("sequence.created-rule.separator");
                                trial.consume(",")?;
                                *cursor = trial;
                                super::enter_created_rule_body(cursor, &rule);
                                memory.sentence_delay = Some((memory.effects.len(), rule));
                                attempt_frame.clear();
                                effect_start = cursor.position();
                                continue;
                            }
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
                                // `Then, if ...`: the comma punctuates the
                                // sentence connector, not a clause boundary.
                                let follows_clause_comma = cursor.position() > 0
                                    && cursor
                                        .token_slice(cursor.position() - 1, cursor.position())
                                        .first()
                                        .is_some_and(|token| token.raw == ",")
                                    && !(cursor.position() > 1
                                        && cursor
                                            .token_slice(
                                                cursor.position() - 2,
                                                cursor.position() - 1,
                                            )
                                            .first()
                                            .is_some_and(|token| {
                                                token.raw.eq_ignore_ascii_case("then")
                                            }));
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
                                memory.sentence_delay = Some((memory.effects.len(), timing.into()));
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
                                attempt_frame.clear();
                                effect_start = cursor.position();
                                // `For each OBJECT VERBed this way, its
                                // controller|owner ACTION.`: one pass per
                                // affected object, performed by that
                                // object's player (CR 608.2c, 608.2h). The
                                // action is that player's own instruction.
                                if let PreposedForEach::ActionResult(reference) = &count
                                    && let Some(relation) = super::affected_object_player_at(cursor)
                                {
                                    attempt_frame.commit();
                                    cursor.enter("sequence.for-each.affected-object-player");
                                    let mut written = memory.effects.clone();
                                    written.extend(memory.effects.last().cloned());
                                    let (producer, filter) = cursor.require_value(
                                        super::resolve_action_result_antecedent(
                                            &written, reference,
                                        ),
                                        "an earlier action of this program whose affected objects the phrase restates",
                                    )?;
                                    cursor.advance_to(cursor.position() + 2)?;
                                    let performer = super::PerformerNode::EachAffectedObject {
                                        relation,
                                        effect_index: producer,
                                        filter,
                                    };
                                    let tokens = super::person_shifted_tokens(
                                        cursor,
                                        super::VerbAgreement::ThirdPersonSingular,
                                        super::performer_description(&performer),
                                    )?;
                                    let mut shifted = cursor.with_tokens(&tokens);
                                    let controller = super::PlayerEffectSubject::Controller;
                                    let action = read_action(
                                        &mut shifted,
                                        Some(&controller),
                                        attempt_frame,
                                    )?;
                                    let end = shifted.position();
                                    if let Some(ambiguous) = super::controller_reference_failure(
                                        cursor,
                                        cursor.position(),
                                        end,
                                    ) {
                                        return Err(ambiguous);
                                    }
                                    cursor.advance_to(end)?;
                                    break LocalEffectNode::Performed(super::PerformedEffectNode {
                                        performer,
                                        effect: Box::new(action),
                                    });
                                }
                                preposed_for_each = Some(count);
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
                if !distributed_rest.is_empty() {
                    memory.distributed_performer = match &effect {
                        LocalEffectNode::Performed(node) => Some(node.performer.continued()),
                        _ => None,
                    };
                }
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
                        apply_player_action_multiplier(&mut effect, count),
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
                        LocalEffectNode::CardAction(super::CardActionNode::GrantKeyword(node)) => {
                            &node.duration == duration
                        }
                        LocalEffectNode::TemporaryPlayerPermission(node) => {
                            node.duration == super::TemporaryRuleDuration::Effect(duration.clone())
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
                // `X becomes TYPES and gets +N/+N until end of turn`: one
                // trailing duration scopes the whole verb chain (CR 611.2a).
                // The type change before it stated none and waited for it.
                if base_action_context.verb_chain_subject.is_some()
                    && let LocalEffectNode::ModifyStats(stats) = &effect
                    && stats.duration != super::EffectDurationNode::Indefinite
                    && let Some(LocalEffectNode::BecomeTypes(change)) = memory.effects.last_mut()
                    && change.duration.is_none()
                {
                    change.duration = Some(stats.duration.clone());
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
                        // CR 107.3: a definition of X the search read after its
                        // counted phrase binds X for this sentence's actions
                        // through this search, like a postposed `where X`.
                        if let Some(expression) = search.where_x.take() {
                            cursor.enter("sequence.where-x.scope");
                            cursor.require(
                                !memory
                                    .where_x
                                    .iter()
                                    .any(|binding| binding.first_effect == memory.sentence_start),
                                "one definition of X for the actions of this sentence",
                            )?;
                            memory.where_x.push(super::WhereXBindingNode {
                                first_effect: memory.sentence_start,
                                end_effect: effect_index + 1,
                                expression,
                            });
                        }
                        // A searched collection is a new object antecedent, but it is
                        // not a target slot. Until collection/object anaphora is
                        // modeled explicitly, never let `it` fall back to a target
                        // chosen by an earlier, unrelated effect.
                        memory.last_target = None;
                        // `Search target player's library ...`: the declared
                        // holder is the player antecedent of a later `that
                        // player` (`Then that player shuffles.`).
                        if matches!(
                            search.origin.holder,
                            super::SearchedPlayer::DeclaredTarget(_)
                        ) {
                            memory.last_player_target = Some(PlayerTargetBinding { effect_index });
                        }
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
                        super::ChoiceDomainNode::PlayerTarget(_)
                        | super::ChoiceDomainNode::Player(_) => {
                            memory.last_player_target = Some(PlayerTargetBinding { effect_index });
                        }
                        // A characteristic choice introduces no object or
                        // player antecedent; earlier antecedents remain.
                        super::ChoiceDomainNode::CreatureType
                        | super::ChoiceDomainNode::Color
                        | super::ChoiceDomainNode::CardName(_)
                        | super::ChoiceDomainNode::Number(_) => {}
                        // Permanents chosen on resolution are no declared
                        // target: the permanent subject scope names them.
                        super::ChoiceDomainNode::Objects(_) => memory.last_target = None,
                    }
                }
                // `look at target player's hand`, `reveal the top card of
                // target opponent's library`: the declared zone owner is the
                // player a later `that player` names (CR 115.1, 608.2c).
                if let LocalEffectNode::CardAction(action) = &effect
                    && card_action_declares_zone_owner(action)
                {
                    memory.last_player_target = Some(PlayerTargetBinding { effect_index });
                }
                if let LocalEffectNode::TargetPlayerLife(life) = &effect
                    && life.declares_target
                {
                    memory.last_player_target = Some(life.player);
                }
                // A declared performer is the program's player antecedent:
                // a later `that player` names it (CR 608.2c).
                if let LocalEffectNode::Performed(node) = &effect
                    && let Some(binding) = node.performer.declared_player()
                {
                    memory.last_player_target = Some(PlayerTargetBinding {
                        effect_index: binding.0,
                    });
                }
                // `defending player reveals ... . If ..., that player ...`:
                // the event's player performed this action; it stays the
                // antecedent of a later `that player` (CR 603.2, 506.2).
                if match &effect {
                    LocalEffectNode::Performed(node) => {
                        node.performer
                            == super::PerformerNode::Player(super::PlayerEffectSubject::EventPlayer)
                    }
                    LocalEffectNode::PlayerAction(action) => {
                        action.player == super::PlayerEffectSubject::EventPlayer
                    }
                    _ => false,
                } {
                    memory.event_player_named = true;
                }
                memory.current_performer = match &effect {
                    // A distributed action continues the agreement its
                    // clause's subject gave the verb.
                    LocalEffectNode::Performed(node) => Some((
                        node.performer.continued(),
                        memory
                            .current_performer
                            .as_ref()
                            .filter(|_| from_distributed)
                            .map(|(_, agreement, _)| *agreement)
                            .unwrap_or_else(|| {
                                performed_agreement(&node.performer, cursor, effect_start)
                            }),
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
                if let Some(relation) = declared_subject {
                    memory.current_performer = Some((
                        super::PerformerNode::Player(super::PlayerEffectSubject::ObjectRelation {
                            relation,
                            object: super::PermanentEffectReference::Target(TargetBinding {
                                effect_index,
                                slot_index: 0,
                            }),
                        }),
                        super::VerbAgreement::ThirdPersonSingular,
                        memory.sentence_start,
                    ));
                    memory.subject_continuation = true;
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
                memory.causative_effect = causative.then_some(effect_index);
                // A gapped conjunct keeps the lemma it elided for the next one
                // (`A, B, and C`); any other action names its own.
                if !from_ellipsis {
                    memory.elided_lemma = super::imperative_lemma_at(cursor, effect_start);
                    // A finite clause of an explicit player subject names its
                    // written verb for a later gapped conjunct of its sentence.
                    let agreement = memory
                        .current_performer
                        .as_ref()
                        .filter(|(_, _, sentence)| *sentence == memory.sentence_start)
                        .map(|(_, agreement, _)| *agreement)
                        .or_else(|| {
                            memory
                                .current_player_subject
                                .as_ref()
                                .map(|_| performed_agreement_of_player(cursor, effect_start))
                        });
                    memory.finite_gap = match (memory.elided_lemma, agreement) {
                        (None, Some(agreement)) => super::finite_clause_verb(
                            cursor,
                            effect_start,
                            cursor.position(),
                            agreement,
                        )
                        .map(|(lemma, form)| FiniteGap {
                            lemma,
                            form,
                            sentence: memory.sentence_start,
                        }),
                        _ => None,
                    };
                }
                // `Destroy all lands or all creatures. Creatures destroyed
                // this way can't be regenerated.`: the amendment after the
                // last alternative restates the destruction of whichever
                // alternative is chosen (CR 701.19c), so it covers the run.
                if let LocalEffectNode::DestroyCollection(amended) = &effect
                    && amended.cannot_regenerate
                {
                    let run_start = memory
                        .connectors
                        .iter()
                        .rposition(|connector| *connector != EffectConnector::Or)
                        .map_or(0, |index| index + 1);
                    if memory.connectors.last() == Some(&EffectConnector::Or) {
                        for earlier in &mut memory.effects[run_start..] {
                            if let LocalEffectNode::DestroyCollection(collection) = earlier {
                                collection.cannot_regenerate = true;
                            }
                        }
                    }
                }
                // CR 608.2d: the player who performs the alternatives chooses
                // among them. Each alternative names that player explicitly:
                // the controller's imperative, or a written performer.
                if memory.connectors.last() == Some(&EffectConnector::Or) {
                    let run_start = memory
                        .connectors
                        .iter()
                        .rposition(|connector| *connector != EffectConnector::Or)
                        .map_or(0, |index| index + 1);
                    cursor.enter("sequence.alternative.chooser");
                    cursor.require(
                        alternative_chooser_is_explicit(&effect)
                            && memory.effects[run_start..]
                                .iter()
                                .all(alternative_chooser_is_explicit),
                        "alternatives the controller or one written performer chooses between",
                    )?;
                }
                memory.last_effect_span = Some((effect_start, cursor.position()));
                memory.effects.push(effect);
                if !matches!(memory.open_branch, Some(ConditionalBranch::Else(_))) {
                    memory.else_candidate = None;
                }
                SequenceState::ConnectorOrEnd
            }
            SequenceState::ConnectorOrEnd => {
                cursor.enter("sequence.connector-or-end");
                // A declared object's player performs the rest of its
                // clause: the verb follows the declaration directly.
                if std::mem::take(&mut memory.subject_continuation) {
                    memory.connectors.push(EffectConnector::And);
                    state = SequenceState::Effect;
                    continue;
                }
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
                        // `where X is its power`: the object of the clause it closes.
                        let antecedent = super::permanent_subject::object_antecedent_after(
                            &memory.effects,
                            memory.permanent_subject_scope,
                            context,
                        );
                        let expression = super::with_object_antecedent(
                            cursor,
                            antecedent,
                            super::recognize_where_x_definition,
                        )?;
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
                    Some("and" | "then" | "," | "or") => {
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
                        memory.connector_start = Some(cursor.checkpoint());
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
                    Some("if")
                        if context.sentence_conditionals
                            && memory.open_branch.is_none()
                            && memory.sentence_delay.is_none()
                            && memory.sentence_duration.is_none()
                            && memory.effects.len() == memory.sentence_start + 1
                            && super::sentence_condition_starts(cursor) =>
                    {
                        // #11 Branch: a postposed `if CONDITION` gates the one
                        // action of its sentence (CR 608.2c), the same
                        // conditional as the preposed form with no else branch.
                        attempt_frame.commit();
                        // The condition is checked before its action is
                        // performed: only earlier sentences' departures and
                        // no card-collection result of this very action.
                        let departed = memory
                            .departed_permanent_targets()
                            .into_iter()
                            .filter(|binding| binding.effect_index < memory.sentence_start)
                            .collect::<Vec<_>>();
                        let mut condition_effects = context.clone();
                        condition_effects.permanent_subject_scope = memory.permanent_subject_scope;
                        let condition_context = super::SentenceConditionContext {
                            name_links,
                            name_failure,
                            effects: &condition_effects,
                            card_target: memory.card_target_antecedent(),
                            card_focus: false,
                            departed_targets: &departed,
                        };
                        // Targets are declared as the spell is cast (CR
                        // 601.2c): the condition may read its own action's.
                        super::publish_installed_declared_target(cursor, &memory.effects);
                        let condition = super::recognize_postposed_sentence_condition(
                            cursor,
                            &condition_context,
                        )?;
                        cursor.require(
                            !condition_reads_previous_action(&condition),
                            "a postposed condition that does not read the result of an earlier action",
                        )?;
                        cursor.enter("sequence.conditional.postposed-scope");
                        cursor.require(
                            matches!(cursor.peek(), None | Some(".")),
                            "a postposed condition ending its sentence",
                        )?;
                        // The anaphors an `Otherwise` sentence would read are
                        // those before this sentence, which are no longer
                        // known here: no `Otherwise` may follow a postposed `if`.
                        let snapshot = memory.anaphor_snapshot();
                        memory.branch_snapshots.push(snapshot);
                        memory.conditionals.push(super::SentenceConditionalNode {
                            condition,
                            then_first: memory.sentence_start,
                            then_end: memory.effects.len(),
                            else_end: None,
                        });
                        memory.else_candidate = None;
                        SequenceState::ConnectorOrEnd
                    }
                    // `ACTION instead.` closes a created replacement's
                    // action (CR 614.1a).
                    Some("instead")
                        if memory
                            .sentence_delay
                            .as_ref()
                            .is_some_and(|(_, rule)| rule.awaits_instead()) =>
                    {
                        attempt_frame.commit();
                        if let Some((_, rule)) = memory.sentence_delay.as_mut() {
                            rule.read_instead(cursor)?;
                        }
                        SequenceState::ConnectorOrEnd
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
                        if super::type_retention_amendment_starts(cursor) {
                            // CR 205.1b: the sentence amends the type change
                            // of the sentence just closed.
                            attempt_frame.commit();
                            super::recognize_type_retention_amendment(
                                cursor,
                                memory.effects.last_mut(),
                            )?;
                            cursor.enter("sequence.type-retention.terminator");
                            cursor.consume(".")?;
                        }
                        if super::granted_keyword_cost_amendment_starts(cursor) {
                            // CR 702.34a: the sentence states the cost of the
                            // keyword the sentence just closed granted.
                            attempt_frame.commit();
                            super::recognize_granted_keyword_cost_amendment(
                                cursor,
                                memory.effects.last_mut(),
                            )?;
                            cursor.enter("sequence.granted-keyword-cost.terminator");
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
                            && let Some(creation) = memory
                                .effects
                                .last()
                                .and_then(super::reminded_token_creation)
                        {
                            super::recognize_token_creation_reminder(cursor, &creation)?;
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
                                // `Then, ACTION.`: the punctuated variant of
                                // the same sentence connector (CR 608.2c).
                                if cursor.peek() == Some(",") {
                                    cursor.consume(",")?;
                                }
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
                            .is_some_and(|effect| apply_player_action_multiplier(effect, count));
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
                            rule: timing.into(),
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
                    // The same adjunct after an exile of every permanent a
                    // query matches (`exile all artifacts your opponents
                    // control until this enchantment leaves the battlefield`).
                    Some("until")
                        if super::zone_change_until_starts(cursor)
                            && matches!(
                                memory.effects.last(),
                                Some(LocalEffectNode::SelectedPermanents(selected))
                                    if selected.action == super::SelectedPermanentAction::Exile
                                        && selected.until.is_none()
                            ) =>
                    {
                        attempt_frame.commit();
                        let until =
                            super::recognize_zone_change_until(cursor, name_links, name_failure)?;
                        if let Some(LocalEffectNode::SelectedPermanents(selected)) =
                            memory.effects.last_mut()
                        {
                            selected.until = Some(until);
                        }
                        SequenceState::ConnectorOrEnd
                    }
                    // `ACTION twice` / `ACTION N times`: the repetition is
                    // read by an action whose node repeats as a whole
                    // (`investigate twice`). After any other action it is no
                    // multiplied quantity (`draw a card twice` is not `draw
                    // two cards` for triggers counting draws): explicit.
                    _ if super::repetition_adjunct_starts(cursor) => {
                        cursor.enter("sequence.repetition.action");
                        return Err(cursor.fail(
                            "an action that owns its repetition (`investigate twice`); this action is not repeated as a whole",
                        ));
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

/// Whether an `or` alternative names the player who chooses it (CR 608.2d):
/// an action of the controller, or one a written performer performs. An
/// ordinary reader's other player subject (`target player gains 3 life or
/// draws a card`) has no performer this frame can make the chooser.
fn alternative_chooser_is_explicit(effect: &LocalEffectNode) -> bool {
    match effect {
        LocalEffectNode::Performed(_) => true,
        LocalEffectNode::PlayerAction(action) => {
            action.player == super::PlayerEffectSubject::Controller
        }
        LocalEffectNode::DiscardCards(discard) => {
            discard.player == super::PlayerEffectSubject::Controller
        }
        LocalEffectNode::GainLife(life) => life.player == super::LocalPlayerReference::Controller,
        LocalEffectNode::LoseLife(life) => life.player == super::LocalPlayerReference::Controller,
        LocalEffectNode::DrawCards(draw) => draw.player == super::LocalPlayerReference::Controller,
        LocalEffectNode::QuantifiedPlayerAction(_)
        | LocalEffectNode::TargetPlayerLife(_)
        | LocalEffectNode::MillCards(_)
        | LocalEffectNode::OptionalSequence(_)
        | LocalEffectNode::OptionalDrawCards(_)
        | LocalEffectNode::OptionalLibrarySearch(_)
        | LocalEffectNode::OptionalCardDeployment(_) => false,
        _ => true,
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

/// True when a sentence condition reads the result of the action before it
/// (`if you can't`): its preposed form names the previous sentence's action,
/// which a postposed condition would misread as its own.
fn condition_reads_previous_action(condition: &super::SentenceConditionNode) -> bool {
    // An earlier action's or optional payment's outcome (`if you can't`,
    // `if you don't`) is the one condition language's ActionOutcome family.
    super::condition_any(condition, &|atom| {
        matches!(atom, super::ConditionAtom::ActionOutcome(_))
    })
}

/// A `for each` multiplier of the action it scales (CR 608.2c): the
/// performed action of a performer, and the quantity slots of the player
/// cells (a counted discard or sacrifice, a declared player's life change,
/// the one action of a player set), each scaled by the shared
/// `scale_by_for_each_count`. Every other action is the shared multiplier's.
fn apply_player_action_multiplier(
    effect: &mut LocalEffectNode,
    count: super::NumericExpression,
) -> bool {
    let player_cell = matches!(
        effect,
        LocalEffectNode::Performed(_)
            | LocalEffectNode::TargetPlayerLife(_)
            | LocalEffectNode::QuantifiedPlayerAction(_)
            | LocalEffectNode::PlayerAction(super::PlayerActionNode {
                action: super::SingularPlayerAction::Discard(_)
                    | super::SingularPlayerAction::Sacrifice(_),
                ..
            })
    );
    if !player_cell {
        return apply_for_each_multiplier(effect, count);
    }
    let quantity = match effect {
        LocalEffectNode::Performed(node) => {
            return apply_player_action_multiplier(&mut node.effect, count);
        }
        LocalEffectNode::TargetPlayerLife(life) => &mut life.amount,
        LocalEffectNode::PlayerAction(super::PlayerActionNode {
            action: super::SingularPlayerAction::Discard(discard),
            ..
        }) => match &mut discard.quantity {
            super::DiscardQuantity::Count(quantity) => quantity,
            _ => return false,
        },
        LocalEffectNode::PlayerAction(super::PlayerActionNode {
            action: super::SingularPlayerAction::Sacrifice(sacrifice),
            ..
        }) => &mut sacrifice.count,
        LocalEffectNode::QuantifiedPlayerAction(node) => match node.actions.as_mut_slice() {
            [super::QuantifiedPlayerAction::Life(life)] => &mut life.amount,
            [super::QuantifiedPlayerAction::Discard(discard)] => match &mut discard.quantity {
                super::DiscardQuantity::Count(quantity) => quantity,
                _ => return false,
            },
            [super::QuantifiedPlayerAction::Sacrifice(sacrifice)] => &mut sacrifice.count,
            [
                super::QuantifiedPlayerAction::Draw(quantity)
                | super::QuantifiedPlayerAction::Mill(quantity),
            ] => quantity,
            [super::QuantifiedPlayerAction::Counters(counters)] => &mut counters.count,
            // A whole-hand action has no quantity; a list of actions would
            // scale only its last one.
            _ => return false,
        },
        _ => return false,
    };
    match super::scale_by_for_each_count(quantity, count) {
        Some(scaled) => {
            *quantity = scaled;
            true
        }
        None => false,
    }
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

/// The inputs every effect frame may read, passed once to the dispatcher's
/// frame readers.
struct EffectFrameInputs<'a> {
    name_links: &'a [ResolvedNameLink],
    name_failure: Option<&'a NameResolutionFailure>,
    antecedent: Option<TargetBinding>,
    object_set_binding: ObjectSetBinding,
    object_set_antecedent: Option<&'a BoundPermanentSetNode>,
    context: &'a EffectRecognitionContext,
}

/// The effect dispatcher (S3): `SUBJECT x LEMMA x MODALITY`.
///
/// ```text
/// Effect := ClaimedFrame                 (retarget, control change, stack copy)
///         | KeywordAction                (the keyword-action lexicon rows)
///         | Head(Imperative|Controller, LEMMA) LemmaFrame
///         | SubjectFrame                 (a subject phrase owns its verb)
/// ```
///
/// The head is classified once (`effect_head.rs`); the lemma comes from the
/// verb lexicon. No branch tests a verb word: a new verb is a lexicon row
/// plus one lemma arm, and a subject the frames do not read fails at
/// `effect.start` with `a supported effect subject` only when no subject
/// frame claims its head.
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
    if super::damage_redirection_starts(cursor) {
        // `AMOUNT damage that would be dealt ... is dealt to PARTY instead`
        // (CR 614.9): the damage is the subject of the redirection shield.
        attempt_frame.commit();
        return super::recognize_damage_redirection_effect(
            cursor,
            name_links,
            name_failure,
            context,
        )
        .map(LocalEffectNode::PreventDamage);
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
    let inputs = EffectFrameInputs {
        name_links,
        name_failure,
        antecedent,
        object_set_binding,
        object_set_antecedent,
        context,
    };
    // Keyword actions (CR 701) are ordinary verbs: the keyword-action
    // lexicon rows own the claim and the operand comes from the shared
    // permanent operand or numeric grammar.
    if let Some(result) = recognize_keyword_action_frame(cursor, &inputs) {
        return result;
    }
    let head = super::read_effect_head(cursor);
    if let Some(result) = recognize_lemma_frame(cursor, &head, &inputs, attempt_frame) {
        return result;
    }
    recognize_subject_frame(cursor, &head, &inputs, attempt_frame)
}

/// The keyword-action rows (CR 701) whose verb names the action.
fn recognize_keyword_action_frame(
    cursor: &mut OracleTokenCursor<'_>,
    inputs: &EffectFrameInputs<'_>,
) -> Option<MachineResult<LocalEffectNode>> {
    let word = cursor.peek()?.to_string();
    let word = word.as_str();
    // A resolved card name that begins with a verb and continues past it
    // (`Turn to Slag deals 5 damage`) is the clause's subject, never the
    // keyword action's verb. A name that is exactly the keyword action
    // (`Manifest dread.` on Manifest Dread, no predicate after the name)
    // stays the action.
    let position = cursor.position();
    if inputs.name_links.iter().any(|link| {
        link.token_start == position
            && link.token_end > position + 1
            // The name is the subject only when a predicate follows it.
            && cursor
                .peek_ahead(link.token_end - position)
                .is_some_and(|raw| !matches!(raw, "." | "," | ";" | "("))
            && matches!(
                link.kind,
                crate::oracle::syntax::NameLinkKind::CurrentCard
                    | crate::oracle::syntax::NameLinkKind::CurrentFace
            )
    }) {
        return None;
    }
    if super::PermanentKeywordAction::from_imperative_object_verb(word).is_some() {
        return Some(
            super::recognize_imperative_permanent_keyword_action(
                cursor,
                inputs.name_links,
                inputs.name_failure,
                inputs.context,
            )
            .map(LocalEffectNode::PermanentKeywordAction),
        );
    }
    if super::PermanentKeywordAction::from_self_quantity_verb(word).is_some() {
        return Some(
            super::recognize_self_quantity_keyword_action(
                cursor,
                inputs.antecedent,
                inputs.context,
            )
            .map(LocalEffectNode::PermanentKeywordAction),
        );
    }
    if super::ControllerKeywordAction::from_imperative_verb(word).is_some() {
        return Some(
            super::recognize_controller_keyword_action(cursor, inputs.antecedent)
                .map(LocalEffectNode::ControllerKeywordAction),
        );
    }
    None
}

/// The frame of a lemma whose subject is the controller: the imperative,
/// `you VERB`, or a preposed iteration of the mana output. `None` leaves the
/// head to the subject frames (a lemma without an imperative frame here, or
/// an operand shape another frame owns).
fn recognize_lemma_frame(
    cursor: &mut OracleTokenCursor<'_>,
    head: &super::EffectHead,
    inputs: &EffectFrameInputs<'_>,
    attempt_frame: &mut SequenceAttemptFrame,
) -> Option<MachineResult<LocalEffectNode>> {
    use super::EffectSubjectKind as Subject;
    let lemma = head.lemma?;
    match (head.subject, head.modality) {
        (Subject::ColorIteration, _) => {
            Some(recognize_add_mana_effect(cursor).map(LocalEffectNode::AddMana))
        }
        (Subject::Imperative, super::ClauseModality::Indicative) => {
            recognize_imperative_frame(cursor, lemma, inputs, attempt_frame)
        }
        (Subject::Controller, super::ClauseModality::Indicative) => match lemma {
            VerbLemma::Discard if head.base(VerbLemma::Discard) => Some(
                super::recognize_ordinary_player_discard(cursor).map(LocalEffectNode::DiscardCards),
            ),
            VerbLemma::Draw if head.base(VerbLemma::Draw) => {
                Some(recognize_draw_frame(cursor, inputs, attempt_frame))
            }
            VerbLemma::Create if head.base(VerbLemma::Create) => Some(
                recognize_token_creation_frame(cursor, inputs, attempt_frame),
            ),
            _ => None,
        },
        (Subject::ObjectRelatedPlayer, super::ClauseModality::Indicative)
            if head.is(VerbLemma::Create, VerbForm::ThirdPersonSingular) =>
        {
            Some(recognize_token_creation_frame(
                cursor,
                inputs,
                attempt_frame,
            ))
        }
        _ => None,
    }
}

/// `draw ...` / `you draw ...`: the shared draw reader.
fn recognize_draw_frame(
    cursor: &mut OracleTokenCursor<'_>,
    inputs: &EffectFrameInputs<'_>,
    attempt_frame: &mut SequenceAttemptFrame,
) -> MachineResult<LocalEffectNode> {
    match try_recognize_draw_cards_effect_with_context(cursor, inputs.context) {
        DrawCardsEffectParseAttempt::Parsed(effect) => Ok(LocalEffectNode::DrawCards(effect)),
        DrawCardsEffectParseAttempt::RejectedCandidate(failure) => Err(failure),
        DrawCardsEffectParseAttempt::FailedCommitted(failure) => {
            attempt_frame.commit();
            Err(failure)
        }
    }
}

/// `[PLAYER] create(s) TOKENS`: the shared token-creation action. A
/// possessed-object controller (`that creature's controller creates`) is an
/// owned action head: its binding and nominal failures stay explicit.
fn recognize_token_creation_frame(
    cursor: &mut OracleTokenCursor<'_>,
    inputs: &EffectFrameInputs<'_>,
    attempt_frame: &mut SequenceAttemptFrame,
) -> MachineResult<LocalEffectNode> {
    if super::operand_determiner(cursor, 0) == super::OperandDeterminer::That {
        attempt_frame.commit();
        // Only this ordinary token action receives the caller's explicit
        // actor-reference capability. Life/damage/duration readers keep
        // their historical unqualified wrapper.
        return super::recognize_create_creature_tokens_effect_with_reference_support(
            cursor,
            inputs.antecedent,
            inputs.context.target_reference_support,
        )
        .map(LocalEffectNode::CreateTokens);
    }
    match super::try_recognize_token_creation_action_in_context(
        cursor,
        inputs.antecedent,
        Some(super::TokenObjectContext {
            name_links: inputs.name_links,
            name_failure: inputs.name_failure,
            context: inputs.context,
        }),
    ) {
        super::TokenCreationAttempt::Recognized(effect) => Ok(effect),
        super::TokenCreationAttempt::FailedCommitted(failure) => Err(failure),
        super::TokenCreationAttempt::NotClaimed => {
            Err(cursor.fail("a complete token creation action"))
        }
    }
}

/// One arm per imperative lemma. Each arm owns every operand shape of its
/// lemma, in the order its readers claim them; `None` hands the head to the
/// subject frames (the lemma has no imperative frame here).
fn recognize_imperative_frame(
    cursor: &mut OracleTokenCursor<'_>,
    lemma: VerbLemma,
    inputs: &EffectFrameInputs<'_>,
    attempt_frame: &mut SequenceAttemptFrame,
) -> Option<MachineResult<LocalEffectNode>> {
    let EffectFrameInputs {
        name_links,
        name_failure,
        antecedent,
        object_set_binding: _,
        object_set_antecedent: _,
        context,
    } = *inputs;
    Some(match lemma {
        VerbLemma::Discard => {
            super::recognize_ordinary_player_discard(cursor).map(LocalEffectNode::DiscardCards)
        }
        VerbLemma::Support => {
            super::recognize_support_action(cursor, context).map(LocalEffectNode::PutCounters)
        }
        // `choose DOMAIN` owns its verb: every domain either binds or fails
        // at its own typed state (CR 115.1, 608.2d). The failure stays
        // tentative so modal parents keep their own claim on mode choices.
        VerbLemma::Choose => {
            super::recognize_choose_instruction(cursor).map(LocalEffectNode::Choose)
        }
        VerbLemma::Attach => {
            let mut trial = cursor.clone();
            super::recognize_attach_effect(&mut trial, name_links, name_failure, context).map(
                |node| {
                    *cursor = trial;
                    LocalEffectNode::Attach(node)
                },
            )
        }
        VerbLemma::Prevent => {
            super::recognize_prevent_damage_effect(cursor, name_links, name_failure, context)
                .map(LocalEffectNode::PreventDamage)
        }
        VerbLemma::Regenerate => {
            super::recognize_regenerate_effect(cursor, name_links, name_failure, context)
                .map(LocalEffectNode::PermanentKeywordAction)
        }
        VerbLemma::Goad => super::recognize_goad_effect(cursor, name_links, name_failure, context)
            .map(LocalEffectNode::PermanentKeywordAction),
        VerbLemma::Tap => recognize_tap_frame(cursor, inputs),
        VerbLemma::Untap => recognize_untap_frame(cursor, inputs),
        VerbLemma::Add => recognize_add_mana_effect(cursor).map(LocalEffectNode::AddMana),
        VerbLemma::Sacrifice => recognize_ordinary_sacrifice_effect(
            cursor,
            name_links,
            name_failure,
            context.source.as_ref(),
        )
        .map(LocalEffectNode::Sacrifice),
        VerbLemma::Draw => recognize_draw_frame(cursor, inputs, attempt_frame),
        VerbLemma::Amass => recognize_amass_effect(cursor, antecedent).map(LocalEffectNode::Amass),
        VerbLemma::Empower => {
            recognize_empower_jace_effect(cursor, antecedent).map(LocalEffectNode::EmpowerJace)
        }
        VerbLemma::Investigate => {
            recognize_investigate_effect(cursor).map(LocalEffectNode::Investigate)
        }
        VerbLemma::Scry => recognize_scry_effect(cursor).map(LocalEffectNode::Scry),
        VerbLemma::Surveil => {
            recognize_surveil_effect(cursor, antecedent).map(LocalEffectNode::Surveil)
        }
        VerbLemma::Earthbend => recognize_earthbend_action(cursor).map(LocalEffectNode::Earthbend),
        VerbLemma::Search => super::recognize_library_search_action(cursor)
            .map(LocalEffectNode::ControllerLibrarySearch),
        VerbLemma::Transform => recognize_transform_source_effect(cursor, name_links, name_failure)
            .map(LocalEffectNode::TransformPermanent),
        // `double strike` is the keyword (CR 702.4), never the doubling action.
        VerbLemma::Double
            if !cursor
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("strike")) =>
        {
            super::recognize_double_counters_effect(cursor, name_links, name_failure, context)
                .map(LocalEffectNode::PutCounters)
        }
        VerbLemma::Move if super::claims_move_counters_effect(cursor) => {
            super::recognize_move_counters_effect(cursor, name_links, name_failure, context)
                .map(LocalEffectNode::MoveCounters)
        }
        VerbLemma::Distribute if super::claims_distribute_counters_effect(cursor) => {
            super::recognize_distribute_counters_effect(cursor, context)
                .map(LocalEffectNode::PutCounters)
        }
        VerbLemma::Remove if super::remove_from_combat_starts(cursor) => {
            super::recognize_remove_from_combat(cursor, name_links, name_failure, context)
                .map(LocalEffectNode::PermanentKeywordAction)
        }
        VerbLemma::Remove if super::claims_remove_counters_effect(cursor) => {
            super::recognize_remove_counters_frame(cursor, name_links, name_failure, context)
        }
        VerbLemma::Put => recognize_put_frame(cursor, inputs),
        VerbLemma::Counter => recognize_counter_effect(cursor),
        VerbLemma::Destroy => recognize_destroy_frame(cursor, inputs),
        VerbLemma::Exile => recognize_exile_frame(cursor, inputs, attempt_frame),
        VerbLemma::Return => recognize_return_frame(cursor, inputs, attempt_frame),
        VerbLemma::Create => recognize_token_creation_frame(cursor, inputs, attempt_frame),
        VerbLemma::Switch => super::permanent_action::recognize_switch_power_toughness_effect(
            cursor,
            name_links,
            name_failure,
            context,
        )
        .map(LocalEffectNode::BecomeTypes),
        // The lemma has no imperative effect frame here (`gain`, `lose` and
        // `deal` belong to the clause readers before the dispatcher; a
        // counter verb without its counter noun is no counter frame); the
        // subject frames keep their claim.
        _ => return None,
    })
}

/// `tap OBJECTS`: the shared contextual permanent operand.
fn recognize_tap_frame(
    cursor: &mut OracleTokenCursor<'_>,
    inputs: &EffectFrameInputs<'_>,
) -> MachineResult<LocalEffectNode> {
    cursor.enter("effect.tap.action");
    let mut trial = cursor.clone();
    trial.consume(
        crate::oracle::canonical::automata::verb_entry(VerbLemma::Tap).surface(VerbForm::Base),
    )?;
    cursor.enter("effect.tap.object");
    if super::resolution_selection_count_starts(&trial) {
        return super::recognize_tap_selected_permanents_effect(cursor)
            .map(|fragment| LocalEffectNode::SelectedPermanents(fragment.node));
    }
    let targets = super::recognize_contextual_permanent_operand(
        &mut trial,
        inputs.name_links,
        inputs.name_failure,
        inputs.context,
    )?;
    *cursor = trial;
    Ok(LocalEffectNode::TapPermanents(
        super::TapPermanentsEffectNode {
            targets,
            untap: false,
        },
    ))
}

/// `untap OBJECTS`: a resolution selection, the shared permanent operand of
/// the tap frame, or `untap it` over the proven permanent antecedent.
fn recognize_untap_frame(
    cursor: &mut OracleTokenCursor<'_>,
    inputs: &EffectFrameInputs<'_>,
) -> MachineResult<LocalEffectNode> {
    let context = inputs.context;
    if super::operand_determiner(cursor, 1) != super::OperandDeterminer::It {
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
                let targets = match super::recognize_contextual_permanent_operand(
                    &mut trial,
                    inputs.name_links,
                    inputs.name_failure,
                    context,
                ) {
                    Ok(targets) => targets,
                    // A reference (`untap them`, `untap those creatures`) is
                    // the shared operand's: its failure, not the count's.
                    Err(contextual) if !super::untap_selection_head(cursor) => {
                        return Err(contextual);
                    }
                    Err(_) => {
                        *cursor = historical;
                        return Err(failure);
                    }
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
    Ok(LocalEffectNode::UntapTarget(fragment.node))
}

/// `put ...`: a declared card target to a zone, counters on an object, a
/// declared permanent to its owner's library, or cards between zones.
fn recognize_put_frame(
    cursor: &mut OracleTokenCursor<'_>,
    inputs: &EffectFrameInputs<'_>,
) -> MachineResult<LocalEffectNode> {
    if targeted_card_zone_move_starts(cursor) {
        return recognize_target_card_zone_change_effect(cursor)
            .map(LocalEffectNode::TargetCardZoneChange);
    }
    // `put its|those counters on X`: the counters of a bound object.
    if super::counter_copy_starts(cursor) {
        return super::recognize_put_object_counters_effect(
            cursor,
            inputs.name_links,
            inputs.name_failure,
            inputs.context,
        );
    }
    if claims_put_counters_effect(cursor) {
        // The effect sequence lowers listed and chosen counter kinds.
        return super::recognize_put_counter_placements_effect(
            cursor,
            inputs.name_links,
            inputs.name_failure,
            inputs.context,
        )
        .map(LocalEffectNode::PutCounters);
    }
    // The object-to-zone frame of `put` with a declared target permanent:
    // the shared targeted zone change, to its owner's library.
    if super::operand_determiner(cursor, 1) == super::OperandDeterminer::Target
        && let Ok(effect) = cursor.attempt(super::recognize_zone_change_effect)
    {
        return Ok(effect);
    }
    // The object-to-zone frame over one bound object: the source card from
    // a named card zone onto the battlefield, else the source or a bound
    // permanent into its owner's library (top, bottom, beneath the top).
    if source_or_anaphor_reference_starts_at(
        cursor.position() + 1,
        cursor.peek_ahead(1),
        inputs.name_links,
        inputs.name_failure,
    ) || (super::operand_determiner(cursor, 1) == super::OperandDeterminer::That
        // `that card` names a card the program observed outside the
        // battlefield; the card-move readers own it.
        && !cursor
            .peek_ahead(2)
            .is_some_and(|noun| noun.eq_ignore_ascii_case("card")))
    {
        if super::source_card_origin_follows(cursor) {
            return recognize_source_zone_move_effect(
                cursor,
                inputs.name_links,
                inputs.name_failure,
                inputs.context,
            )
            .map(LocalEffectNode::MoveSource);
        }
        return super::referenced_permanent_action::recognize_referenced_permanent_action(
            cursor,
            super::ReferencedPermanentAction::PutOnOwnerLibrary(super::OwnerLibraryPlacement::Top),
            inputs.name_links,
            inputs.name_failure,
            inputs.context,
        )
        .map(LocalEffectNode::ReferencedPermanentAction);
    }
    // `put N cards from ZONE ...`: the counted move reads a card count first.
    if put_card_count_follows(cursor) {
        return recognize_move_cards_effect(cursor).map(LocalEffectNode::MoveCards);
    }
    // Every other head is the object-to-zone frame: the shared card operand
    // (a reference, a selection, a zone's cards, a restated antecedent),
    // then a destination whose lexical shape (`into`, `onto`, `on top of`,
    // `on the bottom of`, `ORDINAL from the top`) is read by the shared
    // destination child. The card-action child is read again here so its
    // own state names the failure, never a numeric atom it never claimed.
    cursor.enter("effect.put.object-to-zone");
    match super::recognize_card_action(
        cursor,
        false,
        super::CardActionReadContext::of(inputs.context, inputs.name_links, inputs.name_failure),
    ) {
        Ok(Some(action)) => Ok(LocalEffectNode::CardAction(action)),
        Ok(None) => Err(cursor.fail("an object operand and its destination after `put`")),
        Err(failure) => Err(failure),
    }
}

/// `put a|N|X card(s)`: the head of the counted move (`put two cards from
/// your hand on top of your library`). Pure lookahead after the verb.
fn put_card_count_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek_ahead(1).is_some_and(|raw| {
        raw.eq_ignore_ascii_case("a")
            || raw.eq_ignore_ascii_case("x")
            || raw.parse::<i64>().is_ok()
            || super::super::parse_number_word(raw).is_some()
    })
}

/// `shuffle enchanted|equipped|fortified NOUN into ...`: the permanent an
/// attachment names (CR 301.5, 303.4) is a bound permanent reference, read
/// by the referenced permanent zone action, never a card collection.
fn shuffled_attached_object_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| is_verb_form(raw, VerbLemma::Shuffle, VerbForm::Base))
        && cursor.peek_ahead(1).is_some_and(|raw| {
            ["enchanted", "equipped", "fortified"]
                .iter()
                .any(|role| raw.eq_ignore_ascii_case(role))
        })
}

/// `destroy OBJECTS`: a universal collection, a bound set, a bound
/// reference, or a declared target with its regeneration amendment.
fn recognize_destroy_frame(
    cursor: &mut OracleTokenCursor<'_>,
    inputs: &EffectFrameInputs<'_>,
) -> MachineResult<LocalEffectNode> {
    match super::operand_determiner(cursor, 1) {
        super::OperandDeterminer::Universal => {
            let mut collection = super::recognize_destroy_permanent_collection_effect_with_source(
                cursor,
                inputs.context.source_numeric_reads,
            )?;
            collection.cannot_regenerate = super::recognize_plural_regeneration_amendment(cursor)?
                || super::recognize_restated_collection_regeneration_amendment(
                    cursor,
                    &collection.objects,
                )?;
            Ok(LocalEffectNode::DestroyCollection(collection))
        }
        super::OperandDeterminer::Those => {
            recognize_destroy_referenced_set_effect(cursor, inputs.object_set_antecedent)
                .map(LocalEffectNode::DestroyReferencedSet)
        }
        // `destroy that creature`: a bound reference, never a declaration.
        super::OperandDeterminer::That => super::recognize_destroy_reference_effect(
            cursor,
            inputs.name_links,
            inputs.name_failure,
            inputs.context,
        )
        .map(LocalEffectNode::ReferencedPermanentAction),
        super::OperandDeterminer::Absent => recognize_zone_change_effect_with_policy(
            cursor,
            ZoneChangeTargetSupport::TargetCollection,
            inputs.context.zone_change_domains,
        ),
        super::OperandDeterminer::It
        | super::OperandDeterminer::Target
        | super::OperandDeterminer::Other => {
            recognize_zone_change_with_regeneration_amendment_with_support(
                cursor,
                ZoneChangeTargetSupport::TargetCollection,
            )
        }
    }
}

/// `exile ...`: a declared card target, a universal graveyard collection,
/// the source, or the shared zone change.
fn recognize_exile_frame(
    cursor: &mut OracleTokenCursor<'_>,
    inputs: &EffectFrameInputs<'_>,
    attempt_frame: &mut SequenceAttemptFrame,
) -> MachineResult<LocalEffectNode> {
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
    // `exile all graveyards` names zones; `exile all QUERY` names permanents
    // and stays with the shared object-to-zone operand below.
    if super::operand_determiner(cursor, 1) == super::OperandDeterminer::Universal {
        // A player possessive after the quantifier names the owner of a zone
        // (`each opponent's graveyard`, `all your opponents' graveyards`,
        // CR 400.3); no permanent query starts there, so the zone batch owns
        // the phrase and its failure.
        if zone_owner_possessive_after_quantifier(cursor) {
            attempt_frame.commit();
            return recognize_exile_graveyard_collection_effect(cursor)
                .map(LocalEffectNode::ZoneBatch);
        }
        if let Ok(batch) = cursor.attempt(recognize_exile_graveyard_collection_effect) {
            return Ok(LocalEffectNode::ZoneBatch(batch));
        }
    }
    if source_reference_starts_at(
        cursor.position() + 1,
        cursor.peek_ahead(1),
        inputs.name_links,
        inputs.name_failure,
    ) {
        return recognize_source_zone_move_effect(
            cursor,
            inputs.name_links,
            inputs.name_failure,
            inputs.context,
        )
        .map(LocalEffectNode::MoveSource);
    }
    recognize_zone_change_effect_with_policy(
        cursor,
        ZoneChangeTargetSupport::TargetCollection,
        inputs.context.zone_change_domains,
    )
}

fn zone_owner_possessive_after_quantifier(cursor: &OracleTokenCursor<'_>) -> bool {
    let opponent_possessive = |raw: &str| {
        [
            "opponent's",
            "opponent\u{2019}s",
            "opponents'",
            "opponents\u{2019}",
        ]
        .iter()
        .any(|form| raw.eq_ignore_ascii_case(form))
    };
    match cursor.peek_ahead(2) {
        Some(raw) if opponent_possessive(raw) => true,
        Some(raw) if raw.eq_ignore_ascii_case("your") => {
            cursor.peek_ahead(3).is_some_and(opponent_possessive)
        }
        _ => false,
    }
}

/// `return ...`: a declared card target (counted heads are claimed by the
/// card-target operand itself), the source or an anaphor, a resolution
/// selection, or the shared zone change.
fn recognize_return_frame(
    cursor: &mut OracleTokenCursor<'_>,
    inputs: &EffectFrameInputs<'_>,
    attempt_frame: &mut SequenceAttemptFrame,
) -> MachineResult<LocalEffectNode> {
    if targeted_card_zone_move_starts(cursor) {
        return recognize_target_card_zone_change_effect(cursor)
            .map(LocalEffectNode::TargetCardZoneChange);
    }
    match try_recognize_target_card_zone_change_effect_with_verb(cursor, VerbLemma::Return) {
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
    if source_or_anaphor_reference_starts_at(
        cursor.position() + 1,
        cursor.peek_ahead(1),
        inputs.name_links,
        inputs.name_failure,
    ) {
        return recognize_source_zone_move_effect(
            cursor,
            inputs.name_links,
            inputs.name_failure,
            inputs.context,
        )
        .map(LocalEffectNode::MoveSource);
    }
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
    recognize_zone_change_effect_with_policy(
        cursor,
        ZoneChangeTargetSupport::TargetCollection,
        inputs.context.zone_change_domains,
    )
}

/// The frames whose subject phrase owns the verb: a player permission, a
/// declared or quantified operand, a permanent subject, a source or
/// anaphor, and the modal `may` heads. The final failure means only that no
/// subject frame reads this head.
fn recognize_subject_frame(
    cursor: &mut OracleTokenCursor<'_>,
    head: &super::EffectHead,
    inputs: &EffectFrameInputs<'_>,
    attempt_frame: &mut SequenceAttemptFrame,
) -> MachineResult<LocalEffectNode> {
    use super::EffectSubjectKind as Subject;
    let EffectFrameInputs {
        name_links,
        name_failure,
        antecedent,
        object_set_binding,
        object_set_antecedent: _,
        context,
    } = *inputs;
    match try_recognize_temporary_player_permission(cursor, context.preposed_duration.as_ref()) {
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
    if head.subject == Subject::Declared {
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
    // `SUBJECT phases out` / `SUBJECT assigns no combat damage this turn`:
    // a rules-state predicate of any permanent subject the frames above
    // did not take (S3: object subject x lemma).
    match super::try_recognize_object_rule_clause(cursor, name_links, name_failure, context) {
        super::PermanentActionAttempt::Parsed(node) => return Ok(node),
        super::PermanentActionAttempt::FailedCommitted(failure) => {
            attempt_frame.commit();
            return Err(failure);
        }
        super::PermanentActionAttempt::NotClaimed
        | super::PermanentActionAttempt::RejectedCandidate(_) => {}
    }
    if head.subject == Subject::Declared {
        return recognize_declared_subject_clause(cursor);
    }
    if head.subject == Subject::Quantified {
        if super::quantified_player_action_starts(cursor) {
            attempt_frame.commit();
            return recognize_quantified_player_action(cursor, context)
                .map(LocalEffectNode::QuantifiedPlayerAction);
        }
        // `each player|opponent|other player ...` is a player-set subject,
        // never an object quantifier. When the player-set reader does not
        // take its verb, the failure names the verb and the subject kind.
        if let Some(verb_offset) = super::player_set_subject_length(cursor) {
            let mut verb = cursor.clone();
            verb.advance_to(verb.position() + verb_offset)?;
            if verb
                .peek()
                .and_then(super::lookup_auxiliary)
                .is_some_and(|(auxiliary, _)| auxiliary == super::Auxiliary::May)
            {
                verb.advance_to(verb.position() + 1)?;
            }
            return Err(super::clause_verb_unsupported(
                &verb,
                "a player set (`each player`)",
            ));
        }
        return recognize_set_ability_loss_effect(cursor, object_set_binding)
            .map(LocalEffectNode::LoseAbilities);
    }
    if source_or_anaphor_reference_starts_at(
        cursor.position(),
        cursor.peek(),
        name_links,
        name_failure,
    ) {
        // A source or anaphor subject the permanent-action frames did not
        // take: the damage frame reads only `deal`; another lexicon verb or
        // a modal (`becomes`, `can block`) is named as the clause's own
        // unsupported lemma for this subject kind.
        let mut verb = cursor.clone();
        let other_verb = |raw: &str| {
            super::lookup_effect_verb(raw).is_some_and(|(entry, _)| entry.lemma != VerbLemma::Deal)
                || super::lookup_auxiliary(raw)
                    .is_some_and(|(auxiliary, _)| auxiliary != super::Auxiliary::Would)
        };
        if super::recognize_source_reference_with_typed_antecedent(
            &mut verb,
            name_links,
            name_failure,
            super::permanent_subject::proven_permanent_antecedent(context),
        )
        .is_ok()
            && verb.peek().is_some_and(other_verb)
        {
            return Err(super::clause_verb_unsupported(
                &verb,
                "a source or anaphor object subject",
            ));
        }
        return super::effects::recognize_damage_effect_with_agreement(
            cursor,
            name_links,
            name_failure,
            super::permanent_subject::proven_permanent_antecedent(context),
            context.subject_verb_agreement,
        );
    }
    match (head.subject, head.modality, head.lemma) {
        // `you may draw`: the controller's referenced draw. A possessed
        // player's draw (`its controller may draw`) is that player's own
        // permission, read by the clause layer's performer slot.
        (Subject::Controller, super::ClauseModality::May, Some(lemma))
            if lemma == VerbLemma::Draw =>
        {
            attempt_frame.commit();
            recognize_referenced_draw_effect(cursor, antecedent)
        }
        (Subject::Controller | Subject::Possessed, super::ClauseModality::May, Some(lemma))
            if lemma == VerbLemma::Search =>
        {
            attempt_frame.commit();
            super::recognize_optional_library_search_program(
                cursor,
                antecedent,
                name_links,
                name_failure,
                context,
            )
        }
        // A possessed player's permission is read by the optional search
        // reader, which owns its explicit failure for other verbs.
        (Subject::Possessed, super::ClauseModality::May, _) => {
            recognize_optional_library_search(cursor, antecedent)
                .map(LocalEffectNode::OptionalLibrarySearch)
        }
        (Subject::Controller, super::ClauseModality::May, Some(VerbLemma::Put))
            if head.base(VerbLemma::Put) =>
        {
            recognize_optional_card_deployment_effect_with_context(cursor, context)
                .map(LocalEffectNode::OptionalCardDeployment)
        }
        _ => Err(cursor.fail("a supported effect subject")),
    }
}

/// `target PLAYER VERBs ...` / `target OBJECT VERBs ...` not claimed by the
/// permanent-subject or combat frames (S3, PlayerRef): the declared subject
/// is read with the shared target list, then its verb is looked up in the
/// lexicon. The failure names the lemma and the subject kind, never a
/// missing player; for a declared player it hands the clause to the
/// performer reader, which owns every verb of a declared player.
fn recognize_declared_subject_clause(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LocalEffectNode> {
    let written_target = super::operand_determiner(cursor, 0) == super::OperandDeterminer::Target;
    let mut subject = cursor.clone();
    subject.enter("effect.subject.declared");
    let target = match super::recognize_target_list(&mut subject) {
        Ok(target) => target,
        // The subject phrase's own failure is the deepest component.
        Err(failure) if written_target => return Err(failure),
        Err(_) => return Err(cursor.fail("a supported effect subject")),
    };
    let players = !target.slots.is_empty()
        && target
            .slots
            .iter()
            .all(|slot| matches!(slot.selector, super::TargetSelector::Player(_)));
    // A coordinator the target list stopped at (`target creature or player`)
    // continues the subject phrase, not a verb: the list owns that refusal.
    if subject
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("or") || raw.eq_ignore_ascii_case("and"))
    {
        subject.enter("target-list.complete");
        return Err(subject.fail("a complete target list before the subject's verb"));
    }
    // A declared target player is a performer slot: the sequence's
    // performer reader declares it and reads every verb the clause readers
    // know as that player's instruction. This frame only names the verb.
    subject.enter("effect.subject.declared.verb");
    Err(super::clause_verb_unsupported(
        &subject,
        if players {
            "a declared target player"
        } else {
            "a declared target object"
        },
    ))
}

/// The player an earlier action named only through its singular object
/// antecedent: the owner after a move to `its owner's` hand or library (CR
/// 108.3), the controller of a target chosen among the permanents `an
/// opponent controls` / `you don't control` (CR 109.4). `None` when the
/// object names no other player; the caller then keeps `that player`'s
/// ordinary binding failure.
fn object_player_mention(
    effects: &[LocalEffectNode],
    antecedent: Option<&super::ObjectSubjectAntecedent>,
) -> Option<(super::ObjectPlayerRelation, super::PermanentEffectReference)> {
    // `Search its controller's library ... . Then that player shuffles.`:
    // the searched zones' holder named through its object.
    if let Some(LocalEffectNode::ControllerLibrarySearch(search)) = effects.last()
        && let super::SearchedPlayer::Bound(super::PlayerEffectSubject::ObjectRelation {
            relation,
            object,
        }) = &search.origin.holder
    {
        return Some((*relation, *object));
    }
    let antecedent = antecedent?;
    let super::PermanentEffectReference::Target(binding) = antecedent.reference else {
        return None;
    };
    if let Some(LocalEffectNode::ZoneChange(zone_change)) = effects.get(binding.effect_index)
        && matches!(
            zone_change.action,
            super::ZoneChangeAction::Move(
                super::ZoneChangeDestination::OwnerHand
                    | super::ZoneChangeDestination::OwnerLibrary(_)
            )
        )
    {
        return Some((super::ObjectPlayerRelation::Owner, antecedent.reference));
    }
    super::other_player_controlled_antecedent(antecedent)
        .map(|object| (super::ObjectPlayerRelation::Controller, object))
}

/// True when a card action declares the one player whose hand or library it
/// reads (`target player's hand`, `target opponent's library`).
fn card_action_declares_zone_owner(action: &super::CardActionNode) -> bool {
    let operand = match action {
        super::CardActionNode::LookAt(operand)
        | super::CardActionNode::Reveal(operand)
        | super::CardActionNode::Move { cards: operand, .. } => operand,
        _ => return false,
    };
    match operand {
        super::CardOperandNode::ZoneCollection(zone) => {
            matches!(zone.owner, super::CardZoneOwnerNode::TargetPlayer(_))
        }
        super::CardOperandNode::LibraryTop(top) => {
            matches!(top.owner, super::CardLibraryOwnerNode::TargetPlayer(_))
        }
        _ => false,
    }
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
        // The verbs after `each` take the base form (`two target players
        // each draw a card and lose 1 life`).
        super::PerformerNode::DeclaredPlayers { .. } | super::PerformerNode::BoundPlayers(_) => {
            super::VerbAgreement::Base
        }
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

/// True when a declared target selector names battlefield objects (an
/// object or controlled-object predicate, or a union of them), never a
/// stack item, a player or `any target` (`Counter target spell. Exile it.`).
fn selector_names_permanents(selector: &super::TargetSelector) -> bool {
    match selector {
        super::TargetSelector::Object(_) | super::TargetSelector::ControlledObject { .. } => true,
        super::TargetSelector::Union(branches) => {
            !branches.is_empty() && branches.iter().all(selector_names_permanents)
        }
        super::TargetSelector::Any
        | super::TargetSelector::Player(_)
        | super::TargetSelector::StackItem(_) => false,
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

#[cfg(test)]
#[path = "effect_dispatch_tests.rs"]
mod effect_dispatch_tests;
