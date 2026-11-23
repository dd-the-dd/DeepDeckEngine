//! Lowers a typed damage-event replacement (CR 614.1a) or prevention
//! (CR 615.1a) into one battlefield-active `replaceDamage` modifier.
//!
//! The event pattern keeps its source query, recipient alternatives, damage
//! kind and amount guard as separate fields; the modification is either a
//! new amount or a prevented amount, both arithmetic over `eventQuantity`.
use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) enum DamageReplacementParseAttempt {
    Parsed(CanonicalRuleDraft),
    NotClaimed,
    Failed(MachineFailure),
}

/// The attachment noun an Equipment or Aura face uses for the permanent it is
/// attached to (CR 301.5, 303.4).
fn attachment_word(type_line: &str) -> Option<&'static str> {
    let words = type_line
        .split(|ch: char| !ch.is_alphanumeric())
        .collect::<Vec<_>>();
    if words.contains(&"Equipment") {
        Some("equipped")
    } else if words.contains(&"Aura") {
        Some("enchanted")
    } else {
        None
    }
}

pub(in crate::oracle::canonical) fn try_parse_damage_replacement(
    input: &AbilityInput<'_>,
) -> DamageReplacementParseAttempt {
    let mut cursor = OracleTokenCursor::new("damage-replacement", input.tokens);
    let context = DamageReplacementContext {
        name_links: input.name_links,
        name_failure: input.name_resolution_failure,
        attachment_word: attachment_word(input.face_type_line),
    };
    let node = match try_recognize_damage_replacement(&mut cursor, &context) {
        DamageReplacementRecognition::Parsed(node) => node,
        DamageReplacementRecognition::NotClaimed => {
            return DamageReplacementParseAttempt::NotClaimed;
        }
        DamageReplacementRecognition::Failed(failure) => {
            return DamageReplacementParseAttempt::Failed(failure);
        }
    };
    cursor.enter("replacement.damage-event.source-domain");
    if let Err(failure) = cursor.require(
        type_line_has_permanent_card_type(input.face_type_line),
        "a permanent source face for a battlefield-active damage replacement",
    ) {
        return DamageReplacementParseAttempt::Failed(failure);
    }
    match lower_damage_replacement(&mut cursor, &node) {
        Ok(modifier) => DamageReplacementParseAttempt::Parsed(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [modifier],
            }),
            &[
                "Read the damage event's source, recipients, kind and amount guard",
                "Read the modification as a new amount or a prevented amount",
                "Lower to one battlefield-active damage replacement (CR 614.1a, 615.1a)",
            ],
        )),
        Err(failure) => DamageReplacementParseAttempt::Failed(failure),
    }
}

fn lower_controller(scope: ObjectControllerScope) -> Value {
    lower_object_controller_scope(scope)
}

/// A damage source is any object; a query lowers with the domain it names.
fn lower_source_predicate(
    cursor: &OracleTokenCursor<'_>,
    predicate: &ObjectPredicate,
    source_noun: bool,
) -> MachineResult<(Value, Option<&'static str>)> {
    if predicate.has_domain(ObjectKind::Spell) {
        cursor.require(
            predicate.supports_domain(ObjectKind::Spell),
            "a spell source predicate within the spell domain",
        )?;
        return Ok((
            lower_spell_object_predicate(predicate).unwrap_or(Value::Null),
            Some("spell"),
        ));
    }
    let permanent = if source_noun {
        predicate.has_explicit_domain(ObjectKind::Permanent)
    } else {
        predicate.has_domain(ObjectKind::Permanent)
    };
    if permanent {
        cursor.require(
            predicate.supports_domain(ObjectKind::Permanent),
            "a permanent source predicate within the permanent domain",
        )?;
        return Ok((lower_permanent_object_predicate(predicate), None));
    }
    cursor.require(
        predicate.supports_domain(ObjectKind::Card),
        "source characteristics evaluable on any object",
    )?;
    Ok((lower_card_object_predicate(predicate), None))
}

fn lower_damage_source(
    cursor: &OracleTokenCursor<'_>,
    source: &DamageSourceNode,
) -> MachineResult<Value> {
    Ok(match source {
        DamageSourceNode::SourceSelf => self_ref(),
        DamageSourceNode::AttachedPermanent => {
            json!({ "kind": "attachedPermanent", "attachment": self_ref() })
        }
        DamageSourceNode::Objects {
            predicate,
            controller,
            exclude_source,
            source_noun,
        } => {
            let mut lowered = json!({ "kind": "objects" });
            if let Some(predicate) = predicate {
                let (filter, domain) = lower_source_predicate(cursor, predicate, *source_noun)?;
                if !filter.is_null() {
                    lowered["where"] = filter;
                }
                if let Some(domain) = domain {
                    lowered["domain"] = json!(domain);
                }
            }
            if let Some(scope) = controller {
                lowered["controller"] = lower_controller(*scope);
            }
            if *exclude_source {
                lowered["excludeSource"] = json!(true);
            }
            lowered
        }
    })
}

fn lower_damage_recipient(recipient: &DamageEventRecipient) -> Value {
    match recipient {
        DamageEventRecipient::Player(player) => json!({
            "kind": "player",
            "player": match player {
                DamagePlayerRecipient::You => controller(),
                DamagePlayerRecipient::Opponent => json!({
                    "kind": "opponentsOf",
                    "player": controller(),
                }),
                DamagePlayerRecipient::AnyPlayer => json!({ "kind": "eachPlayer" }),
            },
        }),
        DamageEventRecipient::Permanents {
            predicate,
            controller,
            exclude_source,
        } => {
            let mut lowered = json!({ "kind": "permanents" });
            if let Some(filter) = predicate
                .as_ref()
                .map(lower_permanent_object_predicate)
                .filter(|filter| !filter.is_null())
            {
                lowered["where"] = filter;
            }
            if let Some(scope) = controller {
                lowered["controller"] = lower_controller(*scope);
            }
            if *exclude_source {
                lowered["excludeSource"] = json!(true);
            }
            lowered
        }
        DamageEventRecipient::SourceSelf => self_ref(),
        DamageEventRecipient::AttachedPermanent => {
            json!({ "kind": "attachedPermanent", "attachment": self_ref() })
        }
    }
}

fn lower_event_amount(
    cursor: &OracleTokenCursor<'_>,
    expression: &NumericExpression,
) -> MachineResult<Value> {
    lower_numeric_expression_for_event(expression, EventNumericBinding::ReplacementQuantity)
        .ok_or_else(|| cursor.fail("an amount over the replaced event's quantity"))
}

fn lower_damage_replacement(
    cursor: &mut OracleTokenCursor<'_>,
    node: &DamageReplacementNode,
) -> MachineResult<Value> {
    cursor.enter("replacement.damage-event.lowering");
    let event = &node.event;
    let mut lowered_event = json!({
        "damage": match event.kind {
            DamageKind::Any => "any",
            DamageKind::Combat => "combat",
            DamageKind::Noncombat => "noncombat",
        },
    });
    if let Some(source) = &event.source {
        lowered_event["source"] = lower_damage_source(cursor, source)?;
    }
    if !event.recipients.is_empty() {
        lowered_event["recipients"] = Value::Array(
            event
                .recipients
                .iter()
                .map(lower_damage_recipient)
                .collect(),
        );
    }
    if let Some(minimum) = event.minimum {
        lowered_event["minimum"] = json!(minimum);
    }
    if let Some(maximum) = event.maximum {
        lowered_event["maximum"] = json!(maximum);
    }
    let replacement = match &node.modification {
        DamageModificationNode::Amount(amount) => json!({
            "kind": "modifyAmount",
            "quantity": lower_event_amount(cursor, amount)?,
        }),
        DamageModificationNode::Prevent(amount) => json!({
            "kind": "prevent",
            "amount": lower_event_amount(cursor, amount)?,
        }),
    };
    Ok(json!({
        "kind": "replaceDamage",
        "event": lowered_event,
        "replacement": replacement,
    }))
}

#[cfg(test)]
#[path = "damage_replacement_tests.rs"]
mod tests;
