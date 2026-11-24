//! Damage prevention action `prevent AMOUNT [combat] damage FRAME` (CR 615).
//!
//! The action owns an amount (`all` or `the next N`), an optional combat
//! restriction, an optional recipient, an optional source and exactly one
//! duration. Each slot is a shared child: numeric atom, target list, player
//! reference, permanent operand and effect duration. The two printed frames
//! (`that would be dealt [to RECIPIENT] [by SOURCE]` and `SOURCE would deal
//! [to RECIPIENT]`) differ only in how the source is introduced.
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectDurationNode, EffectRecognitionContext, LocalPlayerReference, NumericAtom,
    PermanentOperandNode, TargetListNode, TargetSelector, damage_target_supported,
    recognize_contextual_permanent_operand, recognize_effect_duration, recognize_numeric_atom,
    recognize_player_reference, recognize_target_list, selector_guarantees_permanent,
};

/// How much damage a prevention shield prevents (CR 615.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PreventedDamageAmount {
    All,
    Next(NumericAtom),
    /// `that damage`: all the damage of the event the enclosing created
    /// rule observes (CR 615.7). Only that rule's lowering gives it a
    /// meaning; anywhere else it is refused.
    EventDamage,
}

/// A damage recipient or source named by a prevention action. Collections of
/// objects are a different concept (a continuous scope) and never reach here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DamagePartyNode {
    Targets(TargetListNode),
    Player(LocalPlayerReference),
    Object(PermanentOperandNode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PreventDamageEffectNode {
    pub amount: PreventedDamageAmount,
    pub combat_only: bool,
    pub recipient: Option<DamagePartyNode>,
    pub source: Option<DamagePartyNode>,
    /// `dealt to and dealt by PARTY`: the recipient party is also a source
    /// whose damage is prevented, two filters over one declaration (CR
    /// 615.1a): damage dealt to it, and damage it deals.
    pub recipient_also_source: bool,
    pub duration: EffectDurationNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PartyRole {
    Recipient,
    Source,
}

fn is_this_turn(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("turn"))
}

fn starts_target_list(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek().is_some_and(|raw| {
        raw.eq_ignore_ascii_case("target")
            || raw.eq_ignore_ascii_case("another")
            || raw.eq_ignore_ascii_case("up")
            || raw.eq_ignore_ascii_case("any")
    }) || cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "quantity")
}

fn recognize_party(
    cursor: &mut OracleTokenCursor<'_>,
    role: PartyRole,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<DamagePartyNode> {
    cursor.enter(match role {
        PartyRole::Recipient => "effect.prevent.recipient",
        PartyRole::Source => "effect.prevent.source",
    });
    if role == PartyRole::Recipient
        && cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
    {
        return Ok(DamagePartyNode::Player(recognize_player_reference(
            cursor, None,
        )?));
    }
    if starts_target_list(cursor) {
        let targets = recognize_target_list(cursor)?;
        cursor.enter("effect.prevent.party.target-slot");
        cursor.require(
            targets.slots.len() == 1 && targets.slots[0].cardinality.maximum == 1,
            "exactly one target slot for a damage prevention party",
        )?;
        let selector = &targets.slots[0].selector;
        cursor.require(
            match role {
                PartyRole::Recipient => damage_target_supported(selector),
                PartyRole::Source => {
                    !matches!(selector, TargetSelector::Any)
                        && selector_guarantees_permanent(selector)
                }
            },
            match role {
                PartyRole::Recipient => "a player, any target, or a damageable permanent selector",
                PartyRole::Source => "a battlefield permanent as the prevented damage source",
            },
        )?;
        return Ok(DamagePartyNode::Targets(targets));
    }
    let operand =
        recognize_contextual_permanent_operand(cursor, name_links, name_failure, context)?;
    cursor.enter("effect.prevent.party.reference");
    cursor.require(
        matches!(
            operand,
            PermanentOperandNode::Reference(_) | PermanentOperandNode::BoundReference(_)
        ),
        "one named or referenced object, not a collection of objects",
    )?;
    Ok(DamagePartyNode::Object(operand))
}

/// Recognizes `prevent AMOUNT [combat] damage ...` through its duration.
/// Punctuation and following effects stay with the enclosing sequence.
pub(in crate::oracle::canonical) fn recognize_prevent_damage_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<PreventDamageEffectNode> {
    cursor.enter("effect.prevent.action");
    let mut trial = cursor.clone();
    trial.consume("prevent")?;
    trial.enter("effect.prevent.amount");
    if trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
        && trial
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("damage"))
    {
        // The event, its parties and its lifetime belong to the rule that
        // introduced the sentence.
        trial.enter("effect.prevent.event-damage");
        trial.consume_phrase(&["that", "damage"])?;
        *cursor = trial;
        return Ok(PreventDamageEffectNode {
            amount: PreventedDamageAmount::EventDamage,
            combat_only: false,
            recipient: None,
            source: None,
            recipient_also_source: false,
            duration: EffectDurationNode::UntilEndOfCurrentTurn,
        });
    }
    let amount = if trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("all"))
    {
        trial.consume("all")?;
        PreventedDamageAmount::All
    } else {
        trial.consume_phrase(&["the", "next"])?;
        PreventedDamageAmount::Next(recognize_numeric_atom(&mut trial)?)
    };
    trial.enter("effect.prevent.damage-kind");
    let combat_only = trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("combat"));
    if combat_only {
        trial.consume("combat")?;
    }
    trial.consume("damage")?;

    let mut recipient = None;
    let mut source = None;
    let mut recipient_also_source = false;
    let mut duration = None;
    trial.enter("effect.prevent.frame");
    if trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
        && trial
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("would"))
    {
        trial.consume("that")?;
        super::recognize_relative_damage_clause(&mut trial, super::Voice::Passive)?;
    } else {
        source = Some(recognize_party(
            &mut trial,
            PartyRole::Source,
            name_links,
            name_failure,
            context,
        )?);
        trial.enter("effect.prevent.frame.would-deal");
        super::recognize_relative_damage_clause(&mut trial, super::Voice::Active)?;
    }
    // The recipient, the `by SOURCE` phrase and the duration are independent
    // optional slots; each may appear once, in any printed order.
    loop {
        trial.enter("effect.prevent.clause");
        if is_this_turn(&trial) {
            trial.require(duration.is_none(), "exactly one duration")?;
            duration = Some(recognize_effect_duration(&mut trial)?);
        } else if ["to", "and", "dealt", "by"]
            .iter()
            .enumerate()
            .all(|(offset, word)| {
                trial
                    .peek_ahead(offset)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
            })
        {
            // `dealt to and dealt by PARTY`: one party in both roles. Its
            // source role needs an object, which the shield filter names by
            // the same declaration.
            trial.enter("effect.prevent.to-and-by");
            trial.require(
                recipient.is_none() && source.is_none(),
                "one party both dealt and dealing the prevented damage",
            )?;
            trial.consume_phrase(&["to", "and", "dealt", "by"])?;
            let party = recognize_party(
                &mut trial,
                PartyRole::Recipient,
                name_links,
                name_failure,
                context,
            )?;
            trial.enter("effect.prevent.to-and-by.object");
            trial.require(
                match &party {
                    DamagePartyNode::Targets(targets) => {
                        selector_guarantees_permanent(&targets.slots[0].selector)
                    }
                    DamagePartyNode::Object(_) => true,
                    DamagePartyNode::Player(_) => false,
                },
                "a permanent that is dealt damage and deals damage",
            )?;
            recipient = Some(party);
            recipient_also_source = true;
        } else if trial
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("to"))
        {
            trial.require(recipient.is_none(), "exactly one damage recipient")?;
            trial.consume("to")?;
            recipient = Some(recognize_party(
                &mut trial,
                PartyRole::Recipient,
                name_links,
                name_failure,
                context,
            )?);
        } else if trial
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("by"))
        {
            trial.require(
                source.is_none() && !recipient_also_source,
                "exactly one damage source",
            )?;
            trial.consume("by")?;
            source = Some(recognize_party(
                &mut trial,
                PartyRole::Source,
                name_links,
                name_failure,
                context,
            )?);
        } else {
            break;
        }
    }
    trial.enter("effect.prevent.duration");
    let duration = trial.require_value(duration, "the duration `this turn`")?;
    trial.require(
        duration == EffectDurationNode::UntilEndOfCurrentTurn,
        "a damage prevention that lasts for the current turn",
    )?;
    trial.require(
        context.preposed_duration.is_none(),
        "no sentence-level duration for a damage prevention",
    )?;
    let targeted =
        |party: &Option<DamagePartyNode>| matches!(party, Some(DamagePartyNode::Targets(_)));
    trial.enter("effect.prevent.targets");
    trial.require(
        !(targeted(&recipient) && targeted(&source)),
        "at most one target declaration in a damage prevention",
    )?;
    *cursor = trial;
    Ok(PreventDamageEffectNode {
        amount,
        combat_only,
        recipient,
        source,
        recipient_also_source,
        duration,
    })
}

#[cfg(test)]
#[path = "damage_prevention_tests.rs"]
mod tests;
