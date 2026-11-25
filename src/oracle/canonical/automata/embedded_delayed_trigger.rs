//! Rules a resolving effect creates over a range of its own actions
//! (CR 603.7, 614.1a).
//!
//! ```text
//! CreatedRule := DelayedStepTiming                       -- `delayed_step_timing.rs`
//!              | [PreposedLifetime ","] ("when" | "whenever") DelayedEvent [Lifetime] ","
//!              | "if" BoundObject "would die" Lifetime "," -- replacement (614.1a)
//!              | "if" BoundObject "would leave the battlefield" ","
//!                                         -- replacement over the object (614.1a)
//!              | "the next time" ChosenSourceDamage Lifetime "," -- shield (615.7)
//! Reflexive   := "when" ("you" ActiveVerb | Quantity Noun "is"|"are" Verbed)
//!                 "this" "way" ","  -- the program's outcome (CR 603.12),
//!                                    -- never a created rule (`action_result.rs`)
//! DelayedEvent := BoundObject ("dies" | "leaves the battlefield")
//!               | "you" "next" "cast" SpellQuery
//!               | "you" "cast" "your" "next" SpellQuery
//!               | Occurrence                             -- `event_clause.rs`
//! Lifetime     := "this" "turn"
//! PreposedLifetime := "until" "end" "of" "turn" | "until" "your" "next" "turn"
//! ```
//!
//! The enclosing effect sequence owns the action range the rule governs
//! (the actions of the same sentence); this module owns only the rule's
//! introducer, event and lifetime. A bound object is read by the shared
//! binding environment (`that creature`, `it`); a spell is read by the shared
//! object query; any other event is the one event clause reader of the
//! triggered abilities (actor, verb, object predicate, origin zone, event
//! unions), read over the tokens without the rule's lifetime. The actions of
//! a delayed trigger read the delayed trigger's own event, not the event of
//! the ability that created it (CR 603.7d-e).
//!
//! `when` creates a trigger that triggers once (CR 603.7c); `whenever`
//! creates one that triggers each time its event occurs until its lifetime
//! ends (CR 603.7b, 611.2a).
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    DamageEventNode, DamageReplacementContext, DelayedStepTimingNode, EventClauseAttempt,
    EventTimeConditionNode, FrameBindings, GenericTriggerEventNode, ObjectKind, ObjectPredicate,
    PermanentEffectReference,
};

/// How a created rule governs its action range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum EffectCreatedRuleNode {
    /// `at the beginning of the next STEP` / `at end of combat`: a delayed
    /// triggered ability that performs the actions at that step (CR 603.7a).
    Step(DelayedStepTimingNode),
    /// `When|Whenever EVENT this turn,`: a delayed triggered ability that
    /// performs the actions when the event occurs (CR 603.7a, 603.7c).
    Trigger(EmbeddedDelayedTriggerNode),
    /// `If OBJECT would die this turn, ACTION instead.`: a replacement effect
    /// the resolving effect creates (CR 614.1a, 614.6).
    Replacement(CreatedReplacementNode),
    /// `The next time SOURCE of your choice would deal damage to RECIPIENT
    /// this turn, prevent that damage.`: a prevention shield that applies
    /// once, to a source chosen on resolution (CR 615.7, 609.7a).
    NextDamage(NextDamageRuleNode),
}

impl From<DelayedStepTimingNode> for EffectCreatedRuleNode {
    fn from(timing: DelayedStepTimingNode) -> Self {
        Self::Step(timing)
    }
}

impl EffectCreatedRuleNode {
    /// The event frame the governed actions read. A delayed trigger's
    /// actions read its own event (CR 603.7d-e); a step trigger and a
    /// replacement read the frame of the ability that created them.
    pub(in crate::oracle::canonical) fn body_frame(&self) -> Option<FrameBindings> {
        match self {
            Self::Step(_) | Self::Replacement(_) | Self::NextDamage(_) => None,
            Self::Trigger(trigger) => Some(match &trigger.event {
                // The departed object is the lexical antecedent the program
                // already names; no other event reference is in scope.
                DelayedTriggerEventNode::BoundObjectDeparture { .. } => FrameBindings::default(),
                // `that spell` / `it`: the spell whose cast triggered it.
                DelayedTriggerEventNode::NextSpellCast { .. } => FrameBindings {
                    event_spell: true,
                    ..FrameBindings::default()
                },
                // The references the created trigger's own event binds: the
                // frame a printed trigger of the same event has.
                DelayedTriggerEventNode::Event { event, .. } => super::event_frame(event),
            }),
        }
    }

    /// The occurrence of a created trigger read by the shared event clause.
    pub(in crate::oracle::canonical) fn created_event(&self) -> Option<&GenericTriggerEventNode> {
        match self {
            Self::Trigger(EmbeddedDelayedTriggerNode {
                event: DelayedTriggerEventNode::Event { event, .. },
                ..
            }) => Some(event),
            _ => None,
        }
    }

    /// Reads the `instead` that closes a replacement's action
    /// (`exile it instead`, CR 614.1a). Any other created rule rejects it.
    /// A leave-the-battlefield replacement may restate that it replaces
    /// every destination (`instead of putting it anywhere else`).
    pub(in crate::oracle::canonical) fn read_instead(
        &mut self,
        cursor: &mut OracleTokenCursor<'_>,
    ) -> MachineResult<()> {
        cursor.enter("created-rule.replacement.instead");
        match self {
            Self::Replacement(replacement) if !replacement.instead => {
                cursor.consume("instead")?;
                if replacement.event == ObjectDeparture::LeavesBattlefield
                    && word_at(cursor, 0, "of")
                {
                    cursor.enter("created-rule.replacement.instead.any-destination");
                    cursor.consume_phrase(&["of", "putting", "it", "anywhere", "else"])?;
                }
                replacement.instead = true;
                Ok(())
            }
            _ => Err(cursor.fail("one `instead` closing a created replacement's action")),
        }
    }

    /// True for a replacement whose action is closed by `instead`.
    pub(in crate::oracle::canonical) fn awaits_instead(&self) -> bool {
        matches!(self, Self::Replacement(replacement) if !replacement.instead)
    }
}

/// The lifetime of a created rule (CR 611.2a). Every supported form is
/// bounded; an unbounded delayed trigger is refused explicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CreatedRuleLifetime {
    /// `this turn` closing the event, or a preposed `Until end of turn,`:
    /// through the cleanup step of this turn (CR 514.2).
    ThisTurn,
    /// A preposed `Until your next turn,`: until the controller's next turn
    /// begins (CR 611.2b).
    UntilYourNextTurn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObjectDeparture {
    /// `dies` (CR 700.4): put into a graveyard from the battlefield.
    Dies,
    /// `leaves the battlefield` (CR 603.6c): to any zone.
    LeavesBattlefield,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DelayedTriggerEventNode {
    /// `OBJECT dies` / `OBJECT leaves the battlefield` over an object the
    /// program already bound (CR 603.7c: it follows that object only).
    BoundObjectDeparture {
        object: PermanentEffectReference,
        departure: ObjectDeparture,
    },
    /// `you next cast SPELL` / `you cast your next SPELL`: the controller's
    /// next matching cast; the trigger is consumed by it (CR 603.7c).
    NextSpellCast { predicate: ObjectPredicate },
    /// Any occurrence of the shared event clause (`a creature dies`, `a
    /// player taps an Island for mana`, `you cast a creature spell`), with
    /// the event-time condition the clause hoists (CR 603.2).
    Event {
        event: GenericTriggerEventNode,
        condition: Option<EventTimeConditionNode>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EmbeddedDelayedTriggerNode {
    pub event: DelayedTriggerEventNode,
    pub lifetime: CreatedRuleLifetime,
    /// `whenever`: the trigger triggers each time its event occurs while it
    /// lasts; `when`: once (CR 603.7c).
    pub repeats: bool,
}

/// The next damage event a created shield modifies, with its lifetime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct NextDamageRuleNode {
    pub event: DamageEventNode,
    pub lifetime: CreatedRuleLifetime,
}

/// `If OBJECT would die this turn, ACTION instead.` / `If OBJECT would leave
/// the battlefield, ACTION instead.` The action is the governed range;
/// lowering proves it is the replacing action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CreatedReplacementNode {
    pub object: PermanentEffectReference,
    /// The replaced event: the object dying, or leaving the battlefield for
    /// any zone.
    pub event: ObjectDeparture,
    /// `this turn`; a leave-the-battlefield replacement written without one
    /// applies to the object as long as it is that object (CR 400.7).
    pub lifetime: Option<CreatedRuleLifetime>,
    /// Set once the sequence read the closing `instead`.
    pub instead: bool,
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

/// The token width of a preposed created-rule lifetime with its comma
/// (`Until end of turn,` / `Until your next turn,`), without consuming. The
/// duration is read by the shared effect-duration reader.
pub(in crate::oracle::canonical) fn preposed_lifetime_width(
    cursor: &OracleTokenCursor<'_>,
) -> Option<usize> {
    if !word_at(cursor, 0, "until") {
        return None;
    }
    let mut probe = cursor.clone();
    recognize_created_rule_lifetime(&mut probe).ok()?;
    word_at(&probe, 0, ",").then(|| probe.position() + 1 - cursor.position())
}

/// A created rule's lifetime (CR 611.2a), read by the shared effect-duration
/// reader (`this turn`, `until end of turn`, `until your next turn`) and
/// mapped to one lifetime. Any other duration is refused explicitly.
pub(in crate::oracle::canonical) fn recognize_created_rule_lifetime(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CreatedRuleLifetime> {
    let duration = super::duration::recognize_effect_duration(cursor)?;
    cursor.enter("created-rule.lifetime");
    match duration {
        super::EffectDurationNode::UntilEndOfCurrentTurn => Ok(CreatedRuleLifetime::ThisTurn),
        super::EffectDurationNode::UntilNextTurn {
            player: super::LocalPlayerReference::Controller,
        } => Ok(CreatedRuleLifetime::UntilYourNextTurn),
        _ => Err(cursor.fail(
            "a created-rule lifetime: `this turn`, `until end of turn` or `until your next turn`",
        )),
    }
}

/// The offset of an inline `this turn` that closes a created trigger's
/// event right before its comma (`Whenever a creature dies this turn,`),
/// within the current sentence.
fn inline_lifetime_offset(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    (1..)
        .take_while(|offset| {
            cursor
                .peek_ahead(*offset)
                .is_some_and(|raw| raw != "." && raw != "(")
        })
        .find(|offset| {
            word_at(cursor, *offset, "this")
                && word_at(cursor, offset + 1, "turn")
                && word_at(cursor, offset + 2, ",")
        })
}

/// Whether the trigger word is followed by a specialized delayed event: the
/// controller's next spell or a followed object's departure.
fn specialized_event_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    // `you next cast` / `you cast your next`: the controller's next spell. A
    // plain `you cast` is a printed cast trigger's event.
    if word_at(cursor, 1, "you")
        && (word_at(cursor, 2, "next")
            || (word_at(cursor, 2, "cast")
                && word_at(cursor, 3, "your")
                && word_at(cursor, 4, "next")))
    {
        return true;
    }
    let mut event = cursor.clone();
    event.advance_to(cursor.position() + 1).is_ok() && super::bound_object_reference_starts(&event)
}

/// Lexical lookahead for a created rule introduced by `when` / `whenever`
/// (a delayed trigger), a preposed lifetime before them, or `if ... would
/// die` (a replacement). An outcome connector (`When you do,`, CR 603.12)
/// belongs to the program parent; a trigger word without a followed object,
/// next spell or lifetime is a printed trigger, never claimed here.
pub(in crate::oracle::canonical) fn effect_created_rule_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    // `When you discard a nonland card this way,` is the reflexive ability
    // of the instruction before it (CR 603.12), also owned by the program.
    if super::outcome_connector_at(cursor, 0).is_some()
        || super::reflexive_result_connector_at(cursor, 0).is_some()
    {
        return false;
    }
    // `Until end of turn, whenever EVENT,`: the preposed lifetime of a
    // created trigger. Any other preposed duration belongs to the sequence's
    // continuous effects.
    if let Some(width) = preposed_lifetime_width(cursor) {
        return word_at(cursor, width, "when") || word_at(cursor, width, "whenever");
    }
    if word_at(cursor, 0, "when") || word_at(cursor, 0, "whenever") {
        return specialized_event_follows(cursor) || inline_lifetime_offset(cursor).is_some();
    }
    if word_at(cursor, 0, "the") && word_at(cursor, 1, "next") && word_at(cursor, 2, "time") {
        return true;
    }
    // `If OBJECT would die`: a bound object (a pronoun, `that NOUN`, `that
    // creature or planeswalker`) whose clause reaches `would die` before the
    // comma that opens the replacing action.
    if !word_at(cursor, 0, "if") {
        return false;
    }
    let mut object = cursor.clone();
    if object.advance_to(cursor.position() + 1).is_err()
        || !super::bound_object_reference_starts(&object)
    {
        return false;
    }
    (1..)
        .take_while(|offset| {
            cursor
                .peek_ahead(*offset)
                .is_some_and(|raw| raw != "," && raw != ".")
        })
        .any(|offset| {
            word_at(cursor, offset, "would")
                && (word_at(cursor, offset + 1, "die")
                    || (word_at(cursor, offset + 1, "leave")
                        && word_at(cursor, offset + 2, "the")
                        && word_at(cursor, offset + 3, "battlefield")))
        })
}

/// Reads one created rule's introducer, event and lifetime, up to (not
/// including) the comma that opens its action range.
pub(in crate::oracle::canonical) fn recognize_effect_created_rule(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<EffectCreatedRuleNode> {
    cursor.enter("created-rule.introducer");
    if word_at(cursor, 0, "the") {
        cursor.consume_phrase(&["the", "next", "time"])?;
        // The shared damage-event children read the source and the event;
        // an attached-permanent noun has no meaning outside an Aura or
        // Equipment's static ability.
        let context = DamageReplacementContext {
            name_links,
            name_failure,
            attachment_word: None,
        };
        let event = super::recognize_chosen_source_damage_event(cursor, &context)?;
        let lifetime = recognize_lifetime(cursor, "next-damage.lifetime")?;
        return Ok(EffectCreatedRuleNode::NextDamage(NextDamageRuleNode {
            event,
            lifetime,
        }));
    }
    if word_at(cursor, 0, "if") {
        cursor.consume("if")?;
        return recognize_created_replacement(cursor).map(EffectCreatedRuleNode::Replacement);
    }
    let preposed = recognize_preposed_lifetime(cursor)?;
    cursor.enter("created-rule.trigger-word");
    let repeats = word_at(cursor, 0, "whenever");
    if preposed.is_none() && specialized_event_follows(cursor) {
        cursor.consume(if repeats { "whenever" } else { "when" })?;
        let trigger = recognize_embedded_delayed_trigger(cursor)?;
        // A followed object departs once, so `whenever` means `when`; the
        // next spell is consumed by its own cast (CR 603.7c).
        cursor.enter("created-rule.repeating-next-spell");
        cursor.require(
            !repeats
                || matches!(
                    trigger.event,
                    DelayedTriggerEventNode::BoundObjectDeparture { .. }
                ),
            "a one-shot `when` trigger for the controller's next spell",
        )?;
        return Ok(EffectCreatedRuleNode::Trigger(trigger));
    }
    recognize_created_event_trigger(cursor, name_links, name_failure, preposed, repeats)
        .map(EffectCreatedRuleNode::Trigger)
}

/// Reads a preposed created-trigger lifetime and its comma, if written.
fn recognize_preposed_lifetime(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<CreatedRuleLifetime>> {
    if preposed_lifetime_width(cursor).is_none() {
        return Ok(None);
    }
    cursor.enter("created-rule.preposed-lifetime");
    let lifetime = recognize_created_rule_lifetime(cursor)?;
    cursor.consume(",")?;
    Ok(Some(lifetime))
}

/// `when|whenever OCCURRENCE [this turn]` after an optional preposed
/// lifetime. The occurrence is read by the shared event clause reader over
/// a view of the sentence without the lifetime (a printed trigger's event
/// has none), whose boundary is the comma that opens the governed actions;
/// that comma stays for the sequence.
fn recognize_created_event_trigger(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    preposed: Option<CreatedRuleLifetime>,
    repeats: bool,
) -> MachineResult<EmbeddedDelayedTriggerNode> {
    cursor.enter("delayed-trigger.event-clause");
    cursor.require(
        word_at(cursor, 0, "when") || word_at(cursor, 0, "whenever"),
        "`when` or `whenever` introducing the created trigger",
    )?;
    let inline = inline_lifetime_offset(cursor);
    cursor.enter("delayed-trigger.lifetime");
    let lifetime = match (preposed, inline) {
        (Some(lifetime), None) => lifetime,
        (None, Some(_)) => CreatedRuleLifetime::ThisTurn,
        (Some(_), Some(_)) => {
            return Err(cursor.fail("one lifetime for the created trigger"));
        }
        (None, None) => {
            return Err(cursor.fail(
                "the created trigger's lifetime: `this turn` or a preposed `Until end of turn,`",
            ));
        }
    };
    let start = cursor.position();
    // The event view keeps every token index and span before the lifetime;
    // the comma follows the event directly.
    let view_tokens = match inline {
        Some(offset) => {
            let lifetime_at = start + offset;
            let mut tokens = cursor.token_slice(0, lifetime_at).to_vec();
            tokens.extend_from_slice(cursor.token_slice(lifetime_at + 2, lifetime_at + 3));
            tokens
        }
        None => cursor.token_slice(0, usize::MAX).to_vec(),
    };
    let mut view = cursor.with_tokens(&view_tokens);
    let event = match super::read_event_clause(&mut view, name_links, name_failure, ",") {
        EventClauseAttempt::Recognized(event) => event,
        EventClauseAttempt::Rejected(failure) | EventClauseAttempt::Committed(failure) => {
            return Err(failure);
        }
        EventClauseAttempt::NotClaimed => {
            return Err(cursor.fail("an event after the created trigger's `when` or `whenever`"));
        }
    };
    // The reader consumed its boundary; the comma belongs to the sequence.
    let event_end = view.position().saturating_sub(1);
    cursor.enter("delayed-trigger.event-boundary");
    match inline {
        Some(offset) => {
            cursor.require(
                event_end == start + offset,
                "the created trigger's event ending at its `this turn`",
            )?;
            cursor.advance_to(event_end)?;
            cursor.enter("delayed-trigger.lifetime");
            cursor.consume_phrase(&["this", "turn"])?;
        }
        None => cursor.advance_to(event_end)?,
    }
    cursor.enter("delayed-trigger.event-boundary");
    cursor.require(
        cursor.peek() == Some(","),
        "the comma that opens the created trigger's actions",
    )?;
    // A single occurrence's event-time condition belongs to the trigger, as
    // for a printed one (`trigger_envelope.rs`).
    let (event, condition) = match event {
        GenericTriggerEventNode::Conditioned { event, condition } => (*event, Some(condition)),
        event => (event, None),
    };
    Ok(EmbeddedDelayedTriggerNode {
        event: DelayedTriggerEventNode::Event { event, condition },
        lifetime,
        repeats,
    })
}

/// `EVENT this turn` after `when`.
pub(in crate::oracle::canonical) fn recognize_embedded_delayed_trigger(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EmbeddedDelayedTriggerNode> {
    cursor.enter("delayed-trigger.event");
    let event = if word_at(cursor, 0, "you")
        && (word_at(cursor, 1, "next") || word_at(cursor, 1, "cast"))
    {
        recognize_next_spell_cast(cursor)?
    } else {
        let object = recognize_bound_object(cursor, "delayed-trigger.object")?;
        cursor.enter("delayed-trigger.departure");
        let departure = if word_at(cursor, 0, "dies") {
            cursor.consume("dies")?;
            ObjectDeparture::Dies
        } else if word_at(cursor, 0, "leaves") {
            cursor.consume_phrase(&["leaves", "the", "battlefield"])?;
            ObjectDeparture::LeavesBattlefield
        } else {
            return Err(cursor.fail("`dies` or `leaves the battlefield` for the bound object"));
        };
        DelayedTriggerEventNode::BoundObjectDeparture { object, departure }
    };
    let lifetime = recognize_lifetime(cursor, "delayed-trigger.lifetime")?;
    Ok(EmbeddedDelayedTriggerNode {
        event,
        lifetime,
        repeats: false,
    })
}

/// `you next cast SPELL` / `you cast your next SPELL`.
fn recognize_next_spell_cast(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DelayedTriggerEventNode> {
    cursor.enter("delayed-trigger.spell-cast.caster");
    cursor.consume("you")?;
    cursor.enter("delayed-trigger.spell-cast.next");
    if word_at(cursor, 0, "next") {
        cursor.consume_phrase(&["next", "cast"])?;
        cursor.enter("delayed-trigger.spell-cast.article");
        let article = cursor
            .peek()
            .filter(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
            .ok_or_else(|| cursor.fail("`a` or `an` before the next spell"))?
            .to_string();
        cursor.consume(&article)?;
    } else {
        cursor.consume_phrase(&["cast", "your", "next"])?;
    }
    cursor.enter("delayed-trigger.spell-cast.query");
    let query = super::recognize_object_noun_query(cursor, super::ObjectQueryContext::ORDINARY)?
        .into_spell_cast_query(cursor)?;
    cursor.enter("delayed-trigger.spell-cast.caster-identity");
    cursor.require(
        query.caster.is_none(),
        "one caster relation, supplied by the event subject",
    )?;
    cursor.enter("delayed-trigger.spell-cast.spell-domain");
    cursor.require(
        query.predicate.is_in_domain(ObjectKind::Spell),
        "an explicit spell-domain ObjectQuery",
    )?;
    cursor.enter("delayed-trigger.spell-cast.predicate-binding");
    cursor.require(
        !super::object_predicate_uses_x(&query.predicate),
        "a spell predicate without an undeclared numeric producer",
    )?;
    Ok(DelayedTriggerEventNode::NextSpellCast {
        predicate: query.predicate,
    })
}

/// `OBJECT would die this turn` / `OBJECT would leave the battlefield
/// [this turn]` after `if`.
fn recognize_created_replacement(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CreatedReplacementNode> {
    let object = recognize_bound_object(cursor, "created-replacement.object")?;
    cursor.enter("created-replacement.event");
    let event = if word_at(cursor, 1, "leave") {
        cursor.consume_phrase(&["would", "leave", "the", "battlefield"])?;
        ObjectDeparture::LeavesBattlefield
    } else {
        cursor.consume_phrase(&["would", "die"])?;
        ObjectDeparture::Dies
    };
    let lifetime = if event == ObjectDeparture::Dies || word_at(cursor, 0, "this") {
        Some(recognize_lifetime(cursor, "created-replacement.lifetime")?)
    } else {
        None
    };
    Ok(CreatedReplacementNode {
        object,
        event,
        lifetime,
        instead: false,
    })
}

/// `it` / `that NOUN`: an object the program already bound, resolved by the
/// shared binding environment of the action being read.
fn recognize_bound_object(
    cursor: &mut OracleTokenCursor<'_>,
    state: &'static str,
) -> MachineResult<PermanentEffectReference> {
    cursor.enter(state);
    cursor.require(
        super::bound_object_reference_starts(cursor),
        "`it` or `that NOUN` naming an object bound earlier in the program",
    )?;
    let environment = cursor
        .binding_environment()
        .cloned()
        .ok_or_else(|| cursor.fail("a binding environment naming the antecedent object"))?;
    let object = super::recognize_bound_object_reference(cursor, &environment)?;
    // An event object of the creating ability already left or will be
    // observed by its own trigger; only a bound permanent is followed.
    cursor.enter(state);
    cursor.require(
        matches!(
            object,
            PermanentEffectReference::Target(_)
                | PermanentEffectReference::Source(_)
                // The permanent a move of the program put onto the
                // battlefield (CR 400.7), bound by that move.
                | PermanentEffectReference::Observed(
                    super::TypedObservedEventObject::MovedOntoBattlefield
                )
        ),
        "a declared target, the source or the permanent the program put onto the battlefield as the followed object",
    )?;
    Ok(object)
}

fn recognize_lifetime(
    cursor: &mut OracleTokenCursor<'_>,
    state: &'static str,
) -> MachineResult<CreatedRuleLifetime> {
    cursor.enter(state);
    cursor.require(word_at(cursor, 0, "this"), "a closing `this turn`")?;
    recognize_created_rule_lifetime(cursor)
}

/// Installs the delayed trigger's own event frame for its governed actions
/// on the cursor's binding environment (CR 603.7d-e).
pub(in crate::oracle::canonical) fn enter_created_rule_body(
    cursor: &mut OracleTokenCursor<'_>,
    rule: &EffectCreatedRuleNode,
) {
    if let Some(frame) = rule.body_frame() {
        let mut environment = cursor.binding_environment().cloned().unwrap_or_default();
        // A created spell-cast trigger's `it` is the spell its own event
        // cast (CR 603.7d-e), not an object the creating ability named
        // (`Remove three charge counters from this artifact: When you next
        // cast an instant spell this turn, copy it.`).
        if frame.event_spell {
            environment.object = None;
        }
        environment.frame = frame;
        cursor.replace_binding_environment(Some(std::sync::Arc::new(environment)));
    }
    // `that much` / `that many` in a created trigger names its own event's
    // quantity, as in a printed trigger of the same event (CR 603.7d-e).
    if let Some(event) = rule.created_event() {
        cursor.set_event_numeric_binding(event.numeric_binding());
    }
}

/// The context of an action governed by `rule`: a delayed trigger's actions
/// read only its own event (CR 603.7d-e). An entry event observes the
/// entering permanent (`it`), as for a printed trigger of that event.
pub(in crate::oracle::canonical) fn scope_created_rule_body(
    rule: Option<&EffectCreatedRuleNode>,
    context: &mut super::EffectRecognitionContext,
) {
    if let Some(frame) = rule.and_then(EffectCreatedRuleNode::body_frame) {
        context.frame = frame;
        context.observed_event_object = rule
            .and_then(EffectCreatedRuleNode::created_event)
            .and_then(GenericTriggerEventNode::observed_object);
    }
}

#[cfg(test)]
#[path = "embedded_delayed_trigger_tests.rs"]
mod tests;
