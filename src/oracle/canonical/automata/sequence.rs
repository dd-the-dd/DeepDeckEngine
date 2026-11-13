use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    BoundPermanentSetNode, CollectionBinding, ConditionalEffectNode, CounterDestinationReplacement,
    CounterReplacementSequenceNode, DrawCardsEffectParseAttempt, EffectConnector,
    EffectRecognitionContext, EffectSequenceNode, LocalEffectNode, ObjectSetBinding,
    PlayerTargetBinding, TargetBinding, TemporaryCombatRestrictionParseAttempt,
    claims_put_counters_effect, recognize_add_mana_effect, recognize_amass_effect,
    recognize_conditional_damage_replacement, recognize_controller_library_search,
    recognize_counter_effect, recognize_create_creature_tokens_effect,
    recognize_create_named_tokens_effect, recognize_damage_effect,
    recognize_destroy_permanent_collection_effect, recognize_destroy_referenced_set_effect,
    recognize_earthbend_action, recognize_effect_connector,
    recognize_exile_graveyard_collection_effect, recognize_investigate_effect,
    recognize_life_effect_with_player_target, recognize_mill_cards_effect,
    recognize_move_cards_effect, recognize_named_token_reminder,
    recognize_optional_card_deployment_effect, recognize_optional_library_search,
    recognize_put_counters_effect, recognize_sacrifice_source_effect,
    recognize_set_ability_loss_effect, recognize_source_zone_move_effect, recognize_surveil_effect,
    recognize_target_card_zone_change_effect, recognize_temporary_animation_effect,
    recognize_temporary_power_toughness, recognize_transform_source_effect,
    recognize_untap_target_effect, recognize_zone_change_effect,
    recognize_zone_change_with_regeneration_amendment, signed_power_toughness_from_raw,
    source_or_anaphor_reference_starts_at, source_reference_starts_at,
    try_recognize_draw_cards_effect_with_context, try_recognize_temporary_combat_restriction,
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
    last_player_target: Option<PlayerTargetBinding>,
    last_object_set: Option<BoundPermanentSetNode>,
    next_search_binding: usize,
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
    recognize_effect_sequence_internal(cursor, name_links, name_failure, context, false)
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

fn recognize_effect_sequence_internal(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
    stop_before_activation_reduction: bool,
) -> MachineResult<EffectSequenceNode> {
    let mut attempt_frame = SequenceAttemptFrame::default();
    recognize_effect_sequence_attempt_internal(
        cursor,
        name_links,
        name_failure,
        context,
        stop_before_activation_reduction,
        &mut attempt_frame,
    )
}

fn recognize_effect_sequence_attempt_internal(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
    stop_before_activation_reduction: bool,
    attempt_frame: &mut SequenceAttemptFrame,
) -> MachineResult<EffectSequenceNode> {
    let mut state = SequenceState::Effect;
    let mut memory = SequenceMemory::default();
    loop {
        state = match state {
            SequenceState::Effect => {
                cursor.enter("sequence.effect");
                let effect_index = memory.effects.len();
                let object_set_binding = ObjectSetBinding(effect_index);
                attempt_frame.clear();
                let mut effect = cursor.attempt(|attempt| {
                    recognize_effect(
                        attempt,
                        name_links,
                        name_failure,
                        memory.last_target,
                        Some(PlayerTargetBinding { effect_index }),
                        memory.last_player_target,
                        object_set_binding,
                        memory.last_object_set.as_ref(),
                        context,
                        attempt_frame,
                    )
                })?;
                attempt_frame.clear();
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
                if matches!(effect, LocalEffectNode::ZoneChange(_)) {
                    memory.last_target = Some(TargetBinding {
                        effect_index,
                        slot_index: 0,
                    });
                }
                if matches!(effect, LocalEffectNode::TargetCardZoneChange(_)) {
                    memory.last_target = Some(TargetBinding {
                        effect_index,
                        slot_index: 0,
                    });
                }
                if let LocalEffectNode::ModifyStats(modifier) = &effect {
                    memory.last_target = (modifier.targets.slots.len() == 1
                        && modifier.targets.slots[0].cardinality.maximum == 1)
                        .then_some(TargetBinding {
                            effect_index,
                            slot_index: 0,
                        });
                }
                if let LocalEffectNode::TargetPlayerLife(life) = &effect
                    && life.declares_target
                {
                    memory.last_player_target = Some(life.player);
                }
                if let LocalEffectNode::LoseAbilities(loss) = &effect {
                    memory.last_object_set = Some(loss.objects.clone());
                }
                memory.effects.push(effect);
                SequenceState::ConnectorOrEnd
            }
            SequenceState::ConnectorOrEnd => {
                cursor.enter("sequence.connector-or-end");
                match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                    Some("and" | "then" | ",") => {
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
                    Some(".") => {
                        cursor.consume(".")?;
                        if cursor.peek().is_some_and(|raw| raw == "(")
                            && let Some(LocalEffectNode::CreateNamedTokens(creation)) =
                                memory.effects.last()
                        {
                            recognize_named_token_reminder(
                                cursor,
                                &creation.quantity,
                                &creation.token_name,
                            )?;
                            cursor.enter("sequence.named-token.complete");
                            cursor.finish()?;
                            return Ok(EffectSequenceNode {
                                effects: memory.effects,
                                connectors: memory.connectors,
                            });
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
                        if at_activation_reduction {
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
                            SequenceState::Effect
                        } else {
                            SequenceState::Complete
                        }
                    }
                    _ => return Err(cursor.fail("`and`, `then`, or sentence terminator")),
                }
            }
            SequenceState::Complete => {
                cursor.enter("sequence.complete");
                if !stop_before_activation_reduction {
                    cursor.finish()?;
                }
                cursor.require(
                    memory.connectors.len() + 1 == memory.effects.len(),
                    "one connector between each pair of effects",
                )?;
                return Ok(EffectSequenceNode {
                    effects: memory.effects,
                    connectors: memory.connectors,
                });
            }
        };
    }
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
                return Ok(LocalEffectNode::TemporaryKeywordGrant(restriction));
            }
            TemporaryCombatRestrictionParseAttempt::RejectedCandidate(_) => {}
            TemporaryCombatRestrictionParseAttempt::FailedCommitted(failure) => {
                attempt_frame.commit();
                return Err(failure);
            }
        }
    }
    let mut modifier_offset = 0_usize;
    let targeted_modifier_shape = targeted_head
        && loop {
            let Some(token) = cursor.peek_ahead(modifier_offset) else {
                break false;
            };
            if token == "." {
                break false;
            }
            if (token.eq_ignore_ascii_case("get") || token.eq_ignore_ascii_case("gets"))
                && cursor
                    .peek_ahead(modifier_offset + 1)
                    .and_then(signed_power_toughness_from_raw)
                    .is_some()
            {
                break true;
            }
            modifier_offset += 1;
        };
    if targeted_modifier_shape {
        let fragment = recognize_temporary_power_toughness(cursor)?;
        cursor.require(
            fragment.end_position == cursor.position(),
            "the modifier child cursor boundary",
        )?;
        return Ok(LocalEffectNode::ModifyStats(fragment.node));
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("untap"))
    {
        let fragment = recognize_untap_target_effect(cursor, antecedent)?;
        cursor.require(
            fragment.end_position == cursor.position(),
            "the untap child cursor boundary",
        )?;
        return Ok(LocalEffectNode::UntapTarget(fragment.node));
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("add"))
    {
        return recognize_add_mana_effect(cursor).map(LocalEffectNode::AddMana);
    }
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("sacrifice"))
    {
        return recognize_sacrifice_source_effect(
            cursor,
            name_links,
            name_failure,
            context.source.as_ref(),
        )
        .map(LocalEffectNode::SacrificeSource);
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
        .is_some_and(|token| token.eq_ignore_ascii_case("investigate"))
    {
        return recognize_investigate_effect(cursor).map(LocalEffectNode::Investigate);
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
        return recognize_destroy_permanent_collection_effect(cursor)
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
        return recognize_zone_change_with_regeneration_amendment(cursor);
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
    if cursor.peek().is_some_and(|token| {
        token.eq_ignore_ascii_case("exile")
            || token.eq_ignore_ascii_case("destroy")
            || token.eq_ignore_ascii_case("return")
    }) {
        return recognize_zone_change_effect(cursor);
    }
    if (cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("its"))
        && cursor
            .peek_ahead(2)
            .is_some_and(|token| token.eq_ignore_ascii_case("creates")))
        || (cursor
            .peek()
            .is_some_and(|token| token.eq_ignore_ascii_case("you"))
            && cursor
                .peek_ahead(1)
                .is_some_and(|token| token.eq_ignore_ascii_case("create")))
        || cursor
            .peek()
            .is_some_and(|token| token.eq_ignore_ascii_case("create"))
    {
        let named_failure = match cursor
            .attempt(|attempt| recognize_create_named_tokens_effect(attempt, antecedent))
        {
            Ok(effect) => return Ok(LocalEffectNode::CreateNamedTokens(effect)),
            Err(failure) => failure,
        };
        let probable_named_token = (cursor.position()..cursor.position().saturating_add(8))
            .take_while(|index| {
                cursor
                    .peek_ahead(index.saturating_sub(cursor.position()))
                    .is_some()
            })
            .map(|index| {
                cursor
                    .peek_ahead(index - cursor.position())
                    .unwrap_or_default()
            })
            .take_while(|raw| *raw != ".")
            .any(|raw| raw.eq_ignore_ascii_case("token") || raw.eq_ignore_ascii_case("tokens"))
            && !(0..8).any(|offset| cursor.peek_ahead(offset) == Some("/"));
        if probable_named_token {
            return Err(named_failure);
        }
        return recognize_create_creature_tokens_effect(cursor, antecedent)
            .map(LocalEffectNode::CreateTokens);
    }
    if source_reference_starts_at(cursor.position(), cursor.peek(), name_links, name_failure) {
        return recognize_damage_effect(cursor, name_links, name_failure);
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
        return recognize_optional_card_deployment_effect(cursor)
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
#[path = "mana_effect_sequence_tests.rs"]
mod mana_effect_tests;

#[cfg(test)]
#[path = "zone_change_life_sequence_tests.rs"]
mod zone_change_life_tests;

#[cfg(test)]
#[path = "surveil_sequence_tests.rs"]
mod surveil_sequence_tests;

#[cfg(test)]
#[path = "controller_subject_sequence_tests.rs"]
mod controller_subject_sequence_tests;

#[cfg(test)]
#[path = "sequence_attempt_tests.rs"]
mod sequence_attempt_tests;
