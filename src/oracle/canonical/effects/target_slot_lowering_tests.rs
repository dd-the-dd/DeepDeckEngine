use super::*;

#[test]
fn another_target_exclusion_survives_a_union_candidate_set() {
    let slot = TargetSlot {
        selector: TargetSelector::Union(vec![
            TargetSelector::Object(ObjectPredicate::CardType(ObjectCardType::Artifact)),
            TargetSelector::Object(ObjectPredicate::CardType(ObjectCardType::Creature)),
        ]),
        cardinality: TargetCardinality::EXACTLY_ONE,
        exclude_source: true,
        count: None,
    };
    let (_, candidates) =
        lower_target_slot_candidates(&slot).expect("the complete target slot lowers");
    assert_eq!(candidates["excludeSource"], true);
    assert_eq!(candidates["kind"], "union");
    assert_eq!(candidates["sets"][0]["where"]["value"], "Artifact");
    assert_eq!(candidates["sets"][1]["where"]["value"], "Creature");
}
