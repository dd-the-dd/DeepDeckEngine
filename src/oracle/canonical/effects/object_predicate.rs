use super::super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObjectPredicateDomain {
    Card,
    Permanent,
    Spell,
}

fn lower_card_type(card_type_value: ObjectCardType) -> Value {
    card_type(card_type_value.ir_name())
}

/// Validate declaration availability and bounded numeric leaves without
/// constructing child IR or interpreting the predicate's Boolean meaning.
pub(in crate::oracle::canonical) fn validate_object_predicate_numeric_expressions(
    predicate: &ObjectPredicate,
    declarations: TargetlessDeclarationSupport,
) -> Result<(), TargetlessLoweringError> {
    match predicate {
        ObjectPredicate::NumericComparison { right, .. } => {
            validate_targetless_numeric_expression(right, declarations)
        }
        ObjectPredicate::Not(child) => {
            validate_object_predicate_numeric_expressions(child, declarations)
        }
        ObjectPredicate::Intersection(children) | ObjectPredicate::Union(children) => {
            for child in children {
                validate_object_predicate_numeric_expressions(child, declarations)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Recursively lowers one shared object-predicate AST. Domain identities are
/// represented exactly as the former context-specific lowerers represented
/// them so this extraction does not alter canonical IR.
fn lower_object_predicate(
    predicate: &ObjectPredicate,
    domain: ObjectPredicateDomain,
) -> Option<Value> {
    match predicate {
        ObjectPredicate::IsCommander => Some(json!({ "kind": "isCommander" })),
        ObjectPredicate::IsToken => Some(json!({ "kind": "isToken" })),
        ObjectPredicate::CardType(card_type_value) => Some(lower_card_type(*card_type_value)),
        ObjectPredicate::Subtype(value) => Some(subtype(value)),
        ObjectPredicate::CharacteristicBatch(batch) => {
            Some(or(characteristic_batch_members(*batch)
                .iter()
                .filter_map(|member| {
                    lower_object_predicate(&member.characteristic.predicate(), domain)
                })
                .collect()))
        }
        ObjectPredicate::Color(color) => Some(json!({
            "kind": "colorContains",
            "value": canonical_color_name(*color),
        })),
        // The shared color-count characteristic compared with a constant.
        ObjectPredicate::ColorQuantity(quantity) => {
            let (operator, count) = match quantity {
                ColorQuantity::Colorless => ("==", 0),
                ColorQuantity::Monocolored => ("==", 1),
                ColorQuantity::Multicolored => (">=", 2),
            };
            Some(compare(
                operator,
                json!({ "kind": "colorCountOf", "object": { "kind": "candidate" } }),
                integer(count),
            ))
        }
        ObjectPredicate::Supertype(CardSupertype::Basic) => {
            Some(json!({ "kind": "typeLineContains", "value": "Basic" }))
        }
        ObjectPredicate::Supertype(CardSupertype::Legendary) => {
            Some(json!({ "kind": "isLegendary" }))
        }
        // CR 205.4a: the printed type line carries the snow supertype.
        ObjectPredicate::Supertype(CardSupertype::Snow) => {
            Some(json!({ "kind": "typeLineContains", "value": "Snow" }))
        }
        ObjectPredicate::ExactManaCost(cost) => Some(json!({
            "kind": "manaCostEquals",
            "value": cost.as_oracle_cost(),
        })),
        ObjectPredicate::Status(status) => {
            let (kind, negated) = status.ir_atom();
            let atom = json!({ "kind": kind });
            Some(if negated { not(atom) } else { atom })
        }
        ObjectPredicate::OwnedByAbilityController { negated } => {
            let atom = json!({ "kind": "ownedBy", "player": controller() });
            Some(if *negated { not(atom) } else { atom })
        }
        ObjectPredicate::Designation(designation) => Some(json!({
            "kind": "hasDesignation",
            "designation": designation.flag(),
        })),
        ObjectPredicate::PermanentCard => Some(json!({ "kind": "isPermanentCard" })),
        ObjectPredicate::HasCounter(presence) => {
            // The established atom is "at least one"; bounds are written only
            // when they differ from it, so the existing IR is unchanged.
            let mut atom = json!({ "kind": "hasCounter" });
            if let Some(counter) = &presence.counter {
                atom["counter"] = json!(counter.as_str());
            }
            if presence.minimum != 1 {
                atom["minimum"] = json!(presence.minimum);
            }
            if let Some(maximum) = presence.maximum {
                atom["maximum"] = json!(maximum);
            }
            Some(atom)
        }
        ObjectPredicate::Relation(ObjectRelation::Named(name)) => Some(json!({
            "kind": "nameEquals",
            "value": name,
        })),
        ObjectPredicate::Relation(ObjectRelation::DealtDamageToPlayerThisTurn {
            recipient,
            combat_only,
        }) => Some(json!({
            "kind": "dealtDamageToPlayerThisTurn",
            "recipient": recipient.ir_name(),
            "combatOnly": combat_only,
        })),
        // CR 115.1: a stack object one of whose targets is a listed player
        // or a permanent of the listed query. Players and controllers are
        // roles relative to the ability controller choosing the candidates.
        ObjectPredicate::Relation(ObjectRelation::Targets(targeted)) => {
            let mut atom = json!({ "kind": "targets" });
            if let Some(player) = targeted.players {
                atom["players"] = json!(player.ir_name());
            }
            if let Some(permanents) = &targeted.permanents {
                let mut query = json!({
                    "where": lower_permanent_object_predicate(&permanents.predicate),
                });
                if let Some(controller) = permanents.controller {
                    query["controller"] = json!(match controller {
                        ObjectControllerScope::AbilityController => "abilityController",
                        ObjectControllerScope::NotAbilityController => "notAbilityController",
                        ObjectControllerScope::OpponentOfAbilityController => {
                            "opponentOfAbilityController"
                        }
                        // Refused by the engine's target-relation validator.
                        ObjectControllerScope::EventPlayer => "eventPlayer",
                        // Refused by the relative clause before lowering.
                        ObjectControllerScope::Player(_) => "relatedPlayer",
                    });
                }
                atom["permanents"] = query;
            }
            Some(atom)
        }
        ObjectPredicate::ChosenCreatureType => Some(json!({
            "kind": "chosenCreatureType",
            "decisionId": crate::oracle::contextual_binding::RESOLUTION_CREATURE_TYPE_DECISION,
        })),
        ObjectPredicate::ChosenColor => Some(json!({
            "kind": "chosenColor",
            "decisionId": crate::oracle::contextual_binding::RESOLUTION_COLOR_DECISION,
        })),
        ObjectPredicate::HasKeyword(keyword) => Some(json!({
            "kind": "hasKeyword",
            "value": keyword.canonical_name(),
        })),
        ObjectPredicate::NumericComparison {
            left,
            operator,
            right,
        } => lower_selector_numeric_comparison(*left, *operator, right),
        ObjectPredicate::ObjectKind(object_kind) => match (domain, object_kind) {
            (ObjectPredicateDomain::Card, ObjectKind::Card)
            | (ObjectPredicateDomain::Permanent, ObjectKind::Permanent)
            | (ObjectPredicateDomain::Spell, ObjectKind::Spell) => None,
            _ => unreachable!("the selector automaton validates its object domain before lowering"),
        },
        ObjectPredicate::Not(operand) => lower_object_predicate(operand, domain).map(not),
        ObjectPredicate::Intersection(operands) => {
            let lowered = operands
                .iter()
                .filter_map(|operand| lower_object_predicate(operand, domain))
                .collect::<Vec<_>>();
            match lowered.len() {
                0 => None,
                1 => lowered.into_iter().next(),
                _ => Some(and(lowered)),
            }
        }
        // A branch naming the whole domain (`permanents or artifacts`) makes
        // the union the whole domain; dropping it would narrow the query.
        ObjectPredicate::Union(operands) => operands
            .iter()
            .map(|operand| lower_object_predicate(operand, domain))
            .collect::<Option<Vec<_>>>()
            .map(or),
    }
}

/// A permanent collection over `predicate`: its filter and, when the query
/// states one, its ownership relation. `None` when an ownership relation is
/// nested where a collection cannot carry it.
pub(in crate::oracle::canonical) fn lower_permanent_collection(
    predicate: &ObjectPredicate,
) -> Option<Value> {
    let (rest, ownership) = split_collection_ownership(predicate)?;
    let mut collection = json!({
        "kind": "permanents",
        "where": lower_permanent_object_predicate(&rest),
    });
    if let Some(ownership) = ownership {
        collection["ownership"] = json!(ownership);
    }
    Some(collection)
}

/// Splits a top-level ownership relation off a permanent collection's
/// predicate: the engine's collections carry it as `ownership`
/// (`owned` / `notOwned`). `None` when an ownership atom is nested where a
/// collection relation cannot express it.
pub(in crate::oracle::canonical) fn split_collection_ownership(
    predicate: &ObjectPredicate,
) -> Option<(ObjectPredicate, Option<&'static str>)> {
    fn contains_owner(predicate: &ObjectPredicate) -> bool {
        match predicate {
            ObjectPredicate::OwnedByAbilityController { .. } => true,
            ObjectPredicate::Not(child) => contains_owner(child),
            ObjectPredicate::Intersection(children) | ObjectPredicate::Union(children) => {
                children.iter().any(contains_owner)
            }
            _ => false,
        }
    }
    let relation = |negated: bool| if negated { "notOwned" } else { "owned" };
    match predicate {
        ObjectPredicate::OwnedByAbilityController { negated } => Some((
            ObjectPredicate::ObjectKind(ObjectKind::Permanent),
            Some(relation(*negated)),
        )),
        ObjectPredicate::Intersection(children) => {
            let mut ownership = None;
            let mut rest = Vec::new();
            for child in children {
                match child {
                    ObjectPredicate::OwnedByAbilityController { negated } => {
                        if ownership.is_some() {
                            return None;
                        }
                        ownership = Some(relation(*negated));
                    }
                    other if contains_owner(other) => return None,
                    other => rest.push(other.clone()),
                }
            }
            let rest = match rest.len() {
                0 => ObjectPredicate::ObjectKind(ObjectKind::Permanent),
                1 => rest.remove(0),
                _ => ObjectPredicate::Intersection(rest),
            };
            Some((rest, ownership))
        }
        other if contains_owner(other) => None,
        other => Some((other.clone(), None)),
    }
}

pub(in crate::oracle::canonical) fn lower_card_object_predicate(
    predicate: &ObjectPredicate,
) -> Value {
    lower_object_predicate(predicate, ObjectPredicateDomain::Card).unwrap_or(Value::Null)
}

pub(in crate::oracle::canonical) fn lower_permanent_object_predicate(
    predicate: &ObjectPredicate,
) -> Value {
    lower_object_predicate(predicate, ObjectPredicateDomain::Permanent).unwrap_or(Value::Null)
}

pub(in crate::oracle::canonical) fn lower_spell_object_predicate(
    predicate: &ObjectPredicate,
) -> Option<Value> {
    lower_object_predicate(predicate, ObjectPredicateDomain::Spell)
}

#[cfg(test)]
#[path = "object_predicate_token_tests.rs"]
mod token_tests;
