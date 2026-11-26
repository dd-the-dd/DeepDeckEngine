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
    // A spell population, possibly narrowed to one spell of the caster's
    // turn history (`the second spell you cast each turn`).
    let (population, ordinal) = match node.subject {
        CastingReductionSubject::CurrentSpell => (None, None),
        CastingReductionSubject::SpellSet(spells) => (Some(spells), None),
        CastingReductionSubject::OrdinalSpell(subject) => (
            Some(subject.spells),
            Some((subject.ordinal, subject.own_turns_only)),
        ),
    };
    let rule = match population {
        None => {
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
        Some(spells) => {
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
            if let Some((ordinal, own_turns_only)) = ordinal {
                lower_spell_ordinal(cursor, &mut modifier, ordinal, own_turns_only)?;
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

/// `the first|second SPELL you cast each turn` (CR 601.2i): the first is the
/// engine's `firstEachTurn` record; a later ordinal counts the caster's
/// earlier matching casts this turn (`castOrdinal`). `during each of your
/// turns` adds the proposition that it is the controller's turn (CR 500.1).
fn lower_spell_ordinal(
    cursor: &mut OracleTokenCursor<'_>,
    modifier: &mut Value,
    ordinal: i64,
    own_turns_only: bool,
) -> MachineResult<()> {
    cursor.enter("static.spell-cost-reduction.ordinal");
    cursor.require(
        (1..=10).contains(&ordinal),
        "a spell ordinal between first and tenth",
    )?;
    if ordinal == 1 {
        modifier["firstEachTurn"] = json!(true);
    } else {
        modifier["castOrdinal"] = json!(ordinal);
    }
    if own_turns_only {
        let your_turn = lower_sentence_condition(
            cursor,
            &BooleanConditionNode::Atom(ConditionAtom::TurnOwnership(
                ControllerTurnPropositionNode {
                    is_controller_turn: true,
                },
            )),
            &ConditionObjectBindings {
                permanent_targets: &[],
                card_targets: &[],
                card_focus: None,
                previous_action_result: None,
            },
        )?;
        modifier["condition"] = match modifier.get("condition") {
            Some(existing) => and(vec![your_turn, existing.clone()]),
            None => your_turn,
        };
    }
    Ok(())
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
