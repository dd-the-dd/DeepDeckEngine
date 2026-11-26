//! Lowering of event replacements read after `would` (CR 614.1a).
//!
//! A replaced draw whose program is `draw N cards` is the static draw-count
//! replacement of the shared runtime: each replaced draw becomes N draws,
//! excepting the first draw of the controller's draw step when stated.
use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn try_parse_event_replacement(
    input: &AbilityInput<'_>,
) -> ReplacementFrameAttempt {
    let mut cursor = OracleTokenCursor::new("event-replacement", input.tokens);
    let node = match try_recognize_event_replacement(&mut cursor) {
        ChildOutcome::Matched(node) => node,
        ChildOutcome::Committed(failure) => return ReplacementFrameAttempt::Committed(failure),
        ChildOutcome::NoMatch(_) => return try_parse_created_replacement_spell(input),
    };
    match lower_event_replacement(&mut cursor, input, &node) {
        Ok(draft) => ReplacementFrameAttempt::Parsed(draft),
        Err(failure) => ReplacementFrameAttempt::Committed(failure),
    }
}

fn lower_event_replacement(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
    node: &EventReplacementNode,
) -> MachineResult<CanonicalRuleDraft> {
    cursor.enter("replacement.event.validation");
    cursor.require(
        type_line_has_permanent_card_type(input.face_type_line),
        "a permanent source face for a battlefield-active replacement",
    )?;
    let modifier = match (&node.event, &node.program) {
        (
            ReplacedEventNode::Draw {
                player,
                except_first_in_draw_step,
            },
            ReplacementProgramNode::Draw { count },
        ) => {
            cursor.require(
                *player == LocalPlayerReference::Controller,
                "the ability controller's draws",
            )?;
            json!({
                "kind": "increaseEachDrawCount",
                "player": controller(),
                "amount": count - 1,
                "exceptFirstInDrawStep": except_first_in_draw_step,
            })
        }
        // CR 500.4: the unspent mana of the controller's pool is kept as
        // one type instead of emptying.
        (
            ReplacedEventNode::LoseUnspentMana { player },
            ReplacementProgramNode::ManaBecomes { symbol },
        ) => {
            cursor.require(
                *player == LocalPlayerReference::Controller,
                "the ability controller's unspent mana",
            )?;
            json!({
                "kind": "convertUnspentMana",
                "player": controller(),
                "to": symbol,
            })
        }
        _ => {
            return Err(cursor.fail("a replacement program that applies to the replaced event"));
        }
    };
    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [modifier],
        }),
        &[
            "Recognize the replaced event with the shared subject and verb lexicon",
            "Read the replacement program performed instead",
            "Install one battlefield-active event replacement modifier",
        ],
    ))
}

/// A spell ability that is one replacement-creating sentence (`If a card
/// would be put into your graveyard from anywhere this turn, exile that
/// card instead.`, CR 611.2a, 614.1a): the spell creates the replacement
/// when it resolves.
fn try_parse_created_replacement_spell(input: &AbilityInput<'_>) -> ReplacementFrameAttempt {
    if !spell_face(input.face_type_line) {
        return ReplacementFrameAttempt::NotClaimed;
    }
    let mut cursor = OracleTokenCursor::new("created-replacement-spell", input.tokens);
    cursor.replace_source_names(Some(input.name_links.into()));
    let node = match try_recognize_created_replacement_sentence(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    ) {
        ChildOutcome::Matched(node) => node,
        ChildOutcome::Committed(failure) => return ReplacementFrameAttempt::Committed(failure),
        ChildOutcome::NoMatch(_) => return ReplacementFrameAttempt::NotClaimed,
    };
    let lowered = cursor.finish().and_then(|()| {
        cursor.enter("created-replacement-spell.lowering");
        lower_created_replacement_step(&mut cursor, &node)
    });
    match lowered {
        Ok(lowered) => ReplacementFrameAttempt::Parsed(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": lowered.effects,
            }),
            &[
                "Read one sentence creating a replacement with the shared frames",
                "Install the replacement when the spell resolves, through this turn",
            ],
        )),
        Err(failure) => ReplacementFrameAttempt::Committed(failure),
    }
}

#[cfg(test)]
#[path = "replacement_event_tests.rs"]
mod tests;

/// One program step creating a replacement (CR 611.2a, 614.1a): its one
/// install instruction, without declarations.
pub(in crate::oracle::canonical) fn lower_created_replacement_step(
    cursor: &mut OracleTokenCursor<'_>,
    replacement: &CreatedReplacementRuleNode,
) -> MachineResult<LoweredEffectSequence> {
    let effect = match replacement {
        CreatedReplacementRuleNode::Departure(node) => {
            lower_created_departure_replacement(cursor, node)?
        }
        CreatedReplacementRuleNode::Quantity {
            replacement,
            lifetime,
        } => {
            cursor.enter("lower.created-quantity-replacement.lifetime");
            cursor.require(
                *lifetime == Some(CreatedRuleLifetime::ThisTurn),
                "a created quantity replacement lasting this turn (`Until end of turn,`)",
            )?;
            json!({
                "kind": "installQuantityReplacement",
                "program": lower_quantity_replacement_program(cursor, replacement.clone())?,
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })
        }
        CreatedReplacementRuleNode::Damage(replacement) => json!({
            "kind": "installDamageReplacement",
            "program": lower_damage_replacement(cursor, replacement)?,
            "duration": { "kind": "untilEndOfCurrentTurn" },
        }),
        // The static uncast-entry modifier, installed as a created
        // continuous effect for the replacement's lifetime (CR 611.2a).
        CreatedReplacementRuleNode::UncastEntry {
            replacement,
            lifetime,
        } => {
            cursor.enter("lower.created-entry-replacement.lifetime");
            cursor.require(
                *lifetime == Some(CreatedRuleLifetime::ThisTurn),
                "a created entry replacement lasting this turn (`Until end of turn,`)",
            )?;
            cursor.enter("lower.created-entry-replacement.status");
            cursor.require(
                replacement.condition.status == ObjectCastStatus::NotCast
                    && replacement.replacement.destination == ReplacementDestination::Exile,
                "an uncast entering object exiled instead",
            )?;
            json!({
                "kind": "installTemporaryStaticModifiers",
                "modifiers": [{
                    "kind": "prohibitUncastBattlefieldEntry",
                    "where": lower_permanent_object_predicate(&replacement.event.object),
                    "destination": "exile",
                }],
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })
        }
    };
    Ok(LoweredEffectSequence {
        decisions: Vec::new(),
        effects: vec![effect],
    })
}
