//! Lowers a spell's additional cost (`automata/additional_cost.rs`) onto the
//! casting declaration: the objects each action names are chosen while
//! casting (CR 601.2h, `chooseObjects`), each action is one primitive cost,
//! alternatives are one announced mode (CR 601.2b) whose branch costs are
//! conditional on it, and an optional cost is a `decline`/`pay` mode the
//! spell's effects may read (`additionalCostMode`).
use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// The decision naming the announced alternative or the optional payment.
pub(in crate::oracle::canonical) const ADDITIONAL_COST_MODE: &str = "additionalCostMode";

pub(in crate::oracle::canonical) fn claims_additional_cost_frame(input: &AbilityInput<'_>) -> bool {
    additional_cost_frame_starts(input.tokens)
}

/// Distinct decision ids for the chosen objects of every action of the
/// frame: the first choice of a kind keeps the kind's plain id, later ones
/// are numbered. Blight keeps its numbered declaration id.
#[derive(Default)]
struct ChoiceIds {
    used: std::collections::BTreeMap<&'static str, usize>,
}

impl ChoiceIds {
    fn next(&mut self, base: &'static str) -> String {
        let used = self.used.entry(base).or_default();
        *used += 1;
        if *used == 1 {
            base.to_string()
        } else {
            format!("{base}{used}")
        }
    }

    fn numbered(&mut self, base: &'static str) -> String {
        let used = self.used.entry(base).or_default();
        *used += 1;
        format!("{base}{used}")
    }
}

/// The engine keys a multi-object choice as `id:index`.
fn chosen_ids(id: &str, count: i64) -> Vec<String> {
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

fn chosen_object(id: &str) -> Value {
    json!({ "kind": "chosenObject", "id": id })
}

fn caster_permanents(object: &ObjectQueryNode) -> Value {
    json!({
        "kind": "permanents",
        "controller": controller(),
        "where": lower_permanent_object_predicate(&object.predicate),
    })
}

fn caster_cards(zone: Value, filter: &ObjectPredicate) -> Value {
    json!({
        "kind": "cards",
        "zone": zone,
        "where": lower_card_object_predicate(filter),
    })
}

fn exactly(count: i64) -> Value {
    json!({ "kind": "exactly", "value": integer(count) })
}

/// The declarations and primitive costs of one action.
fn lower_action(
    action: &AdditionalCostActionNode,
    ids: &mut ChoiceIds,
    decisions: &mut Vec<Value>,
    costs: &mut Vec<Value>,
) {
    match action {
        AdditionalCostActionNode::PayMana(mana) => costs.push(lower_resource_mana_payment(mana)),
        AdditionalCostActionNode::PayLife(life) => costs.push(json!({
            "kind": "payLife",
            "player": controller(),
            "amount": integer(life.amount),
        })),
        AdditionalCostActionNode::Sacrifice { count, object } => {
            let id = ids.next("additionalCostPermanent");
            decisions.push(object_choice(
                &id,
                caster_permanents(object),
                exactly(*count),
            ));
            costs.extend(chosen_ids(&id, *count).into_iter().map(
                |id| json!({ "kind": "sacrificePermanent", "permanent": chosen_object(&id) }),
            ));
        }
        AdditionalCostActionNode::Discard { count, filter } => {
            let id = ids.next("additionalCostCard");
            decisions.push(object_choice(
                &id,
                caster_cards(hand(controller()), filter),
                exactly(*count),
            ));
            costs.extend(
                chosen_ids(&id, *count)
                    .into_iter()
                    .map(|id| json!({ "kind": "discardCard", "card": chosen_object(&id) })),
            );
        }
        AdditionalCostActionNode::ExileCards {
            count,
            filter,
            zone,
        } => {
            let id = ids.next("additionalCostExile");
            let zone = json!({
                "kind": match zone {
                    PaymentOriginZone::Hand => "hand",
                    PaymentOriginZone::Graveyard => "graveyard",
                },
                "player": controller(),
            });
            decisions.push(object_choice(
                &id,
                caster_cards(zone, filter),
                exactly(*count),
            ));
            costs.extend(
                chosen_ids(&id, *count)
                    .into_iter()
                    .map(|id| json!({ "kind": "exileObject", "object": chosen_object(&id) })),
            );
        }
        AdditionalCostActionNode::Tap { count, object } => {
            let id = ids.next("additionalCostTap");
            decisions.push(object_choice(
                &id,
                caster_permanents(object),
                exactly(*count),
            ));
            costs.extend(
                chosen_ids(&id, *count)
                    .into_iter()
                    .map(|id| json!({ "kind": "tap", "object": chosen_object(&id) })),
            );
        }
        AdditionalCostActionNode::ReturnToHand { count, object } => {
            let id = ids.next("additionalCostReturn");
            decisions.push(object_choice(
                &id,
                caster_permanents(object),
                exactly(*count),
            ));
            costs.extend(chosen_ids(&id, *count).into_iter().map(|id| {
                json!({ "kind": "returnPermanentToOwnersHand", "permanent": chosen_object(&id) })
            }));
        }
        AdditionalCostActionNode::Reveal { count, filter } => {
            let id = ids.next("additionalCostReveal");
            decisions.push(object_choice(
                &id,
                caster_cards(hand(controller()), filter),
                exactly(*count),
            ));
            costs.extend(
                chosen_ids(&id, *count)
                    .into_iter()
                    .map(|id| json!({ "kind": "revealCard", "card": chosen_object(&id) })),
            );
        }
        AdditionalCostActionNode::PutCounters {
            counter,
            count,
            object,
        } => {
            let id = ids.next("additionalCostCounterHolder");
            decisions.push(object_choice(&id, caster_permanents(object), exactly(1)));
            costs.push(json!({
                "kind": "putCounters",
                "permanent": chosen_object(&id),
                "controller": controller(),
                "counter": counter.as_str(),
                "count": integer(*count),
            }));
        }
        // Blight N (CR 701.65a): N -1/-1 counters on one creature the
        // caster controls, chosen while casting.
        AdditionalCostActionNode::Blight { amount } => {
            let id = ids.numbered("blightCost");
            decisions.push(target_decision(
                &id,
                json!({
                    "kind": "permanents",
                    "controller": controller(),
                    "where": card_type("Creature"),
                    "ignoreTargetingRestrictions": true,
                }),
                1,
                1,
            ));
            costs.push(json!({
                "kind": "putCounters",
                "permanent": chosen_target(&id),
                "controller": controller(),
                "counter": "-1/-1",
                "count": integer(*amount),
            }));
        }
    }
}

fn conditional_cost(condition: Value, costs: Vec<Value>) -> Value {
    json!({
        "kind": "conditional",
        "condition": condition,
        "then": costs,
        "else": [],
    })
}

/// The mode name of each branch: its first action's kind, numbered when two
/// branches start with the same kind (`sacrifice a creature or sacrifice a
/// modified creature`).
fn branch_modes(frame: &AdditionalCostFrameNode) -> Vec<String> {
    let mut seen = std::collections::BTreeMap::<&'static str, usize>::new();
    frame
        .branches
        .iter()
        .map(|branch| {
            let base = branch
                .actions
                .first()
                .map_or("pay", AdditionalCostActionNode::mode_name);
            let used = seen.entry(base).or_default();
            *used += 1;
            if *used == 1 {
                base.to_string()
            } else {
                format!("{base}{used}")
            }
        })
        .collect()
}

pub(in crate::oracle::canonical) fn parse_additional_cost_frame(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("additional-cost", input.tokens);
    let frame = recognize_additional_cost_frame(&mut cursor)?;
    let mut ids = ChoiceIds::default();
    let mut decisions = Vec::new();
    let mut costs = Vec::new();
    let (options, conditioned) = match (frame.optional, frame.branches.as_slice()) {
        // A mandatory cost without alternatives: every action is paid.
        (false, [branch]) => {
            for action in &branch.actions {
                lower_action(action, &mut ids, &mut decisions, &mut costs);
            }
            (None, false)
        }
        _ => {
            let options = if frame.optional {
                vec!["decline".to_string(), "pay".to_string()]
            } else {
                branch_modes(&frame)
            };
            let paying = if frame.optional {
                vec!["pay".to_string()]
            } else {
                options.clone()
            };
            for (branch, mode) in frame.branches.iter().zip(&paying) {
                let condition = selection(ADDITIONAL_COST_MODE, mode);
                let mut branch_decisions = Vec::new();
                let mut branch_costs = Vec::new();
                for action in &branch.actions {
                    lower_action(action, &mut ids, &mut branch_decisions, &mut branch_costs);
                }
                // A choice belongs to the branch that pays with it: it is
                // declared only when that branch is announced.
                for mut decision in branch_decisions {
                    decision["condition"] = condition.clone();
                    decisions.push(decision);
                }
                costs.push(conditional_cost(condition, branch_costs));
            }
            (Some(options), true)
        }
    };
    if let Some(options) = options {
        decisions.insert(
            0,
            json!({
                "id": ADDITIONAL_COST_MODE,
                "kind": "chooseModes",
                "minimum": 1,
                "maximum": 1,
                "options": options,
            }),
        );
    }
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": decisions,
                "additionalCosts": costs,
            },
            "effects": [],
        }),
        &[
            "Recognize the shared additional-cost header from one token stream",
            "Read every action with the shared payment-action reader",
            if conditioned {
                "Announce one alternative or the optional payment as a casting mode (CR 601.2b)"
            } else {
                "Pay every action of the one mandatory cost (CR 601.2h)"
            },
            "Choose each cost object while casting and pay one primitive cost per object",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "additional_cost_native_tests.rs"]
mod tests;
