use super::super::*;

/// Declares one numeric producer for the shared X value in an activation's
/// typed mana costs. Multiple X symbols share one chosen value; each symbol
/// still contributes independently to the mana payment.
pub(in crate::oracle::canonical) fn lower_activation_cost_decisions(
    costs: &ActivationCostSequenceNode,
) -> Vec<Value> {
    let mut decisions = lower_activation_numeric_cost_decisions(costs);
    decisions.extend(lower_activation_object_cost_decisions(costs));
    decisions
}

fn lower_activation_numeric_cost_decisions(costs: &ActivationCostSequenceNode) -> Vec<Value> {
    let mut decisions = Vec::new();
    // One announced X for every `{X}` symbol and X quantity slot.
    if costs.declares_x() {
        decisions.push(x_value());
    }
    decisions
}

fn activation_cost_object_id(cost: &SelectedSacrificeCostNode) -> String {
    format!("activationCostPermanent{}", cost.cost_index)
}

fn activation_cost_tap_id(cost: &ChosenTapCostNode) -> String {
    format!("activationCostTap{}", cost.cost_index)
}

fn activation_cost_card_id(cost: &ChosenDiscardCostNode) -> String {
    format!("activationCostCard{}", cost.cost_index)
}

fn activation_cost_exile_id(cost: &ChosenExileCostNode) -> String {
    format!("activationCostExile{}", cost.cost_index)
}

fn activation_cost_counter_id(cost: &SourceCounterRemovalCostNode) -> String {
    format!("activationCostCounter{}", cost.cost_index)
}

fn activation_cost_return_id(cost: &ChosenReturnCostNode) -> String {
    format!("activationCostReturn{}", cost.cost_index)
}

fn activation_cost_counter_holder_id(cost: &ChosenCounterPlacementCostNode) -> String {
    format!("activationCostCounterHolder{}", cost.cost_index)
}

/// The engine keys a multi-object choice as `id:index`.
fn chosen_object_ids(id: &str, count: i64) -> Vec<String> {
    (0..count)
        .map(|index| {
            if count == 1 {
                id.to_string()
            } else {
                format!("{id}:{index}")
            }
        })
        .collect()
}

/// `another`: the engine removes the ability's source from the candidates.
fn with_source_exclusion(mut candidates: Value, exclude_source: bool) -> Value {
    if exclude_source {
        crate::oracle::canonical::automata::mark_other_target_slot(&mut candidates);
    }
    candidates
}

fn payment_zone(zone: PaymentOriginZone) -> Value {
    json!({
        "kind": match zone {
            PaymentOriginZone::Hand => "hand",
            PaymentOriginZone::Graveyard => "graveyard",
        },
        "player": controller(),
    })
}

fn lower_activation_object_cost_decisions(costs: &ActivationCostSequenceNode) -> Vec<Value> {
    costs
        .costs
        .iter()
        .filter_map(|cost| match cost {
            LocalActivationCostNode::SacrificeObject(cost) => Some(object_choice(
                &activation_cost_object_id(cost),
                with_source_exclusion(
                    json!({
                        "kind": "permanents",
                        "controller": controller(),
                        "where": lower_permanent_object_predicate(&cost.object.predicate),
                    }),
                    cost.exclude_source,
                ),
                json!({ "kind": "exactly", "value": integer(cost.count) }),
            )),
            LocalActivationCostNode::TapObjects(cost) => Some(object_choice(
                &activation_cost_tap_id(cost),
                with_source_exclusion(
                    json!({
                        "kind": "permanents",
                        "controller": controller(),
                        "where": lower_permanent_object_predicate(&cost.tap.object.predicate),
                    }),
                    cost.tap.exclude_source,
                ),
                json!({ "kind": "exactly", "value": integer(cost.tap.count) }),
            )),
            LocalActivationCostNode::ExileCards(cost) => Some(object_choice(
                &activation_cost_exile_id(cost),
                with_source_exclusion(
                    json!({
                        "kind": "cards",
                        "zone": payment_zone(cost.zone),
                        "where": lower_card_object_predicate(&cost.filter),
                    }),
                    cost.exclude_source,
                ),
                json!({ "kind": "exactly", "value": integer(cost.count) }),
            )),
            LocalActivationCostNode::ReturnObjects(cost) => Some(object_choice(
                &activation_cost_return_id(cost),
                with_source_exclusion(
                    json!({
                        "kind": "permanents",
                        "controller": controller(),
                        "where": lower_permanent_object_predicate(&cost.object.predicate),
                    }),
                    cost.exclude_source,
                ),
                json!({ "kind": "exactly", "value": integer(cost.count) }),
            )),
            LocalActivationCostNode::PutChosenCounters(cost) => Some(object_choice(
                &activation_cost_counter_holder_id(cost),
                json!({
                    "kind": "permanents",
                    "controller": controller(),
                    "where": lower_permanent_object_predicate(&cost.object.predicate),
                }),
                json!({ "kind": "exactly", "value": integer(1) }),
            )),
            // A random discard is selected by the engine while paying
            // (CR 701.9b): the payer declares no card.
            LocalActivationCostNode::DiscardCard(cost)
                if cost.method == CollectionSelectionMethod::Choice =>
            {
                Some(object_choice(
                    &activation_cost_card_id(cost),
                    json!({
                        "kind": "cards",
                        "zone": hand(controller()),
                        "where": lower_card_object_predicate(&cost.filter),
                    }),
                    json!({ "kind": "exactly", "value": integer(cost.count) }),
                ))
            }
            _ => None,
        })
        .collect()
}

pub(in crate::oracle::canonical) fn with_activation_object_cost_source_context(
    mut rule: Value,
    costs: &ActivationCostSequenceNode,
) -> Value {
    if costs
        .costs
        .iter()
        .any(|cost| matches!(cost, LocalActivationCostNode::SacrificeObject(_)))
        && costs.source_zone_requirements().hand
    {
        rule["activationZone"] = json!("hand");
    }
    // An exile-self cost names the zone the source is paid from, so the
    // ability can only be activated there, whichever parent lowered it.
    if costs.source_zone_requirements().graveyard {
        rule["activationZone"] = json!("graveyard");
    }
    rule
}

/// Existing specialized activation parents keep their declaration shape when
/// no selected cost exists. New object choices follow every existing numeric
/// producer and its preceding dependencies, preserving the order and payload
/// of all preexisting decisions. Typed mana-X costs supply a missing producer;
/// an existing declaration with the same id is retained rather than duplicated.
pub(in crate::oracle::canonical) fn with_activation_object_cost_decisions(
    mut rule: Value,
    costs: &ActivationCostSequenceNode,
) -> Value {
    let objects = lower_activation_object_cost_decisions(costs);
    if objects.is_empty() {
        return with_activation_object_cost_source_context(rule, costs);
    }
    let existing = rule["declaration"]["decisions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut decisions = lower_activation_numeric_cost_decisions(costs);
    decisions.retain(|producer| {
        !existing.iter().any(|decision| {
            decision["kind"] == producer["kind"] && decision["id"] == producer["id"]
        })
    });
    let insertion = existing
        .iter()
        .rposition(|decision| decision["kind"].as_str() == Some("chooseNumber"))
        .map_or(0, |position| position + 1);
    decisions.extend(existing[..insertion].iter().cloned());
    decisions.extend(objects);
    decisions.extend(existing[insertion..].iter().cloned());
    rule["declaration"]["kind"] = json!("castingDeclaration");
    rule["declaration"]["decisions"] = json!(decisions);
    with_activation_object_cost_source_context(rule, costs)
}

/// One activation cost can lower to several payments: tapping N chosen
/// permanents is N independent `tap` costs, one per declared choice.
pub(in crate::oracle::canonical) fn lower_local_activation_costs(
    cost: &LocalActivationCostNode,
) -> Vec<Value> {
    match cost {
        LocalActivationCostNode::TapObjects(cost) => {
            let id = activation_cost_tap_id(cost);
            (0..cost.tap.count)
                .map(|index| {
                    // The engine keys a multi-object choice as `id:index`.
                    let object_id = if cost.tap.count == 1 {
                        id.clone()
                    } else {
                        format!("{id}:{index}")
                    };
                    json!({
                        "kind": "tap",
                        "object": { "kind": "chosenTarget", "id": object_id },
                    })
                })
                .collect()
        }
        // One sacrifice per chosen permanent, each naming its own choice.
        LocalActivationCostNode::SacrificeObject(cost) => {
            chosen_object_ids(&activation_cost_object_id(cost), cost.count)
                .into_iter()
                .map(|id| {
                    json!({
                        "kind": "sacrificePermanent",
                        "permanent": { "kind": "chosenObject", "id": id },
                    })
                })
                .collect()
        }
        // One discard per chosen card; a random discard is one counted
        // payment whose cards the engine selects while paying.
        LocalActivationCostNode::DiscardCard(cost) => match cost.method {
            CollectionSelectionMethod::Choice => {
                chosen_object_ids(&activation_cost_card_id(cost), cost.count)
                    .into_iter()
                    .map(|id| {
                        json!({
                            "kind": "discardCard",
                            "card": { "kind": "chosenObject", "id": id },
                        })
                    })
                    .collect()
            }
            CollectionSelectionMethod::Random => vec![json!({
                "kind": "discardCards",
                "player": controller(),
                "count": integer(cost.count),
                "where": lower_card_object_predicate(&cost.filter),
                "selection": { "kind": "random" },
            })],
        },
        // One return per chosen permanent, each naming its own choice.
        LocalActivationCostNode::ReturnObjects(cost) => {
            chosen_object_ids(&activation_cost_return_id(cost), cost.count)
                .into_iter()
                .map(|id| {
                    json!({
                        "kind": "returnPermanentToOwnersHand",
                        "permanent": { "kind": "chosenObject", "id": id },
                    })
                })
                .collect()
        }
        other => vec![lower_local_activation_cost(other)],
    }
}

pub(in crate::oracle::canonical) fn lower_local_activation_cost(
    cost: &LocalActivationCostNode,
) -> Value {
    match cost {
        LocalActivationCostNode::PayMana(mana) => lower_resource_mana_payment(mana),
        LocalActivationCostNode::TapSelf(_) => json!({
            "kind": "tap",
            "object": self_ref(),
        }),
        LocalActivationCostNode::PayLife(payment) => {
            lower_resource_life_payment(payment, ResourcePaymentPayer::ActivationController)
        }
        LocalActivationCostNode::PayPlayerCounters(payment) => {
            lower_resource_player_counter_payment(
                payment,
                Some(ResourcePaymentPayer::ActivationController),
            )
        }
        LocalActivationCostNode::SacrificeSelf(_) => json!({
            "kind": "sacrificePermanent",
            "permanent": self_ref(),
        }),
        LocalActivationCostNode::DiscardSelf(_) => json!({
            "kind": "discardCard",
            "card": self_ref(),
        }),
        LocalActivationCostNode::ExileSelf(exile) => json!({
            "kind": "exileSource",
            "object": self_ref(),
            "zone": match exile.source_zone {
                ExileSelfSourceZone::Graveyard => "graveyard",
                ExileSelfSourceZone::Battlefield => "battlefield",
            },
        }),
        LocalActivationCostNode::TapObjects(_)
        | LocalActivationCostNode::ReturnObjects(_)
        | LocalActivationCostNode::SacrificeObject(_)
        | LocalActivationCostNode::DiscardCard(_) => {
            unreachable!("a chosen multi-object cost lowers through lower_local_activation_costs")
        }
        LocalActivationCostNode::ReturnSelf => json!({
            "kind": "returnPermanentToOwnersHand",
            "permanent": self_ref(),
        }),
        LocalActivationCostNode::PutSourceCounters(cost) => json!({
            "kind": "putCountersOnSource",
            "counter": cost.counter.as_str(),
            "count": integer(cost.count),
        }),
        LocalActivationCostNode::PutChosenCounters(cost) => json!({
            "kind": "putCounters",
            "permanent": { "kind": "chosenObject", "id": activation_cost_counter_holder_id(cost) },
            "counter": cost.counter.as_str(),
            "count": integer(cost.count),
        }),
        // An unnamed counter kind is declared while paying: the engine
        // expands the decision from the source's counters (CR 602.2b).
        LocalActivationCostNode::RemoveSourceCounters(cost) => json!({
            "kind": "removeCounters",
            "permanent": self_ref(),
            "counter": cost.counter.as_ref().map_or_else(
                || json!({ "kind": "decisionResult", "decisionId": activation_cost_counter_id(cost) }),
                |counter| json!(counter.as_str()),
            ),
            // A literal or this activation's chosen X (`xValue`).
            "count": lower_numeric_atom(cost.count),
        }),
        LocalActivationCostNode::RemoveControlledCounters(cost) => {
            let mut value = json!({
                "kind": "removeCountersFromControlledPermanents",
                "count": integer(cost.count),
            });
            match &cost.holder {
                CounterDistributionHolder::Source => value["permanent"] = self_ref(),
                CounterDistributionHolder::Permanents(object) => {
                    value["where"] = lower_permanent_object_predicate(&object.predicate)
                }
            }
            // Without a named kind, each removed counter may be of any kind.
            if let Some(counter) = &cost.counter {
                value["counter"] = json!(counter.as_str());
            }
            value
        }
        LocalActivationCostNode::ExileCards(cost) => json!({
            "kind": "move",
            "objects": { "kind": "chosenObjects", "id": activation_cost_exile_id(cost) },
            "from": payment_zone(cost.zone),
            "to": { "kind": "exile" },
        }),
    }
}

#[cfg(test)]
#[path = "activation_cost_decisions_tests.rs"]
mod activation_cost_decisions_tests;

#[cfg(test)]
#[path = "compound_zone_claim_tests.rs"]
mod compound_zone_claim_tests;

#[cfg(test)]
#[path = "activation_discard_cost_tests.rs"]
mod activation_discard_cost_tests;

#[cfg(test)]
#[path = "activation_graveyard_exile_cost_tests.rs"]
mod activation_graveyard_exile_cost_tests;

#[cfg(test)]
#[path = "activation_tap_cost_tests.rs"]
mod activation_tap_cost_tests;
