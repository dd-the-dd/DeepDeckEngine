//! Typed composition over one immutable token stream. Each child returns its
//! own semantic node; the parent owns separators and complete consumption.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) struct SyntaxNode<T> {
    pub tokens: std::ops::Range<usize>,
    pub value: T,
}

/// A triggered ability body: one effect program, or printed modes chosen as
/// the trigger is put on the stack (CR 700.2b).
#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) enum TriggerBodyNode {
    Program(EffectProgramNode),
    Modal(EffectSequenceModalNode),
}

impl TriggerBodyNode {
    pub(in crate::oracle::canonical) fn action_count(&self) -> usize {
        match self {
            Self::Program(program) => program.action_count(),
            Self::Modal(modal) => modal.modes.iter().map(|mode| mode.effects.len()).sum(),
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) struct TriggeredProgramNode {
    pub event: SyntaxNode<GenericTriggerEventNode>,
    pub guard: Option<SyntaxNode<SentenceConditionNode>>,
    pub program: SyntaxNode<TriggerBodyNode>,
    /// `This ability triggers only once each turn.`
    pub once_each_turn: bool,
}

pub(in crate::oracle::canonical) fn recognize_trigger_program_tail(
    cursor: &mut OracleTokenCursor<'_>,
    event: SyntaxNode<GenericTriggerEventNode>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<TriggeredProgramNode> {
    let start = cursor.position();
    // The guard is read before the effect program, so only the ability's own
    // source can be a referent: no declared target or observed object is
    // bound yet, and the shared condition atoms fail explicitly otherwise.
    let guard_context = trigger_guard_context(&event.value);
    let guard = recognize_trigger_guard(
        cursor,
        &SentenceConditionContext {
            name_links,
            name_failure,
            effects: &guard_context,
            card_target: None,
            card_focus: false,
            departed_targets: &[],
        },
    )?
    .map(|value| SyntaxNode {
        tokens: start..cursor.position(),
        value,
    });
    if guard.is_some() {
        cursor.enter("trigger.program.guard-separator");
        cursor.consume(",")?;
    }
    cursor.enter("trigger.program.body");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("choose"))
        && cursor
            .peek_ahead(1)
            .and_then(super::super::parse_number_word)
            .is_some()
    {
        // Modes are declared with the trigger's other choices (CR 700.2b);
        // every bullet body is the shared effect sequence.
        let start = cursor.position();
        let modal = recognize_effect_sequence_modal(cursor, name_links, name_failure)?;
        return Ok(TriggeredProgramNode {
            event,
            guard,
            program: SyntaxNode {
                tokens: start..cursor.position(),
                value: TriggerBodyNode::Modal(modal),
            },
            once_each_turn: false,
        });
    }
    // Ability source and observed object have separate roles. OR branches
    // select an event; they do not multiply objects within its occurrence.
    let mut context = trigger_guard_context(&event.value);
    if let Some(object) = event.value.observed_object() {
        context = context.with_observed_event_object(object);
    }
    let mut context = context
        .with_zone_change_domains(ZoneChangeDomains::SpellOrBattlefield)
        .with_sentence_conditionals()
        .with_unless_frames();
    // This parent reads `This ability triggers only ...` after the body.
    context.ability_restriction_owner = AbilityRestrictionOwner::Triggered;
    context.event_player = event_names_player(&event.value);
    let start = cursor.position();
    // The event owns the `that many`/`that much` quantity; the body only reads it.
    cursor.set_event_numeric_binding(event.value.numeric_binding());
    let value = recognize_effect_program_with_context(cursor, name_links, name_failure, &context)?;
    let program_end = cursor.position();
    let once_each_turn = recognize_optional_trigger_limit(cursor)?;
    // A token reminder may follow the limit sentence; it still belongs to the
    // body's last named-token creation and is validated against its catalog.
    if once_each_turn
        && cursor.peek() == Some("(")
        && let Some(LocalEffectNode::CreateNamedTokens(creation)) =
            value.inner_sequence().effects.last()
    {
        recognize_named_token_creation_reminder(cursor, &creation.creation)?;
    }
    cursor.enter("trigger.program.complete");
    cursor.finish()?;
    Ok(TriggeredProgramNode {
        event,
        guard,
        program: SyntaxNode {
            tokens: start..program_end,
            value: TriggerBodyNode::Program(value),
        },
        once_each_turn,
    })
}

/// Events whose occurrence names exactly one player that the runtime records
/// as the triggering player: the player dealt combat damage, or the player
/// whose turn step began when the step owner is not fixed to the controller.
fn event_names_player(event: &GenericTriggerEventNode) -> bool {
    matches!(
        event,
        GenericTriggerEventNode::Local(
            LocalTriggerEventNode::CombatDamageToPlayer { .. }
                | LocalTriggerEventNode::AttachedObjectEvent {
                    action: AttachedObjectEventAction::DealCombatDamageToPlayer,
                    ..
                }
        )
    ) || matches!(
        event,
        GenericTriggerEventNode::Step(StepEventNode { player, .. })
            if *player != StepPlayerScope::AbilityController
    )
}

/// The referents available to a trigger's intervening `if` (CR 603.4): the
/// ability source and, for a source event, the source as the event subject.
fn trigger_guard_context(event: &GenericTriggerEventNode) -> EffectRecognitionContext {
    let mut context = match event {
        GenericTriggerEventNode::SourceTransition(_)
        | GenericTriggerEventNode::Local(LocalTriggerEventNode::SourceAttacked { .. })
        | GenericTriggerEventNode::Local(LocalTriggerEventNode::CombatDamageToPlayer { .. })
        | GenericTriggerEventNode::Local(LocalTriggerEventNode::SourceDealtDamage { .. })
        | GenericTriggerEventNode::SourceEvent(_) => {
            EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf)
                .with_source_event_subject(LocalObjectReference::SourceSelf)
        }
        // Any other battlefield trigger is observed by its source permanent:
        // `this creature` names that owner (CR 109.2, 113.6). The event
        // subject is someone else, so `it` is not bound to the source.
        event if event.requires_permanent_face() => {
            EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf)
        }
        _ => EffectRecognitionContext::default(),
    };
    if event.observed_object_is_source() {
        context.source = Some(LocalObjectReference::SourceSelf);
        context = context.with_source_event_subject(LocalObjectReference::SourceSelf);
        context.observed_source_relationship = ObservedSourceRelationship::SameEnteringObject;
    }
    context
}
