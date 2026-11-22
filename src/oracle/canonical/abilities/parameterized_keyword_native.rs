//! Lowers a typed parameterized keyword (CR 702) into ordinary primitives.
//!
//! The keyword's definition is expanded with the printed parameter into the
//! rule kinds the engine already executes for printed text: a triggered
//! ability, a casting-time optional additional cost, a casting-cost static
//! ability, or an intrinsic keyword whose runtime the engine already owns.
//! No keyword introduces a new operation; an unsupported parameter fails.
//!
//! A keyword whose CR definition the ordinary parser compiles to the same
//! executable rules belongs to the template catalog instead
//! (`keyword-templates-v1.json`, `keyword_template_expansion.rs`). The
//! keywords left here either have an intrinsic engine rule (toxic,
//! annihilator) or a definition the shared grammar does not compile yet;
//! `docs/oracle-keyword-templates.md` lists the reason for each.
use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_parameterized_keyword_family(
    input: &AbilityInput<'_>,
) -> bool {
    parameterized_keyword_starts(input.tokens).is_some()
}

fn count_parameter(
    cursor: &OracleTokenCursor<'_>,
    node: &ParameterizedKeywordNode,
) -> MachineResult<i64> {
    match node.parameter {
        KeywordParameterNode::Count(value) => Ok(value),
        _ => Err(cursor.fail("a positive keyword count")),
    }
}

fn cost_parameter<'a>(
    cursor: &OracleTokenCursor<'_>,
    node: &'a ParameterizedKeywordNode,
) -> MachineResult<&'a ResourcePaymentNode> {
    match &node.parameter {
        KeywordParameterNode::Cost(cost) => Ok(cost),
        _ => Err(cursor.fail("a keyword cost")),
    }
}

fn keyword_ability(ability: Value) -> Value {
    json!({ "kind": "keywordAbility", "source": self_ref(), "ability": ability })
}

/// CR 509.3c: this creature becomes blocked (once per combat).
fn became_blocked() -> Value {
    json!({ "kind": "permanentBecameBlocked", "object": self_ref() })
}

/// CR 702.130a: whenever this creature becomes blocked, defending player
/// loses N life. The engine binds the defending player as the triggering one.
fn afflict_rule(count: i64) -> Value {
    json!({
        "kind": "triggeredAbility",
        "source": self_ref(),
        "event": became_blocked(),
        "effects": [{
            "kind": "loseLife",
            "player": { "kind": "triggeringPlayer" },
            "amount": integer(count),
        }],
    })
}

/// CR 702.24a: put an age counter on this permanent, then pay the cost once
/// for each age counter on it or sacrifice it. A mana cost is repeated per age
/// counter; a life payment is multiplied by their number.
fn cumulative_upkeep_rule(
    cursor: &OracleTokenCursor<'_>,
    cost: &ResourcePaymentNode,
) -> MachineResult<Value> {
    let age_counters = json!({ "kind": "countCounters", "object": self_ref(), "counter": "age" });
    let payment = match cost {
        ResourcePaymentNode::Mana(mana) => json!({
            "kind": "payMana",
            "manaCost": mana.as_oracle_cost(),
            "times": age_counters,
        }),
        ResourcePaymentNode::Life(life) => {
            cursor.require(life.amount > 0, "a positive cumulative upkeep life payment")?;
            let amount = if life.amount == 1 {
                age_counters
            } else {
                json!({ "kind": "multiply", "left": integer(life.amount), "right": age_counters })
            };
            json!({ "kind": "payLife", "player": controller(), "amount": amount })
        }
    };
    let age_counter = json!({
        "kind": "putCounters",
        "permanent": self_ref(),
        "counter": "age",
        "count": integer(1),
    });
    Ok(upkeep_payment_rule(
        None,
        vec![age_counter],
        "cumulativeUpkeepPayment",
        payment,
    ))
}

/// The shared upkeep program of CR 702.24a and 702.30a: optional effects,
/// then the controller may pay the cost; if it is not paid, sacrifice this.
fn upkeep_payment_rule(
    condition: Option<Value>,
    mut effects: Vec<Value>,
    payment_id: &str,
    payment: Value,
) -> Value {
    effects.push(json!({
        "kind": "tryPayCost",
        "id": payment_id,
        "player": controller(),
        "cost": payment,
    }));
    effects.push(json!({
        "kind": "conditional",
        "condition": {
            "kind": "costWasPaid",
            "decisionId": payment_id,
            "required": true,
            "value": false,
        },
        "then": [{ "kind": "sacrificePermanent", "permanent": self_ref() }],
        "else": [],
    }));
    let mut rule = json!({
        "kind": "triggeredAbility",
        "source": self_ref(),
        "event": { "kind": "stepBegan", "player": controller(), "step": "upkeep" },
        "effects": effects,
    });
    if let Some(condition) = condition {
        rule["condition"] = condition;
    }
    rule
}

/// CR 702.30a: at the beginning of your upkeep, if this came under your
/// control since the beginning of your last upkeep, sacrifice it unless you
/// pay [cost]. The intervening condition is the engine's control-history fact.
fn echo_rule(cost: &ResourcePaymentNode) -> Value {
    upkeep_payment_rule(
        Some(json!({ "kind": "sourceNeedsEcho" })),
        Vec::new(),
        "echoPayment",
        lower_resource_payment(cost, ResourcePaymentPayer::ActivationController),
    )
}

/// CR 702.27a: an optional additional cost; the engine returns the spell to
/// its owner's hand when the `buyback` mode was chosen (CR 702.27a).
fn buyback_rule(cost: &ResourcePaymentNode) -> Value {
    let paid = selection("buybackMode", "buyback");
    json!({
        "kind": "spellAbility",
        "source": self_ref(),
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "buybackMode",
                "kind": "chooseModes",
                "minimum": 1,
                "maximum": 1,
                "options": ["decline", "buyback"],
            }],
            "additionalCosts": [{
                "kind": "conditional",
                "condition": paid,
                "then": [lower_resource_payment(cost, ResourcePaymentPayer::ActivationController)],
                "else": [],
            }],
        },
        "effects": [],
    })
}

/// CR 702.136a: you may have this permanent enter with an additional +1/+1
/// counter on it; if you don't, it gains haste. One typed entry replacement
/// choice applied by the engine's shared Riot choice.
fn riot_rule() -> Value {
    json!({
        "kind": "replacementEffect",
        "source": self_ref(),
        "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
        "decisions": [{
            "id": "riotChoice",
            "kind": "chooseOption",
            "options": ["+1/+1 counter", "haste"],
        }],
        "replacement": [{ "kind": "applyRiotChoice", "decisionId": "riotChoice" }],
    })
}

/// CR 702.74a: an alternative cost; the engine's evoke mode records that it
/// was paid and sacrifices the permanent when it enters.
fn evoke_ability(cost: &ResourcePaymentNode) -> Value {
    json!({
        "kind": "alternativeCost",
        "mode": "evoke",
        "costs": [lower_resource_payment(cost, ResourcePaymentPayer::ActivationController)],
    })
}

/// CR 702.175a: an optional additional cost and an entry trigger creating a
/// 1/1 token copy when that cost was paid.
fn offspring_ability(cost: &ResourcePaymentNode) -> Value {
    json!({
        "kind": "offspring",
        "cost": lower_resource_payment(cost, ResourcePaymentPayer::ActivationController),
        "repeatable": false,
        "trigger": {
            "event": { "kind": "enterBattlefield", "object": self_ref() },
            "condition": { "kind": "offspringCostWasPaid", "spell": self_ref() },
            "effects": [{
                "kind": "createTokenCopy",
                "controller": controller(),
                "object": self_ref(),
                "basePower": integer(1),
                "baseToughness": integer(1),
            }],
        },
    })
}

pub(in crate::oracle::canonical) fn parse_parameterized_keyword(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("parameterized-keyword", input.tokens);
    let node = recognize_parameterized_keyword(&mut cursor)?;
    cursor.enter("keyword.parameterized.lowering");
    let rule = match node.keyword {
        ParameterizedKeyword::Toxic => keyword_ability(json!({
            "kind": "toxic",
            "count": integer(count_parameter(&cursor, &node)?),
        })),
        ParameterizedKeyword::Annihilator => keyword_ability(json!({
            "kind": "annihilator",
            "quantity": integer(count_parameter(&cursor, &node)?),
        })),
        ParameterizedKeyword::Riot => riot_rule(),
        ParameterizedKeyword::Afflict => afflict_rule(count_parameter(&cursor, &node)?),
        ParameterizedKeyword::CumulativeUpkeep => {
            cumulative_upkeep_rule(&cursor, cost_parameter(&cursor, &node)?)?
        }
        ParameterizedKeyword::Buyback => buyback_rule(cost_parameter(&cursor, &node)?),
        ParameterizedKeyword::Echo => echo_rule(cost_parameter(&cursor, &node)?),
        ParameterizedKeyword::Evoke => {
            keyword_ability(evoke_ability(cost_parameter(&cursor, &node)?))
        }
        ParameterizedKeyword::Offspring => {
            keyword_ability(offspring_ability(cost_parameter(&cursor, &node)?))
        }
        ParameterizedKeyword::Affinity => {
            let KeywordParameterNode::Quality(quality) = &node.parameter else {
                return Err(cursor.fail("an affinity quality"));
            };
            let reduction = affinity_reduction(quality);
            return lower_casting_cost_reduction(input, &mut cursor, reduction).map(|mut draft| {
                draft.operations.insert(
                    0,
                    format!(
                        "Expand Affinity (CR {}) to the shared casting-cost reduction",
                        node.keyword.rules_section()
                    ),
                );
                draft
            });
        }
    };
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    let expansion = format!(
        "Expand {} (CR {}) with its printed parameter into ordinary primitives",
        node.keyword.canonical_name(),
        node.keyword.rules_section()
    );
    Ok(draft(
        rule,
        &[
            "Recognize the keyword lexeme from the shared token stream",
            "Read its parameter with the shared count, cost or collection child",
            "Validate an optional reminder against the printed parameter",
            &expansion,
            &trace,
        ],
    ))
}
