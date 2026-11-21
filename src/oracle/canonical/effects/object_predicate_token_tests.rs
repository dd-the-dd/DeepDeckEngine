use super::{ObjectCardType, ObjectPredicate, lower_permanent_object_predicate};

#[test]
fn nontoken_creature_lowers_as_two_independent_predicates_in_source_order() {
    let predicate = ObjectPredicate::Intersection(vec![
        ObjectPredicate::Not(Box::new(ObjectPredicate::IsToken)),
        ObjectPredicate::CardType(ObjectCardType::Creature),
    ]);

    assert_eq!(
        lower_permanent_object_predicate(&predicate),
        serde_json::json!({
            "kind": "and",
            "operands": [
                { "kind": "not", "operand": { "kind": "isToken" } },
                { "kind": "cardTypeContains", "value": "Creature" },
            ],
        })
    );
}
