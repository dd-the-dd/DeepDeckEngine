use super::super::*;
use crate::oracle::{
    characteristic_vocabulary::type_line_has_permanent_card_type, syntax::AbilityInput,
};

/// Compatibility name; the reader/lowerer themselves accept one semantic node.
pub(in crate::oracle::canonical) fn claims_promoted_permanent_scaled_casting_reduction_family(
    input: &AbilityInput<'_>,
) -> bool {
    input
        .tokens
        .first()
        .is_some_and(|t| t.raw.eq_ignore_ascii_case("this"))
        && input
            .tokens
            .get(1)
            .is_some_and(|t| t.raw.eq_ignore_ascii_case("spell"))
        && input
            .tokens
            .get(2)
            .is_some_and(|t| t.raw.eq_ignore_ascii_case("costs"))
        && input.tokens.windows(3).any(|window| {
            [["less", "to", "cast"], ["more", "to", "cast"]]
                .iter()
                .any(|phrase| {
                    phrase
                        .iter()
                        .zip(window)
                        .all(|(word, token)| token.raw.eq_ignore_ascii_case(word))
                })
        })
}

pub(in crate::oracle::canonical) fn lower_casting_cost_reduction(
    input: &AbilityInput<'_>,
    cursor: &mut OracleTokenCursor<'_>,
    node: CastingCostReductionNode,
) -> MachineResult<CanonicalRuleDraft> {
    cursor.enter("casting-reduction.amount-capability");
    let amount = lower_numeric_expression_with_query_support(
        &node.amount,
        NumericQueryLoweringSupport::CastingCost,
    )
    .ok_or_else(|| {
        cursor.fail("a symbolic reduction amount with a proved query/player/zone context")
    })?;
    // CR 118.7b-c: the colored symbols of the delta, applied `times` times.
    let colored = match &node.colored {
        None => None,
        Some(colored) => {
            cursor.enter("casting-reduction.colored-capability");
            let times = lower_numeric_expression_with_query_support(
                &colored.times,
                NumericQueryLoweringSupport::CastingCost,
            )
            .ok_or_else(|| cursor.fail("a symbolic colored reduction multiplier"))?;
            let symbols = ["W", "U", "B", "R", "G", "C"]
                .iter()
                .zip(colored.counts)
                .flat_map(|(symbol, count)| std::iter::repeat_n(*symbol, usize::from(count)))
                .collect::<Vec<_>>();
            Some(json!({
                "symbols": symbols,
                "times": times,
                "excessReducesGeneric": colored.excess_reduces_generic,
            }))
        }
    };
    let rule = match node.subject {
        CastingReductionSubject::CurrentSpell => {
            let mut modifier = json!({"kind":"reduceOwnGenericCastingCost","amount":amount});
            if let Some(colored) = &colored {
                modifier["coloredMana"] = colored.clone();
            }
            if let Some(condition) = &node.condition {
                cursor.enter("casting-reduction.condition-lowering");
                modifier["condition"] = lower_sentence_condition(
                    cursor,
                    condition,
                    &ConditionObjectBindings {
                        permanent_targets: &[],
                        card_targets: &[],
                        card_focus: None,
                        previous_action_result: None,
                    },
                )?;
            }
            if let Some(query) = &node.target_query {
                let filter = lower_permanent_object_predicate(&query.predicate);
                cursor.require(
                    !filter.is_null(),
                    "a lowerable permanent predicate for the targeted object",
                )?;
                modifier["targetWhere"] = filter;
            }
            json!({
                "kind":"staticAbility","source":self_ref(),
                "activeWhile":{"kind":"inZone","object":self_ref(),"zone":{"kind":"stackOrCast"}},
                "modifiers":[modifier],
            })
        }
        CastingReductionSubject::SpellSet(spells) => {
            cursor.enter("static.spell-cost-reduction.source-zone");
            cursor.require(
                node.target_query.is_none(),
                "no target condition for a spell-set subject",
            )?;
            cursor.require(
                type_line_has_permanent_card_type(input.face_type_line),
                "a permanent card face for a battlefield-active static ability",
            )?;
            let player = match spells.caster {
                None => json!({ "kind": "eachPlayer" }),
                Some(SpellCasterScope::AbilityController) => controller(),
                Some(SpellCasterScope::OpponentOfAbilityController) => {
                    json!({ "kind": "opponentsOf", "player": controller() })
                }
                Some(SpellCasterScope::NotAbilityController | SpellCasterScope::EventPlayer) => {
                    return Err(cursor.fail("a proved caster relation for the reduction subject"));
                }
            };
            // CR 601.2a: the zone a spell is cast from is a record of the
            // cast, read by the reduction itself (`sourceZone`), never as a
            // characteristic filter.
            cursor.enter("static.spell-cost-reduction.cast-origin");
            let (filter, origin) = cursor.require_value(
                split_cast_origin(&spells.predicate),
                "one cast origin in the reduction's spell conjunction",
            )?;
            let mut modifier = json!({"kind":"reduceCastingCost","player":player,
                "where":lower_spell_object_predicate(&filter).unwrap_or(Value::Null),"amount":amount});
            if let Some(colored) = &colored {
                modifier["coloredMana"] = colored.clone();
            }
            if let Some((origin, negated)) = origin {
                cursor.require(
                    origin.zone != CastOriginZone::Library,
                    "a cast zone the reduction records (hand, graveyard, exile)",
                )?;
                modifier[if negated {
                    "sourceZoneNot"
                } else {
                    "sourceZone"
                }] = json!(origin.zone.ir_name());
                // `your graveyard` (CR 400.3): the card is the caster's own.
                if origin.caster_owned && origin.zone != CastOriginZone::Hand {
                    cursor.require(!negated, "a negated cast origin of the caster's own hand")?;
                    modifier["sourceZoneOwner"] = json!("caster");
                }
            }
            // `as long as COND`: evaluated for the static's controller when
            // the cost is determined (CR 601.2f).
            if let Some(condition) = &node.condition {
                cursor.enter("casting-reduction.condition-lowering");
                modifier["condition"] = lower_sentence_condition(
                    cursor,
                    condition,
                    &ConditionObjectBindings {
                        permanent_targets: &[],
                        card_targets: &[],
                        card_focus: None,
                        previous_action_result: None,
                    },
                )?;
            }
            json!({"kind":"staticAbility","source":self_ref(),"activeWhile":active_while_battlefield(),
                "modifiers":[modifier]})
        }
    };
    Ok(draft(
        rule,
        &[
            "Read the reduction subject with the shared spell/object query",
            "Retain a symbolic numeric amount and optional ordinary Count(Query)",
            "Keep zone holders independent of object ownership and controller",
            "Project existing reduction and battlefield count primitives without changing their meaning",
        ],
    ))
}

pub(in crate::oracle::canonical) fn parse_promoted_permanent_scaled_casting_reduction(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("permanent-scaled-casting-reduction", input.tokens);
    // The increase direction of the same own-spell frame (`more`).
    match cursor.transaction(recognize_own_spell_cost_increase) {
        ChildOutcome::Matched(node) => return Ok(lower_own_spell_cost_increase(&node)),
        ChildOutcome::Committed(failure) => return Err(failure),
        ChildOutcome::NoMatch(_) => {}
    }
    let node = recognize_permanent_scaled_casting_reduction(&mut cursor)?;
    lower_casting_cost_reduction(input, &mut cursor, node)
}

/// `increaseOwnCastingCost`: one copy of the fixed cost for each counted
/// unit, added to the spell's own total cost (CR 601.2f).
fn lower_own_spell_cost_increase(node: &OwnSpellCostIncreaseNode) -> CanonicalRuleDraft {
    let per = match node.unit {
        OwnSpellIncreaseUnit::TargetBeyondFirst => "targetBeyondFirst",
    };
    draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": {"kind": "inZone", "object": self_ref(), "zone": {"kind": "stackOrCast"}},
            "modifiers": [{
                "kind": "increaseOwnCastingCost",
                "manaCost": node.cost.as_oracle_cost(),
                "per": per,
            }],
        }),
        &[
            "Read the own-spell subject and the increase direction of the cost frame",
            "Keep the fixed mana delta as one typed cost",
            "Count the unit on the declared targets when the total cost is determined",
        ],
    )
}

#[cfg(test)]
#[path = "casting_reduction_tests.rs"]
mod tests;
