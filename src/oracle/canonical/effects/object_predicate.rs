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
                .map(|member| subtype(member.subtype))
                .collect()))
        }
        ObjectPredicate::Color(color) => Some(json!({
            "kind": "colorContains",
            "value": canonical_color_name(*color),
        })),
        ObjectPredicate::Supertype(CardSupertype::Basic) => {
            Some(json!({ "kind": "typeLineContains", "value": "Basic" }))
        }
        ObjectPredicate::Supertype(CardSupertype::Legendary) => {
            Some(json!({ "kind": "isLegendary" }))
        }
        ObjectPredicate::ExactManaCost(cost) => Some(json!({
            "kind": "manaCostEquals",
            "value": cost.as_oracle_cost(),
        })),
        ObjectPredicate::HasKeyword(keyword) => Some(json!({
            "kind": "hasKeyword",
            "value": keyword.canonical_name(),
        })),
        ObjectPredicate::NumericComparison {
            left,
            operator,
            right,
        } => Some(lower_selector_numeric_comparison(*left, *operator, right)),
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
        ObjectPredicate::Union(operands) => Some(or(operands
            .iter()
            .filter_map(|operand| lower_object_predicate(operand, domain))
            .collect())),
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
