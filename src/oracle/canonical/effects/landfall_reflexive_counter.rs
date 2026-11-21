use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use serde_json::{Value, json};

use super::super::*;
use crate::oracle::syntax::AbilityInput;

const PLACED_COUNTERS_BINDING: &str = "antecedentCountersPlaced";
const REFLEXIVE_TARGET_ID: &str = "targetPermanent";

/// Claims the structural envelope without naming an ability word, permanent
/// type, counter, threshold, target predicate, or granted keyword. Once the
/// label, controlled-entry head, counter-placement head, and reflexive clause
/// are present, malformed tails remain committed native failures.
pub(in crate::oracle::canonical) fn claims_promoted_labeled_entry_reflexive_counter_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    matches!(tokens, [label, separator, trigger, article, ..]
        if label.token_type == "abilityWord"
            && separator.token_type == "separator"
            && separator.raw == "\u{2014}"
            && trigger.token_type == "triggerWord"
            && trigger.raw.eq_ignore_ascii_case("whenever")
            && (article.raw.eq_ignore_ascii_case("a")
                || article.raw.eq_ignore_ascii_case("an")))
        && tokens.windows(2).any(|window| {
            window[0].raw == "," && is_verb_form(&window[1].raw, VerbLemma::Put, VerbForm::Base)
        })
        && tokens
            .windows(2)
            .any(|window| window[0].raw == "." && window[1].raw.eq_ignore_ascii_case("when"))
        && tokens
            .windows(2)
            .any(|window| window[0].raw == "," && window[1].raw.eq_ignore_ascii_case("if"))
        && tokens.last().is_some_and(|token| token.raw == ".")
}

fn comparison_operator(operator: NumericComparisonOperator) -> &'static str {
    match operator {
        NumericComparisonOperator::Equal => "==",
        NumericComparisonOperator::LessThanOrEqual => "<=",
        NumericComparisonOperator::GreaterThanOrEqual => ">=",
    }
}

fn lower_duration(duration: &EffectDurationNode) -> Value {
    match duration {
        EffectDurationNode::UntilEndOfCurrentTurn => {
            json!({ "kind": "untilEndOfCurrentTurn" })
        }
        EffectDurationNode::UntilNextTurn {
            player: LocalPlayerReference::Controller,
        } => json!({
            "kind": "untilNextTurn",
            "player": controller(),
        }),
        EffectDurationNode::UntilNextTurn {
            player:
                LocalPlayerReference::ControllerOf(_) | LocalPlayerReference::ControllerOfQualified(_),
        } => unreachable!("the bound keyword grant has no player-target duration"),
        EffectDurationNode::UntilEndOfNextTurn { .. }
        | EffectDurationNode::WhileSourceControlled => {
            unreachable!("the bound keyword grant reads only the continuous-effect durations")
        }
    }
}

/// Lowers the typed composition into independent generic primitives. The
/// antecedent placement exports its actual placed quantity so `When you do`
/// can suppress reflexive creation when replacement effects prevent the
/// action. The nested condition is retained on the reflexive ability for the
/// intervening-if checks at creation and resolution.
pub(in crate::oracle::canonical) fn parse_promoted_labeled_entry_reflexive_counter(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor =
        OracleTokenCursor::new("labeled-controlled-entry-reflexive-counter", input.tokens);
    let node = recognize_labeled_controlled_entry_reflexive_counter(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;

    let [slot] = node.followup.placement.targets.slots.as_slice() else {
        return Err(cursor.fail("one reflexive target slot"));
    };
    cursor.require(
        node.followup.grant.object.effect_index == 0 && node.followup.grant.object.slot_index == 0,
        "the targeted counter placement as the keyword-grant antecedent",
    )?;
    let (_, mut candidates) = lower_target_selector_candidates(&slot.selector)
        .ok_or_else(|| cursor.fail("a permanent target selector"))?;
    if slot.exclude_source {
        candidates["excludeSource"] = Value::Bool(true);
    }

    let source_counter_condition = compare(
        comparison_operator(node.followup.condition.operator),
        json!({
            "kind": "countCounters",
            "object": self_ref(),
            "counter": node.followup.condition.counter.as_str(),
        }),
        lower_numeric_atom(node.followup.condition.threshold.clone()),
    );
    let target = chosen_target(REFLEXIVE_TARGET_ID);
    let mut reflexive_effects = vec![json!({
        "kind": "putCounters",
        "permanent": target.clone(),
        "counter": node.followup.placement.counter.as_str(),
        "count": lower_numeric_atom(node.followup.placement.count.clone()),
    })];
    let duration = lower_duration(&node.followup.grant.duration);
    reflexive_effects.extend(node.followup.grant.keywords.iter().map(|keyword| {
        json!({
            "kind": "grantKeyword",
            "object": target.clone(),
            "keyword": keyword.canonical_name(),
            "duration": duration.clone(),
        })
    }));

    let reflexive_ability = json!({
        "kind": "triggeredAbility",
        "source": self_ref(),
        "event": {
            "kind": "reflexiveTriggerCreated",
            "object": self_ref(),
        },
        "condition": source_counter_condition,
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [target_decision(
                REFLEXIVE_TARGET_ID,
                candidates,
                i64::from(slot.cardinality.minimum),
                i64::from(slot.cardinality.maximum),
            )],
        },
        "effects": reflexive_effects,
    });

    let antecedent = json!({
        "kind": "putCounters",
        "permanent": self_ref(),
        "counter": node.antecedent.counter.as_str(),
        "count": lower_numeric_expression(&node.antecedent.count)
            .ok_or_else(|| cursor.fail("a lowerable source counter quantity"))?,
        "bind": PLACED_COUNTERS_BINDING,
    });
    let create_reflexive = json!({
        "kind": "createReflexiveTrigger",
        "source": self_ref(),
        "controller": controller(),
        "ability": reflexive_ability,
    });
    let only_when_antecedent_performed = json!({
        "kind": "conditionalEffect",
        "condition": compare(
            ">",
            json!({ "kind": "boundValue", "id": PLACED_COUNTERS_BINDING }),
            integer(0),
        ),
        "then": [create_reflexive],
        "else": [],
    });

    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": {
                "kind": "permanentEntered",
                "player": controller(),
                "where": lower_permanent_object_predicate(&node.event.object),
            },
            "effects": [antecedent, only_when_antecedent_performed],
        }),
        &[
            "Recognize an ability-word marker without assigning rules meaning",
            "Recognize a controller-scoped permanent-entry event",
            "Bind the actual result of an opaque source-counter placement",
            "Create a separately targeted reflexive ability only when the antecedent occurs",
            "Retain the source counter threshold as an intervening-if condition",
            "Lower target counters and temporary keyword grants as independent primitives",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "landfall_reflexive_counter_tests.rs"]
mod tests;
