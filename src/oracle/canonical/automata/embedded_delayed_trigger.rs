//! Rules a resolving effect creates over a range of its own actions
//! (CR 603.7, 614.1a).
//!
//! ```text
//! CreatedRule := DelayedStepTiming                       -- `delayed_step_timing.rs`
//!              | "when" DelayedEvent Lifetime ","         -- delayed trigger (603.7)
//!              | "if" BoundObject "would die" Lifetime "," -- replacement (614.1a)
//!              | "the next time" ChosenSourceDamage Lifetime "," -- shield (615.7)
//! DelayedEvent := BoundObject ("dies" | "leaves the battlefield")
//!               | "you" "next" "cast" SpellQuery
//!               | "you" "cast" "your" "next" SpellQuery
//! Lifetime     := "this" "turn"
//! ```
//!
//! The enclosing effect sequence owns the action range the rule governs
//! (the actions of the same sentence); this module owns only the rule's
//! introducer, event and lifetime. A bound object is read by the shared
//! binding environment (`that creature`, `it`); a spell is read by the shared
//! object query. The actions of a delayed trigger read the delayed trigger's
//! own event, not the event of the ability that created it (CR 603.7d-e).
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    DamageEventNode, DamageReplacementContext, DelayedStepTimingNode, FrameBindings, ObjectKind,
    ObjectPredicate, PermanentEffectReference,
};

/// How a created rule governs its action range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum EffectCreatedRuleNode {
    /// `at the beginning of the next STEP` / `at end of combat`: a delayed
    /// triggered ability that performs the actions at that step (CR 603.7a).
    Step(DelayedStepTimingNode),
    /// `When EVENT this turn,`: a delayed triggered ability that performs the
    /// actions the first time the event occurs (CR 603.7a, 603.7c).
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
            Self::Trigger(trigger) => Some(match trigger.event {
                // The departed object is the lexical antecedent the program
                // already names; no other event reference is in scope.
                DelayedTriggerEventNode::BoundObjectDeparture { .. } => FrameBindings::default(),
                // `that spell` / `it`: the spell whose cast triggered it.
                DelayedTriggerEventNode::NextSpellCast { .. } => FrameBindings {
                    event_spell: true,
                    ..FrameBindings::default()
                },
            }),
        }
    }

    /// Reads the `instead` that closes a replacement's action
    /// (`exile it instead`, CR 614.1a). Any other created rule rejects it.
    pub(in crate::oracle::canonical) fn read_instead(
        &mut self,
        cursor: &mut OracleTokenCursor<'_>,
    ) -> MachineResult<()> {
        cursor.enter("created-rule.replacement.instead");
        match self {
            Self::Replacement(replacement) if !replacement.instead => {
                cursor.consume("instead")?;
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

/// The lifetime of a created rule. Every supported form is bounded to the
/// current turn; an unbounded delayed trigger is refused explicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CreatedRuleLifetime {
    /// `this turn`.
    ThisTurn,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EmbeddedDelayedTriggerNode {
    pub event: DelayedTriggerEventNode,
    pub lifetime: CreatedRuleLifetime,
}

/// The next damage event a created shield modifies, with its lifetime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct NextDamageRuleNode {
    pub event: DamageEventNode,
    pub lifetime: CreatedRuleLifetime,
}

/// `If OBJECT would die this turn, ACTION instead.` The action is the
/// governed range; lowering proves it is the replacing action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CreatedReplacementNode {
    pub object: PermanentEffectReference,
    pub lifetime: CreatedRuleLifetime,
    /// Set once the sequence read the closing `instead`.
    pub instead: bool,
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

/// Lexical lookahead for a created rule introduced by `when` / `whenever`
/// (a delayed trigger) or `if ... would die` (a replacement). An outcome
/// connector (`When you do,`, CR 603.12) belongs to the program parent.
pub(in crate::oracle::canonical) fn effect_created_rule_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    if super::outcome_connector_at(cursor, 0).is_some() {
        return false;
    }
    if word_at(cursor, 0, "when") || word_at(cursor, 0, "whenever") {
        // The created rule's event is a bound object's departure or the
        // controller's next spell. Another event after `when` (a reflexive
        // `When an opponent does,`) is not this frame's to claim.
        if word_at(cursor, 1, "you") && (word_at(cursor, 2, "next") || word_at(cursor, 2, "cast")) {
            return true;
        }
        let mut event = cursor.clone();
        return event.advance_to(cursor.position() + 1).is_ok()
            && super::bound_object_reference_starts(&event);
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
        .any(|offset| word_at(cursor, offset, "would") && word_at(cursor, offset + 1, "die"))
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
    if word_at(cursor, 0, "whenever") {
        // CR 603.7c: a delayed trigger triggers once; a repeating
        // `whenever` rule created by an effect has no runtime.
        return Err(cursor.fail("a one-shot `when` delayed trigger, not a repeating `whenever`"));
    }
    cursor.consume("when")?;
    recognize_embedded_delayed_trigger(cursor).map(EffectCreatedRuleNode::Trigger)
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
    Ok(EmbeddedDelayedTriggerNode { event, lifetime })
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

/// `OBJECT would die this turn` after `if`.
fn recognize_created_replacement(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CreatedReplacementNode> {
    let object = recognize_bound_object(cursor, "created-replacement.object")?;
    cursor.enter("created-replacement.event");
    cursor.consume_phrase(&["would", "die"])?;
    let lifetime = recognize_lifetime(cursor, "created-replacement.lifetime")?;
    Ok(CreatedReplacementNode {
        object,
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
            PermanentEffectReference::Target(_) | PermanentEffectReference::Source(_)
        ),
        "a declared target or the source as the followed object",
    )?;
    Ok(object)
}

fn recognize_lifetime(
    cursor: &mut OracleTokenCursor<'_>,
    state: &'static str,
) -> MachineResult<CreatedRuleLifetime> {
    cursor.enter(state);
    cursor.consume_phrase(&["this", "turn"])?;
    Ok(CreatedRuleLifetime::ThisTurn)
}

/// Installs the delayed trigger's own event frame for its governed actions
/// on the cursor's binding environment (CR 603.7d-e).
pub(in crate::oracle::canonical) fn enter_created_rule_body(
    cursor: &mut OracleTokenCursor<'_>,
    rule: &EffectCreatedRuleNode,
) {
    if let Some(frame) = rule.body_frame() {
        let mut environment = cursor.binding_environment().cloned().unwrap_or_default();
        environment.frame = frame;
        cursor.replace_binding_environment(Some(std::sync::Arc::new(environment)));
    }
}

/// The context of an action governed by `rule`: a delayed trigger's actions
/// read only its own event (CR 603.7d-e).
pub(in crate::oracle::canonical) fn scope_created_rule_body(
    rule: Option<&EffectCreatedRuleNode>,
    context: &mut super::EffectRecognitionContext,
) {
    if let Some(frame) = rule.and_then(EffectCreatedRuleNode::body_frame) {
        context.frame = frame;
        context.observed_event_object = None;
    }
}

#[cfg(test)]
#[path = "embedded_delayed_trigger_tests.rs"]
mod tests;
